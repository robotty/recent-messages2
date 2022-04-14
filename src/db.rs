use crate::config::Config;
use crate::web::auth::{TwitchUserAccessToken, UserAuthorization};
use chrono::{DateTime, TimeZone, Utc};
use deadpool_postgres::{ManagerConfig, PoolConfig, RecyclingMethod};
use itertools::Itertools;
use lazy_static::lazy_static;
use prometheus::{register_int_counter, register_int_gauge};
use prometheus::{IntCounter, IntGauge};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{HashMap, HashSet, VecDeque};
use std::ops::{DerefMut, RangeTo};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};
use tokio_postgres::tls::NoTls;

// TODO support TLS if needed
// see https://docs.rs/postgres-native-tls/0.3.0/postgres_native_tls/index.html

lazy_static! {
    static ref MESSAGES_APPENDED: IntCounter = register_int_counter!(
        "recentmessages_messages_appended",
        "Total number of messages appended to storage"
    )
    .unwrap();
    static ref MESSAGES_STORED: IntGauge = register_int_gauge!(
        "recentmessages_messages_stored",
        "Number of messages currently stored in storage"
    )
    .unwrap();
    static ref MESSAGES_VACUUMED: IntCounter = register_int_counter!(
        "recentmessages_messages_vacuumed",
        "Total number of messages that were removed by the automatic vacuum runner"
    )
    .unwrap();
    static ref VACUUM_RUNS: IntCounter = register_int_counter!(
        "recentmessages_message_vacuum_runs",
        "Total number of times the automatic vacuum runner has been started for a certain channel"
    )
    .unwrap();
}

type PgPool = deadpool_postgres::Pool;

pub async fn connect_to_postgresql(config: &Config) -> PgPool {
    let pg_config = tokio_postgres::Config::from(config.db.clone());
    tracing::debug!("PostgreSQL config: {:#?}", pg_config);

    let mgr_config = ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    };
    let pool_config = PoolConfig {
        max_size: config.app.db_pool_max_size,
        // For now I've set all of these to `None` intentionally
        timeouts: deadpool_postgres::Timeouts {
            create: None,
            wait: None,
            recycle: None,
        },
    };

    let manager = deadpool_postgres::Manager::from_config(pg_config, NoTls, mgr_config);
    PgPool::builder(manager)
        .config(pool_config)
        .build()
        .unwrap()
}

mod migrations {
    use refinery::embed_migrations;
    // refers to the "migrations" directory in the project root
    embed_migrations!("migrations");
}

pub async fn run_migrations(db: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let mut db = db.get().await?;
    migrations::migrations::runner()
        .run_async(db.as_mut().deref_mut())
        .await?;
    Ok(())
}

pub type StorageError = deadpool_postgres::PoolError;

// TODO could possibly optimize further by storing the ServerMessage instead of its source?
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessage {
    #[serde(
        serialize_with = "to_utc_milliseconds",
        deserialize_with = "from_utc_milliseconds"
    )]
    pub time_received: DateTime<Utc>,
    pub message_source: String,
}

fn from_utc_milliseconds<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
where
    D: Deserializer<'de>,
{
    let millis = Deserialize::deserialize(deserializer)?;
    Ok(Utc.timestamp_millis(millis))
}

fn to_utc_milliseconds<S>(timestamp: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_i64(timestamp.timestamp_millis())
}

#[derive(Clone)]
pub struct DataStorage {
    db: PgPool,
    #[allow(clippy::type_complexity)] // type is not used anywhere except here
    messages: Arc<RwLock<HashMap<String, Arc<Mutex<VecDeque<StoredMessage>>>>>>,
}

impl DataStorage {
    pub fn new(db: PgPool) -> DataStorage {
        DataStorage {
            db,
            messages: Default::default(),
        }
    }

    pub async fn get_channel_logins_to_join(
        &self,
        channel_expiry: Duration,
    ) -> Result<HashSet<String>, StorageError> {
        let db_conn = self.db.get().await?;

        // TODO figure out whether this has to be sped up using an index.
        let rows = db_conn
            .query(
                r"SELECT channel_login
FROM channel
WHERE ignored_at IS NULL
  AND last_access > now() - make_interval(secs => $1)
ORDER BY last_access DESC",
                &[&channel_expiry.as_secs_f64()],
            )
            .await?;
        let channels = rows.into_iter().map(|row| row.get(0)).collect();

        Ok(channels)
    }

