use crate::config::ListenAddr;
use crate::irc_listener::IrcListener;
use crate::web::error::ApiError;
use crate::{Config, DataStorage};
use axum::handler::Handler;
use axum::routing::{any_service, get};
use axum::{middleware, Extension, Router};
use futures::future::BoxFuture;
use http::{header, Method};
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use tower::ServiceBuilder;
use tower_http::cors::{self, CorsLayer};

pub mod auth;
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
    };

    let cors = CorsLayer::new()
        .allow_methods(vec![Method::GET, Method::POST])
        .allow_headers(vec![
            header::AUTHORIZATION,
            header::ACCEPT,
            header::CONTENT_TYPE,
        ])
        .allow_origin(cors::Any);

    let method_fallback = || (|| async { ApiError::MethodNotAllowed }).into_service();

    let api = Router::new()
        .route(
            "/recent-messages/:channel_login",
            get(get_recent_messages::get_recent_messages).fallback(method_fallback()),
        )
        .layer(
            ServiceBuilder::new()
                .layer(cors)
                .layer(Extension(shared_state)),
        );

    let app = Router::new()
        .route(
            "/metrics",
            get(get_metrics::get_metrics).fallback(method_fallback()),
        )
        .nest("/api/v2", api)
        .fallback((|| async { ApiError::NotFound }).into_service())
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
