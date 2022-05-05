use crate::web::error::ApiError;
use crate::web::WebAppData;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path, Query};
use axum::response::IntoResponse;
use axum::{Extension, Json};
use lazy_static::lazy_static;
use prometheus::{register_histogram_vec, HistogramVec};
use serde::{Deserialize, Serialize};
use std::time::Duration;

lazy_static! {
    static ref GET_RM2_AWAITS: HistogramVec = register_histogram_vec!(
        "recentmessages_get_recent_messages_endpoint_async_components_seconds",
        "Time taken to complete the different async stages of the /api/v2/recent-messages/:channel_login endpoint",
        &["stage"]
    )
    .unwrap();
}

#[derive(Debug, Clone, Deserialize)]
pub struct GetRecentMessagesPath {
    channel_login: String,
}

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

#[derive(Debug, Serialize)]
struct GetRecentMessagesResponse {
    messages: Vec<String>,
    error: Option<&'static str>,
    error_code: Option<&'static str>,
}

pub async fn get_recent_messages(
    path_options: Result<Path<GetRecentMessagesPath>, PathRejection>,
    query_options: Result<Query<GetRecentMessagesQueryOptions>, QueryRejection>,
    Extension(app_data): Extension<WebAppData>,
) -> impl IntoResponse {
    let Path(GetRecentMessagesPath { channel_login }) =
        path_options.map_err(|_| ApiError::InvalidPath)?;
    let Query(query_options) = query_options.map_err(|_| ApiError::InvalidQuery)?;

    if let Err(e) = twitch_irc::validate::validate_login(&channel_login) {
        return Err(ApiError::InvalidChannelLogin(e));
    }

    let timer = GET_RM2_AWAITS
        .with_label_values(&["is_channel_ignored"])
        .start_timer();
    let result = app_data
        .data_storage
        .is_channel_ignored(&channel_login)
        .await;
    timer.observe_duration();
    if result.map_err(ApiError::GetChannelIgnored)? {
        return Err(ApiError::ChannelIgnored(channel_login));
    }

    let timer = GET_RM2_AWAITS
        .with_label_values(&["get_messages"])
        .start_timer();
    let result = app_data
        .data_storage
        .get_messages(
            &channel_login,
            query_options.limit,
            app_data.config.app.max_buffer_size,
        )
        .await;
    timer.observe_duration();
    let stored_messages = result.map_err(ApiError::GetMessages)?;

    let exported_messages =
        crate::message_export::export_stored_messages(stored_messages, query_options);

    let timer = GET_RM2_AWAITS
        .with_label_values(&["is_join_confirmed"])
        .start_timer();
    let mut is_confirmed_joined = app_data
        .irc_listener
        .is_join_confirmed(channel_login.clone())
        .await;
    timer.observe_duration();

    tokio::spawn(async move {
        app_data.irc_listener.join_if_needed(channel_login.clone());

        if !is_confirmed_joined {
            // wait 5 seconds then check again
            tokio::time::sleep(Duration::from_secs(5)).await;
            is_confirmed_joined = app_data
                .irc_listener
                .is_join_confirmed(channel_login.clone())
                .await;
        }

        // if we managed to join the channel then add/touch it in the database
        if is_confirmed_joined {
            tracing::trace!("Adding/touching channel: {}", channel_login);
            let res = app_data
                .data_storage
                .touch_or_add_channel(&channel_login)
                .await;
            if let Err(e) = res {
                tracing::error!("Failed to touch_or_add_channel: {}", e);
            }
        }
    });

    let (error, error_code) = if is_confirmed_joined {
        (None, None)
    } else {
        (Some("The bot is currently not joined to this channel (in progress or failed previously)"), Some("channel_not_joined"))
    };

    Ok(Json(GetRecentMessagesResponse {
        messages: exported_messages,
        error,
        error_code,
    }))
}
