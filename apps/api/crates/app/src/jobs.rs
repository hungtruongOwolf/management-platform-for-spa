//! In-process background jobs. Each run is idempotent, so restarts and overlaps are harmless.
//!
//! The loop wakes up at start and every hour; each job runs only when no instance ran it within
//! its interval (`app.claim_maintenance`, 0035). On a scale-to-zero host (Vercel) many instances
//! start and stop: the claim keeps the purge hourly and the checks daily for the whole fleet
//! instead of once per instance start (db review F10).

use sqlx::PgPool;
use std::time::Duration;

/// Idempotency records older than this are dropped: well past any client retry.
pub const IDEMPOTENCY_RETENTION: &str = "48 hours";
/// Sessions revoked or expired longer ago than this are deleted (EM 2026-11-09, Q-DB4).
pub const SESSION_RETENTION: &str = "30 days";
const PURGE_EVERY: &str = "55 minutes";
const CHECKS_EVERY: &str = "23 hours";

/// What one maintenance pass did; `None` = not due (another instance ran it recently).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct MaintenanceReport {
    pub idempotency_purged: Option<i64>,
    pub sessions_purged: Option<i64>,
    /// Checks of `app.invariant_violations` that found rows breaking them: (check, rows).
    pub violations: Option<Vec<(String, i64)>>,
}

pub fn spawn_maintenance(pool: PgPool) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(60 * 60));
        loop {
            tick.tick().await;
            if let Err(error) = run_maintenance(&pool).await {
                tracing::error!(%error, "maintenance failed");
            }
        }
    })
}

/// One pass: the purges when due (hourly), the data checks when due (daily). A check that finds
/// drift is logged at ERROR with its name and row count: the signal the hosting logs alert on.
pub async fn run_maintenance(pool: &PgPool) -> Result<MaintenanceReport, sqlx::Error> {
    let mut report = MaintenanceReport::default();
    if claim(pool, "purge", PURGE_EVERY).await? {
        let idempotency = purge_idempotency(pool).await?;
        let sessions = purge_sessions(pool).await?;
        tracing::info!(idempotency, sessions, "purged expired records");
        report.idempotency_purged = Some(idempotency);
        report.sessions_purged = Some(sessions);
    }
    if claim(pool, "invariants", CHECKS_EVERY).await? {
        let broken = invariant_violations(pool).await?;
        for (check, rows) in &broken {
            tracing::error!(check = %check, rows, "data invariant violated");
        }
        if broken.is_empty() {
            tracing::info!("data invariants hold");
        }
        report.violations = Some(broken);
    }
    Ok(report)
}

async fn claim(pool: &PgPool, job: &str, every: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT app.claim_maintenance($1, $2::text::interval) AS "due!""#,
        job,
        every
    )
    .fetch_one(pool)
    .await
}

pub async fn purge_idempotency(pool: &PgPool) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT app.purge_idempotency_records($1::text::interval) AS "purged!""#,
        IDEMPOTENCY_RETENTION
    )
    .fetch_one(pool)
    .await
}

pub async fn purge_sessions(pool: &PgPool) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT app.purge_sessions($1::text::interval) AS "purged!""#,
        SESSION_RETENTION
    )
    .fetch_one(pool)
    .await
}

/// The checks that found drift (rows > 0), by name.
pub async fn invariant_violations(pool: &PgPool) -> Result<Vec<(String, i64)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT check_name AS "check!", violations AS "rows!"
           FROM app.invariant_violations() WHERE violations > 0 ORDER BY check_name"#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| (r.check, r.rows)).collect())
}
