use crate::config::ListenAddr;
use crate::Config;
use axum::routing::get;
use axum::Router;
use futures::future::BoxFuture;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

pub async fn run(
    config: &'static Config,
    app_shutdown_signal: CancellationToken,
) -> Result<BoxFuture<'static, hyper::Result<()>>, BindError> {
    // build our application with a single route
    let app = Router::new().route("/", get(|| async { "Hello, World!" }));

    // run it with hyper on localhost:3000
    Ok(match &config.web.listen_address {
        ListenAddr::Tcp { address } => Box::pin(
            axum::Server::try_bind(address)?
                .serve(app.into_make_service())
                .with_graceful_shutdown(async move {
                    app_shutdown_signal.cancelled().await;
                }),
        ),
        #[cfg(unix)]
        ListenAddr::Unix { path } => {
            use hyperlocal::UnixServerExt;

            Box::pin(
                axum::Server::bind_unix(path)?
                    .serve(app.into_make_service())
                    .with_graceful_shutdown(async move {
                        app_shutdown_signal.cancelled().await;
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
