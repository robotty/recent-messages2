use crate::config::ListenAddr;
use crate::shutdown::ShutdownNoticeReceiver;
use crate::web::error::ApiError;
use crate::Config;
use actix_web::{get, web, App, HttpServer, Responder};
use futures::pin_mut;
use std::future::Future;
use thiserror::Error;

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
    mut shutdown_receiver: ShutdownNoticeReceiver,
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

        loop {
            tokio::select! {
                result = (&mut running_server) => return result,
                notice = shutdown_receiver.next_shutdown_notice(), if shutdown_receiver.may_have_more_notices() => {
                    drop(server_handle.stop(notice.graceful));
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
