use crate::config::{Config, DatabaseConfig};
use crate::web::auth::{TwitchUserAccessToken, UserAuthorization};
use chrono::{DateTime, Utc};
use deadpool_postgres::{ManagerConfig, PoolConfig, RecyclingMethod};
use itertools::Itertools;
use lazy_static::lazy_static;
use prometheus::{register_histogram_vec, register_int_counter_vec, register_int_gauge_vec};
use prometheus::{HistogramVec, IntCounterVec, IntGaugeVec};
use rustls::{OwnedTrustAnchor, RootCertStore};
use std::collections::HashSet;
use std::fmt::{Display, Formatter};
use std::io::Cursor;
use std::ops::DerefMut;
use std::time::Duration;
use tokio::time::MissedTickBehavior;
use tokio_postgres::types::ToSql;
use tokio_postgres_rustls::MakeRustlsConnect;
use tokio_util::sync::CancellationToken;
use murmur3::murmur3_32;

lazy_static! {
    static ref MESSAGES_APPENDED: IntCounterVec = register_int_counter_vec!(
        "recentmessages_messages_appended",
        "Total number of messages appended to storage",
        &["db"]
    )
    .unwrap();
    static ref MESSAGES_STORED: IntGaugeVec = register_int_gauge_vec!(
        "recentmessages_messages_stored",
        "Number of messages currently stored in storage",
        &["db"]
    )
    .unwrap();
    static ref STORE_CHUNK_RUNS: IntCounterVec = register_int_counter_vec!(
        "recentmessages_irc_forwarder_store_chunk_runs",
        "Number of runs the IRC forwarder has completed",
        &["db"]
    )
    .unwrap();
    static ref STORE_CHUNK_ERRORS: IntCounterVec = register_int_counter_vec!(
        "recentmessages_irc_forwarder_store_chunk_errors",
        "Number of times a chunk could not be appended to the database successfully",
        &["db"]
    )
    .unwrap();
    static ref STORE_CHUNK_TIME_TAKEN: HistogramVec = register_histogram_vec!(
        "recentmessages_irc_forwarder_store_chunk_time_taken_seconds",
        "Time taken to forward individual chunks of messages to the database",
        &["db"]
    )
    .unwrap();
    static ref MESSAGES_VACUUMED: IntCounterVec = register_int_counter_vec!(
        "recentmessages_messages_vacuumed",
        "Total number of messages that were removed by the automatic vacuum runner",
        &["db"]
    )
    .unwrap();
    static ref VACUUM_RUNS: IntCounterVec = register_int_counter_vec!(
        "recentmessages_message_vacuum_runs",
        "Total number of times the automatic vacuum runner has been started for a certain channel",
        &["db"]
    )
    .unwrap();
    static ref DB_CONNECTIONS_IN_USE: IntGaugeVec = register_int_gauge_vec!(
        "recentmessages_db_pool_connections_in_use",
        "Number of database connections currently in use",
        &["db"]
    )
    .unwrap();
    static ref DB_CONNECTIONS_MAX: IntGaugeVec = register_int_gauge_vec!(
        "recentmessages_db_pool_connections_max",
        "Configured maximum size of the database connection pool",
        &["db"]
    )
    .unwrap();
    static ref TIME_TAKEN_TO_GET_DB_CONN: HistogramVec = register_histogram_vec!(
        "recentmessages_db_pool_retrieval_time_seconds",
        "Time taken to retrieve a DB connection from the database pool",
        &["db"]
    )
    .unwrap();
}

#[derive(Clone)]
pub struct DatabaseAccess {
    db_pool: deadpool_postgres::Pool,
    cached_name: &'static str
}

