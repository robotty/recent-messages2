use crate::config::ListenAddr;
use crate::web::error::ApiError;
use crate::Config;
use actix_web::{get, web, App, HttpServer, Responder};
use futures::future::FusedFuture;
use futures::{pin_mut, FutureExt};
use std::future::Future;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

pub mod auth;
pub mod error;
pub mod get_recent_messages;
pub mod ignored;
pub mod purge;

#[get("/{id}/{name}/index.html")]
async fn index(path: web::Path<(u32, String)>) -> impl Responder {
    let (id, name) = path.into_inner();
    format!("Hello {}! id:{}", name, id)
}

pub async fn run(
    config: &'static Config,
    shutdown_signal: CancellationToken,
) -> std::io::Result<impl Future<Output = std::io::Result<()>> + Send + 'static> {
    let app_factory = || App::new().service(index);

    let mut server = HttpServer::new(app_factory).disable_signals();

    match &config.web.listen_address {
        ListenAddr::Tcp { address } => {
            server = server.bind(address)?;
        }
        #[cfg(unix)]
        ListenAddr::Unix { path } => {
            server = server.bind_uds(path)?;
        }
    };
    let running_server = server.run();

    Ok(async move {
        let server_handle = running_server.handle();
        pin_mut!(running_server);
        let shutdown = shutdown_signal.cancelled().fuse();
        pin_mut!(shutdown);

        loop {
            tokio::select! {
                result = &mut running_server => return result,
                _ = &mut shutdown, if !shutdown.is_terminated() => {
                    // drop() the returned future, it's just a oneshot channel receiver
                    // returned from an otherwise sync function (that means it does NOT need to be
                    // polled to even run it in the first place)
                    drop(server_handle.stop(true));
                },
            }
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
