use crate::config::Config;
use crate::web::auth::{TwitchUserAccessToken, UserAuthorization};
use chrono::{DateTime, Utc};
use deadpool_postgres::{ManagerConfig, PoolConfig, RecyclingMethod};
use itertools::Itertools;
use lazy_static::lazy_static;
use prometheus::{register_histogram, register_int_counter, register_int_gauge};
use prometheus::{Histogram, IntCounter, IntGauge};
use std::collections::HashSet;
use std::ops::DerefMut;
use std::time::Duration;
use tokio::time::MissedTickBehavior;
use tokio_postgres::tls::NoTls;
use tokio_util::sync::CancellationToken;

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
    static ref TIME_TAKEN_TO_GET_DB_CONN: Histogram = register_histogram!(
        "recentmessages_db_pool_retrieval_time_seconds",
        "Time taken to retrieve a DB connection from the database pool"
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
        max_size: config.db.pool.max_size,
        timeouts: deadpool_postgres::Timeouts::from(config.db.pool),
    };

    let manager = deadpool_postgres::Manager::from_config(pg_config, NoTls, mgr_config);
    PgPool::builder(manager)
        .config(pool_config)
        .runtime(deadpool_postgres::Runtime::Tokio1)
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

#[derive(Debug, Clone)]
pub struct StoredMessage {
    pub time_received: DateTime<Utc>,
    pub message_source: String,
}

#[derive(Clone)]
pub struct DataStorage {
    db: PgPool,
}

impl DataStorage {
    pub fn new(db: PgPool) -> DataStorage {
        DataStorage { db }
    }

    async fn get_db_conn(&self) -> Result<deadpool_postgres::Object, StorageError> {
        let timer = TIME_TAKEN_TO_GET_DB_CONN.start_timer();
        let db_conn = self.db.get().await;
        timer.observe_duration();
        db_conn
    }

    pub async fn fetch_initial_metrics_values(&self) -> Result<(), StorageError> {
        let count: i64 = self
            .get_db_conn()
            .await?
            .query_one("SELECT COUNT(*) AS count FROM message", &[])
            .await?
            .get("count");
        MESSAGES_STORED.set(count);
        Ok(())
    }

    pub async fn get_channel_logins_to_join(
        &self,
        channel_expiry: Duration,
    ) -> Result<HashSet<String>, StorageError> {
        let db_conn = self.get_db_conn().await?;

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
        let db_conn = self.get_db_conn().await?;
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
        let db_conn = self.get_db_conn().await?;
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
        let db_conn = self.get_db_conn().await?;
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
        let db_conn = self.get_db_conn().await?;

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
        let db_conn = self.get_db_conn().await?;

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
        let db_conn = self.get_db_conn().await?;

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

    pub async fn delete_user_authorization(&self, access_token: &str) -> Result<(), StorageError> {
        let db_conn = self.get_db_conn().await?;

        db_conn
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
        let db_conn = self.get_db_conn().await?;

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
        self.get_db_conn()
            .await?
            .execute(
                "DELETE FROM message WHERE channel_login = $1",
                &[&channel_login],
            )
            .await?;
        Ok(())
    }

    /// Append a message to the storage.
    pub async fn append_messages(
        &self,
        messages: Vec<(String, DateTime<Utc>, String)>,
    ) -> Result<(), StorageError> {
        if messages.len() <= 0 {
            return Ok(());
        }
        let mut db_conn = self.get_db_conn().await?;
        let tx = db_conn.transaction().await?;
        let num_messages = messages.len();
        for (channel_login, time_received, message_source) in messages {
            tx.execute("INSERT INTO message(channel_login, time_received, message_source) VALUES ($1, $2, $3)", &[&channel_login, &time_received, &message_source]).await?;
        }
        tx.commit().await?;
        MESSAGES_APPENDED.inc_by(num_messages as u64);
        MESSAGES_STORED.add(num_messages as i64);
        Ok(())
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
                let res = self
                    .run_message_vacuum(
                        vacuum_messages_every,
                        message_expire_after,
                        max_buffer_size,
                    )
                    .await;

                if let Err(e) = res {
                    tracing::error!(
                        "Failed to start message vacuum batch, skipping entire batch: {}",
                        e
                    );
                }
            }
        };

        tokio::select! {
            _ = worker => {},
            _ = shutdown_signal.cancelled() => {}
        }
    }

    /// Delete messages older than `messages_expire_after`.
    async fn run_message_vacuum(
        &self,
        vacuum_messages_every: Duration,
        messages_expire_after: Duration,
        max_buffer_size: usize,
    ) -> Result<(), StorageError> {
        let db_conn = self.get_db_conn().await?;

        let channels_with_messages: Vec<String> = db_conn
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
            VACUUM_RUNS.inc();

            let execute_result = db_conn
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
                    tracing::error!("Failed to vacuum channel {}: {}", channel, e);
                    continue;
                }
            };

            MESSAGES_VACUUMED.inc_by(messages_deleted);
            MESSAGES_STORED.add(-(messages_deleted as i64));
        }

        Ok(())
    }
}

#[cfg(test)]
pub mod test {
    #[test]
    pub fn dump_migrations() {
        dbg!(super::migrations::migrations::runner().get_migrations());
    }
}
