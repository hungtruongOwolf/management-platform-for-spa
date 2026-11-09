//! Housekeeping of the db review (2026-11): sessions purged 30 days after they end (C1), the daily
//! data checks (C2), one run per interval for the whole fleet (F10), the idle-in-transaction bound
//! and the FKs of the "who did it" columns (C4). Migrations 0035–0037.

mod common;

use app::jobs::{self, MaintenanceReport};
use axum::http::StatusCode;
use common::{TestResult, add_member, call, harness, session_for};
use identity::SystemRole;
use kernel::TenantTx;
use kernel::testing::app_pool;
use serde_json::json;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

const MANAGER: &str = "0912000777";

async fn owner_user(owner: &PgPool) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM app.user_account ORDER BY created_at LIMIT 1")
        .fetch_one(owner)
        .await
}

/// A session row ending (revoked or idle-expired) `days_ago`; `None` = still live.
async fn session(
    owner: &PgPool,
    user: Uuid,
    revoked_days_ago: Option<i32>,
    idle_days_ago: Option<i32>,
) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO app.session (id, token_hash, user_id, idle_expires_at, absolute_expires_at, recent_auth_at, revoked_at)
         VALUES ($1, sha256($1::text::bytea), $2,
                 CASE WHEN $4::int IS NULL THEN now() + interval '1 hour' ELSE now() - make_interval(days => $4) END,
                 CASE WHEN $4::int IS NULL THEN now() + interval '1 day' ELSE now() - make_interval(days => $4) END,
                 now() - interval '40 days',
                 CASE WHEN $3::int IS NULL THEN NULL ELSE now() - make_interval(days => $3) END)",
    )
    .bind(id)
    .bind(user)
    .bind(revoked_days_ago)
    .bind(idle_days_ago)
    .execute(owner)
    .await?;
    Ok(id)
}

