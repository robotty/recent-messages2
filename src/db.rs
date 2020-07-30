use crate::config::Config;
use crate::web::auth::{TwitchUserAccessToken, UserAuthorization};
use chrono::{DateTime, TimeZone, Utc};
use itertools::Itertools;
use mobc::Pool;
use mobc_postgres::PgConnectionManager;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{HashMap, VecDeque};
use std::ops::RangeTo;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};
use tokio_postgres::error::Error as PgError;
use tokio_postgres::tls::NoTls;

// TODO support TLS if needed
// see https://docs.rs/postgres-native-tls/0.3.0/postgres_native_tls/index.html

type PgPool = Pool<PgConnectionManager<NoTls>>;

pub async fn connect_to_postgresql(config: &Config) -> PgPool {
    let pg_config = tokio_postgres::Config::from(config.db.clone());
    log::debug!("PostgreSQL config: {:#?}", pg_config);
    let manager = PgConnectionManager::new(pg_config, NoTls);
    Pool::builder().max_open(128).build(manager)
}

mod migrations {
    use refinery::embed_migrations;
    // refers to the "migrations" directory in the project root
    embed_migrations!("migrations");
}

pub async fn run_migrations(db: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let mut db = db.get().await?;
    migrations::migrations::runner().run_async(&mut *db).await?;
    Ok(())
}

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Timed out while connecting to PostgreSQL database server")]
    Timeout,
    #[error("Bad connection in the pool")]
    BadConn,
    #[error("Error communicating with PostgreSQL database server: {0}")]
    PgError(#[from] PgError),
}

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

// mobc::Error<PgError> gets returned from the db pool .get() call.
impl From<mobc::Error<PgError>> for StorageError {
    fn from(err: mobc::Error<PgError>) -> Self {
        match err {
            mobc::Error::Timeout => StorageError::Timeout,
            mobc::Error::BadConn => StorageError::BadConn,
            mobc::Error::Inner(pg_error) => StorageError::PgError(pg_error),
        }
    }
}

#[derive(Clone)]
pub struct DataStorage {
    db: PgPool,
    messages: Arc<RwLock<HashMap<String, Arc<Mutex<VecDeque<StoredMessage>>>>>>,
    messages_stored: Arc<AtomicU64>,
}

/// This is used for "channels to join" and "channels to part" queries.
/// From experience with version 1 of recent messages, the number of channels that are not
/// joined over time greatly overpowers the number of joined channels.
///
/// If "channels to part" returned all channels since the beginning of time, the query response
/// and therefore the resulting workload would grow extremely large.
///
/// However, to our advantage, we know the exact `now()`-time used during the "channels to join"
/// or "channels to part" query. With this information, it is possible to query
/// a list of "channels to part", but only the channels whose status has changed since the
/// last query.
#[derive(Debug, Clone, Copy)]
pub struct IterationTimestamp(DateTime<Utc>);

impl DataStorage {
    pub fn new(db: PgPool) -> DataStorage {
        DataStorage {
            db,
            messages: Default::default(),
            messages_stored: Default::default(),
        }
    }

    pub async fn get_channel_logins_to_join(
        &self,
        channel_expiry: Duration,
    ) -> Result<(Vec<String>, IterationTimestamp), StorageError> {
        let mut db_conn = self.db.get().await?;
        let transaction = db_conn.transaction().await?;

        let current_time_rows = transaction.query("SELECT now()", &[]).await?;
        let current_time: DateTime<Utc> = current_time_rows[0].get(0);
        let next_query_token = IterationTimestamp(current_time);

        // TODO figure out whether this has to be sped up using an index.
        let rows = transaction
            .query(
                r"SELECT channel_login
FROM channel
WHERE ignored_at IS NULL
  AND last_access > now() - make_interval(secs => $1)
ORDER BY last_access DESC",
                &[&channel_expiry.as_secs_f64()],
            )
            .await?;
        let channels = rows
            .into_iter()
            .map(|row| row.get("channel_login"))
            .collect_vec();

        transaction.commit().await?;

        Ok((channels, next_query_token))
    }

