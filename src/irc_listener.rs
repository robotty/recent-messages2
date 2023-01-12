use crate::config::Config;
use crate::db::DataStorage;
use chrono::Utc;
use lazy_static::lazy_static;
use prometheus::{linear_buckets, register_histogram, Histogram};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use twitch_irc::login::StaticLoginCredentials;
use twitch_irc::message::{AsRawIRC, ServerMessage};
use twitch_irc::{ClientConfig, SecureTCPTransport, TwitchIRCClient};

lazy_static! {
    static ref INTERNAL_FORWARD_TIME_TAKEN: Histogram = register_histogram!(
        "recentmessages_irc_forwarder_internal_forward_message_time_taken_seconds",
        "Time taken to add a message to the internal channel, this amount will climb if the system is overloaded"
    )
    .unwrap();
}

#[derive(Debug, Clone)]
pub struct IrcListener {
    pub irc_client: TwitchIRCClient<SecureTCPTransport, StaticLoginCredentials>,
}

impl IrcListener {
    pub fn start(
        data_storage: &'static DataStorage,
        config: &'static Config,
        shutdown_signal: CancellationToken,
    ) -> (IrcListener, JoinHandle<()>, JoinHandle<()>, JoinHandle<()>) {
        let (incoming_messages, client) = TwitchIRCClient::new(ClientConfig {
            new_connection_every: config.irc.new_connection_every,
            ..ClientConfig::default()
        });

        let (forward_worker_join_handle, chunk_worker_join_handle) = IrcListener::run_forwarder(
            incoming_messages,
            data_storage,
            config,
            shutdown_signal.clone(),
        );

        let channel_jp_join_handle = tokio::spawn(IrcListener::run_channel_join_parter(
            client.clone(),
            config,
            data_storage,
            shutdown_signal,
        ));

        (
            IrcListener { irc_client: client },
            forward_worker_join_handle,
            chunk_worker_join_handle,
            channel_jp_join_handle,
        )
    }

    fn run_forwarder(
        mut incoming_messages: mpsc::UnboundedReceiver<ServerMessage>,
        data_storage: &'static DataStorage,
        config: &'static Config,
        shutdown_signal: CancellationToken,
    ) -> (JoinHandle<()>, JoinHandle<()>) {
        let buckets = linear_buckets(10.0, 10.0, 50).unwrap();

        let store_chunk_chunk_size = register_histogram!(
            "recentmessages_irc_forwarder_store_chunk_chunk_size",
            "Number of messages per individual chunk of messages forwarded to the database",
            buckets
        )
        .unwrap();

        let (tx, mut rx) = mpsc::unbounded_channel();

        let forward_worker = async move {
            let tx = tx.clone();
            while let Some(message) = incoming_messages.recv().await {
                if let Some(channel_login) = message.channel_login() {
                    let message_source = message.source().as_raw_irc();
                    let timer = INTERNAL_FORWARD_TIME_TAKEN.start_timer();
                    tx.send((channel_login.to_owned(), Utc::now(), message_source))
                        .ok();
                    timer.observe_duration();
                }
            }
        };

        let chunk_worker = async move {
            let max_chunk_size = 10000;
            loop {
                let mut chunk = Vec::<_>::with_capacity(max_chunk_size);
                loop {
                    match rx.try_recv() {
                        Ok(message) => chunk.push(message),
                        Err(_) => break,
                    }
                    if chunk.len() >= max_chunk_size {
                        break;
                    }
                }
                if chunk.len() < max_chunk_size {
                    tokio::time::sleep(config.irc.forwarder_run_every).await;
                }
                store_chunk_chunk_size.observe(chunk.len() as f64);
                if chunk.len() == 0 {
                    continue;
                }

                data_storage.append_messages(chunk);
            }
        };

        let shutdown_signal_1 = shutdown_signal.clone();
        let forward_worker_join_handle = tokio::spawn(async move {
            tokio::select! {
                _ = forward_worker => {
                    if !shutdown_signal_1.is_cancelled() {
                        panic!("forward worker should never end")
                    }
                },
                _ = shutdown_signal_1.cancelled() => {}
            }
        });

        let chunk_worker_join_handle = tokio::spawn(async move {
            tokio::select! {
                _ = chunk_worker => {
                    if !shutdown_signal.is_cancelled() {
                        panic!("chunk worker should never end")
                    }
                },
                _ = shutdown_signal.cancelled() => {}
            }
        });

        (forward_worker_join_handle, chunk_worker_join_handle)
    }

    /// Start background loop to vacuum/part channels that are not used.
    pub async fn run_channel_join_parter(
        irc_client: TwitchIRCClient<SecureTCPTransport, StaticLoginCredentials>,
        config: &'static Config,
        data_storage: &'static DataStorage,
        shutdown_signal: CancellationToken,
    ) {
        let mut check_interval = tokio::time::interval(config.app.vacuum_channels_every);

        let worker = async move {
            loop {
                check_interval.tick().await;

                let res = data_storage
                    .get_channel_logins_to_join(config.app.channels_expire_after)
                    .await;
                let channels = match res {
                    Ok(channels_to_part) => channels_to_part,
                    Err(e) => {
                        tracing::error!("Failed to query the DB for a list of channels that should be joined. This iteration will be skipped. Cause: {}", e);
                        continue;
                    }
                };

                tracing::info!(
                    "Checked database for channels that should be joined, now at {} channels",
                    channels.len()
                );
                irc_client.set_wanted_channels(channels).unwrap();
            }
        };

        tokio::select! {
            _ = worker => {},
            _ = shutdown_signal.cancelled() => {}
        }
    }

    pub fn join_if_needed(&self, channel_login: String) {
        // the twitch_irc crate only does a JOIN if necessary
        self.irc_client.join(channel_login).unwrap();
    }

    pub async fn is_join_confirmed(&self, channel_login: String) -> bool {
        self.irc_client.get_channel_status(channel_login).await == (true, true)
    }
}

trait ServerMessageExt {
    /// Get the channel login if this message was sent to a channel.
    fn channel_login(&self) -> Option<&str>;
}

impl ServerMessageExt for ServerMessage {
    fn channel_login(&self) -> Option<&str> {
        match self {
            ServerMessage::ClearChat(m) => Some(&m.channel_login),
            ServerMessage::ClearMsg(m) => Some(&m.channel_login),
            ServerMessage::Join(m) => Some(&m.channel_login),
            ServerMessage::Notice(m) => m.channel_login.as_deref(),
            ServerMessage::Part(m) => Some(&m.channel_login),
            ServerMessage::Privmsg(m) => Some(&m.channel_login),
            ServerMessage::RoomState(m) => Some(&m.channel_login),
            ServerMessage::UserNotice(m) => Some(&m.channel_login),
            ServerMessage::UserState(m) => Some(&m.channel_login),
            _ => None,
        }
    }
}
