use crate::config::ListenAddr;
use crate::irc_listener::IrcListener;
use crate::web::error::ApiError;
use crate::{Config, DataStorage};
use axum::handler::Handler;
use axum::routing::{get, post};
use axum::{middleware, Extension, Router};
use axum::response::IntoResponse;
use futures::future::BoxFuture;
use http::{header, Method, Request, StatusCode};
use lazy_static::lazy_static;
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use tower::ServiceBuilder;
use tower_http::cors::{self, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use hyper::Body;
use tower::Service;

pub mod auth;
mod auth_endpoints;
mod auth_middleware;
pub mod error;
mod get_metrics;
pub mod get_recent_messages;
mod ignored;
mod purge;
mod record_metrics;

#[derive(Clone, Copy)]
pub struct WebAppData {
    data_storage: &'static DataStorage,
    irc_listener: &'static IrcListener,
    config: &'static Config,
}

lazy_static! {
    static ref HTTP_CLIENT: reqwest::Client = reqwest::Client::new();
}

pub async fn run(
    data_storage: &'static DataStorage,
    irc_listener: &'static IrcListener,
    config: &'static Config,
    shutdown_signal: CancellationToken,
) -> Result<BoxFuture<'static, hyper::Result<()>>, BindError> {
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
    let method_fallback = || (|| async { ApiError::MethodNotAllowed }).into_service();

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
        .layer(
            ServiceBuilder::new()
                .layer(cors)
                .layer(Extension(shared_state)),
        );

    let mut servedir = ServeDir::new("web/dist")
        .append_index_html_on_directories(true)
        .fallback(ServeFile::new("web/dist/index.html"));

    let app = Router::new()
        .nest("/api/v2", api)
        .fallback((|request: Request<Body>| async move {
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
        }).into_service())
        .route_layer(middleware::from_fn(record_metrics::record_metrics));

    Ok(match &config.web.listen_address {
        ListenAddr::Tcp { address } => Box::pin(
            axum::Server::try_bind(address)?
                .serve(app.into_make_service())
                .with_graceful_shutdown(async move {
                    shutdown_signal.cancelled().await;
                }),
        ),
        #[cfg(unix)]
        ListenAddr::Unix { path } => {
            use hyperlocal::UnixServerExt;

            Box::pin(
                axum::Server::bind_unix(path)?
                    .serve(app.into_make_service())
                    .with_graceful_shutdown(async move {
                        shutdown_signal.cancelled().await;
                    }),
            )
        }
    })
}

#[derive(Error, Debug)]
pub enum BindError {
    #[error("{0}")]
    Tcp(#[from] hyper::Error),
    #[cfg(unix)]
    #[error("{0}")]
    Unix(#[from] std::io::Error),
}
