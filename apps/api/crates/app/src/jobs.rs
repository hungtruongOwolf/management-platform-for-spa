//! In-process background jobs. Each run is idempotent, so restarts and overlaps are harmless.

use sqlx::PgPool;
use std::time::Duration;

/// Hourly, and once at start (the first `interval` tick is immediate): drop idempotency records older
/// than 48 hours. On a scale-to-zero host (Vercel) instances rarely live an hour, so the start-up run
/// is what keeps the table small there.
pub fn spawn_idempotency_purge(pool: PgPool) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(60 * 60));
        loop {
            tick.tick().await;
            match purge_idempotency(&pool).await {
                Ok(count) => tracing::info!(count, "purged idempotency records"),
                Err(error) => tracing::error!(%error, "idempotency purge failed"),
            }
        }
    })
}

pub async fn purge_idempotency(pool: &PgPool) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT app.purge_idempotency_records(interval '48 hours') AS "purged!""#)
        .fetch_one(pool)
        .await
}
