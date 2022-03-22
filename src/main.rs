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
    tracing_subscriber::fmt::init();

    // unix: increase NOFILE rlimit
    #[cfg(unix)]
    increase_nofile_rlimit();

    // init metrics system
    let prom_handle = PrometheusBuilder::new()
        .install_recorder()
        .expect("failed to install prometheus recorder");
    system_monitoring::spawn_system_monitoring();
    register_application_metrics();

    // args and config parsing
    let args = Args::from_args();
    let config = config::load_config(&args).await;
    let config = match config {
        Ok(config) => config,
        Err(e) => {
            tracing::debug!("Parsed args: {:#?}", args);
            tracing::error!(
                "Failed to load config from `{}`: {}",
                args.config_path.to_string_lossy(),
                e,
            );
            std::process::exit(1);
        }
    };
    let config: &'static Config = Box::leak(Box::new(config));

    tracing::info!("Successfully loaded config");
    tracing::debug!("Parsed args: {:#?}", args);
    tracing::debug!("Config: {:#?}", config);

    // db init
    let db = db::connect_to_postgresql(&config).await;
    let migrations_result = db::run_migrations(&db).await;
    match migrations_result {
        Ok(()) => {
            tracing::info!("Successfully ran database migrations");
        }
        Err(e) => {
            tracing::error!("Failed to run database migrations: {}", e);
            std::process::exit(1);
        }
    }

    // web server init
    let listener = web::bind(&config).await;
    let listener = match listener {
        Ok(listener) => {
            tracing::info!(
                "Web server bound successfully, listening for requests at `{}`",
                config.web.listen_address
            );
            listener
        }
        Err(e) => {
            // e is a custom error here so it's already nicely formatted (WebServerStartError)
            tracing::error!("{}", e);
            std::process::exit(1);
        }
    };

    let data_storage = db::DataStorage::new(db);
    let data_storage: &'static DataStorage = Box::leak(Box::new(data_storage));
    let res = data_storage.load_messages_from_disk(config).await;
    match res {
        Ok(()) => tracing::info!("Finished loading stored messages"),
        Err(e) => {
            tracing::error!("Failed to load stored messages: {}", e);
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
            tracing::info!("Interrupted, shutting down");
        }
        _ = web_join_handle => {
            tracing::error!("Web task ended with some sort of error (see log output above!) - Shutting down!")
        }
    }

    let res = data_storage.save_messages_to_disk(config).await;
    match res {
        Ok(()) => tracing::info!("Finished saving stored messages"),
        Err(e) => {
            tracing::error!("Failed to save messages: {}", e);
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
            tracing::error!(
                "Failed to get NOFILE rlimit, will not attempt to increase rlimit: {}",
                e
            );
            return;
        }
    };
    tracing::debug!(
        "NOFILE rlimit: process was started with limits set to {} soft, {} hard",
        soft,
        hard
    );

    if soft < hard {
        match Resource::NOFILE.set(hard, hard) {
            Ok(()) => tracing::info!(
                "Successfully increased NOFILE rlimit to {}, was at {}",
                hard,
                soft
            ),
            Err(e) => tracing::error!("Failed to increase NOFILE rlimit to {}: {}", hard, e),
        }
    } else {
        tracing::debug!("NOFILE rlimit: no need to increase (soft limit is not below hard limit)")
    }
}

/// Register all created metrics to give them their description and appropriate units.
fn register_application_metrics() {
    metrics::describe_counter!(
        "recent_messages_messages_appended",
        "Total number of messages appended to storage"
    );
    metrics::describe_gauge!(
        "recent_messages_messages_stored",
        "Number of messages currently stored in storage"
    );
    metrics::describe_counter!(
        "recent_messages_messages_vacuumed",
        "Total number of messages that were removed by the automatic vacuum runner"
    );
    metrics::describe_counter!(
        "recent_messages_message_vacuum_runs",
        "Total number of times the automatic vacuum runner has been started for a certain channel"
    );
    metrics::describe_histogram!(
        "http_request_duration_milliseconds",
        metrics::Unit::Milliseconds,
        "Distribution of how many milliseconds incoming web requests took to answer them"
    );
    metrics::describe_counter!("http_request", "Total number of incoming HTTP requests");
}
