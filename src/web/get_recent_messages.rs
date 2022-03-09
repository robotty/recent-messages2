use crate::db::DataStorage;
use crate::irc_listener::IrcListener;
use crate::web::ApiError;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use warp::reject::Rejection;

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct GetRecentMessagesQueryOptions {
    // aliases are used to keep compatibility with the API from version 1.
    #[serde(alias = "hideModerationMessages")]
    pub hide_moderation_messages: bool,
    #[serde(alias = "hideModeratedMessages")]
    pub hide_moderated_messages: bool,
    #[serde(alias = "clearchatToNotice")]
    pub clearchat_to_notice: bool,
    pub limit: Option<usize>,
}

impl Default for GetRecentMessagesQueryOptions {
    fn default() -> Self {
        GetRecentMessagesQueryOptions {
            hide_moderation_messages: false,
            hide_moderated_messages: false,
            clearchat_to_notice: false,
            limit: None,
        }
    }
}

// this is async so we can directly plug it into a warp::Filter::and_then
pub async fn validate_channel_login(channel_login: String) -> Result<String, Rejection> {
    match twitch_irc::validate::validate_login(&channel_login) {
        Ok(()) => Ok(channel_login),
        Err(e) => Err(warp::reject::custom(ApiError::InvalidChannelLogin(e))),
    }
}

#[derive(Debug, Serialize)]
struct GetRecentMessagesResponse {
    messages: Vec<String>,
    error: Option<&'static str>,
    error_code: Option<&'static str>,
}

// GET /api/v2/recent-messages/:channel?clearchat_to_notice=bool&hide_moderation_messages=bool&hide_moderated_messages=bool&limit=usize
pub async fn get_recent_messages(
    channel_login: String,
    options: GetRecentMessagesQueryOptions,
    data_storage: &'static DataStorage,
    irc_listener: &'static IrcListener,
) -> Result<impl warp::Reply, Rejection> {
    if data_storage
        .is_channel_ignored(&channel_login)
        .await
        .map_err(ApiError::GetChannelIgnored)?
    {
        return Err(warp::reject::custom(ApiError::ChannelIgnored(
            channel_login,
        )));
    }

    let stored_messages = data_storage
        .get_messages(&channel_login, options.limit)
        .await;

    let exported_messages = crate::message_export::export_stored_messages(stored_messages, options);

    let mut is_confirmed_joined = irc_listener.is_join_confirmed(channel_login.clone()).await;

    tokio::spawn(async move {
        irc_listener.join_if_needed(channel_login.clone());

        if !is_confirmed_joined {
            // wait 5 seconds then check again
            tokio::time::sleep(Duration::from_secs(5)).await;
            is_confirmed_joined = irc_listener.is_join_confirmed(channel_login.clone()).await;
        }

        // if we managed to join the channel then add/touch it in the database
        if is_confirmed_joined {
            log::trace!("Adding/touching channel: {}", channel_login);
            let res = data_storage.touch_or_add_channel(&channel_login).await;
            if let Err(e) = res {
                log::error!("Failed to touch_or_add_channel: {}", e);
            }
        }
    });

    let (error, error_code) = if is_confirmed_joined {
        (None, None)
    } else {
        (Some("The bot is currently not joined to this channel (in progress or failed previously)"), Some("channel_not_joined"))
    };

    Ok(warp::reply::json(&GetRecentMessagesResponse {
        messages: exported_messages,
        error,
        error_code,
    }))
}
