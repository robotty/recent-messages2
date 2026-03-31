use crate::config::ListenAddr;
use crate::irc_listener::IrcListener;
use crate::web::error::ApiError;
use crate::{Config, DataStorage};
use axum::routing::{get, post};
use axum::{Extension, Router, middleware};
use futures::future::BoxFuture;
use http::{Method, header};
use std::{net::SocketAddr, sync::LazyLock};
use thiserror::Error;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tower::ServiceBuilder;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::{
    cors::{self, CorsLayer},
    trace::TraceLayer,
};
#[cfg(unix)]
use {
    std::fs::Permissions, std::io::ErrorKind, std::os::unix::fs::PermissionsExt, std::path::Path,
    tokio::net::UnixListener,
};

pub mod auth;
mod auth_endpoints;
mod auth_middleware;
pub mod error;
mod get_metrics;
pub mod get_recent_messages;
mod ignored;
mod purge;
mod record_metrics;
mod timeout;

#[derive(Clone, Copy)]
pub struct WebAppData {
    data_storage: &'static DataStorage,
    irc_listener: &'static IrcListener,
    config: &'static Config,
}

static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(reqwest::Client::new);

#[derive(Error, Debug)]
pub enum BindError {
    #[error("Failed to bind to address `{0}`: {1}")]
    BindTcp(&'static SocketAddr, std::io::Error),
    #[cfg(unix)]
    #[error(
        "Failed to delete old unix socket at `{path}`: {err}",
        path = .0.display(),
        err = .1
    )]
    DeleteOldSocketFile(&'static Path, std::io::Error),
    #[cfg(unix)]
    #[error(
        "Failed to create parent directory for unix socket `{path}`: {err}",
        path = .0.display(),
        err = .1
    )]
    CreateParentDir(&'static Path, std::io::Error),
    #[cfg(unix)]
    #[error(
        "Failed to bind to unix socket `{path}`: {err}",
        path = .0.display(),
        err = .1
    )]
    BindUnix(&'static Path, std::io::Error),
    #[cfg(unix)]
    #[error(
        "Failed to alter permissions on unix socket `{path}` to `{permissions:?}`: {err}",
        path = .0.display(),
        permissions = .1,
        err = .2
    )]
    SetPermissions(&'static Path, Permissions, std::io::Error),
}

#[cfg_attr(not(unix), allow(clippy::unused_async))]
pub async fn run(
    data_storage: &'static DataStorage,
    irc_listener: &'static IrcListener,
    config: &'static Config,
    shutdown_signal: CancellationToken,
) -> Result<BoxFuture<'static, std::io::Result<()>>, BindError> {
    let shared_state = WebAppData {
        data_storage,
        irc_listener,
        config,
    };

    let cors = CorsLayer::new()
        .allow_methods(vec![Method::GET, Method::POST])
        .allow_headers(vec![
            header::AUTHORIZATION,
            header::ACCEPT,
            header::CONTENT_TYPE,
        ])
        .allow_origin(cors::Any);

    let auth_middleware = || {
        middleware::from_fn(move |req, next| {
            auth_middleware::with_authorization(req, next, shared_state)
        })
    };
    let api = Router::new()
        .route(
            "/recent-messages/{channel_login}",
            get(get_recent_messages::get_recent_messages),
        )
        .route(
            "/ignored",
            get(ignored::get_ignored)
                .post(ignored::set_ignored)
                .route_layer(auth_middleware()),
        )
        .route(
            "/purge",
            post(purge::purge_messages).route_layer(auth_middleware()),
        )
        .route("/auth/create", post(auth_endpoints::create_token))
        .route(
            "/auth/extend",
            post(auth_endpoints::extend_token).route_layer(auth_middleware()),
        )
        .route(
            "/auth/revoke",
            post(auth_endpoints::revoke_token).route_layer(auth_middleware()),
        )
        .route("/metrics", get(get_metrics::get_metrics))
        .method_not_allowed_fallback(|| async { ApiError::MethodNotAllowed })
        .fallback(|| async { ApiError::NotFound })
        .layer(cors);

    let servedir = ServeDir::new("web/dist")
        .append_index_html_on_directories(true)
        .fallback(ServeFile::new("web/dist/index.html"));

    let app = Router::new()
        .nest("/api/v2", api)
        .fallback_service(servedir)
        .layer(
            ServiceBuilder::new()
                .layer(Extension(shared_state))
                .layer(middleware::from_fn(record_metrics::record_metrics))
                .layer(middleware::from_fn(timeout::timeout))
                .layer(TraceLayer::new_for_http()),
        );

    Ok(match &config.web.listen_address {
        ListenAddr::Tcp { address } => {
            let listener = TcpListener::bind(address)
                .await
                .map_err(|e| BindError::BindTcp(address, e))?;

            Box::pin(
                axum::serve(listener, app)
                    .with_graceful_shutdown(async move {
                        shutdown_signal.cancelled().await;
                    })
                    .into_future(),
            )
        }
        #[cfg(unix)]
        ListenAddr::Unix { path } => {
            if let Err(e) = tokio::fs::remove_file(&path).await
                && e.kind() != ErrorKind::NotFound
            {
                return Err(BindError::DeleteOldSocketFile(path, e));
            }
            tokio::fs::create_dir_all(path.parent().unwrap())
                .await
                .map_err(|e| BindError::CreateParentDir(path, e))?;

            let listener =
                UnixListener::bind(path.clone()).map_err(|e| BindError::BindUnix(path, e))?;

            let permissions = Permissions::from_mode(0o777);
            tokio::fs::set_permissions(path, permissions.clone())
                .await
                .map_err(|e| BindError::SetPermissions(path, permissions, e))?;

            Box::pin(
                axum::serve(listener, app)
                    .with_graceful_shutdown(async move {
                        shutdown_signal.cancelled().await;
                    })
                    .into_future(),
            )
        }
    })
}
