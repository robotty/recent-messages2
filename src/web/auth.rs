use crate::config::TwitchApiClientCredentials;
use crate::db::DataStorage;
use crate::web::ApiError;
use chrono::{DateTime, Utc};
use futures::prelude::*;
use http::StatusCode;
use lazy_static::lazy_static;
use rand::distributions::Standard;
use rand::Rng;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fmt::Write;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use warp::Filter;
use warp::Rejection;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TwitchUserAccessToken {
    pub access_token: String,
    pub refresh_token: String,
    // we're not interested in the rest of the fields, so they are omitted
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserAuthorization {
    /// unique, random string identifying this access token.
    pub access_token: String,
    pub twitch_token: TwitchUserAccessToken,
    /// last time the twitch authorization was validated to be still active
    pub twitch_authorization_last_validated: DateTime<Utc>,
    /// this authorization is valid until this date, regardless of the validity date of the Twitch
    /// integration, the token has to be extended before this expiry date, otherwise it will be invalidated
    /// forever.
    ///
    /// The authorization typically can live for a long time after the twitch validation expires
    /// (the twitch authorization validation expires 1 hour after twitch_authorization_last_validated)
    pub valid_until: DateTime<Utc>,
    pub user_id: String,
    pub user_login: String,
    pub user_name: String,
    pub user_profile_image_url: String,
}

#[derive(Serialize)]
struct UserAuthorizationResponse<'a> {
    access_token: &'a str,
    valid_until: DateTime<Utc>,
    user_id: &'a str,
    user_login: &'a str,
    user_name: &'a str,
    user_profile_image_url: &'a str,
    user_details_valid_until: DateTime<Utc>,
}

impl<'a> UserAuthorizationResponse<'a> {
    fn from_auth(
        auth: &'a UserAuthorization,
        user_details_valid_for: Duration,
    ) -> UserAuthorizationResponse<'a> {
        UserAuthorizationResponse {
            access_token: &auth.access_token,
            valid_until: auth.valid_until,
            user_id: &auth.user_id,
            user_login: &auth.user_login,
            user_name: &auth.user_name,
            user_profile_image_url: &auth.user_profile_image_url,
            user_details_valid_until: auth.twitch_authorization_last_validated
                + chrono::Duration::from_std(user_details_valid_for).unwrap(),
        }
    }
}

#[derive(Deserialize)]
struct HelixGetUserResponse {
    // we expect a list of size 1
    data: (HelixUser,),
}

#[derive(Deserialize)]
struct HelixUser {
    id: String,
    login: String,
    display_name: String,
    profile_image_url: String,
}

lazy_static! {
    static ref HTTP_CLIENT: reqwest::Client = reqwest::Client::new();
}

#[derive(Deserialize)]
pub struct GetAuthorizationQueryOptions {
    pub code: String,
}

// POST /api/v2/auth/create?code=abcdef123456
pub async fn create_token(
    data_storage: &'static DataStorage,
    credentials: TwitchApiClientCredentials,
    sessions_expire_after: Duration,
    recheck_twitch_auth_after: Duration,
    code: String,
) -> Result<impl warp::Reply, Rejection> {
    let user_access_token = HTTP_CLIENT
        .post("https://id.twitch.tv/oauth2/token")
        .query(&[
            ("client_id", credentials.client_id.clone()),
            ("client_secret", credentials.client_secret),
            ("redirect_uri", credentials.redirect_uri),
            ("code", code),
            ("grant_type", "authorization_code".to_owned()),
        ])
        .send()
        .await
        .map_err(ApiError::ExchangeCodeForAccessToken)?
        .error_for_status()
        .map_err(|e| {
            if e.status().unwrap() == StatusCode::BAD_REQUEST {
                ApiError::InvalidAuthorizationCode
            } else {
                ApiError::ExchangeCodeForAccessToken(e)
            }
        })?
        .json::<TwitchUserAccessToken>()
        .await
        .map_err(ApiError::ExchangeCodeForAccessToken)?;

    let user_api_response = HTTP_CLIENT
        .get("https://api.twitch.tv/helix/users")
        .header("Client-ID", credentials.client_id)
        .header(
            "Authorization",
            format!("Bearer {}", user_access_token.access_token),
        )
        .send()
        .await
        .map_err(ApiError::QueryUserDetails)?
        .error_for_status()
        .map_err(ApiError::QueryUserDetails)?
        .json::<HelixGetUserResponse>()
        .await
        .map_err(ApiError::QueryUserDetails)?
        .data
        .0;

    // 512 bit random hex string
    // thread_rng() is cryptographically safe
    let access_token = rand::thread_rng().sample_iter(Standard).take(512 / 8).fold(
        String::with_capacity(512 / 4),
        |mut s, x: u8| {
            // format as hex, padded with a leading 0 if needed (e.g. 0x0 -> "00", 0xFF -> "ff")
            write!(&mut s, "{:02x}", x).unwrap();
            s
        },
    );

    let now = Utc::now();
    let user_authorization = UserAuthorization {
        access_token,
        twitch_token: user_access_token,
        twitch_authorization_last_validated: now,
        valid_until: now + chrono::Duration::from_std(sessions_expire_after).unwrap(),
        user_id: user_api_response.id,
        user_login: user_api_response.login,
        user_name: user_api_response.display_name,
        user_profile_image_url: user_api_response.profile_image_url,
    };

    data_storage
        .append_user_authorization(&user_authorization)
        .await
        .map_err(ApiError::SaveUserAuthorization)?;

    log::debug!(
        "User {} ({}, {}) authorized successfully",
        user_authorization.user_name,
        user_authorization.user_login,
        user_authorization.user_id
    );

    Ok(warp::reply::json(&UserAuthorizationResponse::from_auth(
        &user_authorization,
        recheck_twitch_auth_after,
    )))
}