    pub async fn touch_or_add_channel(&self, channel_login: &str) -> Result<(), StorageError> {
        let db_conn = self.db.get().await?;
        // this way we only update the last_access if it's been at least 30 minutes since
        // the last time the last_access was updated for that channel. For high traffic
        // channels this massively cuts down on the amount of writes the DB has to do
        db_conn
            .execute(
                r"INSERT INTO channel (channel_login) VALUES ($1)
ON CONFLICT ON CONSTRAINT channel_pkey DO UPDATE
    SET last_access = now()
    WHERE channel.last_access < now() - INTERVAL '30 minutes'",
                &[&channel_login],
            )
            .await?;
        Ok(())
    }

    pub async fn is_channel_ignored(&self, channel_login: &str) -> Result<bool, StorageError> {
        let db_conn = self.db.get().await?;
        let rows = db_conn
            .query(
                r"SELECT ignored_at IS NOT NULL FROM channel
WHERE channel_login = $1",
                &[&channel_login],
            )
            .await?;
        // if found, get the value from the returned row, otherwise, the channel is not known
        // and therefore not ignored
        Ok(rows.get(0).map(|row| row.get(0)).unwrap_or(false))
    }

    pub async fn set_channel_ignored(
        &self,
        channel_login: &str,
        ignored: bool,
    ) -> Result<(), StorageError> {
        let db_conn = self.db.get().await?;
        db_conn
            .query(
                r"INSERT INTO channel (channel_login, ignored_at)
VALUES ($1, CASE WHEN $2 THEN now() ELSE NULL END)
ON CONFLICT ON CONSTRAINT channel_pkey DO UPDATE
    SET ignored_at = CASE WHEN $2 THEN now() ELSE NULL END",
                &[&channel_login, &ignored],
            )
            .await?;
        Ok(())
    }

    pub async fn append_user_authorization(
        &self,
        user_authorization: &UserAuthorization,
    ) -> Result<(), StorageError> {
        let db_conn = self.db.get().await?;

        db_conn
            .execute(
                "INSERT INTO user_authorization(access_token, twitch_access_token,
twitch_refresh_token, twitch_authorization_last_validated, valid_until, user_id,
user_login, user_name, user_profile_image_url)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
                &[
                    &user_authorization.access_token,
                    &user_authorization.twitch_token.access_token,
                    &user_authorization.twitch_token.refresh_token,
                    &user_authorization.twitch_authorization_last_validated,
                    &user_authorization.valid_until,
                    &user_authorization.user_id,
                    &user_authorization.user_login,
                    &user_authorization.user_name,
                    &user_authorization.user_profile_image_url,
                ],
            )
            .await?;

