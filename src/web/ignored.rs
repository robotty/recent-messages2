use crate::db::DataStorage;
use crate::irc_listener::IrcListener;
use crate::web::auth::UserAuthorization;
use crate::web::ApiError;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use warp::Rejection;

#[derive(Serialize)]
struct GetIgnoredResponse {
    ignored: bool,
}

pub async fn get_ignored(
    authorization: UserAuthorization,
    data_storage: &'static DataStorage,
) -> Result<impl warp::Reply, Rejection> {
    let is_ignored = data_storage
        .is_channel_ignored(&authorization.user_login)
        .await
        .map_err(ApiError::GetChannelIgnored)?;

    Ok(warp::reply::json(&GetIgnoredResponse {
        ignored: is_ignored,
    }))
}

#[derive(Deserialize)]
pub struct SetIgnoredBodyOptions {
    ignored: bool,
}

pub async fn set_ignored(
    authorization: UserAuthorization,
    data_storage: &'static DataStorage,
    irc_listener: &'static IrcListener,
    options: SetIgnoredBodyOptions,
) -> Result<impl warp::Reply, Rejection> {
    data_storage
        .set_channel_ignored(&authorization.user_login, options.ignored)
        .await
        .map_err(ApiError::SetChannelIgnored)?;

    if options.ignored {
        // TODO: There can be messages getting added to the message store between the purge
        // and the time that the PART command reaches the Twitch server. The 3 second time delay
        // "solution" is a hack, needs a better solution
        // maybe put a "blocker"/poison type into the db storage
        // (enum ChannelMessages { Ignored, Normal(VecDeque<StoredMessage> } or so)
        irc_listener
            .irc_client
            .part(authorization.user_login.clone());

        data_storage.purge_messages(&authorization.user_login).await;
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(3)).await;
            data_storage.purge_messages(&authorization.user_login).await;
        });
    } else {
        irc_listener.irc_client.join(authorization.user_login).unwrap();
    }

    // 200 OK with empty body
    Ok(warp::reply())
}