impl UserAuthorization {
    /// Try to refresh the access token
    async fn refresh_token(
        &mut self,
        credentials: &TwitchApiClientCredentials,
    ) -> Result<(), ApiError> {
        log::info!("Refreshing access token for user {}", self.user_login);
        let new_access_token = HTTP_CLIENT
            .post("https://id.twitch.tv/oauth2/token")
            .query(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", &self.twitch_token.refresh_token),
                ("client_id", &credentials.client_id),
                ("client_secret", &credentials.client_secret),
            ])
            .send()
            .await
            .map_err(ApiError::FailedTwitchAccessTokenRefresh)?
            .error_for_status()
            .map_err(|e| {
                if e.status().unwrap() == StatusCode::BAD_REQUEST {
                    // user has definitely revoked the connection
                    ApiError::Unauthorized
                } else {
                    ApiError::FailedTwitchAccessTokenRefresh(e)
                }
            })?
            .json::<TwitchUserAccessToken>()
            .await
            .map_err(ApiError::FailedTwitchAccessTokenRefresh)?;

        self.twitch_token = new_access_token;

        Ok(())
    }

    /// Ensure that the Twitch authorization grant has not been revoked by the user.
    ///
    /// `try_refresh_if_invalid` is the flag whether to recurse if the initial query for the
    /// user details fails due to a bad token. If the query fails, then the token is refreshed
    /// and this method calls itself again, only this time with `try_refresh_if_invalid=false`.
    ///
    /// (`try_refresh_if_invalid` should be `true` when called from outside)
    fn validate_still_valid_inner<'a>(
        &'a mut self,
        credentials: &'a TwitchApiClientCredentials,
        recheck_twitch_auth_after: Duration,
        try_refresh_if_invalid: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), ApiError>> + Send + 'a>> {
        // the boxed future is necessary because of the recursive call
        async move {
            log::debug!("Executing auth validation for user {}: Querying Helix API for user", self.user_login);
            // query helix for the user. success => token still valid, error => token expired/revoked
            // the async {}.await acts like a try{} block (but try blocks are not in stable rust yet)
            let user_api_response_result = async {
                Ok(HTTP_CLIENT
                    .get("https://api.twitch.tv/helix/users")
                    .header("Client-ID", credentials.client_id.clone())
                    .header(
                        "Authorization",
                        format!("Bearer {}", self.twitch_token.access_token),
                    )
                    .send()
                    .await
                    .map_err(ApiError::QueryUserDetails)?
                    .error_for_status()
                    .map_err(|e| {
                        if e.status().unwrap() == StatusCode::UNAUTHORIZED {
                            // token has expired or user has revoked authorization
                            ApiError::Unauthorized
                        } else {
                            ApiError::FailedTwitchAccessTokenRefresh(e)
                        }
                    })?
                    .json::<HelixGetUserResponse>()
                    .await
                    .map_err(ApiError::QueryUserDetails)?
                    .data
                    .0)
            }
                .await;

            match user_api_response_result {
                Ok(response) => {
                    log::debug!("Executing auth validation for user {}: Success, connection still active", self.user_login);
                    self.twitch_authorization_last_validated = Utc::now();
                    self.user_id = response.id;
                    self.user_login = response.login;
                    self.user_name = response.display_name;
                    Ok(())
                }
                Err(ApiError::Unauthorized) if try_refresh_if_invalid => {
                    log::debug!("Executing auth validation for user {}: Failure! Unauthorized. Trying refresh", self.user_login);
                    self.refresh_token(credentials).boxed().await?;
                    // recurse: try the above again, now that the token is successfully refreshed.
                    self.validate_still_valid_inner(credentials, recheck_twitch_auth_after, false)
                        .await
                }
                Err(e) => {
                    log::debug!("Executing auth validation for user {}: Other error: {}", self.user_login, e);
                    Err(e)
                }
            }
        }
            .boxed()
    }

    async fn validate_still_valid(
        &mut self,
        credentials: &TwitchApiClientCredentials,
        recheck_twitch_auth_after: Duration,
    ) -> Result<(), ApiError> {
        if (Utc::now() - self.twitch_authorization_last_validated)
            .to_std()
            .unwrap()
            <= recheck_twitch_auth_after
        {
            // skip the check, last validation less than `recheck_twitch_auth_after` ago
            log::debug!(
                "Auth validation for user {} skipped (validated recently)",
                self.user_login
            );
            return Ok(());
        }

        self.validate_still_valid_inner(credentials, recheck_twitch_auth_after, true)
            .await
    }
}

