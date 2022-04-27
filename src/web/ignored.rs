use crate::web::auth::UserAuthorization;
use crate::web::{ApiError, WebAppData};
use axum::extract::rejection::JsonRejection;
use axum::{Extension, Json};
use http::StatusCode;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Serialize)]
pub struct GetIgnoredResponse {
    ignored: bool,
}

pub async fn get_ignored(
    Extension(authorization): Extension<UserAuthorization>,
    Extension(app_data): Extension<WebAppData>,
) -> Result<Json<GetIgnoredResponse>, ApiError> {
    let is_ignored = app_data
        .data_storage
        .is_channel_ignored(&authorization.user_login)
        .await
        .map_err(ApiError::GetChannelIgnored)?;

    Ok(Json(GetIgnoredResponse {
        ignored: is_ignored,
    }))
}

#[derive(Deserialize)]
pub struct SetIgnoredBodyOptions {
    ignored: bool,
}

pub async fn set_ignored(
    Extension(authorization): Extension<UserAuthorization>,
    Extension(app_data): Extension<WebAppData>,
    options: Result<Json<SetIgnoredBodyOptions>, JsonRejection>,
) -> Result<StatusCode, ApiError> {
    let Json(SetIgnoredBodyOptions {
        ignored: should_be_ignored,
    }) = options.map_err(|_| ApiError::InvalidPayload)?;

    app_data
        .data_storage
        .set_channel_ignored(&authorization.user_login, should_be_ignored)
        .await
        .map_err(ApiError::SetChannelIgnored)?;

    if should_be_ignored {
        // TODO: There can be messages getting added to the message store between the purge
        // and the time that the PART command reaches the Twitch server. The 3 second time delay
        // "solution" is a hack, needs a better solution
        // maybe put a "blocker"/poison type into the db storage
        // (enum ChannelMessages { Ignored, Normal(VecDeque<StoredMessage> } or so)
        app_data
            .irc_listener
            .irc_client
            .part(authorization.user_login.clone());

        app_data
            .data_storage
            .purge_messages(&authorization.user_login)
            .await;
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(3)).await;
            app_data
                .data_storage
                .purge_messages(&authorization.user_login)
                .await;
        });
    } else {
        app_data
            .irc_listener
            .irc_client
            .join(authorization.user_login)
            .unwrap();
    }

    // 204 No Content, empty body
    Ok(StatusCode::NO_CONTENT)
}
