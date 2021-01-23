pub mod auth;
pub mod get_recent_messages;
pub mod ignored;
pub mod purge;

use crate::config::{Config, ListenAddr};
use crate::db::{DataStorage, StorageError};
use crate::irc_listener::IrcListener;
use http::status::StatusCode;
use serde::Serialize;
use std::convert::Infallible;
use thiserror::Error;
use tokio::net::TcpListener;
use warp::filters::log::{Info, Log};
use warp::reject::{IsReject, Reject};
use warp::Filter;
use warp::{path, Rejection, Reply};

use metrics_exporter_prometheus::PrometheusHandle;
#[cfg(unix)]
use {
    std::fs::Permissions, std::os::unix::fs::PermissionsExt, std::path::PathBuf,
    tokio::net::UnixListener,
};

#[derive(Error, Debug)]
pub enum WebServerStartError {
    #[error("Failed to bind to address `{0}`: {1}")]
    Bind(ListenAddr, std::io::Error),
    #[cfg(unix)]
    #[error("Failed to alter permissions on unix socket `{}` to `{1:?}`: {2}", .0.to_string_lossy())]
    SetPermissions(PathBuf, Permissions, std::io::Error),
}

#[derive(Debug)]
pub enum Listener {
    Tcp(TcpListener),
    #[cfg(unix)]
    Unix(UnixListener),
}

pub async fn bind(config: &Config) -> Result<Listener, WebServerStartError> {
    match &config.web.listen_address {
        ListenAddr::Tcp { address } => TcpListener::bind(address)
            .await
            .map_err(|e| WebServerStartError::Bind(config.web.listen_address.clone(), e))
            .map(Listener::Tcp),
        #[cfg(unix)]
        ListenAddr::Unix { path } => {
            let listener = UnixListener::bind(path)
                .map_err(|e| WebServerStartError::Bind(config.web.listen_address.clone(), e))
                .map(Listener::Unix)?;

            let permissions = Permissions::from_mode(0o777);
            tokio::fs::set_permissions(path, permissions.clone())
                .await
                .map_err(|e| WebServerStartError::SetPermissions(path.clone(), permissions, e))?;

            Ok(listener)
        }
    }
}

pub async fn run(
    listener: Listener,
    prom_handle: PrometheusHandle,
    data_storage: &'static DataStorage,
    irc_listener: &'static IrcListener,
    config: &'static Config,
) {
    let get_recent_messages = path!("recent-messages" / String)
        .and(warp::get())
        .and_then(get_recent_messages::validate_channel_login)
        .and(warp::query::<
            get_recent_messages::GetRecentMessagesQueryOptions,
        >())
        .and_then(move |channel_login, options| {
            get_recent_messages::get_recent_messages(
                channel_login,
                options,
                data_storage,
                irc_listener,
            )
        });

    let get_metrics = path!("metrics")
        .and(warp::get())
        .map(move || prom_handle.render());

    let create_token = path!("auth" / "create")
        .and(warp::post())
        .and(warp::query::<auth::GetAuthorizationQueryOptions>())
        .and_then(move |query_opts: auth::GetAuthorizationQueryOptions| {
            auth::create_token(
                data_storage,
                &config.web.twitch_api_credentials,
                config.web.sessions_expire_after,
                config.web.recheck_twitch_auth_after,
                query_opts.code,
            )
        });
    let recheck_twitch_auth_after = config.web.recheck_twitch_auth_after;

    let extend_token = path!("auth" / "extend")
        .and(warp::post())
        .and(auth::with_authorization(
            data_storage,
            &config.web.twitch_api_credentials,
            recheck_twitch_auth_after,
        ))
        .and_then(move |auth| {
            auth::extend_token(
                auth,
                data_storage,
                config.web.sessions_expire_after,
                config.web.recheck_twitch_auth_after,
            )
        });

    let revoke_token = path!("auth" / "revoke")
        .and(warp::post())
        .and(auth::with_authorization(
            data_storage,
            &config.web.twitch_api_credentials,
            recheck_twitch_auth_after,
        ))
        .and_then(move |auth| auth::revoke_token(auth, data_storage));

    let get_ignored = path!("ignored")
        .and(warp::get())
        .and(auth::with_authorization(
            data_storage,
            &config.web.twitch_api_credentials,
            recheck_twitch_auth_after,
        ))
        .and_then(move |auth| ignored::get_ignored(auth, data_storage));

    let set_ignored = path!("ignored")
        .and(warp::post())
        .and(warp::filters::body::json())
        .and(auth::with_authorization(
            data_storage,
            &config.web.twitch_api_credentials,
            recheck_twitch_auth_after,
        ))
        .and_then(move |body, auth| ignored::set_ignored(auth, data_storage, irc_listener, body));

    let purge_messages = path!("purge")
        .and(warp::post())
        .and(auth::with_authorization(
            data_storage,
            &config.web.twitch_api_credentials,
            recheck_twitch_auth_after,
        ))
        .and_then(move |auth| purge::purge_messages(auth, data_storage));

    let api = get_recent_messages
        .or(get_metrics)
        .or(create_token)
        .or(extend_token)
        .or(revoke_token)
        .or(get_ignored)
        .or(set_ignored)
        .or(purge_messages)
        .or(warp::options()
            .map(warp::reply)
            .or_else(|_| std::future::ready(Err(warp::reject::not_found()))))
        .recover(handle_api_rejection)
        .map(|r| warp::reply::with_header(r, "Access-Control-Allow-Methods", "GET, POST"))
        .map(|r| {
            warp::reply::with_header(
                r,
                "Access-Control-Allow-Headers",
                "Content-Type, Authorization",
            )
        })
        .map(|r| warp::reply::with_header(r, "Access-Control-Allow-Origin", "*"));

    let app = path!("api" / "v2" / ..).and(api).with(collect_timings());

    match listener {
        Listener::Tcp(tcp_listener) => {
            // TODO remove this again when tokio gets stream support back
            let tcp_listener = async_stream::stream! {
                loop {
                    yield tcp_listener.accept().await.map(|(sock, _addr)| sock);
                }
            };
            warp::serve(app).serve_incoming(tcp_listener).await
        }
        #[cfg(unix)]
        Listener::Unix(unix_listener) => warp::serve(app).serve_incoming(unix_listener).await,
    }
}

