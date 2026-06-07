use crate::config::Config;
use crate::web::auth::{TwitchUserAccessToken, UserAuthorization};
use chrono::{DateTime, Utc};
use deadpool_postgres::{ManagerConfig, PoolConfig, RecyclingMethod};
use itertools::Itertools;
use prometheus::{Histogram, IntCounter, IntGauge, register_histogram, register_int_counter, register_int_gauge};
use rustls::ClientConfig;
use rustls_platform_verifier::ConfigVerifierExt;
use std::ops::DerefMut;
use std::time::Duration;
use std::{collections::HashSet, sync::LazyLock};
use tokio::time::MissedTickBehavior;
use tokio_postgres::types::ToSql;
use tokio_postgres_rustls::MakeRustlsConnect;
use tokio_util::sync::CancellationToken;

static MESSAGES_APPENDED: LazyLock<IntCounter> = LazyLock::new(|| {
    register_int_counter!(
        "recentmessages_messages_appended",
        "Total number of messages appended to storage"
    )
    .unwrap()
});

static MESSAGES_STORED: LazyLock<IntGauge> = LazyLock::new(|| {
    register_int_gauge!(
        "recentmessages_messages_stored",
        "Number of messages currently stored in storage"
    )
    .unwrap()
});
static MESSAGES_VACUUMED: LazyLock<IntCounter> = LazyLock::new(|| {
    register_int_counter!(
        "recentmessages_messages_vacuumed",
        "Total number of messages that were removed by the automatic vacuum runner"
    )
    .unwrap()
});
static VACUUM_RUNS: LazyLock<IntCounter> = LazyLock::new(|| {
    register_int_counter!(
        "recentmessages_message_vacuum_runs",
        "Total number of times the automatic vacuum runner has been started for a certain channel"
    )
    .unwrap()
});
static DB_CONNECTIONS_IN_USE: LazyLock<IntGauge> = LazyLock::new(|| {
    register_int_gauge!(
        "recentmessages_db_pool_connections_in_use",
        "Number of database connections currently in use"
    )
    .unwrap()
});
static DB_CONNECTIONS_MAX: LazyLock<IntGauge> = LazyLock::new(|| {
    register_int_gauge!(
        "recentmessages_db_pool_connections_max",
        "Configured maximum size of the database connection pool"
    )
    .unwrap()
});
static TIME_TAKEN_TO_GET_DB_CONN: LazyLock<Histogram> = LazyLock::new(|| {
    register_histogram!(
        "recentmessages_db_pool_retrieval_time_seconds",
        "Time taken to retrieve a DB connection from the database pool"
    )
    .unwrap()
});

pub fn connect_to_postgresql(config: &Config) -> DataStorage {
    let pg_config = tokio_postgres::Config::from(config.db.clone());
    tracing::debug!("PostgreSQL config: {:#?}", pg_config);

    let mgr_config = ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    };
    let pool_config = PoolConfig {
        max_size: config.db.pool.max_size,
        timeouts: deadpool_postgres::Timeouts::from(config.db.pool),
        ..Default::default()
    };
    DB_CONNECTIONS_MAX.set(config.db.pool.max_size as i64);
    DB_CONNECTIONS_IN_USE.set(0);

    let tls_config = ClientConfig::with_platform_verifier().unwrap();
    let tls = MakeRustlsConnect::new(tls_config);

    let manager = deadpool_postgres::Manager::from_config(pg_config, tls, mgr_config);
    let db_pool = deadpool_postgres::Pool::builder(manager)
        .config(pool_config)
        .runtime(deadpool_postgres::Runtime::Tokio1)
        .build()
        .unwrap();

    DataStorage { db: db_pool }
}

mod migrations {
    use refinery::embed_migrations;
    // refers to the "migrations" directory in the project root
    embed_migrations!("migrations");
}

pub type StorageError = deadpool_postgres::PoolError;

#[derive(Debug, Clone)]
pub struct StoredMessage {
    pub time_received: DateTime<Utc>,
    pub message_source: String,
}

#[derive(Clone)]
pub struct DataStorage {
    db: deadpool_postgres::Pool,
}

struct WrappedDbConn(deadpool_postgres::Object);

impl WrappedDbConn {
    pub fn new(inner: deadpool_postgres::Object) -> WrappedDbConn {
        DB_CONNECTIONS_IN_USE.inc();
        WrappedDbConn(inner)
    }
}

impl Drop for WrappedDbConn {
    fn drop(&mut self) {
        DB_CONNECTIONS_IN_USE.dec();
    }
}

