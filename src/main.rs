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
#[cfg(not(unix))]
use futures::prelude::*;
use metrics_exporter_prometheus::PrometheusBuilder;
use structopt::StructOpt;

#[tokio::main]
async fn main() {
    env_logger::init();

    // unix: increase NOFILE rlimit
    #[cfg(unix)]
    increase_nofile_rlimit();

    // init metrics system
    let prom_recorder = Box::leak(Box::new(PrometheusBuilder::new().build()));
    let prom_handle = prom_recorder.handle();
    metrics::set_recorder(prom_recorder).unwrap();
    system_monitoring::spawn_system_monitoring();
    register_application_metrics();

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
        prom_handle,
        data_storage,
        irc_listener,
        config,
    ));

    #[cfg(unix)]
    let ctrl_c_event = async {
        use tokio::signal::unix::{signal, SignalKind};

        let mut sigint = signal(SignalKind::interrupt()).unwrap();
        let mut sigterm = signal(SignalKind::terminate()).unwrap();

        tokio::select! {
            _ = sigint.recv() => {},
            _ = sigterm.recv() => {}
        }
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

#[cfg(unix)]
fn increase_nofile_rlimit() {
    use rlimit::Resource;
    let (soft, hard) = match Resource::NOFILE.get() {
        Ok((soft, hard)) => (soft, hard),
        Err(e) => {
            log::error!(
                "Failed to get NOFILE rlimit, will not attempt to increase rlimit: {}",
                e
            );
            return;
        }
    };
    log::debug!(
        "NOFILE rlimit: process was started with limits set to {} soft, {} hard",
        soft,
        hard
    );

    if soft < hard {
        match Resource::NOFILE.set(hard, hard) {
            Ok(()) => log::info!(
                "Successfully increased NOFILE rlimit to {}, was at {}",
                hard,
                soft
            ),
            Err(e) => log::error!("Failed to increase NOFILE rlimit to {}: {}", hard, e),
        }
    } else {
        log::debug!("NOFILE rlimit: no need to increase (soft limit is not below hard limit)")
    }
}

/// Register all created metrics to initialize them as zero and give them their description.
fn register_application_metrics() {
    metrics::register_counter!(
        "recent_messages_messages_appended",
        "Total number of messages appended to storage"
    );
    metrics::register_gauge!(
        "recent_messages_messages_stored",
        "Number of messages currently stored in storage"
    );
    metrics::register_counter!(
        "recent_messages_messages_vacuumed",
        "Total number of messages that were removed by the automatic vacuum runner"
    );
    metrics::register_counter!(
        "recent_messages_message_vacuum_runs",
        "Total number of times the automatic vacuum runner has been started for a certain channel"
    );
    metrics::register_histogram!(
        "http_request_duration_milliseconds",
        metrics::Unit::Milliseconds,
        "Distribution of how many milliseconds incoming web requests took to answer them"
    );
    metrics::register_counter!("http_request", "Total number of incoming HTTP requests");
}
