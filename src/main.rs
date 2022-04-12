#![type_length_limit = "99999999"]
#![deny(clippy::all)]
#![deny(clippy::cargo)]

mod config;
// mod db;
// mod irc_listener;
// mod message_export;
mod monitoring;
mod web;

use crate::config::{Args, Config};
// use crate::db::DataStorage;
use futures::prelude::*;
// use metrics_exporter_prometheus::PrometheusBuilder;
use futures::future::FusedFuture;
use structopt::StructOpt;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

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

    // unix: increase NOFILE rlimit
    #[cfg(unix)]
    increase_nofile_rlimit();

    let app_shutdown_signal = CancellationToken::new();

    let process_monitoring_join_handle = tokio::spawn(monitoring::run_process_monitoring(app_shutdown_signal.clone()));

    /*
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
    */
    let webserver = match web::run(config, app_shutdown_signal.clone()).await {
        Ok(webserver) => webserver,
        Err(bind_error) => {
            tracing::error!(
                "Failed to bind webserver to {}: {}",
                config.web.listen_address,
                bind_error
            );
            std::process::exit(1);
        }
    };
    let webserver_join_handle = tokio::spawn(webserver);

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
    let ctrl_c_event = ctrl_c_event.fuse();
    futures::pin_mut!(ctrl_c_event);
    let mut process_monitoring_join_handle = process_monitoring_join_handle.fuse();
    let mut webserver_join_handle = webserver_join_handle.fuse();
    let mut exit_code: i32 = 0;
    loop {
        tokio::select! {
            _ = (&mut ctrl_c_event), if !ctrl_c_event.is_terminated() => {
                tracing::info!("Interrupted, shutting down gracefully");
                app_shutdown_signal.cancel();
            },
            process_monitoring_result = (&mut process_monitoring_join_handle), if !process_monitoring_join_handle.is_terminated() => {
                match process_monitoring_result {
                    Ok(()) => {
                        if !ctrl_c_event.is_terminated() {
                            tracing::error!("Process monitoring ended without error even though no shutdown was requested (shutting down other parts of application gracefully)");
                            app_shutdown_signal.cancel();
                            exit_code = 1;
                        } else {
                            // regular end after graceful shutdown request
                            tracing::info!("Process monitoring has successfully shut down gracefully");
                        }
                    },
                    Err(join_error) => {
                        tracing::error!("Process monitoring task ended abnormally (shutting down other parts of application gracefully): {}", join_error);
                        app_shutdown_signal.cancel();
                        exit_code = 1;
                    }
                }
            }
            webserver_result = (&mut webserver_join_handle), if !webserver_join_handle.is_terminated() => {
                // two cases:
                // - webserver ends on its own WITHOUT us sending the
                //   shutdown signal first (fatal error probably)
                //   ctrl_c_event.is_terminated() will be FALSE
                // - webserver ends after Ctrl-C shutdown request
                //   ctrl_c_event.is_terminated() will be TRUE
                match webserver_result {
                    Ok(Ok(())) => {
                        if !ctrl_c_event.is_terminated() {
                            tracing::error!("Webserver ended without error even though no shutdown was requested (shutting down other parts of application gracefully)");
                            app_shutdown_signal.cancel();
                            exit_code = 1;
                        } else {
                            // regular end after graceful shutdown request
                            tracing::info!("Webserver has successfully shut down gracefully");
                        }
                    },
                    Ok(Err(tower_error)) => {
                        tracing::error!("Webserver encountered fatal error (shutting down other parts of application gracefully): {}", tower_error);
                        app_shutdown_signal.cancel();
                        exit_code = 1;
                    },
                    Err(join_error) => {
                        tracing::error!("Webserver tokio task ended abnormally (shutting down other parts of application gracefully): {}", join_error);
                        app_shutdown_signal.cancel();
                        exit_code = 1;
                    }
                }
            },
            else => {
                tracing::info!("Everything shut down successfully, ending");
                std::process::exit(exit_code);
            }
        }
    }

    /*
    let res = data_storage.save_messages_to_disk(config).await;
    match res {
        Ok(()) => tracing::info!("Finished saving stored messages"),
        Err(e) => {
            tracing::error!("Failed to save messages: {}", e);
            std::process::exit(1);
        }
    }
    */
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

// /// Register all created metrics to give them their description and appropriate units.
// fn register_application_metrics() {
//     metrics::describe_counter!(
//         "recent_messages_messages_appended",
//         "Total number of messages appended to storage"
//     );
//     metrics::describe_gauge!(
//         "recent_messages_messages_stored",
//         "Number of messages currently stored in storage"
//     );
//     metrics::describe_counter!(
//         "recent_messages_messages_vacuumed",
//         "Total number of messages that were removed by the automatic vacuum runner"
//     );
//     metrics::describe_counter!(
//         "recent_messages_message_vacuum_runs",
//         "Total number of times the automatic vacuum runner has been started for a certain channel"
//     );
//     metrics::describe_histogram!(
//         "http_request_duration_milliseconds",
//         metrics::Unit::Milliseconds,
//         "Distribution of how many milliseconds incoming web requests took to answer them"
//     );
//     metrics::describe_counter!("http_request", "Total number of incoming HTTP requests");
// }