impl DatabaseAccess {
    /// Warning: this leaks a small amount of memory for the name, but it shouldn't be a problem
    /// since this happens only once during application startup and the "leaked" value
    /// is needed for the entirety of the program runtime
    pub fn new(custom_name: Option<String>,
               partition_id: usize,db_pool: deadpool_postgres::Pool) -> Self {
        let shard_or_main = if partition_id == 0 { "main" } else { "shard" };
        let cached_name = if let Some(custom_name) = &custom_name {
            format!("db{}({}, {})", partition_id, shard_or_main, custom_name)
        } else {
            format!("db{}({})", partition_id, shard_or_main)
        };
        let cached_name = Box::leak(Box::new(cached_name));
        DatabaseAccess {
            db_pool, cached_name
        }
    }
}

impl Display for DatabaseAccess {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.cached_name)
    }
}

pub fn connect_to_postgresql(config: &Config) -> DataStorage {
    let mut partition_id_counter = 0usize;
    let main_db = connect_to_single_postgres_server(&config.main_db, &mut partition_id_counter);
    let mut shard_dbs = Vec::new();
    for shard_db_config in config.shard_db.iter() {
        shard_dbs.push(connect_to_single_postgres_server(shard_db_config, &mut partition_id_counter));
    }

    DataStorage::new(
        main_db,
        shard_dbs
    )
}

fn connect_to_single_postgres_server(config: &DatabaseConfig, partition_id_counter: &mut usize) -> DatabaseAccess {
    let partition_id = *partition_id_counter;
    *partition_id_counter += 1;

    let pg_config = tokio_postgres::Config::from(config.clone());
    tracing::debug!("PostgreSQL config for db{}: {:#?}", partition_id, pg_config);

    let mgr_config = ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    };
    let pool_config = PoolConfig {
        max_size: config.pool.max_size,
        timeouts: deadpool_postgres::Timeouts::from(config.pool),
    };

    let mut root_certificates = RootCertStore::empty();
    let trust_anchors = webpki_roots::TLS_SERVER_ROOTS.0.iter().map(|trust_anchor| {
        OwnedTrustAnchor::from_subject_spki_name_constraints(
            trust_anchor.subject,
            trust_anchor.spki,
            trust_anchor.name_constraints,
        )
    });
    root_certificates.add_server_trust_anchors(trust_anchors);

    let tls_config = rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(root_certificates) // TODO support custom root certificates as well
        .with_no_client_auth(); // TODO support client auth if needed

    let tls = MakeRustlsConnect::new(tls_config);

    let manager = deadpool_postgres::Manager::from_config(pg_config, tls, mgr_config);
    let db_pool = deadpool_postgres::Pool::builder(manager)
        .config(pool_config)
        .runtime(deadpool_postgres::Runtime::Tokio1)
        .build()
        .unwrap();

    let db = DatabaseAccess::new(config.name.clone(), partition_id, db_pool);

    DB_CONNECTIONS_MAX.with_label_values(&[db.cached_name]).set(config.pool.max_size as i64);
    DB_CONNECTIONS_IN_USE.with_label_values(&[db.cached_name]).set(0);

    db
}

mod migrations_main {
    use refinery::embed_migrations;
    // refers to the "migrations_main" directory in the project root
    embed_migrations!("migrations_main");
}
mod migrations_shard {
    use refinery::embed_migrations;
    // refers to the "migrations_shard" directory in the project root
    embed_migrations!("migrations_shard");
}

pub type StorageError = deadpool_postgres::PoolError;

#[derive(Debug, Clone)]
pub struct StoredMessage {
    pub time_received: DateTime<Utc>,
    pub message_source: String,
}

#[derive(Clone)]
pub struct DataStorage {
    main_db: DatabaseAccess,
    shard_dbs: Vec<DatabaseAccess>
}

