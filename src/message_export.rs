use crate::db::StoredMessage;
use crate::web::get_recent_messages::GetRecentMessagesQueryOptions;
use chrono::{DateTime, Utc};
use humantime::format_duration;
use itertools::Itertools;
use lazy_static::lazy_static;
use std::collections::HashSet;
use std::convert::TryFrom;
use twitch_irc::message::{
    AsRawIRC, ClearChatAction, ClearMsgMessage, IRCMessage, IRCPrefix, IRCTags, NoticeMessage,
    ServerMessage,
};

#[derive(Debug)]
struct ContainerFrame {
    /// The original message that was received from IRC.
    original_message: ServerMessage,

    /// Time when the recent-messages service received this message. Gets converted
    /// to `rm-received-ts` on export
    time_received: DateTime<Utc>,

    /// Whether this message is marked "deleted" due to a `CLEARCHAT` or `CLEARMSG` message.
    /// Gets converted to `rm-deleted=1` on export.
    deleted_by_moderation: bool,
}

impl ContainerFrame {
    fn export(self, options: &GetRecentMessagesQueryOptions) -> Option<String> {
        if options.hide_moderated_messages && self.deleted_by_moderation {
            return None;
        }

        if options.hide_moderation_messages
            && matches!(
                self.original_message,
                ServerMessage::ClearChat(_) | ServerMessage::ClearMsg(_)
            )
        {
            return None;
        }

        let mut message_to_export = if options.clearchat_to_notice {
            if let ServerMessage::ClearChat(clearchat_msg) = self.original_message {
                let (message, extra_tag) = match clearchat_msg.action {
                    ClearChatAction::ChatCleared => (
                        "Chat has been cleared by a moderator.".to_owned(),
                        "rm-clearchat".to_owned(),
                    ),
                    ClearChatAction::UserTimedOut {
                        user_login,
                        timeout_length,
                        ..
                    } => (
                        format!(
                            "{} has been timed out for {}.",
                            user_login,
                            format_duration(timeout_length)
                        ),
                        "rm-timeout".to_owned(),
                    ),
                    ClearChatAction::UserBanned { user_login, .. } => (
                        format!("{} has been permanently banned.", user_login),
                        "rm-permaban".to_owned(),
                    ),
                };

                let mut tags = IRCTags::new();
                // @msg-id=rm-clearchat/rm-timeout/rm-permaban
                tags.0.insert("msg-id".to_owned(), Some(extra_tag));

                // @msg-id=rm-timeout :tmi.twitch.tv NOTICE #channel :a_bad_user has been timed out for 5m 2s.
                IRCMessage::new(
                    tags,
                    Some(IRCPrefix::HostOnly {
                        host: "tmi.twitch.tv".to_owned(),
                    }),
                    "NOTICE".to_owned(),
                    vec![format!("#{}", clearchat_msg.channel_login), message],
                )
            } else {
                IRCMessage::from(self.original_message)
            }
        } else {
            IRCMessage::from(self.original_message)
        };

        // Add historical=1
        message_to_export
            .tags
            .0
            .insert("historical".to_owned(), Some("1".to_owned()));
        // Add rm-received-ts=<timestamp>
        message_to_export.tags.0.insert(
            "rm-received-ts".to_owned(),
            Some(self.time_received.timestamp_millis().to_string()),
        );

        // Add rm-deleted=1 if needed
        if self.deleted_by_moderation {
            message_to_export
                .tags
                .0
                .insert("rm-deleted".to_owned(), Some("1".to_owned()));
        }

        Some(message_to_export.as_raw_irc())
    }
}

#[derive(Debug)]
struct MessageContainer {
    options: GetRecentMessagesQueryOptions,
    frames: Vec<ContainerFrame>,
}

lazy_static! {
    static ref IGNORED_NOTICE_IDS: HashSet<&'static str> = [
        "no_permission",
        "host_on",
        "host_off",
        "host_target_went_offline",
        "msg_channel_suspended"
    ]
    .iter()
    .cloned()
    .collect();
}

impl MessageContainer {
    pub fn append_stored_msg(&mut self, message: &StoredMessage) {
        // parse the retrieved source back into a struct
        let server_message =
            ServerMessage::try_from(IRCMessage::parse(&message.message_source).unwrap()).unwrap();

        // we export PRIVMSG, CLEARCHAT, CLEARMSG, USERNOTICE, NOTICE and ROOMSTATE
        if !matches!(
            server_message,
            ServerMessage::Privmsg(_)
                | ServerMessage::ClearChat(_)
                | ServerMessage::ClearMsg(_)
                | ServerMessage::UserNotice(_)
                | ServerMessage::Notice(_)
                | ServerMessage::RoomState(_)
        ) {
            return;
        }

        // apply `deleted_by_moderation` flag
        match &server_message {
            ServerMessage::ClearChat(clearchat_msg) => match &clearchat_msg.action {
                ClearChatAction::ChatCleared => {
                    self.frames
                        .iter_mut()
                        .for_each(|frame| frame.deleted_by_moderation = true);
                }
                ClearChatAction::UserTimedOut { user_id, .. }
                | ClearChatAction::UserBanned { user_id, .. } => {
                    self.frames
                        .iter_mut()
                        .filter(|frame| match &frame.original_message {
                            ServerMessage::Privmsg(msg) => &msg.sender.id == user_id,
                            ServerMessage::UserNotice(msg) => &msg.sender.id == user_id,
                            _ => false,
                        })
                        .for_each(|frame| frame.deleted_by_moderation = true);
                }
            },
            ServerMessage::ClearMsg(ClearMsgMessage { message_id, .. }) => {
                self.frames
                    .iter_mut()
                    .filter(|frame| match &frame.original_message {
                        ServerMessage::Privmsg(msg) => &msg.message_id == message_id,
                        ServerMessage::UserNotice(msg) => &msg.message_id == message_id,
                        _ => false,
                    })
                    .for_each(|frame| frame.deleted_by_moderation = true);
            }
            ServerMessage::Notice(NoticeMessage {
                message_id: Some(message_id),
                ..
            }) => {
                // Don't export ignored NOTICE types
                if IGNORED_NOTICE_IDS.contains(&message_id.as_str()) {
                    return;
                }
            }
            _ => {}
        }

        // rest of the options are handled during the `export()` call

        let frame = ContainerFrame {
            original_message: server_message,
            time_received: message.time_received,
            deleted_by_moderation: false,
        };
        self.frames.push(frame);
    }

    pub fn export(self) -> Vec<String> {
        let MessageContainer { frames, options } = self;
        frames
            .into_iter()
            .filter_map(|frame| frame.export(&options))
            .collect_vec()
    }
}

/// Processes the stored message and applies the options specified by `options`.
pub fn export_stored_messages(
    stored_messages: Vec<StoredMessage>,
    options: GetRecentMessagesQueryOptions,
) -> Vec<String> {
    let mut container = MessageContainer {
        options,
        frames: vec![],
    };

    for stored_message in stored_messages {
        container.append_stored_msg(&stored_message);
    }

    container.export()
}
