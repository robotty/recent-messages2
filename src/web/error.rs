use crate::db::StorageError;
use axum::response::{IntoResponse, Response};
use axum::Json;
use http::StatusCode;
use serde::Serialize;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Not Found")]
    NotFound,
    #[error("Method Not Allowed")]
    MethodNotAllowed,
    #[error("Invalid or missing path parameters")]
    InvalidPath,
    #[error("Invalid or missing query parameters")]
    InvalidQuery,
    #[error("Invalid channel login: {0}")]
    InvalidChannelLogin(twitch_irc::validate::Error),
    #[error("The channel login `{0}` is excluded from this service")]
    ChannelIgnored(String),
    #[error("Provided `code` could not be exchanged for a token, it is not valid")]
    InvalidAuthorizationCode,
    #[error("Malformed `Authorization` header")]
    MalformedAuthorizationHeader,
    #[error("Unauthorized (access token expired or invalid)")]
    Unauthorized,
    #[error("Failed to exchange code for an access token: {0}")]
    ExchangeCodeForAccessToken(reqwest::Error),
    #[error("Failed to query details about authorized user: {0}")]
    QueryUserDetails(reqwest::Error),
    #[error("Failed to save user authorization to database: {0}")]
    SaveUserAuthorization(StorageError),
    #[error("Failed to update user authorization to database: {0}")]
    UpdateUserAuthorization(StorageError),
    #[error("Failed to query database for access token: {0}")]
    QueryAccessToken(StorageError),
    #[error("Failed to refresh Twitch OAuth access token: {0}")]
    FailedTwitchAccessTokenRefresh(reqwest::Error),
    #[error("Failed to revoke authorization: {0}")]
    AuthorizationRevokeFailed(StorageError),
    #[error("Failed to get channel's ignored status: {0}")]
    GetChannelIgnored(StorageError),
    #[error("Failed to set channel's ignored status: {0}")]
    SetChannelIgnored(StorageError),
}

impl ApiError {
    fn status_code(&self) -> StatusCode {
        match self {
            ApiError::ExchangeCodeForAccessToken(_)
            | ApiError::QueryUserDetails(_)
            | ApiError::SaveUserAuthorization(_)
            | ApiError::UpdateUserAuthorization(_)
            | ApiError::QueryAccessToken(_)
            | ApiError::FailedTwitchAccessTokenRefresh(_)
            | ApiError::AuthorizationRevokeFailed(_)
            | ApiError::GetChannelIgnored(_)
            | ApiError::SetChannelIgnored(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::NotFound => StatusCode::NOT_FOUND,
            ApiError::MethodNotAllowed => StatusCode::METHOD_NOT_ALLOWED,
            ApiError::InvalidPath => StatusCode::BAD_REQUEST,
            ApiError::InvalidQuery => StatusCode::BAD_REQUEST,
            ApiError::InvalidChannelLogin(_) => StatusCode::BAD_REQUEST,
            ApiError::ChannelIgnored(_) => StatusCode::FORBIDDEN,
            ApiError::InvalidAuthorizationCode => StatusCode::BAD_REQUEST,
            ApiError::MalformedAuthorizationHeader => StatusCode::BAD_REQUEST,
            ApiError::Unauthorized => StatusCode::UNAUTHORIZED,
        }
    }

    fn user_message(&self) -> String {
        // custom overrides for some error types, where there is an internal cause error,
        // but we don't want to print that error to the API user.
        match self {
            ApiError::ExchangeCodeForAccessToken(_)
            | ApiError::QueryUserDetails(_)
            | ApiError::SaveUserAuthorization(_)
            | ApiError::UpdateUserAuthorization(_)
            | ApiError::QueryAccessToken(_)
            | ApiError::FailedTwitchAccessTokenRefresh(_)
            | ApiError::AuthorizationRevokeFailed(_)
            | ApiError::GetChannelIgnored(_)
            | ApiError::SetChannelIgnored(_) => "Internal Server Error".to_owned(),
            rest => format!("{}", rest),
        }
    }

    fn error_code(&self) -> &'static str {
        match self {
            ApiError::ExchangeCodeForAccessToken(_)
            | ApiError::QueryUserDetails(_)
            | ApiError::SaveUserAuthorization(_)
            | ApiError::UpdateUserAuthorization(_)
            | ApiError::QueryAccessToken(_)
            | ApiError::FailedTwitchAccessTokenRefresh(_)
            | ApiError::AuthorizationRevokeFailed(_)
            | ApiError::GetChannelIgnored(_)
            | ApiError::SetChannelIgnored(_) => "internal_server_error",
            ApiError::NotFound => "not_found",
            ApiError::MethodNotAllowed => "method_not_allowed",
            ApiError::InvalidPath => "invalid_path",
            ApiError::InvalidQuery => "invalid_query",
            ApiError::InvalidChannelLogin(_) => "invalid_channel_login",
            ApiError::ChannelIgnored(_) => "channel_ignored",
            ApiError::InvalidAuthorizationCode => "invalid_authorization_code",
            ApiError::MalformedAuthorizationHeader => "malformed_authorization_header",
            ApiError::Unauthorized => "unauthorized",
        }
    }
}

#[derive(Debug, Serialize)]
struct ApiErrorResponse {
    status: u16,
    status_message: &'static str,
    error: String,
    error_code: &'static str,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status_code(),
            Json(ApiErrorResponse {
                status: self.status_code().as_u16(),
                status_message: self.status_code().canonical_reason().unwrap(),
                error: self.user_message(),
                error_code: self.error_code(),
            }),
        )
            .into_response()
    }
}