#[sqlx::test(migrations = "../../migrations")]
async fn sessions_ended_over_30_days_ago_are_purged_and_only_those(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    let user = owner_user(&h.owner).await?;
    let old_revoked = session(&h.owner, user, Some(31), None).await?;
    let old_expired = session(&h.owner, user, None, Some(31)).await?;
    let recent_revoked = session(&h.owner, user, Some(2), None).await?;
    let recent_expired = session(&h.owner, user, None, Some(2)).await?;
    let live = session(&h.owner, user, None, None).await?;
    let app = app_pool(&h.owner).await?;

    let denied = sqlx::query("DELETE FROM app.session").execute(&app).await;
    assert!(denied.is_err(), "app_rw has no DELETE on app.session");

    assert_eq!(jobs::purge_sessions(&app).await?, 2);
    let left: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM app.session ORDER BY id")
        .fetch_all(&h.owner)
        .await?;
    for gone in [old_revoked, old_expired] {
        assert!(!left.contains(&gone), "{gone} purged");
    }
    for kept in [recent_revoked, recent_expired, live] {
        assert!(left.contains(&kept), "{kept} kept");
    }
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn each_job_runs_once_per_interval_for_the_whole_fleet(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    let app = app_pool(&h.owner).await?;
    let first = jobs::run_maintenance(&app).await?;
    assert_eq!(first.idempotency_purged, Some(0));
    assert_eq!(first.sessions_purged, Some(0));
    assert_eq!(first.violations, Some(vec![]), "a fresh spa is consistent");
    // Another instance starting right after: nothing is due.
    assert_eq!(
        jobs::run_maintenance(&app).await?,
        MaintenanceReport::default()
    );
    // An hour later the purge is due again, the daily checks are not.
    sqlx::query("UPDATE app.maintenance_run SET last_run_at = now() - interval '1 hour'")
        .execute(&h.owner)
        .await?;
    let later = jobs::run_maintenance(&app).await?;
    assert!(later.sessions_purged.is_some());
    assert_eq!(later.violations, None);
    Ok(())
}

/// The daily check runs across every spa (owner function, no tenant set) and reports drift by
/// check name and row count only.
#[sqlx::test(migrations = "../../migrations")]
async fn the_daily_check_reports_a_drifted_copy(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    add_member(&h.owner, "vpyd", MANAGER, "Quản lý", SystemRole::Manager).await?;
    let manager = session_for(&h, MANAGER).await;
    let created = call(
        &h.app,
        "POST",
        "/customers",
        Some(&manager),
        Some(json!({"full_name": "Nguyễn Thị Hà", "phone": "0905111222", "gender": "female"})),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    let app = app_pool(&h.owner).await?;
    assert_eq!(jobs::invariant_violations(&app).await?, vec![]);

    sqlx::query("UPDATE app.customer SET search_text = 'lech'")
        .execute(&h.owner)
        .await?;
    assert_eq!(
        jobs::invariant_violations(&app).await?,
        vec![("customer_search_text".to_owned(), 1)]
    );
    let checks: i64 = sqlx::query_scalar("SELECT count(*) FROM app.invariant_violations()")
        .fetch_one(&app)
        .await?;
    assert_eq!(checks, 11, "every check reports, drift or not");
    Ok(())
}

async fn idle(conn: &mut sqlx::PgConnection) -> Result<String, sqlx::Error> {
    sqlx::query_scalar("SELECT current_setting('idle_in_transaction_session_timeout')")
        .fetch_one(conn)
        .await
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_tenant_transaction_bounds_its_idle_time(owner: PgPool) -> TestResult {
    let tenant = kernel::testing::seed_tenant(&owner, "spa-a").await?;
    let options = (*app_pool(&owner).await?.connect_options()).clone();
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    let mut tx = TenantTx::begin(&pool, tenant).await?;
    assert_eq!(idle(tx.conn()).await?, kernel::db::IDLE_IN_TRANSACTION);
    tx.commit().await?;
    let mut conn = pool.acquire().await?;
    assert_ne!(
        idle(&mut conn).await?,
        kernel::db::IDLE_IN_TRANSACTION,
        "transaction-local: the next user of the connection is not affected"
    );
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn who_did_it_columns_reference_a_real_account(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    let unvalidated: Vec<String> = sqlx::query_scalar(
        "SELECT conname FROM pg_constraint
         WHERE conname = ANY($1) AND (NOT convalidated OR confrelid <> 'app.user_account'::regclass)",
    )
    .bind(vec![
        "visit_created_by_fkey",
        "visit_voided_by_fkey",
        "sale_created_by_fkey",
        "sale_voided_by_fkey",
        "payment_created_by_fkey",
        "refund_created_by_fkey",
        "customer_package_created_by_fkey",
        "customer_package_cancelled_by_fkey",
        "appointment_created_by_fkey",
        "appointment_closed_by_fkey",
    ])
    .fetch_all(&h.owner)
    .await?;
    assert_eq!(
        unvalidated,
        Vec::<String>::new(),
        "all ten exist, validated"
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_constraint WHERE contype = 'f' AND conname LIKE '%\\_by\\_fkey'
         AND conrelid IN ('app.visit'::regclass, 'app.sale'::regclass, 'app.payment'::regclass,
                          'app.refund'::regclass, 'app.customer_package'::regclass, 'app.appointment'::regclass)",
    )
    .fetch_one(&h.owner)
    .await?;
    assert_eq!(count, 10);

    let book = |created_by: Uuid| {
        let owner = h.owner.clone();
        async move {
            sqlx::query(
                "INSERT INTO app.appointment (id, tenant_id, branch_id, guest_name, guest_phone, business_date,
                                              starts_at, ends_at, created_by)
                 SELECT $1, b.tenant_id, b.id, 'Khách mới', '+84905000999', current_date + 1,
                        now() + interval '1 day', now() + interval '1 day 1 hour', $2
                 FROM app.branch b LIMIT 1",
            )
            .bind(Uuid::now_v7())
            .bind(created_by)
            .execute(&owner)
            .await
        }
    };
    let nobody = book(Uuid::now_v7()).await;
    assert!(
        nobody
            .as_ref()
            .err()
            .and_then(|e| e.as_database_error())
            .is_some_and(|e| e.is_foreign_key_violation()),
        "{nobody:?}"
    );
    assert!(book(owner_user(&h.owner).await?).await.is_ok());
    Ok(())
}
