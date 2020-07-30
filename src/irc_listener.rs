use crate::config::Config;
use crate::db::{DataStorage, IterationTimestamp, StorageError};
use futures::prelude::*;
use std::borrow::Cow;
use tokio::sync::mpsc;
use twitch_irc::login::StaticLoginCredentials;
use twitch_irc::message::{AsRawIRC, ServerMessage};
use twitch_irc::{ClientConfig, TCPTransport, TwitchIRCClient};

#[derive(Debug, Clone)]
pub struct IrcListener {
    pub irc_client: TwitchIRCClient<TCPTransport, StaticLoginCredentials>,
}

impl IrcListener {
    pub fn start(data_storage: &'static DataStorage, config: &Config) -> IrcListener {
        let (incoming_messages, client) = TwitchIRCClient::new(ClientConfig {
            metrics_identifier: Some(Cow::Borrowed("listener")),
            ..ClientConfig::default()
        });

        tokio::spawn(IrcListener::run_forwarder(
            incoming_messages,
            data_storage,
            config.app.max_buffer_size,
        ));

        IrcListener { irc_client: client }
    }

    pub async fn join_channels_and_start_part_task(
        &self,
        config: &'static Config,
        data_storage: &'static DataStorage,
    ) -> Result<(), StorageError> {
        let (initial_channels, timestamp) = data_storage
            .get_channel_logins_to_join(config.app.channels_expire_after)
            .await?;

        log::info!("Startup: Joining {} channel(s)", initial_channels.len());
        for channel in initial_channels {
            self.join_if_needed(channel);
        }

        let self_clone = self.clone();
        tokio::spawn(self_clone.start_channel_part_loop(config, data_storage, timestamp));

        Ok(())
    }

    async fn run_forwarder(
        mut incoming_messages: mpsc::UnboundedReceiver<ServerMessage>,
        data_storage: &'static DataStorage,
        max_buffer_size: usize,
    ) {
        while let Some(message) = incoming_messages.next().await {
            let message_storage = data_storage.clone();
            tokio::spawn(async move {
                if let Some(channel_login) = message.channel_login() {
                    let message_source = message.source().as_raw_irc();
                    message_storage
                        .append_message(channel_login.to_owned(), message_source, max_buffer_size)
                        .await;
                }
            });
        }
        unreachable!("stream should never end");
    }

    /// Start background loop to vacuum/part channels that are not used.
    async fn start_channel_part_loop(
        self,
        config: &'static Config,
        data_storage: &'static DataStorage,
        initial_query_timestamp: IterationTimestamp,
    ) {
        let mut last_iteration_timestamp = initial_query_timestamp;
        let mut check_interval = tokio::time::interval(config.app.vacuum_channels_every);

        // this task starts right after the initial join. we dont need to check for channels to
        // part right immediately after checking for channels to join. so one tick is awaited
        // before the loop
        check_interval.tick().await;
        loop {
            check_interval.tick().await;

            let res = data_storage
                .get_channel_logins_to_part(
                    last_iteration_timestamp,
                    config.app.channels_expire_after,
                )
                .await;
            let channels_to_part = match res {
                Ok((channels_to_part, this_iteration_timestamp)) => {
                    last_iteration_timestamp = this_iteration_timestamp;
                    channels_to_part
                }
                Err(e) => {
                    log::error!("Failed to query the DB for a list of channels to part. This iteration will be skipped. Cause: {}", e);
                    continue;
                }
            };

            log::info!(
                "Parting {} channel(s) that have expired",
                channels_to_part.len()
            );
            for channel in channels_to_part {
                self.irc_client.part(channel);
            }
        }
    }

    pub fn join_if_needed(&self, channel_login: String) {
        // the twitch_irc crate only does a JOIN if necessary
        self.irc_client.join(channel_login);
    }

    pub async fn is_join_confirmed(&self, channel_login: String) -> bool {
        self.irc_client.get_channel_status(channel_login).await == (true, true)
    }
}

trait ServerMessageExt {
    fn channel_login(&self) -> Option<&str>;
}

impl ServerMessageExt for ServerMessage {
    /// Get the channel login if this message was sent to a channel.
    fn channel_login(&self) -> Option<&str> {
        match self {
            ServerMessage::ClearChat(m) => Some(&m.channel_login),
            ServerMessage::ClearMsg(m) => Some(&m.channel_login),
            ServerMessage::HostTarget(m) => Some(&m.channel_login),
            ServerMessage::Join(m) => Some(&m.channel_login),
            ServerMessage::Notice(m) => m.channel_login.as_ref().map(|s| s.as_str()),
            ServerMessage::Part(m) => Some(&m.channel_login),
            ServerMessage::Privmsg(m) => Some(&m.channel_login),
            ServerMessage::RoomState(m) => Some(&m.channel_login),
            ServerMessage::UserNotice(m) => Some(&m.channel_login),
            ServerMessage::UserState(m) => Some(&m.channel_login),
            _ => None,
        }
    }
}
