use crate::web::auth::{
    HelixGetUserResponse, TwitchUserAccessToken, UserAuthorization, UserAuthorizationResponse,
};
use crate::web::error::ApiError;
use crate::web::WebAppData;
use axum::extract::rejection::QueryRejection;
use axum::extract::Query;
use axum::{Extension, Json};
use chrono::Utc;
use http::StatusCode;
use rand::distributions::Standard;
use rand::Rng;
use serde::Deserialize;
use std::fmt::Write;

#[derive(Debug, Clone, Deserialize)]
pub struct CreateAuthTokenQueryOptions {
    code: String,
}

// POST /api/v2/auth/create?code=abcdef123456
pub async fn create_token(
    Extension(app_data): Extension<WebAppData>,
    query_options: Result<Query<CreateAuthTokenQueryOptions>, QueryRejection>,
) -> Result<Json<UserAuthorizationResponse>, ApiError> {
    let Query(CreateAuthTokenQueryOptions { code }) =
        query_options.map_err(|_| ApiError::InvalidQuery)?;

    let user_access_token = crate::web::HTTP_CLIENT
        .post("https://id.twitch.tv/oauth2/token")
        .query(&[
            (
                "client_id",
                app_data
                    .config
                    .web
                    .twitch_api_credentials
                    .client_id
                    .as_str(),
            ),
            (
                "client_secret",
                app_data
                    .config
                    .web
                    .twitch_api_credentials
                    .client_secret
                    .as_str(),
            ),
            (
                "redirect_uri",
                app_data
                    .config
                    .web
                    .twitch_api_credentials
                    .redirect_uri
                    .as_str(),
            ),
            ("code", code.as_str()),
            ("grant_type", "authorization_code"),
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

    let user_api_response = crate::web::HTTP_CLIENT
        .get("https://api.twitch.tv/helix/users")
        .header(
            "Client-ID",
            app_data
                .config
                .web
                .twitch_api_credentials
                .client_id
                .as_str(),
        )
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
        valid_until: now
            + chrono::Duration::from_std(app_data.config.web.sessions_expire_after).unwrap(),
        user_id: user_api_response.id,
        user_login: user_api_response.login,
        user_name: user_api_response.display_name,
        user_profile_image_url: user_api_response.profile_image_url,
    };

    app_data
        .data_storage
        .append_user_authorization(&user_authorization)
        .await
        .map_err(ApiError::SaveUserAuthorization)?;

    tracing::debug!(
        "User {} ({}, {}) authorized successfully",
        user_authorization.user_name,
        user_authorization.user_login,
        user_authorization.user_id
    );

    Ok(Json(UserAuthorizationResponse::from_auth(
        &user_authorization,
        app_data.config.web.recheck_twitch_auth_after,
    )))
}

// POST /api/v2/auth/extend
pub async fn extend_token(
    Extension(app_data): Extension<WebAppData>,
    Extension(mut authorization): Extension<UserAuthorization>,
) -> Result<Json<UserAuthorizationResponse>, ApiError> {
    let new_expiry =
        Utc::now() + chrono::Duration::from_std(app_data.config.web.sessions_expire_after).unwrap();
    authorization.valid_until = new_expiry;

    app_data
        .data_storage
        .update_user_authorization(&authorization)
        .await
        .map_err(ApiError::UpdateUserAuthorization)?;

    Ok(Json(UserAuthorizationResponse::from_auth(
        &authorization,
        app_data.config.web.recheck_twitch_auth_after,
    )))
}

// POST /api/v2/auth/revoke
pub async fn revoke_token(
    Extension(app_data): Extension<WebAppData>,
    Extension(authorization): Extension<UserAuthorization>,
) -> Result<StatusCode, ApiError> {
    app_data
        .data_storage
        .delete_user_authorization(&authorization.access_token)
        .await
        .map_err(ApiError::AuthorizationRevokeFailed)?;
    Ok(StatusCode::NO_CONTENT)
}