struct WrappedDbConn(deadpool_postgres::Object, &'static str);

impl WrappedDbConn {
    pub fn new(inner: deadpool_postgres::Object, db_partition_name: &'static str) -> WrappedDbConn {
        DB_CONNECTIONS_IN_USE.with_label_values(&[db_partition_name]).inc();
        WrappedDbConn(inner, db_partition_name)
    }
}

impl Drop for WrappedDbConn {
    fn drop(&mut self) {
        DB_CONNECTIONS_IN_USE.with_label_values(&[self.1]).dec();
    }
}

impl DataStorage {
    pub fn new(main_db: DatabaseAccess, shard_dbs: Vec<DatabaseAccess>) -> DataStorage {
        DataStorage { main_db, shard_dbs }
    }

    fn get_partition(&self, partition_id: usize) -> &DatabaseAccess {
        if partition_id == 0 {
            &self.main_db
        } else {
            // will panic if partition_id is out of bounds
            self.shard_dbs.get(partition_id - 1).unwrap()
        }
    }

    async fn get_db_conn(&self, partition_id: usize) -> Result<WrappedDbConn, StorageError> {
        let timer = TIME_TAKEN_TO_GET_DB_CONN.with_label_values(&[self.name_partition(partition_id)]).start_timer();
        let db_conn = self.get_partition(partition_id).db_pool.get().await;
        timer.observe_duration();
        Ok(WrappedDbConn::new(db_conn?, self.name_partition(partition_id)))
    }

    async fn get_db_conn_main(&self) -> Result<WrappedDbConn, StorageError> {
        self.get_db_conn(0).await
    }

    fn name_partition(&self, partition_id: usize) -> &'static str {
        self.get_partition(partition_id).cached_name
    }

    fn channel_to_partition_id(&self, channel_login: &str) -> usize {
        let hash_result: u32 = murmur3_32(&mut Cursor::new(channel_login), 0).unwrap();
        (hash_result % ((self.shard_dbs.len() + 1) as u32)) as usize
    }

    pub async fn run_migrations(&self) -> Result<(), Box<dyn std::error::Error>> {
        migrations_main::migrations::runner()
            .run_async(self.get_db_conn_main().await?.0.as_mut().deref_mut())
            .await?;

        for i in 0..self.shard_dbs.len() {
            migrations_shard::migrations::runner()
                .run_async(self.get_db_conn(i + 1).await?.0.as_mut().deref_mut())
                .await?;
        }

        Ok(())
    }

    pub async fn fetch_initial_metrics_values(&self) -> Result<(), StorageError> {
        for i in 0..self.shard_dbs.len()+1 {
            let count: i64 = self
                .get_db_conn(i)
                .await?
                .0
                .query_one("SELECT COUNT(*) AS count FROM message", &[])
                .await?
                .get("count");
            MESSAGES_STORED
                .with_label_values(&[self.name_partition(i)])
                .set(count);
        }
        Ok(())
    }

    pub async fn get_channel_logins_to_join(
        &self,
        channel_expiry: Duration,
    ) -> Result<HashSet<String>, StorageError> {
        let db_conn = self.get_db_conn_main().await?;

        // TODO figure out whether this has to be sped up using an index.
        let rows = db_conn
            .0
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
        let db_conn = self.get_db_conn_main().await?;
        // this way we only update the last_access if it's been at least 30 minutes since
        // the last time the last_access was updated for that channel. For high traffic
        // channels this massively cuts down on the amount of writes the DB has to do
        db_conn
            .0
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
        let db_conn = self.get_db_conn_main().await?;
        let rows = db_conn
            .0
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
        let db_conn = self.get_db_conn_main().await?;
        db_conn
            .0
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
        let db_conn = self.get_db_conn_main().await?;

        db_conn
            .0
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
        let db_conn = self.get_db_conn_main().await?;

        let rows = db_conn
            .0
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
        let db_conn = self.get_db_conn_main().await?;

        db_conn
            .0
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

    pub async fn delete_user_authorization(&self, access_token: &str) -> Result<(), StorageError> {
        let db_conn = self.get_db_conn_main().await?;

        db_conn
            .0
            .execute(
                "DELETE FROM user_authorization WHERE access_token = $1",
                &[&access_token],
            )
            .await?;

        Ok(())
    }

    // left(start) of the vec: oldest messages
    pub async fn get_messages(
        &self,
        channel_login: &str,
        limit: Option<usize>,
        max_buffer_size: usize,
    ) -> Result<Vec<StoredMessage>, StorageError> {
        // limit: If specified, take the newest N messages.
        let partition_id = self.channel_to_partition_id(channel_login);
        let db_conn = self.get_db_conn(partition_id).await?;

        let limit = match limit {
            Some(limit) => usize::min(limit, max_buffer_size),
            None => max_buffer_size,
        };

        let query = "SELECT time_received, message_source
FROM message
WHERE channel_login = $1
ORDER BY time_received DESC
LIMIT $2";

        Ok(db_conn
            .0
            .query(query, &[&channel_login, &(limit as i64)])
            .await?
            .into_iter()
            .rev()
            .map(|row| StoredMessage {
                time_received: row.get("time_received"),
                message_source: row.get("message_source"),
            })
            .collect_vec())
    }

    pub async fn purge_messages(&self, channel_login: &str) -> Result<(), StorageError> {
        let partition_id = self.channel_to_partition_id(channel_login);
        let num_messages_deleted = self.get_db_conn(partition_id)
            .await?
            .0
            .execute(
                "DELETE FROM message WHERE channel_login = $1",
                &[&channel_login],
            )
            .await?;
        MESSAGES_STORED.with_label_values(&[self.name_partition(partition_id)]).sub(num_messages_deleted as i64);
        Ok(())
    }

    /// Append a message to the storage.
    pub fn append_messages(
        &self,
        messages: Vec<(String, DateTime<Utc>, String)>,
    ) {
        let group_map = messages.into_iter().into_group_map_by(|(channel_login, _, _)| self.channel_to_partition_id(channel_login));

        for (partition_id, messages) in group_map.into_iter() {
            let self_clone = self.clone();
            tokio::spawn(async move {
                STORE_CHUNK_RUNS.with_label_values(&[self_clone.name_partition(partition_id)]).inc();
                let timer = STORE_CHUNK_TIME_TAKEN
                    .with_label_values(&[self_clone.name_partition(partition_id)])
                    .start_timer();

                let res = self_clone.append_messages_partition(partition_id, messages).await;
                if let Err(e) = res {
                    tracing::error!("Failed to append message chunk to {}: {}", self_clone.name_partition(partition_id), e);
                    STORE_CHUNK_ERRORS.with_label_values(&[self_clone.name_partition(partition_id)]).inc();
                }

                timer.observe_duration();
            });
        }
    }

    async fn append_messages_partition(
        &self,
        partition_id: usize,
        messages: Vec<(String, DateTime<Utc>, String)>
    ) -> Result<(), StorageError> {
        STORE_CHUNK_RUNS.with_label_values(&[self.name_partition(partition_id)]).inc();

        if messages.len() <= 0 {
            return Ok(());
        }
        let num_messages = messages.len();
        self.get_db_conn(partition_id)
            .await?
            .0
            .execute(
                &DataStorage::batch_message_insert_query(messages.len(), 3),
                DataStorage::batch_message_insert_values(&messages).as_slice(),
            )
            .await?;
        MESSAGES_APPENDED.with_label_values(&[self.name_partition(partition_id)]).inc_by(num_messages as u64);
        MESSAGES_STORED.with_label_values(&[self.name_partition(partition_id)]).add(num_messages as i64);
        Ok(())
    }

    fn batch_message_insert_values(
        rows: &Vec<(String, DateTime<Utc>, String)>,
    ) -> Vec<&(dyn ToSql + Sync)> {
        let mut out: Vec<&(dyn ToSql + Sync)> = vec![];
        for (a, b, c) in rows {
            out.push(a);
            out.push(b);
            out.push(c);
        }
        out
    }

    fn batch_message_insert_query(num_rows: usize, num_columns: usize) -> String {
        let mut buf = String::from(
            "INSERT INTO message(channel_login, time_received, message_source) VALUES ",
        );
        for i in 0..num_rows {
            buf.push_str("(");
            for j in 0..num_columns {
                buf.push_str(format!("${}", i * num_columns + j + 1).as_str());
                if j != num_columns - 1 {
                    buf.push_str(", ");
                }
            }
            buf.push_str(")");
            if i != num_rows - 1 {
                buf.push_str(", ");
            }
        }
        buf
    }

    pub async fn run_task_vacuum_old_messages(
        &'static self,
        config: &'static Config,
        shutdown_signal: CancellationToken,
    ) {
        let vacuum_messages_every = config.app.vacuum_messages_every;
        let message_expire_after = config.app.messages_expire_after;
        let max_buffer_size = config.app.max_buffer_size;

        let mut check_interval = tokio::time::interval(vacuum_messages_every);
        check_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

        let worker = async move {
            loop {
                check_interval.tick().await;
                tracing::info!("Running vacuum for old messages");
                for partition_id in 0..self.shard_dbs.len()+1 {
                    tokio::spawn(async move {
                        let res = self
                            .run_message_vacuum(
                                partition_id,
                                vacuum_messages_every,
                                message_expire_after,
                                max_buffer_size,
                            )
                            .await;

                        if let Err(e) = res {
                            tracing::error!(
                        "Failed to start message vacuum batch ({}), skipping entire batch: {}",
                        self.name_partition(partition_id),e);
                        };
                    });
                }
            }
        };

        tokio::select! {
            _ = worker => {},
            _ = shutdown_signal.cancelled() => {}
        }
    }

    /// Delete messages older than `messages_expire_after` and messages that go beyond the
    /// maximum buffer size.
    async fn run_message_vacuum(
        &self,
        partition_id: usize,
        vacuum_messages_every: Duration,
        messages_expire_after: Duration,
        max_buffer_size: usize,
    ) -> Result<(), StorageError> {
        let db_conn = self.get_db_conn(partition_id).await?;

        let channels_with_messages: Vec<String> = db_conn
            .0
            .query("SELECT DISTINCT channel_login FROM message", &[])
            .await?
            .into_iter()
            .map(|row| row.get("channel_login"))
            .collect_vec();

        if channels_with_messages.is_empty() {
            return Ok(()); // dont want to divide by 0
        }

        let time_between_channels = vacuum_messages_every / channels_with_messages.len() as u32;
        let mut interval = tokio::time::interval(time_between_channels);

        for channel in channels_with_messages {
            interval.tick().await;
            VACUUM_RUNS.with_label_values(&[self.name_partition(partition_id)]).inc();

            let execute_result = db_conn
                .0
                .execute(
                    "DELETE FROM message
WHERE channel_login = $1
AND (
	time_received < (
		SELECT time_received
		FROM message
		WHERE channel_login = $1
		ORDER BY time_received DESC
		OFFSET $2
		LIMIT 1
	)

	OR

	time_received < now() - make_interval(secs => $3)
)",
                    &[
                        &channel,
                        &((max_buffer_size as i64) - 1),
                        &messages_expire_after.as_secs_f64(),
                    ],
                )
                .await;

            let messages_deleted = match execute_result {
                Ok(messages_deleted) => messages_deleted,
                Err(e) => {
                    tracing::error!("({}) Failed to vacuum channel {}: {}", self.name_partition(partition_id), channel, e);
                    continue;
                }
            };

            MESSAGES_VACUUMED.with_label_values(&[self.name_partition(partition_id)]).inc_by(messages_deleted);
            MESSAGES_STORED.with_label_values(&[self.name_partition(partition_id)]).sub(messages_deleted as i64);
        }

        Ok(())
    }
}

#[cfg(test)]
pub mod test {
    #[test]
    pub fn dump_migrations() {
        dbg!(super::migrations_main::migrations::runner().get_migrations());
        dbg!(super::migrations_shard::migrations::runner().get_migrations());
    }
}