    pub async fn touch_or_add_channel(&self, channel_login: &str) -> Result<(), StorageError> {
        let db_conn = self.db.get().await?;
        db_conn
            .query(
                r"INSERT INTO channel (channel_login)
VALUES ($1)
ON CONFLICT ON CONSTRAINT channel_pkey DO UPDATE
    SET last_access = now()",
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

    /// List of channels that we DON'T join. The exact opposite of `get_channel_logins_to_join`.
    pub async fn get_channel_logins_to_part(
        &self,
        last_iteration_timestamp: IterationTimestamp,
        channel_expiry: Duration,
    ) -> Result<(Vec<String>, IterationTimestamp), StorageError> {
        let mut db_conn = self.db.get().await?;
        let transaction = db_conn.transaction().await?;

        let current_time_rows = transaction.query("SELECT now()", &[]).await?;
        let current_time: DateTime<Utc> = current_time_rows[0].get(0);
        let next_query_token = IterationTimestamp(current_time);

        // has_not_been_accessed_for := now() - last_access
        // channel_expiry := make_interval(secs => $1)
        // channel_is_expired := has_not_been_accessed_for >= channel_expiry

        // last_iteration_timestamp := $2
        // last_iteration_had_not_been_accessed_for := last_iteration_timestamp - last_access
        // channel_was_considered_expired_on_last_check := last_iteration_had_not_been_accessed_for >= channel_expiry

        // resulting condition: channel_is_expired AND NOT channel_was_considered_expired_on_last_check

        // additionally we check the ignored status, but the check is simple: If the time the channel
        // was ignored is after the last check, it is returned.
        let rows = transaction
            .query(
                r"SELECT channel_login
FROM channel
WHERE ignored_at >= $2
  OR (now() - last_access >= make_interval(secs => $1) AND NOT $2 - last_access >= make_interval(secs => $1))",
                &[&channel_expiry.as_secs_f64(), &last_iteration_timestamp.0],
            )
            .await?;
        let channels = rows
            .into_iter()
            .map(|row| row.get("channel_login"))
            .collect_vec();

        transaction.commit().await?;

        Ok((channels, next_query_token))
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
    pub async fn get_messages(&self, channel_login: &str) -> Vec<StoredMessage> {
        let channel_messages = self
            .messages
            .read()
            .await
            .get(channel_login)
            .map(|e| Arc::clone(&e));
        match channel_messages {
            Some(channel_messages) => {
                let channel_messages = channel_messages.lock().await;
                channel_messages.iter().cloned().collect()
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
    ///
    /// Returns by how much the amount of stored messages increased as a result of the operation.
    /// If the message was appended, but at the same time an old overflowing message was removed
    /// as well, then this returns `0`. If the message was appended without another message being
    /// removed at the same time, then this returns `1`.
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
        metrics::counter!("recent_messages_messages_appended", 1);

        if channel_messages.len() > max_buffer_size {
            channel_messages.pop_front();
        } else {
            let new_gauge_value = self.messages_stored.fetch_add(1, Ordering::SeqCst) + 1;
            metrics::gauge!("recent_messages_messages_stored", new_gauge_value as i64);
        }
    }

    pub async fn run_task_vacuum_old_messages(self, config: &'static Config) {
        metrics::counter!("recent_messages_messages_vacuumed", 0);
        // initialize to 0
        metrics::counter!("recent_messages_message_vacuum_runs", 0); // initialize to 0
        let vacuum_messages_every = config.app.vacuum_messages_every;
        let message_expire_after = config.app.messages_expire_after;

        let mut check_interval = tokio::time::interval(vacuum_messages_every);
        // uses up the initial tick, there is no need to run immediately
        // after application startup

        loop {
            check_interval.tick().await;
            let self_clone = self.clone();
            tokio::spawn(async move {
                log::info!("Running vacuum for old messages");
                self_clone
                    .run_message_vacuum(vacuum_messages_every, message_expire_after)
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
        if channels_with_messages.len() == 0 {
            return; // dont want to divide by 0
        }

        let time_between_channels = vacuum_messages_every / channels_with_messages.len() as u32;
        let mut interval = tokio::time::interval(time_between_channels);

        for channel in channels_with_messages {
            interval.tick().await;

            let messages_map = self.messages.read().await;
            let channel_messages = messages_map.get(&channel).map(|e| Arc::clone(&e));
            drop(messages_map); // unlock mutex
            match channel_messages {
                Some(channel_messages) => {
                    let mut channel_messages = channel_messages.lock().await;

                    // iter() begins at the front of the list, which is where the oldest message lives
                    let cutoff_time =
                        Utc::now() - chrono::Duration::from_std(messages_expire_after).unwrap();
                    let mut remove_until = None;
                    for (i, StoredMessage { time_received, .. }) in
                        channel_messages.iter().enumerate()
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

                        let messages_deleted = (remove_until + 1) as u64;
                        metrics::counter!("recent_messages_messages_vacuumed", messages_deleted);

                        let new_gauge_value = self
                            .messages_stored
                            .fetch_sub(messages_deleted, Ordering::SeqCst)
                            - messages_deleted;
                        metrics::gauge!("recent_messages_messages_stored", new_gauge_value as i64);

                        // remove the mapping from the map if there are no more messages.
                        if channel_messages.len() == 0 {
                            self.messages.write().await.remove(&channel);
                        }
                    }
                    // else: (None) no messages to delete.
                }
                None => {} // channel does not have messages
            };

            metrics::counter!("recent_messages_message_vacuum_runs", 1);
        }
    }

    pub async fn load_messages_from_disk(
        &self,
        config: &'static Config,
    ) -> Result<(), FileStorageError> {
        log::info!("Loading snapshot of messages from disk...");
        let save_file_directory = config.app.save_file_directory.clone();
        let directory_contents_res = tokio::fs::read_dir(&save_file_directory).await;
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
                log::debug!(
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

            let messages_added = channel_messages.len() as u64;
            let new_gauge_value = self
                .messages_stored
                .fetch_add(messages_added, Ordering::SeqCst)
                + messages_added;
            metrics::gauge!("recent_messages_messages_stored", new_gauge_value as i64);

            messages_map.insert(channel_login, Arc::new(Mutex::new(channel_messages)));
        }

        Ok(())
    }

    pub async fn save_messages_to_disk(
        &self,
        config: &'static Config,
    ) -> Result<(), FileStorageError> {
        log::info!("Saving snapshot of messages to disk...");
        let save_file_directory = config.app.save_file_directory.clone();
        let mkdir_result = tokio::fs::DirBuilder::new()
            .create(&save_file_directory)
            .await;
        if let Err(e) = mkdir_result {
            // it's not an error condition if the directory already exists.
            if e.kind() != std::io::ErrorKind::AlreadyExists {
                return Err(e.into());
            }
        }

        // delete files that were there previously
        let mut directory_contents = tokio::fs::read_dir(&save_file_directory).await?;
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
