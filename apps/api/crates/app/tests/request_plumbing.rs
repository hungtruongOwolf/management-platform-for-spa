//! Per-request plumbing made cheaper by the perf work (perf page §3.4, §4d): the tenant
//! transaction opens in one round trip with the same RLS settings, the permission lookup is one
//! statement, and every response reports its server time.

mod common;

use axum::http::StatusCode;
use common::{PASSWORD, Reply, TestResult, add_member, call, harness, login};
use identity::SystemRole;
use kernel::TenantTx;
use kernel::testing::{app_pool, seed_tenant};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::layer::SubscriberExt;

const MANAGER_PHONE: &str = "0912000444";

async fn settings(conn: &mut sqlx::PgConnection) -> Result<(String, String), sqlx::Error> {
    sqlx::query_as::<_, (String, String)>(
        "SELECT coalesce(current_setting('app.tenant_id', true), ''), current_setting('statement_timeout')",
    )
    .fetch_one(conn)
    .await
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn tenant_settings_last_exactly_one_transaction(owner: PgPool) -> TestResult {
    let tenant = seed_tenant(&owner, "spa-a").await?;
    // One connection, so the second transaction reuses the first one's session.
    let options = (*app_pool(&owner).await?.connect_options()).clone();
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;

    let mut tx = TenantTx::begin(&pool, tenant).await?;
    assert_eq!(
        settings(tx.conn()).await?,
        (tenant.to_string(), "5s".to_owned())
    );
    tx.commit().await?;

    let mut conn = pool.acquire().await?;
    let (after_tenant, after_timeout) = settings(&mut conn).await?;
    assert_eq!(
        after_tenant, "",
        "app.tenant_id never outlives its transaction"
    );
    assert_ne!(after_timeout, "5s");
    drop(conn);

    // A rolled-back transaction leaves nothing behind either.
    let mut tx = TenantTx::begin(&pool, tenant).await?;
    let _ = sqlx::query("SELECT 1/0").execute(tx.conn()).await;
    drop(tx);
    let mut conn = pool.acquire().await?;
    assert_eq!(settings(&mut conn).await?.0, "");
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn rls_still_hides_other_spas_inside_a_tenant_transaction(owner: PgPool) -> TestResult {
    let a = seed_tenant(&owner, "spa-a").await?;
    let _b = seed_tenant(&owner, "spa-b").await?;
    let pool = app_pool(&owner).await?;
    let mut tx = TenantTx::begin(&pool, a).await?;
    let seen: Vec<uuid::Uuid> = sqlx::query_scalar("SELECT id FROM app.tenant")
        .fetch_all(tx.conn())
        .await?;
    assert_eq!(
        seen,
        vec![a.as_uuid()],
        "RLS keys on the setting made by the BEGIN statement"
    );
    tx.commit().await?;
    Ok(())
}

fn timing(reply: &Reply) -> Option<String> {
    reply
        .headers
        .get("server-timing")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn responses_report_db_time_statement_count_and_total(pool: PgPool) -> TestResult {
    // The same per-layer setup as `main`: only the timing layer listens to sqlx statements.
    let subscriber =
        tracing_subscriber::registry().with(app::server_timing::DbTimingLayer::filtered());
    let _guard = tracing::subscriber::set_default(subscriber);

    let h = harness(pool).await?;
    add_member(
        &h.owner,
        "vpyd",
        MANAGER_PHONE,
        "Quản lý Ngọc",
        SystemRole::Manager,
    )
    .await?;
    let manager = login(&h.app, MANAGER_PHONE, PASSWORD).await;
    assert_eq!(manager.status, StatusCode::OK);
    assert_eq!(
        timing(&manager),
        None,
        "never on /auth/*: a statement count would tell accounts apart"
    );
    let session = manager.session().unwrap_or_default();

    let reply = call(&h.app, "GET", "/staff", Some(&session), None).await;
    assert_eq!(reply.status, StatusCode::OK);
    let header = timing(&reply).ok_or("no Server-Timing")?;
    let statements: u32 = header
        .split("desc=\"")
        .nth(1)
        .and_then(|rest| rest.split(' ').next())
        .and_then(|n| n.parse().ok())
        .ok_or_else(|| format!("no statement count in {header}"))?;
    assert!(header.starts_with("db;dur="), "{header}");
    assert!(header.contains(", total;dur="), "{header}");
    // Session lookup (+ touch), auth transaction (BEGIN+settings, permissions, COMMIT) and the
    // use case's own transaction: a handful, never zero, never the old 10+ for a plain list.
    assert!((3..=9).contains(&statements), "{header}");

    let anonymous = call(&h.app, "GET", "/staff", None, None).await;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
    assert!(
        timing(&anonymous).is_some_and(|t| t.contains("desc=\"0 statements\"")),
        "{:?}",
        timing(&anonymous)
    );
    Ok(())
}
