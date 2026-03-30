use crate::config::ListenAddr;
use crate::irc_listener::IrcListener;
use crate::web::error::ApiError;
use crate::{Config, DataStorage};
use axum::routing::{get, post};
use axum::{Extension, Router, middleware};
use axum::{body::Body, response::IntoResponse};
use futures::future::BoxFuture;
use http::{Method, Request, StatusCode, header};
use std::{net::SocketAddr, sync::LazyLock};
use thiserror::Error;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tower::Service;
use tower::ServiceBuilder;
use tower_http::cors::{self, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
#[cfg(unix)]
use {
    std::fs::Permissions, std::io::ErrorKind,
    std::os::unix::fs::PermissionsExt, std::path::Path, tokio::net::UnixListener,
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
    let method_fallback = || || async { ApiError::MethodNotAllowed };
    let api = Router::new()
        .route(
            "/recent-messages/:channel_login",
            get(get_recent_messages::get_recent_messages).fallback(method_fallback()),
        )
        .route(
            "/ignored",
            get(ignored::get_ignored)
                .post(ignored::set_ignored)
                .route_layer(auth_middleware())
                .fallback(method_fallback()),
        )
        .route(
            "/purge",
            post(purge::purge_messages)
                .route_layer(auth_middleware())
                .fallback(method_fallback()),
        )
        .route(
            "/auth/create",
            post(auth_endpoints::create_token).fallback(method_fallback()),
        )
        .route(
            "/auth/extend",
            post(auth_endpoints::extend_token)
                .route_layer(auth_middleware())
                .fallback(method_fallback()),
        )
        .route(
            "/auth/revoke",
            post(auth_endpoints::revoke_token)
                .route_layer(auth_middleware())
                .fallback(method_fallback()),
        )
        .route(
            "/metrics",
            get(get_metrics::get_metrics).fallback(method_fallback()),
        )
        .layer(cors);

    let mut servedir = ServeDir::new("web/dist")
        .append_index_html_on_directories(true)
        .fallback(ServeFile::new("web/dist/index.html"));

    let app = Router::new()
        .nest("/api/v2", api)
        .fallback(|request: Request<Body>| async move {
            if request.uri().path().starts_with("/api/v2/") || request.uri().path() == "/api/v2" {
                ApiError::NotFound.into_response()
            } else {
                // try for a file
                match servedir.call(request).await {
                    Ok(response) => response.into_response(),
                    Err(e) => {
                        tracing::error!("Error trying to serve static file: {}", e);
                        StatusCode::INTERNAL_SERVER_ERROR.into_response()
                    }
                }
            }
        })
        .layer(
            ServiceBuilder::new()
                .layer(Extension(shared_state))
                .layer(middleware::from_fn(record_metrics::record_metrics))
                .layer(middleware::from_fn(timeout::timeout)),
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
            };
            tokio::fs::create_dir_all(path.parent().unwrap())
                .await
                .map_err(|e| BindError::CreateParentDir(path, e))?;

            let listener = UnixListener::bind(path.clone()).map_err(|e| BindError::BindUnix(path, e))?;

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
