#![type_length_limit = "99999999"]
#![deny(clippy::all)]
#![deny(clippy::cargo)]

mod config;
mod db;
mod irc_listener;
mod message_export;
mod monitoring;
mod shutdown;
mod web;

use crate::config::{Args, Config};
use crate::db::DataStorage;
use futures::future::FusedFuture;
use futures::prelude::*;
use structopt::StructOpt;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // args and config parsing
    let args = Args::from_args();
    tracing::debug!("Parsed args: {:#?}", args);

    let config = config::load_config(&args).await;
    let config = match config {
        Ok(config) => config,
        Err(e) => {
            tracing::error!(
                "Failed to load config from `{}`: {}",
                args.config_path.display(),
                e,
            );
            std::process::exit(1);
        }
    };
    let config: &'static Config = Box::leak(Box::new(config));

    tracing::debug!("Config: {:#?}", config);

    #[cfg(unix)]
    increase_nofile_rlimit();
    let shutdown_signal = CancellationToken::new();

    let process_monitoring_join_handle =
        tokio::spawn(monitoring::run_process_monitoring(shutdown_signal.clone()));

    // db init
    let data_storage = Box::leak(Box::new(db::connect_to_postgresql(&config).await));
    let migrations_result = data_storage.run_migrations().await;
    match migrations_result {
        Ok(()) => {
            tracing::info!("Successfully ran database migrations");
        }
        Err(e) => {
            tracing::error!("Failed to run database migrations: {}", e);
            std::process::exit(1);
        }
    }
    if let Err(e) = data_storage.fetch_initial_metrics_values().await {
        tracing::error!("Failed to query some initial message count from the DB to initialize exported metrics: {}", e);
        std::process::exit(1);
    }

    let (
        irc_listener,
        forward_worker_join_handle,
        chunk_worker_join_handle,
        channel_jp_join_handle,
    ) = irc_listener::IrcListener::start(data_storage, config, shutdown_signal.clone());
    let irc_listener = Box::leak(Box::new(irc_listener));

    let old_msg_vacuum_join_handle =
        tokio::spawn(data_storage.run_task_vacuum_old_messages(config, shutdown_signal.clone()));

    let webserver =
        match web::run(data_storage, irc_listener, config, shutdown_signal.clone()).await {
            Ok(webserver) => webserver,
            Err(bind_error) => {
                tracing::error!("{}", bind_error);
                std::process::exit(1);
            }
        };
    let webserver_join_handle = tokio::spawn(webserver);

    // await termination.
    let os_shutdown_signal = shutdown::shutdown_signal().fuse();
    futures::pin_mut!(os_shutdown_signal);

    let with_name = move |fut: JoinHandle<()>, name| fut.map(move |x| (x, name));
    let mut simple_workers = [
        with_name(process_monitoring_join_handle, "Process Monitoring task").fuse(),
        with_name(
            forward_worker_join_handle,
            "IRC message forwarder (preprocessor)",
        )
        .fuse(),
        with_name(
            chunk_worker_join_handle,
            "IRC message-to-database-forwarder",
        )
        .fuse(),
        with_name(channel_jp_join_handle, "IRC channel join/part task").fuse(),
        with_name(old_msg_vacuum_join_handle, "Old message vacuum task").fuse(),
    ];

    let mut webserver_join_handle = webserver_join_handle.fuse();
    let mut exit_code: i32 = 0;
    loop {
        let all_simple_workers_terminated = simple_workers.iter().all(|fut| fut.is_terminated());
        if all_simple_workers_terminated && webserver_join_handle.is_terminated() {
            tracing::info!("Everything shut down successfully, ending");
            break;
        }

        let any_simple_worker = futures::future::select_all(simple_workers.iter_mut());

        tokio::select! {
            _ = &mut os_shutdown_signal, if !os_shutdown_signal.is_terminated() => {
                tracing::debug!("Received shutdown signal");
                shutdown_signal.cancel();
            },
            fut_output = any_simple_worker, if !all_simple_workers_terminated => {
                let ((worker_result, name), _, _) = fut_output;
                match worker_result {
                    Ok(()) => {
                        if !shutdown_signal.is_cancelled() {
                            tracing::error!("{} ended without error even though no shutdown was requested (shutting down other parts of application gracefully)", name);
                            shutdown_signal.cancel();
                            exit_code = 1;
                        } else {
                            // regular end after graceful shutdown request
                            tracing::info!("{} has successfully shut down gracefully", name);
                        }
                    }
                    Err(join_error) => {
                        tracing::error!(
                            "{} ended abnormally (shutting down other parts of application gracefully): {}",
                            name,
                            join_error
                        );
                        shutdown_signal.cancel();
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
                        if !shutdown_signal.is_cancelled() {
                            tracing::error!("Webserver ended without error even though no shutdown was requested (shutting down other parts of application gracefully)");
                            shutdown_signal.cancel();
                            exit_code = 1;
                        } else {
                            // regular end after graceful shutdown request
                            tracing::info!("Webserver has successfully shut down gracefully");
                        }
                    },
                    Ok(Err(tower_error)) => {
                        tracing::error!("Webserver encountered fatal error (shutting down other parts of application gracefully): {}", tower_error);
                        shutdown_signal.cancel();
                        exit_code = 1;
                    },
                    Err(join_error) => {
                        tracing::error!("Webserver tokio task ended abnormally (shutting down other parts of application gracefully): {}", join_error);
                        shutdown_signal.cancel();
                        exit_code = 1;
                    }
                }
            }
        }
    }

    std::process::exit(exit_code);
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
