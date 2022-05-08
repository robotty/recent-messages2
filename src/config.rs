use serde::Deserialize;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;
use structopt::StructOpt;
use thiserror::Error;
use tokio_postgres as postgres;

const DEFAULT_CONFIG_PATH: &str = "config.toml";

/// Command line arguments
#[derive(Clone, Debug, StructOpt)]
#[structopt(rename_all = "kebab")]
pub struct Args {
    /// File path to read config from
    #[structopt(
        short = "C",
        long = "config",
        env = "RM2_CONFIG",
        default_value = DEFAULT_CONFIG_PATH
    )]
    pub config_path: PathBuf,
}

/// Config file options
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub app: AppConfig,

    #[serde(default)]
    pub irc: IrcConfig,

    pub web: WebConfig,

    #[serde(default)]
    pub db: DatabaseConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    #[serde(with = "humantime_serde")]
    pub vacuum_channels_every: Duration,
    #[serde(with = "humantime_serde")]
    pub channels_expire_after: Duration,
    #[serde(with = "humantime_serde")]
    pub vacuum_messages_every: Duration,
    #[serde(with = "humantime_serde")]
    pub messages_expire_after: Duration,
    pub max_buffer_size: usize,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            vacuum_channels_every: Duration::from_secs(30 * 60), // 30 minutes
            channels_expire_after: Duration::from_secs(24 * 60 * 60), // 24 hours
            vacuum_messages_every: Duration::from_secs(30 * 60), // 30 minutes
            messages_expire_after: Duration::from_secs(24 * 60 * 60), // 24 hours
            max_buffer_size: 500,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct IrcConfig {
    #[serde(with = "humantime_serde")]
    pub new_connection_every: Duration,

    #[serde(with = "humantime_serde")]
    pub forwarder_run_every: Duration,
    pub forwarder_max_chunk_size: usize,
}

