#![type_length_limit = "99999999"]
#![deny(clippy::all)]
#![deny(clippy::cargo)]

mod config;
mod db;
mod irc_listener;
mod message_export;
mod system_monitoring;
mod web;

use crate::config::{Args, Config};
use crate::db::DataStorage;
use futures::prelude::*;
use metrics_runtime::Receiver;
use structopt::StructOpt;

#[tokio::main]
async fn main() {
    env_logger::init();

    // init metrics system
    let metrics_receiver = Receiver::builder()
        .build()
        .expect("failed to create receiver");
    let metrics_controller = metrics_receiver.controller();
    metrics_receiver.install();
    system_monitoring::spawn_system_monitoring();

    // args and config parsing
    let args = Args::from_args();
    let config = config::load_config(&args).await;
    let config = match config {
        Ok(config) => config,
        Err(e) => {
            log::debug!("Parsed args: {:#?}", args);
            log::error!(
                "Failed to load config from `{}`: {}",
                args.config_path.to_string_lossy(),
                e,
            );
            std::process::exit(1);
        }
    };
    let config: &'static Config = Box::leak(Box::new(config));

    log::info!("Successfully loaded config");
    log::debug!("Parsed args: {:#?}", args);
    log::debug!("Config: {:#?}", config);

    // db init
    let db = db::connect_to_postgresql(&config).await;
    let migrations_result = db::run_migrations(&db).await;
    match migrations_result {
        Ok(()) => {
            log::info!("Successfully ran database migrations");
        }
        Err(e) => {
            log::error!("Failed to run database migrations: {}", e);
            std::process::exit(1);
        }
    }

    // web server init
    let listener = web::bind(&config).await;
    let listener = match listener {
        Ok(listener) => {
            log::info!(
                "Web server bound successfully, listening for requests at `{}`",
                config.web.listen_address
            );
            listener
        }
        Err(e) => {
            // e is a custom error here so it's already nicely formatted (WebServerStartError)
            log::error!("{}", e);
            std::process::exit(1);
        }
    };

    let data_storage = db::DataStorage::new(db);
    let data_storage: &'static DataStorage = Box::leak(Box::new(data_storage));
    let res = data_storage.load_messages_from_disk(config).await;
    match res {
        Ok(()) => log::info!("Finished loading stored messages"),
        Err(e) => {
            log::error!("Failed to load stored messages: {}", e);
            std::process::exit(1);
        }
    }

    let irc_listener = Box::leak(Box::new(irc_listener::IrcListener::start(
        data_storage,
        config,
    )));

    tokio::spawn(data_storage.run_task_vacuum_old_messages(config));
    let web_join_handle = tokio::spawn(web::run(
        listener,
        metrics_controller,
        data_storage,
        irc_listener,
        config,
    ));

    #[cfg(unix)]
    let ctrl_c_event = async {
        use tokio::signal::unix::{signal, SignalKind};

        let sigint = signal(SignalKind::interrupt()).unwrap();
        let sigterm = signal(SignalKind::terminate()).unwrap();

        futures::stream::select(sigint, sigterm).next().await
    };
    #[cfg(not(unix))]
    let ctrl_c_event =
        tokio::signal::ctrl_c().map(|res| res.expect("Failed to listen to Ctrl-C event"));

    // await termination.
    tokio::select! {
        _ = ctrl_c_event => {
            log::info!("Interrupted, shutting down");
        }
        _ = web_join_handle => {
            log::error!("Web task ended with some sort of error (see log output above!) - Shutting down!")
        }
    }

    let res = data_storage.save_messages_to_disk(config).await;
    match res {
        Ok(()) => log::info!("Finished saving stored messages"),
        Err(e) => {
            log::error!("Failed to save messages: {}", e);
            std::process::exit(1);
        }
    }
}