        Ok(())
    }

    pub async fn get_user_authorization(
        &self,
        access_token: &str,
    ) -> Result<Option<UserAuthorization>, StorageError> {
        let db_conn = self.db.get().await?;

        let rows = db_conn
            .query(
                "SELECT access_token, twitch_access_token, twitch_refresh_token,
twitch_authorization_last_validated, valid_until, user_id,
user_login, user_name, user_profile_image_url
FROM user_authorization
WHERE access_token = $1
AND valid_until >= now()",
                &[&access_token],
            )
            .await?;

        if let Some(row) = rows.get(0) {
            // token found in DB and not expired
            Ok(Some(UserAuthorization {
                access_token: row.get("access_token"),
                twitch_token: TwitchUserAccessToken {
                    access_token: row.get("twitch_access_token"),
                    refresh_token: row.get("twitch_refresh_token"),
                },
                twitch_authorization_last_validated: row.get("twitch_authorization_last_validated"),
                valid_until: row.get("valid_until"),
                user_id: row.get("user_id"),
                user_login: row.get("user_login"),
                user_name: row.get("user_name"),
                user_profile_image_url: row.get("user_profile_image_url"),
            }))
        } else {
            // token not found in DB, or it's expired
            Ok(None)
        }
    }

    pub async fn update_user_authorization(
        &self,
        user_authorization: &UserAuthorization,
    ) -> Result<(), StorageError> {
        let db_conn = self.db.get().await?;

        db_conn
            .execute(
                "UPDATE user_authorization
SET twitch_access_token = $2,
twitch_refresh_token = $3,
twitch_authorization_last_validated = $4,
valid_until = $5,
user_id = $6,
user_login = $7,
user_name = $8,
user_profile_image_url = $9
WHERE access_token = $1",
                &[
                    &user_authorization.access_token,
                    &user_authorization.twitch_token.access_token,
                    &user_authorization.twitch_token.refresh_token,
                    &user_authorization.twitch_authorization_last_validated,
                    &user_authorization.valid_until,
                    &user_authorization.user_id,
                    &user_authorization.user_login,
                    &user_authorization.user_name,
                    &user_authorization.user_profile_image_url,
                ],
            )
            .await?;

        Ok(())
    }

    // TODO background task to purge expired authorizations

    // left(start) of the vec: oldest messages
    pub async fn get_messages(
        &self,
        channel_login: &str,
        limit: Option<usize>,
    ) -> Vec<StoredMessage> {
        // limit: If specified, take the newest N messages.

        let channel_messages = self
            .messages
            .read()
            .await
            .get(channel_login)
            .map(|e| Arc::clone(&e));
        match channel_messages {
            Some(channel_messages) => {
                let channel_messages = channel_messages.lock().await;
                let limit = limit.unwrap_or_else(|| channel_messages.len());
                channel_messages
                    .iter()
                    .rev()
                    .take(limit)
                    .rev()
                    .cloned()
                    .collect()
            }
            None => vec![],
        }
    }

    pub async fn purge_messages(&self, channel_login: &str) {
        self.messages.write().await.remove(channel_login);
    }

    pub async fn delete_user_authorization(&self, access_token: &str) -> Result<(), StorageError> {
        let db_conn = self.db.get().await?;

        db_conn
            .execute(
                "DELETE FROM user_authorization WHERE access_token = $1",
                &[&access_token],
            )
            .await?;

        Ok(())
    }

    /// Append a message to the storage.
    pub async fn append_message(
        &self,
        channel_login: String,
        message_source: String,
        max_buffer_size: usize,
    ) {
        let mut messages_map = self.messages.write().await;
        // default is a new Mutex holding an empty vec
        let channel_entry = Arc::clone(&messages_map.entry(channel_login).or_default());
        drop(messages_map); // unlock mutex

        let mut channel_messages = channel_entry.lock().await;
        channel_messages.push_back(StoredMessage {
            time_received: Utc::now(),
            message_source,
        });
        MESSAGES_APPENDED.inc();

        if channel_messages.len() > max_buffer_size {
            channel_messages.pop_front();
        } else {
            MESSAGES_STORED.inc();
        }
    }

    pub async fn run_task_vacuum_old_messages(&'static self, config: &'static Config) {
        let vacuum_messages_every = config.app.vacuum_messages_every;
        let message_expire_after = config.app.messages_expire_after;

        let mut check_interval = tokio::time::interval(vacuum_messages_every);
        // uses up the initial tick, there is no need to run immediately
        // after application startup

        loop {
            check_interval.tick().await;
            tokio::spawn(async move {
                tracing::info!("Running vacuum for old messages");
                self.run_message_vacuum(vacuum_messages_every, message_expire_after)
                    .await;
            });
        }
    }

    /// Delete messages older than `messages_expire_after`.
    async fn run_message_vacuum(
        &self,
        vacuum_messages_every: Duration,
        messages_expire_after: Duration,
    ) {
        let channels_with_messages = self.messages.read().await.keys().cloned().collect_vec();
        if channels_with_messages.is_empty() {
            return; // dont want to divide by 0
        }

        let time_between_channels = vacuum_messages_every / channels_with_messages.len() as u32;
        let mut interval = tokio::time::interval(time_between_channels);

        for channel in channels_with_messages {
            interval.tick().await;

            let messages_map = self.messages.read().await;
            let channel_messages = messages_map.get(&channel).map(|e| Arc::clone(&e));
            drop(messages_map); // unlock mutex
            if let Some(channel_messages) = channel_messages {
                let mut channel_messages = channel_messages.lock().await;

                // iter() begins at the front of the list, which is where the oldest message lives
                let cutoff_time =
                    Utc::now() - chrono::Duration::from_std(messages_expire_after).unwrap();
                let mut remove_until: Option<usize> = None;
                for (i, StoredMessage { time_received, .. }) in channel_messages.iter().enumerate()
                {
                    if time_received < &cutoff_time {
                        // this message should be deleted
                        remove_until = Some(i);
                    } else {
                        // message should be preserved
                        // no point further looking since all following messages will be
                        // younger
                        break;
                    }
                }

                if let Some(remove_until) = remove_until {
                    channel_messages.drain(RangeTo {
                        end: remove_until + 1,
                    });
                    channel_messages.shrink_to_fit();

                    let messages_deleted = remove_until as u64 + 1;
                    MESSAGES_VACUUMED.inc_by(messages_deleted);
                    MESSAGES_STORED.add(-(messages_deleted as i64));

                    // remove the mapping from the map if there are no more messages.
                    if channel_messages.len() == 0 {
                        self.messages.write().await.remove(&channel);
                    }
                }
            } // else: (None) no messages stored for that channel.

            VACUUM_RUNS.inc();
        }
    }

    pub async fn load_messages_from_disk(
        &self,
        config: &'static Config,
    ) -> Result<(), FileStorageError> {
        tracing::info!("Loading snapshot of messages from disk...");
        let save_file_directory = &config.app.save_file_directory;
        let directory_contents_res = tokio::fs::read_dir(save_file_directory).await;
        let mut directory_contents = match directory_contents_res {
            Ok(directory_contents) => directory_contents,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    return Ok(());
                } else {
                    return Err(e.into());
                }
            }
        };

        let mut messages_map = self.messages.write().await;
        messages_map.clear();

        while let Some(dir_entry) = directory_contents.next_entry().await? {
            let file_path = dir_entry.path();
            if file_path
                .extension()
                .map(|ext| ext != "dat")
                .unwrap_or(true)
            {
                // either has an extension that is not `dat` or has no extension
                tracing::debug!(
                    "Ignoring file {} from messages directory, extension is not `dat`",
                    file_path.to_string_lossy()
                );
                continue;
            }

            let channel_login = file_path.file_stem().unwrap().to_str().unwrap().to_owned();

            let channel_messages = tokio::task::spawn_blocking(move || {
                let file = std::fs::File::open(file_path)?;
                let channel_messages = rmp_serde::decode::from_read(file)?;
                Ok::<VecDeque<StoredMessage>, FileStorageError>(channel_messages)
            })
            .await
            .unwrap()?;

            let messages_added = channel_messages.len() as i64;
            MESSAGES_STORED.add(messages_added);

            messages_map.insert(channel_login, Arc::new(Mutex::new(channel_messages)));
        }

        Ok(())
    }

    pub async fn save_messages_to_disk(
        &self,
        config: &'static Config,
    ) -> Result<(), FileStorageError> {
        tracing::info!("Saving snapshot of messages to disk...");
        let save_file_directory = &config.app.save_file_directory;
        let mkdir_result = tokio::fs::DirBuilder::new()
            .create(save_file_directory)
            .await;
        if let Err(e) = mkdir_result {
            // it's not an error condition if the directory already exists.
            if e.kind() != std::io::ErrorKind::AlreadyExists {
                return Err(e.into());
            }
        }

        // delete files that were there previously
        let mut directory_contents = tokio::fs::read_dir(save_file_directory).await?;
        while let Some(dir_entry) = directory_contents.next_entry().await? {
            tokio::fs::remove_file(&dir_entry.path()).await?;
        }

        // now save files
        let messages_map = self.messages.read().await;
        for (channel_login, messages) in messages_map.iter() {
            let messages = Arc::clone(messages);
            let messages = messages.lock_owned().await;

            let save_file_path = save_file_directory
                .clone()
                .join(&channel_login)
                .with_extension("dat");

            tokio::task::spawn_blocking(move || {
                let mut file = std::fs::File::create(save_file_path)?;
                rmp_serde::encode::write_named(&mut file, &*messages)?;
                Ok::<(), FileStorageError>(())
            })
            .await
            .unwrap()?;
        }

        Ok(())
    }
}

#[derive(Error, Debug)]
pub enum FileStorageError {
    #[error("I/O error: {0}")]
    IO(#[from] std::io::Error),
    #[error("Error while encoding: {0}")]
    Encode(#[from] rmp_serde::encode::Error),
    #[error("Error while decoding: {0}")]
    Decode(#[from] rmp_serde::decode::Error),
}

#[cfg(test)]
pub mod test {
    #[test]
    pub fn dump_migrations() {
        dbg!(super::migrations::migrations::runner().get_migrations());
    }
}