impl Default for IrcConfig {
    fn default() -> Self {
        IrcConfig {
            new_connection_every: Duration::from_millis(550), // value determined empirically
            forwarder_run_every: Duration::from_millis(100),
            forwarder_max_chunk_size: 256,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TwitchApiClientCredentials {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WebConfig {
    #[serde(default = "default_listen_addr")]
    pub listen_address: ListenAddr,
    #[serde(flatten)]
    pub twitch_api_credentials: TwitchApiClientCredentials,
    #[serde(with = "humantime_serde", default = "seven_days")]
    pub sessions_expire_after: Duration,
    #[serde(with = "humantime_serde", default = "one_hour")]
    pub recheck_twitch_auth_after: Duration,
    #[serde(with = "humantime_serde", default = "ten_seconds")]
    pub request_timeout: Duration,
}

fn default_listen_addr() -> ListenAddr {
    ListenAddr::Tcp {
        address: "127.0.0.1:2790".parse().unwrap(),
    }
}

fn seven_days() -> Duration {
    Duration::from_secs(7 * 24 * 60 * 60)
}

fn one_hour() -> Duration {
    Duration::from_secs(60 * 60)
}

fn ten_seconds() -> Duration {
    Duration::from_secs(10)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum ListenAddr {
    #[serde(rename = "tcp")]
    Tcp { address: SocketAddr },
    #[cfg(unix)]
    #[serde(rename = "unix")]
    Unix { path: PathBuf },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DatabaseConfig {
    pub user: Option<String>,
    // psql seems to accept arbitrary bytes instead of just valid UTF-8 here
    // (the password in the tokio_postgres library is a Vec<u8>)
    // However since TOML does not support "raw" strings and you would have to type out an array
    // of bytes, using a String is my compromise.
    // Create a GitHub issue if you need non-UTF8 passwords.
    pub password: Option<String>,
    pub dbname: Option<String>,
    pub options: Option<String>,
    pub application_name: Option<String>,
    pub ssl_mode: PgSslMode,
    pub host: Vec<PgHost>,
    #[serde(with = "humantime_serde")]
    pub connect_timeout: Option<Duration>,
    pub keepalives: bool,
    #[serde(with = "humantime_serde")]
    pub keepalives_idle: Duration,
    pub target_session_attrs: PgTargetSessionAttrs,
    pub channel_binding: PgChannelBinding,
    #[serde(default)]
    pub pool: PoolConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PgSslMode {
    Disable,
    Prefer,
    Require,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum PgHost {
    #[cfg(unix)]
    Unix {
        path: PathBuf,
        #[serde(default = "default_pg_port")]
        port: u16,
    },
    Tcp {
        hostname: String,
        #[serde(default = "default_pg_port")]
        port: u16,
    },
}

fn default_pg_port() -> u16 {
    5432
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PgTargetSessionAttrs {
    Any,
    ReadWrite,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PgChannelBinding {
    Disable,
    Prefer,
    Require,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct PoolConfig {
    pub max_size: usize,
    #[serde(with = "humantime_serde")]
    pub create_timeout: Duration,
    #[serde(with = "humantime_serde")]
    pub wait_timeout: Duration,
    #[serde(with = "humantime_serde")]
    pub recycle_timeout: Duration,
}

impl Default for PoolConfig {
    fn default() -> Self {
        PoolConfig {
            max_size: num_cpus::get() * 4,
            create_timeout: Duration::from_secs(5),
            wait_timeout: Duration::from_secs(5),
            recycle_timeout: Duration::from_secs(5),
        }
    }
}

impl From<PoolConfig> for deadpool_postgres::Timeouts {
    fn from(cfg: PoolConfig) -> Self {
        deadpool_postgres::Timeouts {
            create: Some(cfg.create_timeout),
            wait: Some(cfg.wait_timeout),
            recycle: Some(cfg.recycle_timeout),
        }
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        DatabaseConfig::from(postgres::Config::default())
    }
}

impl From<postgres::Config> for DatabaseConfig {
    fn from(config: postgres::Config) -> DatabaseConfig {
        let ports: Box<dyn Iterator<Item = u16>> = if config.get_ports().len() == 1 {
            Box::new(itertools::repeat_n(
                config.get_ports()[0],
                config.get_hosts().len(),
            ))
        } else {
            Box::new(itertools::cloned(config.get_ports().iter()))
        };

        let mut hosts = vec![];
        for (host, port) in config.get_hosts().iter().zip(ports) {
            let new_host = match host {
                postgres::config::Host::Tcp(hostname) => PgHost::Tcp {
                    hostname: hostname.to_owned(),
                    port,
                },
                #[cfg(unix)]
                postgres::config::Host::Unix(path) => PgHost::Unix {
                    path: path.clone(),
                    port,
                },
            };
            hosts.push(new_host);
        }

        DatabaseConfig {
            user: config.get_user().map(String::from),
            password: config
                .get_password()
                .map(|p| String::from_utf8_lossy(p).into_owned()),
            dbname: config.get_dbname().map(String::from),
            options: config.get_options().map(String::from),
            application_name: config.get_application_name().map(String::from),
            ssl_mode: match config.get_ssl_mode() {
                postgres::config::SslMode::Disable => PgSslMode::Disable,
                postgres::config::SslMode::Prefer => PgSslMode::Prefer,
                postgres::config::SslMode::Require => PgSslMode::Require,
                _ => panic!("unhandled variant"),
            },
            host: hosts,
            connect_timeout: config.get_connect_timeout().cloned(),
            keepalives: config.get_keepalives(),
            keepalives_idle: config.get_keepalives_idle(),
            target_session_attrs: match config.get_target_session_attrs() {
                postgres::config::TargetSessionAttrs::Any => PgTargetSessionAttrs::Any,
                postgres::config::TargetSessionAttrs::ReadWrite => PgTargetSessionAttrs::ReadWrite,
                _ => panic!("unhandled variant"),
            },
            channel_binding: match config.get_channel_binding() {
                postgres::config::ChannelBinding::Disable => PgChannelBinding::Disable,
                postgres::config::ChannelBinding::Prefer => PgChannelBinding::Prefer,
                postgres::config::ChannelBinding::Require => PgChannelBinding::Require,
                _ => panic!("unhandled variant"),
            },
            pool: PoolConfig::default(),
        }
    }
}

impl From<DatabaseConfig> for postgres::Config {
    fn from(config: DatabaseConfig) -> Self {
        let mut new_cfg = postgres::Config::new();
        if let Some(ref user) = config.user {
            new_cfg.user(user);
        }
        if let Some(ref password) = config.password {
            new_cfg.password(password);
        }
        if let Some(ref dbname) = config.dbname {
            new_cfg.dbname(dbname);
        }
        if let Some(ref options) = config.options {
            new_cfg.dbname(options);
        }
        if let Some(ref application_name) = config.application_name {
            new_cfg.application_name(application_name);
        } else {
            new_cfg.application_name("recent-messages2");
        }
        new_cfg.ssl_mode(match config.ssl_mode {
            PgSslMode::Disable => postgres::config::SslMode::Disable,
            PgSslMode::Prefer => postgres::config::SslMode::Prefer,
            PgSslMode::Require => postgres::config::SslMode::Require,
        });
        for host in config.host {
            match host {
                PgHost::Tcp { ref hostname, port } => {
                    new_cfg.host(hostname);
                    new_cfg.port(port);
                }
                #[cfg(unix)]
                PgHost::Unix { ref path, port } => {
                    new_cfg.host_path(path);
                    new_cfg.port(port);
                }
            }
        }

        if let Some(ref connect_timeout) = config.connect_timeout {
            new_cfg.connect_timeout(*connect_timeout);
        }
        new_cfg.keepalives(config.keepalives);
        new_cfg.keepalives_idle(config.keepalives_idle);
        new_cfg.target_session_attrs(match config.target_session_attrs {
            PgTargetSessionAttrs::Any => postgres::config::TargetSessionAttrs::Any,
            PgTargetSessionAttrs::ReadWrite => postgres::config::TargetSessionAttrs::ReadWrite,
        });
        new_cfg.channel_binding(match config.channel_binding {
            PgChannelBinding::Disable => postgres::config::ChannelBinding::Disable,
            PgChannelBinding::Prefer => postgres::config::ChannelBinding::Prefer,
            PgChannelBinding::Require => postgres::config::ChannelBinding::Require,
        });

        new_cfg
    }
}

#[derive(Error, Debug)]
pub enum LoadConfigError {
    #[error("Failed to read file: {0}")]
    ReadFile(std::io::Error),
    #[error("Failed to parse contents: {0}")]
    ParseContents(toml::de::Error),
}

pub async fn load_config(args: &Args) -> Result<Config, LoadConfigError> {
    let file_contents = tokio::fs::read(&args.config_path)
        .await
        .map_err(LoadConfigError::ReadFile)?;
    let config = toml::from_slice(&file_contents).map_err(LoadConfigError::ParseContents)?;
    Ok(config)
}
