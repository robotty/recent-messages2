use crate::db::StorageError;
use axum::response::{IntoResponse, Response};
use axum::Json;
use http::header::HeaderName;
use http::StatusCode;
use serde::Serialize;
use thiserror::Error;
use tracing::error;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Not Found")]
    NotFound,
    #[error("Request Timeout")]
    RequestTimeout,
    #[error("Method Not Allowed")]
    MethodNotAllowed,
    #[error("Invalid or missing path parameters")]
    InvalidPath,
    #[error("Invalid or missing query parameters")]
    InvalidQuery,
    #[error("Invalid or missing payload in request body")]
    InvalidPayload,
    #[error("Header value for Header `{0}` was not valid UTF-8")]
    HeaderValueNotUtf8(HeaderName),
    #[error("Missing header `{0}`")]
    MissingHeader(HeaderName),
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
    #[error("Failed get a channel's messages: {0}")]
    GetMessages(StorageError),
    #[error("Failed to purge a channel's messages: {0}")]
    PurgeMessages(StorageError),
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
            | ApiError::SetChannelIgnored(_)
            | ApiError::GetMessages(_)
            | ApiError::PurgeMessages(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::NotFound => StatusCode::NOT_FOUND,
            ApiError::RequestTimeout => StatusCode::REQUEST_TIMEOUT,
            ApiError::MethodNotAllowed => StatusCode::METHOD_NOT_ALLOWED,
            ApiError::InvalidPath => StatusCode::BAD_REQUEST,
            ApiError::InvalidQuery => StatusCode::BAD_REQUEST,
            ApiError::InvalidPayload => StatusCode::BAD_REQUEST,
            ApiError::HeaderValueNotUtf8(_) => StatusCode::BAD_REQUEST,
            ApiError::MissingHeader(_) => StatusCode::BAD_REQUEST,
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
            | ApiError::SetChannelIgnored(_)
            | ApiError::GetMessages(_)
            | ApiError::PurgeMessages(_) => "Internal Server Error".to_owned(),
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
            | ApiError::SetChannelIgnored(_)
            | ApiError::GetMessages(_)
            | ApiError::PurgeMessages(_) => "internal_server_error",
            ApiError::NotFound => "not_found",
            ApiError::RequestTimeout => "request_timeout",
            ApiError::MethodNotAllowed => "method_not_allowed",
            ApiError::InvalidPath => "invalid_path",
            ApiError::InvalidQuery => "invalid_query",
            ApiError::InvalidPayload => "invalid_payload",
            ApiError::HeaderValueNotUtf8(_) => "header_value_not_utf8",
            ApiError::MissingHeader(_) => "missing_header",
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
        // If error is in the 5xx range, log it.
        if self.status_code().is_server_error() {
            error!("Returning Internal Server Error to a user: {}", self);
        }

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
