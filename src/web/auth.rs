use crate::config::TwitchApiClientCredentials;
use crate::web::ApiError;
use chrono::{DateTime, Utc};
use futures::prelude::*;
use http::StatusCode;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

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
pub struct UserAuthorizationResponse {
    pub access_token: String,
    pub valid_until: DateTime<Utc>,
    pub user_id: String,
    pub user_login: String,
    pub user_name: String,
    pub user_profile_image_url: String,
    pub user_details_valid_until: DateTime<Utc>,
}

impl UserAuthorizationResponse {
    pub(crate) fn from_auth(
        auth: &UserAuthorization,
        user_details_valid_for: Duration,
    ) -> UserAuthorizationResponse {
        UserAuthorizationResponse {
            access_token: auth.access_token.clone(),
            valid_until: auth.valid_until,
            user_id: auth.user_id.clone(),
            user_login: auth.user_login.clone(),
            user_name: auth.user_name.clone(),
            user_profile_image_url: auth.user_profile_image_url.clone(),
            user_details_valid_until: auth.twitch_authorization_last_validated
                + chrono::Duration::from_std(user_details_valid_for).unwrap(),
        }
    }
}

#[derive(Deserialize)]
pub struct HelixGetUserResponse {
    // we expect a list of size 1
    pub data: (HelixUser,),
}

#[derive(Deserialize)]
pub struct HelixUser {
    pub id: String,
    pub login: String,
    pub display_name: String,
    pub profile_image_url: String,
}

lazy_static! {
    static ref HTTP_CLIENT: reqwest::Client = reqwest::Client::new();
}

#[derive(Deserialize)]
pub struct GetAuthorizationQueryOptions {
    pub code: String,
}

impl UserAuthorization {
    /// Try to refresh the access token
    async fn refresh_token(
        &mut self,
        credentials: &TwitchApiClientCredentials,
    ) -> Result<(), ApiError> {
        tracing::info!("Refreshing access token for user {}", self.user_login);
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
            tracing::debug!("Executing auth validation for user {}: Querying Helix API for user", self.user_login);
            // query helix for the user. success => token still valid, error => token expired/revoked
            // the async {}.await acts like a try{} block (but try blocks are not in stable rust yet)
            let user_api_response_result = async {
                Ok(HTTP_CLIENT
                    .get("https://api.twitch.tv/helix/users")
                    .header("Client-ID", &credentials.client_id)
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
                    tracing::debug!("Executing auth validation for user {}: Success, connection still active", self.user_login);
                    self.twitch_authorization_last_validated = Utc::now();
                    self.user_id = response.id;
                    self.user_login = response.login;
                    self.user_name = response.display_name;
                    Ok(())
                }
                Err(ApiError::Unauthorized) if try_refresh_if_invalid => {
                    tracing::debug!("Executing auth validation for user {}: Failure! Unauthorized. Trying refresh", self.user_login);
                    self.refresh_token(credentials).boxed().await?;
                    // recurse: try the above again, now that the token is successfully refreshed.
                    self.validate_still_valid_inner(credentials, recheck_twitch_auth_after, false)
                        .await
                }
                Err(e) => {
                    tracing::debug!("Executing auth validation for user {}: Other error: {}", self.user_login, e);
                    Err(e)
                }
            }
        }
            .boxed()
    }

    pub(crate) async fn validate_still_valid(
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
            tracing::debug!(
                "Auth validation for user {} skipped (validated recently)",
                self.user_login
            );
            return Ok(());
        }

        self.validate_still_valid_inner(credentials, recheck_twitch_auth_after, true)
            .await
    }
}
