use crate::config::Config;
use crate::db::DataStorage;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use twitch_irc::login::StaticLoginCredentials;
use twitch_irc::message::{AsRawIRC, ServerMessage};
use twitch_irc::{ClientConfig, SecureTCPTransport, TwitchIRCClient};

#[derive(Debug, Clone)]
pub struct IrcListener {
    pub irc_client: TwitchIRCClient<SecureTCPTransport, StaticLoginCredentials>,
}

impl IrcListener {
    pub fn start(
        data_storage: &'static DataStorage,
        config: &'static Config,
        shutdown_signal: CancellationToken,
    ) -> (IrcListener, JoinHandle<()>, JoinHandle<()>) {
        let (incoming_messages, client) = TwitchIRCClient::new(ClientConfig {
            new_connection_every: Duration::from_millis(200), // TODO should make this and probably some more options configurable
            ..ClientConfig::default()
        });

        let forwarder_join_handle = tokio::spawn(IrcListener::run_forwarder(
            incoming_messages,
            data_storage,
            shutdown_signal.clone(),
        ));

        let channel_jp_join_handle = tokio::spawn(IrcListener::run_channel_join_parter(
            client.clone(),
            config,
            data_storage,
            shutdown_signal,
        ));

        (
            IrcListener { irc_client: client },
            forwarder_join_handle,
            channel_jp_join_handle,
        )
    }

    async fn run_forwarder(
        mut incoming_messages: mpsc::UnboundedReceiver<ServerMessage>,
        data_storage: &'static DataStorage,
        shutdown_signal: CancellationToken,
    ) {
        let worker = async move {
            while let Some(message) = incoming_messages.recv().await {
                tokio::spawn(async move {
                    if let Some(channel_login) = message.channel_login() {
                        let message_source = message.source().as_raw_irc();
                        let res = data_storage
                            .append_message(channel_login.to_owned(), message_source)
                            .await;

                        if let Err(e) = res {
                            tracing::error!("Failed to append message to storage: {}", e);
                        }
                    }
                });
            }
        };

        tokio::select! {
            _ = worker => {},
            _ = shutdown_signal.cancelled() => {}
        }
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
            ServerMessage::HostTarget(m) => Some(&m.channel_login),
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
