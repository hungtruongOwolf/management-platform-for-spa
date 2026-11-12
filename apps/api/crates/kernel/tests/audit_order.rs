//! When an audit entry says it happened (ADR-0023, spec record-history K60): the order of a
//! record's history is the order its changes were made, also when two requests race on it.
//! Real Postgres as `app_rw` with RLS, like `tenant_isolation.rs`.

use chrono::{DateTime, TimeZone, Utc};
use kernel::audit::{self, Action, AuditEntry};
use kernel::testing::{app_pool, seed_tenant};
use kernel::{Actor, Permission, TenantId, TenantTx, UserId};
use sqlx::PgPool;
use uuid::Uuid;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn actor(tenant: TenantId) -> Actor {
    Actor::new(UserId::new(), tenant, [Permission::CatalogManage], None)
}

/// An entry saying the service group `id` was renamed to `name` (it stands for any record).
fn renamed(id: Uuid, name: &str) -> AuditEntry<'static> {
    AuditEntry {
        action: Action::CatalogCategoryUpdate,
        entity_type: "service_category",
        entity_id: id,
        before: None,
        after: Some(serde_json::json!({ "name": name })),
        reason: None,
    }
}

async fn new_category(pool: &PgPool, tenant: TenantId) -> Result<Uuid, Box<dyn std::error::Error>> {
    let id = Uuid::now_v7();
    let mut tx = TenantTx::begin(pool, tenant).await?;
    sqlx::query("INSERT INTO app.service_category (id, tenant_id, name) VALUES ($1, $2, 'A')")
        .bind(id)
        .bind(tenant.as_uuid())
        .execute(tx.conn())
        .await?;
    tx.commit().await?;
    Ok(id)
}

async fn rename(tx: &mut TenantTx, id: Uuid, name: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE app.service_category SET name = $2 WHERE id = $1")
        .bind(id)
        .bind(name)
        .execute(tx.conn())
        .await
        .map(|_| ())
}

/// The history of `id` as every reader orders it: `(at, id)`.
async fn history(owner: &PgPool, id: Uuid) -> Result<Vec<(String, DateTime<Utc>)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT after ->> 'name', at FROM app.audit_log WHERE entity_id = $1 ORDER BY at, id",
    )
    .bind(id)
    .fetch_all(owner)
    .await
}

/// The race of the bug: request 1 begins first, then waits on the row lock that request 2 holds
/// while it renames the same record; request 2 commits, then request 1's rename goes through. The
/// history must read A → B (2) → C (1), as the record went. Stamped with the transaction's start
/// (`now()`, before 0053 / ADR-0023), request 1's entry sorted first: A → C → B.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_request_that_waited_on_a_lock_sorts_after_the_one_it_waited_for(
    owner: PgPool,
) -> TestResult {
    let tenant = seed_tenant(&owner, "spa-a").await?;
    let pool = app_pool(&owner).await?;
    let who = actor(tenant);
    let category = new_category(&pool, tenant).await?;

    let mut first = TenantTx::for_actor(&pool, &who).await?;
    let first_began: DateTime<Utc> = sqlx::query_scalar("SELECT now()")
        .fetch_one(first.conn())
        .await?;

    let mut second = TenantTx::for_actor(&pool, &who).await?;
    rename(&mut second, category, "B").await?;
    audit::record(&mut second, &who, renamed(category, "B")).await?;

    // Request 1 now blocks on request 2's row lock.
    let waiting = {
        let who = who.clone();
        tokio::spawn(async move {
            rename(&mut first, category, "C").await?;
            audit::record(&mut first, &who, renamed(category, "C")).await?;
            first.commit().await?;
            Ok::<(), kernel::AppError>(())
        })
    };
    // Polled as app_rw: a role always sees its own sessions' wait events.
    let mut blocked = false;
    for _ in 0..2_000 {
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_stat_activity
             WHERE datname = current_database() AND wait_event_type = 'Lock'",
        )
        .fetch_one(&pool)
        .await?;
        if n > 0 {
            blocked = true;
            break;
        }
    }
    assert!(blocked, "request 1 never waited on request 2's lock");
    second.commit().await?;
    waiting.await??;

    let rows = history(&owner, category).await?;
    let names: Vec<&str> = rows.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["B", "C"], "{rows:?}");
    assert!(
        rows[1].1 > rows[0].1,
        "request 1's entry is later than request 2's: {rows:?}"
    );
    assert!(
        rows[1].1 > first_began,
        "stamped when written, not when request 1 began"
    );
    Ok(())
}

/// Every entry of one transaction carries one instant (the readers fold them into one line), in
/// the order recorded, also around a write a reply asked for midway (`write_audit`); none is
/// stamped with the transaction's start. A transaction that never commits leaves nothing.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn one_transaction_writes_one_instant_in_recorded_order(owner: PgPool) -> TestResult {
    let tenant = seed_tenant(&owner, "spa-a").await?;
    let pool = app_pool(&owner).await?;
    let who = actor(tenant);
    let category = new_category(&pool, tenant).await?;

    let mut tx = TenantTx::for_actor(&pool, &who).await?;
    let began: DateTime<Utc> = sqlx::query_scalar("SELECT now()")
        .fetch_one(tx.conn())
        .await?;
    for name in ["B", "C"] {
        audit::record(&mut tx, &who, renamed(category, name)).await?;
    }
    let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM app.audit_log")
        .fetch_one(tx.conn())
        .await?;
    assert_eq!(pending, 0, "recorded entries wait for the transaction's writes");
    tx.write_audit().await?;
    for name in ["D", "E"] {
        audit::record(&mut tx, &who, renamed(category, name)).await?;
    }
    tx.commit().await?;

    let rows = history(&owner, category).await?;
    let names: Vec<&str> = rows.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["B", "C", "D", "E"]);
    assert!(rows.iter().all(|(_, at)| *at == rows[0].1), "{rows:?}");
    assert!(rows[0].1 > began, "not the transaction's start");

    let mut dropped = TenantTx::for_actor(&pool, &who).await?;
    audit::record(&mut dropped, &who, renamed(category, "F")).await?;
    dropped.write_audit().await?;
    drop(dropped);
    assert_eq!(history(&owner, category).await?.len(), 4, "rolled back");
    Ok(())
}

/// Back-fill (`app seed-demo`): an actor carrying the scenario's instant stamps every entry with
/// it — the only clock that does.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_back_filling_actor_stamps_its_own_instant(owner: PgPool) -> TestResult {
    let tenant = seed_tenant(&owner, "spa-a").await?;
    let pool = app_pool(&owner).await?;
    let at = Utc.with_ymd_and_hms(2026, 3, 2, 3, 30, 0).single().ok_or("date")?;
    let who = actor(tenant).at_clock(at);
    let category = new_category(&pool, tenant).await?;

    let mut tx = TenantTx::for_actor(&pool, &who).await?;
    audit::record(&mut tx, &who, renamed(category, "B")).await?;
    tx.write_audit().await?;
    audit::record(&mut tx, &who, renamed(category, "C")).await?;
    tx.commit().await?;

    let rows = history(&owner, category).await?;
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|(_, t)| *t == at), "{rows:?}");
    Ok(())
}
