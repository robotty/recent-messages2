use crate::config::{AppConfig, Config};
use crate::web::auth::{TwitchUserAccessToken, UserAuthorization};
use chrono::{DateTime, Utc};
use deadpool_postgres::{ManagerConfig, PoolConfig, RecyclingMethod};
use itertools::Itertools;
use prometheus::{
    Histogram, HistogramVec, IntCounter, IntGauge, register_histogram, register_histogram_vec,
    register_int_counter, register_int_gauge,
};
use rustls::ClientConfig;
use rustls_platform_verifier::ConfigVerifierExt;
use std::ops::DerefMut;
use std::time::Duration;
use std::{collections::HashSet, sync::LazyLock};
use tokio::time::MissedTickBehavior;
use tokio_postgres::types::{ToSql, Type};
use tokio_postgres_rustls::MakeRustlsConnect;
use tokio_util::sync::CancellationToken;

static MESSAGES_APPENDED: LazyLock<IntCounter> = LazyLock::new(|| {
    register_int_counter!(
        "recentmessages_messages_appended",
        "Total number of messages appended to storage"
    )
    .unwrap()
});

const REFRESH_MESSAGES_STORED_EVERY: Duration = Duration::from_secs(15);

static MESSAGES_STORED: LazyLock<IntGauge> = LazyLock::new(|| {
    register_int_gauge!(
        "recentmessages_messages_stored",
        "Number of messages currently stored in storage"
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
static TIME_TAKEN_TO_POLL_METRICS: LazyLock<HistogramVec> = LazyLock::new(|| {
    register_histogram_vec!(
        "recentmessages_db_poll_metrics_time_seconds",
        "Time taken to poll various metrics from the database",
        &["metric"]
    )
    .unwrap()
});

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
    pub fn connect(config: &Config) -> DataStorage {
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

    pub async fn setup_retention_policy(&self, app_config: &AppConfig) -> Result<(), StorageError> {
        let mut db_conn = self.get_db_conn().await?;

        let tx = db_conn.0.transaction().await?;

        tx.execute(
            r"SELECT remove_retention_policy(
    relation => 'message',
    if_exists => true
)",
            &[],
        )
        .await?;

        tx.execute(
            r"SELECT add_retention_policy(
    relation => 'message',
    drop_after => make_interval(secs => $1),
    schedule_interval => make_interval(secs => $2)
)",
            &[
                &app_config.messages_expire_after.as_secs_f64(),
                &app_config.vacuum_messages_every.as_secs_f64(),
            ],
        )
        .await?;

        tx.commit().await?;

        Ok(())
    }

    pub async fn run_task_update_metrics_values(&self, shutdown_signal: CancellationToken) {
        let mut poll_interval = tokio::time::interval(REFRESH_MESSAGES_STORED_EVERY);
        poll_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

        let worker = async move {
            loop {
                poll_interval.tick().await;
                let res = self.update_metrics_values().await;

                if let Err(e) = res {
                    tracing::error!("Failed to refresh metrics: {}", e);
                }
            }
        };

        tokio::select! {
            _ = worker => {},
            () = shutdown_signal.cancelled() => {}
        }
    }

    pub async fn update_metrics_values(&self) -> Result<(), StorageError> {
        let db_conn = self.get_db_conn().await?;

        {
            let timer = TIME_TAKEN_TO_POLL_METRICS
                .with_label_values(&["messages_stored"])
                .start_timer();
            let statement = db_conn
                .0
                .prepare_typed_cached(
                    "SELECT approximate_row_count FROM approximate_row_count('message')",
                    &[],
                )
                .await?;
            let count: i64 = db_conn
                .0
                .query_one(&statement, &[])
                .await?
                .get("approximate_row_count");
            timer.observe_duration();
            MESSAGES_STORED.set(count);
        }

        Ok(())
    }

    pub async fn get_channel_logins_to_join(
        &self,
        channel_expiry: Duration,
    ) -> Result<HashSet<String>, StorageError> {
        let db_conn = self.get_db_conn().await?;

        // TODO figure out whether this has to be sped up using an index.
        let statement = db_conn
            .0
            .prepare_typed_cached(
                r"SELECT channel_login
FROM channel
WHERE ignored_at IS NULL
  AND last_access > now() - make_interval(secs => $1)
ORDER BY last_access DESC",
                &[Type::FLOAT8],
            )
            .await?;
        let rows = db_conn
            .0
            .query(&statement, &[&channel_expiry.as_secs_f64()])
            .await?;
        let channels = rows.into_iter().map(|row| row.get(0)).collect();

        Ok(channels)
    }

    pub async fn touch_or_add_channel(&self, channel_login: &str) -> Result<(), StorageError> {
        let db_conn = self.get_db_conn().await?;
        // this way we only update the last_access if it's been at least 30 minutes since
        // the last time the last_access was updated for that channel. For high traffic
        // channels this massively cuts down on the amount of writes the DB has to do
        let statement = db_conn
            .0
            .prepare_typed_cached(
                r"INSERT INTO channel (channel_login) VALUES ($1)
ON CONFLICT ON CONSTRAINT channel_pkey DO UPDATE
    SET last_access = now()
    WHERE channel.last_access < now() - INTERVAL '30 minutes'",
                &[Type::TEXT],
            )
            .await?;
        db_conn.0.execute(&statement, &[&channel_login]).await?;
        Ok(())
    }

    pub async fn is_channel_ignored(&self, channel_login: &str) -> Result<bool, StorageError> {
        let db_conn = self.get_db_conn().await?;
        let statement = db_conn
            .0
            .prepare_typed_cached(
                r"SELECT ignored_at IS NOT NULL FROM channel
WHERE channel_login = $1",
                &[Type::TEXT],
            )
            .await?;
        let rows = db_conn.0.query(&statement, &[&channel_login]).await?;
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
        let statement = db_conn
            .0
            .prepare_typed_cached(
                r"INSERT INTO channel (channel_login, ignored_at)
VALUES ($1, CASE WHEN $2 THEN now() ELSE NULL END)
ON CONFLICT ON CONSTRAINT channel_pkey DO UPDATE
    SET ignored_at = CASE WHEN $2 THEN now() ELSE NULL END",
                &[Type::TEXT, Type::BOOL],
            )
            .await?;
        db_conn
            .0
            .execute(&statement, &[&channel_login, &ignored])
            .await?;
        Ok(())
    }

    pub async fn append_user_authorization(
        &self,
        user_authorization: &UserAuthorization,
    ) -> Result<(), StorageError> {
        let db_conn = self.get_db_conn().await?;
        let statement = db_conn
            .0
            .prepare_typed_cached(
                "INSERT INTO user_authorization(access_token, twitch_access_token,
twitch_refresh_token, twitch_authorization_last_validated, valid_until, user_id,
user_login, user_name, user_profile_image_url)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
                &[
                    Type::TEXT,
                    Type::TEXT,
                    Type::TEXT,
                    Type::TIMESTAMPTZ,
                    Type::TIMESTAMPTZ,
                    Type::TEXT,
                    Type::TEXT,
                    Type::TEXT,
                    Type::TEXT,
                ],
            )
            .await?;
        db_conn
            .0
            .execute(
                &statement,
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
        let statement = db_conn
            .0
            .prepare_typed_cached(
                "SELECT access_token, twitch_access_token, twitch_refresh_token,
twitch_authorization_last_validated, valid_until, user_id,
user_login, user_name, user_profile_image_url
FROM user_authorization
WHERE access_token = $1
AND valid_until >= now()",
                &[Type::TEXT],
            )
            .await?;

        let rows = db_conn.0.query(&statement, &[&access_token]).await?;

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
        let statement = db_conn
            .0
            .prepare_typed_cached(
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
                    Type::TEXT,
                    Type::TEXT,
                    Type::TEXT,
                    Type::TIMESTAMPTZ,
                    Type::TIMESTAMPTZ,
                    Type::TEXT,
                    Type::TEXT,
                    Type::TEXT,
                    Type::TEXT,
                ],
            )
            .await?;
        db_conn
            .0
            .execute(
                &statement,
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
        let statement = db_conn
            .0
            .prepare_typed_cached(
                "DELETE FROM user_authorization WHERE access_token = $1",
                &[Type::TEXT],
            )
            .await?;
        db_conn.0.execute(&statement, &[&access_token]).await?;

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
        let db_conn = self.get_db_conn().await?;

        // limit: If specified, take the newest N messages.
        let limit = match limit {
            Some(limit) => usize::min(limit, max_buffer_size),
            None => max_buffer_size,
        };

        // The cast() below is to allow the PostgreSQL server to unambiguously detect the
        // type of $2 and $3. See: https://stackoverflow.com/a/64223435
        let statement = db_conn
            .0
            .prepare_typed_cached(
                "\
            SELECT time_received, message_source
            FROM message
            WHERE channel_login = $1
            AND   (cast($2 AS TIMESTAMP WITH TIME ZONE) IS NULL OR time_received < $2)
            AND   (cast($3 AS TIMESTAMP WITH TIME ZONE) IS NULL OR time_received > $3)
            ORDER BY time_received DESC
            LIMIT $4",
                &[Type::TEXT, Type::TIMESTAMPTZ, Type::TIMESTAMPTZ, Type::INT8],
            )
            .await?;

        Ok(db_conn
            .0
            .query(
                &statement,
                &[&channel_login, &before, &after, &(limit as i64)],
            )
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
        let db_conn = self.get_db_conn().await?;
        let statement = db_conn
            .0
            .prepare_typed_cached(
                "DELETE FROM message WHERE channel_login = $1",
                &[Type::TEXT],
            )
            .await?;
        let num_messages_deleted = db_conn.0.execute(&statement, &[&channel_login]).await?;
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
        let db_conn = self.get_db_conn().await?;
        let query = DataStorage::batch_message_insert_query(num_messages, 3);
        let types = DataStorage::batch_message_insert_types(num_messages);
        let statement = db_conn.0.prepare_typed_cached(&query, &types).await?;
        db_conn
            .0
            .execute(
                &statement,
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

    fn batch_message_insert_types(num_rows: usize) -> Vec<Type> {
        let mut types = Vec::with_capacity(num_rows * 3);
        for _ in 0..num_rows {
            types.push(Type::TEXT);
            types.push(Type::TIMESTAMPTZ);
            types.push(Type::TEXT);
        }
        types
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
}

#[cfg(test)]
pub mod test {
    #[test]
    pub fn dump_migrations() {
        dbg!(super::migrations::migrations::runner().get_migrations());
    }
}