fn collect_timings() -> Log<impl Fn(Info) + Clone> {
    warp::filters::log::custom(|info| {
        log::trace!(
            "{} {:?} {} - {} in {}",
            info.method().as_str(),
            info.version(),
            info.path(),
            info.status().as_u16(),
            humantime::format_duration(info.elapsed())
        );
        metrics::histogram!("http_request_duration_nanoseconds", info.elapsed(),
            "method" =>  info.method().as_str().to_owned(),
            "status_code" => info.status().as_str().to_owned(), // FIXME this can be without .to_owned() if only http fixed their API to specify 'static.
        );
        metrics::increment_counter!("http_request",
            "method" =>  info.method().as_str().to_owned(),
            "status_code" => info.status().as_str().to_owned(),
        );
    })
}

#[derive(Error, Debug)]
enum ApiError {
    #[error("Invalid channel login `{0}`")]
    InvalidChannelLogin(String),
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
    fn status(&self) -> StatusCode {
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
            ApiError::InvalidChannelLogin(_) => "invalid_channel_login",
            ApiError::ChannelIgnored(_) => "channel_ignored",
            ApiError::InvalidAuthorizationCode => "invalid_authorization_code",
            ApiError::MalformedAuthorizationHeader => "malformed_authorization_header",
            ApiError::Unauthorized => "unauthorized",
        }
    }
}

impl Reject for ApiError {}

#[derive(Debug, Serialize)]
struct ApiErrorResponse {
    status: u16,
    status_message: &'static str,
    error: String,
    error_code: &'static str,
}

async fn handle_api_rejection(err: Rejection) -> Result<impl Reply, Infallible> {
    let status;
    let error_string;
    let error_code;

    if let Some(custom_api_error) = err.find::<ApiError>() {
        // custom errors
        log::error!("API error: {}", custom_api_error);
        status = custom_api_error.status();
        error_string = custom_api_error.user_message();
        error_code = custom_api_error.error_code();
    } else {
        // warp errors
        status = err.status();
        error_string = err.user_message().unwrap_or_else(|| {
            log::warn!(
                "warp rejection was not an ApiError (no user_message): {0}\n{0:?}",
                err
            );
            "Internal Server Error".to_owned()
        });
        error_code = err.error_code().unwrap_or_else(|| {
            log::warn!(
                "warp rejection was not an ApiError (no error_code): {0}\n{0:?}",
                err
            );
            "internal_server_error"
        });
    }

    Ok(warp::reply::with_status(
        warp::reply::json(&ApiErrorResponse {
            status: status.as_u16(),
            status_message: status.canonical_reason().unwrap(),
            error: error_string,
            error_code,
        }),
        status,
    ))
}