impl DataStorage {
    async fn get_db_conn(&self) -> Result<WrappedDbConn, StorageError> {
        let timer = TIME_TAKEN_TO_GET_DB_CONN.start_timer();
        let db_conn = self.db.get().await;
        timer.observe_duration();
        Ok(WrappedDbConn::new(db_conn?))
    }

    pub async fn run_migrations(&self) -> Result<(), Box<dyn std::error::Error>> {
        migrations::migrations::runner()
            .run_async(self.get_db_conn().await?.0.as_mut().deref_mut())
            .await?;
        Ok(())
    }

    pub async fn fetch_initial_metrics_values(&self) -> Result<(), StorageError> {
        let count: i64 = self
            .get_db_conn()
            .await?
            .0
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
        let db_conn = self.get_db_conn().await?;
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
        let db_conn = self.get_db_conn().await?;
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
        Ok(rows.first().is_some_and(|row| row.get(0)))
    }

    pub async fn set_channel_ignored(
        &self,
        channel_login: &str,
        ignored: bool,
    ) -> Result<(), StorageError> {
        let db_conn = self.get_db_conn().await?;
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
        let db_conn = self.get_db_conn().await?;

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
        let db_conn = self.get_db_conn().await?;

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

        if let Some(row) = rows.first() {
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
        let db_conn = self.get_db_conn().await?;

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
        before: Option<DateTime<Utc>>,
        after: Option<DateTime<Utc>>,
        max_buffer_size: usize,
    ) -> Result<Vec<StoredMessage>, StorageError> {
        // limit: If specified, take the newest N messages.
        let db_conn = self.get_db_conn().await?;

        let limit = match limit {
            Some(limit) => usize::min(limit, max_buffer_size),
            None => max_buffer_size,
        };

        // The cast() below is to allow the PostgreSQL server to unambiguously detect the
        // type of $2 and $3. See: https://stackoverflow.com/a/64223435
        let query = "\
            SELECT time_received, message_source
            FROM message
            WHERE channel_login = $1
            AND   (cast($2 AS TIMESTAMP WITH TIME ZONE) IS NULL OR time_received < $2)
            AND   (cast($3 AS TIMESTAMP WITH TIME ZONE) IS NULL OR time_received > $3)
            ORDER BY time_received DESC
            LIMIT $4";

        Ok(db_conn
            .0
            .query(query, &[&channel_login, &before, &after, &(limit as i64)])
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
        let num_messages_deleted = self
            .get_db_conn()
            .await?
            .0
            .execute(
                "DELETE FROM message WHERE channel_login = $1",
                &[&channel_login],
            )
            .await?;
        MESSAGES_STORED.sub(num_messages_deleted as i64);
        Ok(())
    }

    /// Append a message to the storage.
    pub async fn append_messages(
        &self,
        messages: &[(String, DateTime<Utc>, String)],
    ) -> Result<(), StorageError> {
        if messages.is_empty() {
            return Ok(());
        }
        let num_messages = messages.len();
        self.get_db_conn()
            .await?
            .0
            .execute(
                &DataStorage::batch_message_insert_query(num_messages, 3),
                DataStorage::batch_message_insert_values(messages).as_slice(),
            )
            .await?;
        MESSAGES_APPENDED.inc_by(num_messages as u64);
        MESSAGES_STORED.add(num_messages as i64);

        Ok(())
    }

    fn batch_message_insert_values(
        rows: &[(String, DateTime<Utc>, String)],
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
            buf.push('(');
            for j in 0..num_columns {
                buf.push_str(format!("${}", i * num_columns + j + 1).as_str());
                if j != num_columns - 1 {
                    buf.push_str(", ");
                }
            }
            buf.push(')');
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
            () = shutdown_signal.cancelled() => {}
        }
    }

    /// Delete messages older than `messages_expire_after` and messages that go beyond the
    /// maximum buffer size.
    async fn run_message_vacuum(
        &self,
        vacuum_messages_every: Duration,
        messages_expire_after: Duration,
        max_buffer_size: usize,
    ) -> Result<(), StorageError> {
        let db_conn = self.get_db_conn().await?;

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
            VACUUM_RUNS.inc();

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
                    tracing::error!("Failed to vacuum channel {}: {}", channel, e);
                    continue;
                }
            };

            MESSAGES_VACUUMED.inc_by(messages_deleted);
            MESSAGES_STORED.sub(messages_deleted as i64);
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