pub fn with_authorization(
    data_storage: &'static DataStorage,
    credentials: TwitchApiClientCredentials,
    recheck_twitch_auth_after: Duration,
) -> impl warp::Filter<Extract = (UserAuthorization,), Error = warp::Rejection> + Clone {
    lazy_static! {
        static ref RE_AUTHORIZATION_HEADER: Regex = Regex::new("^Bearer ([0-9a-f]{128})$").unwrap();
    }

    warp::filters::header::header::<String>("Authorization").and_then(
        move |authorization_header: String| {
            let data_storage = data_storage.clone();
            let credentials = credentials.clone();

            async move {
                let access_token = RE_AUTHORIZATION_HEADER
                    .captures(&authorization_header)
                    .ok_or_else(|| ApiError::MalformedAuthorizationHeader)?
                    .get(1)
                    .unwrap()
                    .as_str();

                // data storage query ensures token is not totally expired
                let mut authorization = data_storage
                    .get_user_authorization(access_token)
                    .await
                    .map_err(ApiError::QueryAccessToken)?
                    .ok_or_else(|| ApiError::Unauthorized)?;

                // and then this ensures that the user has not revoked the connection from the Twitch side
                let pre_validation_auth = authorization.clone();
                authorization
                    .validate_still_valid(&credentials, recheck_twitch_auth_after)
                    .await?;

                if pre_validation_auth != authorization {
                    data_storage
                        .update_user_authorization(&authorization)
                        .await
                        .map_err(ApiError::UpdateUserAuthorization)?;
                }

                Ok::<UserAuthorization, warp::Rejection>(authorization)
            }
        },
    )
}

// POST /api/v2/auth/extend
pub async fn extend_token(
    mut authorization: UserAuthorization,
    data_storage: &'static DataStorage,
    sessions_expire_after: Duration,
    recheck_twitch_auth_after: Duration,
) -> Result<impl warp::Reply, Rejection> {
    let new_expiry = Utc::now() + chrono::Duration::from_std(sessions_expire_after).unwrap();
    authorization.valid_until = new_expiry;

    data_storage
        .update_user_authorization(&authorization)
        .await
        .map_err(ApiError::UpdateUserAuthorization)?;

    Ok(warp::reply::json(&UserAuthorizationResponse::from_auth(
        &authorization,
        recheck_twitch_auth_after,
    )))
}

// POST /api/v2/auth/revoke
pub async fn revoke_token(
    authorization: UserAuthorization,
    data_storage: &'static DataStorage,
) -> Result<impl warp::Reply, Rejection> {
    data_storage
        .delete_user_authorization(&authorization.access_token)
        .await
        .map_err(ApiError::AuthorizationRevokeFailed)?;
    // 200 OK with empty body
    Ok(warp::reply())
}
