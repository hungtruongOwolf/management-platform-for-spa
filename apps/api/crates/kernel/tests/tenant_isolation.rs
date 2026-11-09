//! Integration tests against real Postgres, connected as the runtime role `app_rw`.
//! Run with DATABASE_URL pointing at the dev/CI server (schema owner); each test gets a fresh database.

use kernel::audit::{self, AuditEntry};
use kernel::idempotency::{self, Claim};
use kernel::testing::{app_pool, seed_tenant};
use kernel::{Actor, AppError, Permission, TenantId, TenantTx, UserId};
use sqlx::PgPool;
use uuid::Uuid;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn actor(tenant: TenantId) -> Actor {
    Actor::new(UserId::new(), tenant, [Permission::CatalogRead], None)
}

async fn insert_branch(tx: &mut TenantTx, tenant: TenantId, name: &str) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO app.branch (id, tenant_id, name) VALUES ($1, $2, $3)")
        .bind(Uuid::now_v7())
        .bind(tenant.as_uuid())
        .bind(name)
        .execute(tx.conn())
        .await
        .map(|_| ())
}

async fn count_branches(tx: &mut TenantTx) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT count(*) FROM app.branch")
        .fetch_one(tx.conn())
        .await
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_tenant_only_sees_its_own_rows(owner: PgPool) -> TestResult {
    let (a, b) = (
        seed_tenant(&owner, "spa-a").await?,
        seed_tenant(&owner, "spa-b").await?,
    );
    let pool = app_pool(&owner).await?;

    let mut tx = TenantTx::begin(&pool, a).await?;
    insert_branch(&mut tx, a, "Đông Hà").await?;
    tx.commit().await?;

    let mut tx = TenantTx::begin(&pool, a).await?;
    assert_eq!(count_branches(&mut tx).await?, 1);
    drop(tx);

    let mut tx = TenantTx::begin(&pool, b).await?;
    assert_eq!(
        count_branches(&mut tx).await?,
        0,
        "tenant B must not see tenant A's branch"
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn writing_another_tenants_id_is_rejected_by_rls(owner: PgPool) -> TestResult {
    let (a, b) = (
        seed_tenant(&owner, "spa-a").await?,
        seed_tenant(&owner, "spa-b").await?,
    );
    let pool = app_pool(&owner).await?;

    let mut tx = TenantTx::begin(&pool, b).await?;
    let err = insert_branch(&mut tx, a, "evil").await.err();
    let message = err.map(|e| e.to_string()).unwrap_or_default();
    assert!(
        message.contains("row-level security"),
        "unexpected: {message}"
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn without_tenant_context_nothing_is_visible(owner: PgPool) -> TestResult {
    let a = seed_tenant(&owner, "spa-a").await?;
    let pool = app_pool(&owner).await?;
    let mut tx = TenantTx::begin(&pool, a).await?;
    insert_branch(&mut tx, a, "Đông Hà").await?;
    tx.commit().await?;

    let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM app.branch")
        .fetch_one(&pool)
        .await?;
    let tenants: i64 = sqlx::query_scalar("SELECT count(*) FROM app.tenant")
        .fetch_one(&pool)
        .await?;
    assert_eq!((visible, tenants), (0, 0));
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn transaction_bounds_statement_time(owner: PgPool) -> TestResult {
    let a = seed_tenant(&owner, "spa-a").await?;
    let pool = app_pool(&owner).await?;
    let mut tx = TenantTx::begin(&pool, a).await?;
    let timeout: String = sqlx::query_scalar("SHOW statement_timeout")
        .fetch_one(tx.conn())
        .await?;
    assert_eq!(timeout, "5s");
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn audit_log_is_append_only(owner: PgPool) -> TestResult {
    let a = seed_tenant(&owner, "spa-a").await?;
    let pool = app_pool(&owner).await?;
    let who = actor(a);

    let mut tx = TenantTx::begin(&pool, a).await?;
    let entry = AuditEntry {
        action: "branch.create",
        entity_type: "branch",
        entity_id: Uuid::now_v7(),
        before: None,
        after: Some(serde_json::json!({"name": "Đông Hà"})),
        reason: None,
    };
    audit::record(&mut tx, &who, entry).await?;
    tx.commit().await?;

    let mut tx = TenantTx::begin(&pool, a).await?;
    let update = sqlx::query("UPDATE app.audit_log SET reason = 'tampered'")
        .execute(tx.conn())
        .await;
    assert!(
        update
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default()
            .contains("permission denied")
    );
    drop(tx);

    let mut tx = TenantTx::begin(&pool, a).await?;
    let delete = sqlx::query("DELETE FROM app.audit_log")
        .execute(tx.conn())
        .await;
    assert!(
        delete
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default()
            .contains("permission denied")
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn idempotent_retry_replays_and_mismatch_is_rejected(owner: PgPool) -> TestResult {
    let a = seed_tenant(&owner, "spa-a").await?;
    let pool = app_pool(&owner).await?;
    let who = actor(a);
    let key = Uuid::now_v7();
    let hash = idempotency::hash_request(b"{\"amount\":100000}");
    let body = serde_json::json!({"visit_id": "v1"});

    let mut tx = TenantTx::begin(&pool, a).await?;
    assert_eq!(
        idempotency::claim(&mut tx, &who, key, hash).await?,
        Claim::Fresh
    );
    idempotency::complete(&mut tx, &who, key, 201, &body).await?;
    tx.commit().await?;

    let mut tx = TenantTx::begin(&pool, a).await?;
    assert_eq!(
        idempotency::claim(&mut tx, &who, key, hash).await?,
        Claim::Replay {
            status: 201,
            body: body.clone()
        }
    );
    drop(tx);

    let mut tx = TenantTx::begin(&pool, a).await?;
    let other = idempotency::hash_request(b"{\"amount\":999}");
    let mismatch = idempotency::claim(&mut tx, &who, key, other).await;
    assert!(matches!(
        mismatch,
        Err(AppError::Unprocessable {
            code: "idempotency.key_reuse_mismatch"
        })
    ));
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn concurrent_duplicate_waits_then_replays(owner: PgPool) -> TestResult {
    let a = seed_tenant(&owner, "spa-a").await?;
    let pool = app_pool(&owner).await?;
    let who = actor(a);
    let key = Uuid::now_v7();
    let hash = idempotency::hash_request(b"same body");

    let mut first = TenantTx::begin(&pool, a).await?;
    assert_eq!(
        idempotency::claim(&mut first, &who, key, hash).await?,
        Claim::Fresh
    );

    // Second submit starts while the first is still uncommitted: it must block, not run.
    let (pool2, who2) = (pool.clone(), who.clone());
    let second = tokio::spawn(async move {
        let mut tx = TenantTx::begin(&pool2, a).await?;
        idempotency::claim(&mut tx, &who2, key, hash).await
    });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(
        !second.is_finished(),
        "duplicate must wait for the first transaction"
    );

    idempotency::complete(&mut first, &who, key, 201, &serde_json::json!({"ok": true})).await?;
    first.commit().await?;

    let outcome = second.await??;
    assert_eq!(
        outcome,
        Claim::Replay {
            status: 201,
            body: serde_json::json!({"ok": true})
        }
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn rolled_back_attempt_lets_the_retry_run_fresh(owner: PgPool) -> TestResult {
    let a = seed_tenant(&owner, "spa-a").await?;
    let pool = app_pool(&owner).await?;
    let who = actor(a);
    let key = Uuid::now_v7();
    let hash = idempotency::hash_request(b"body");

    let mut failed = TenantTx::begin(&pool, a).await?;
    assert_eq!(
        idempotency::claim(&mut failed, &who, key, hash).await?,
        Claim::Fresh
    );
    drop(failed); // use case failed → rollback

    let mut retry = TenantTx::begin(&pool, a).await?;
    assert_eq!(
        idempotency::claim(&mut retry, &who, key, hash).await?,
        Claim::Fresh
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn purge_removes_old_records_across_tenants(owner: PgPool) -> TestResult {
    let (a, b) = (
        seed_tenant(&owner, "spa-a").await?,
        seed_tenant(&owner, "spa-b").await?,
    );
    for tenant in [a, b] {
        sqlx::query(
            "INSERT INTO app.idempotency_record (tenant_id, actor_user_id, key, request_hash, created_at)
             VALUES ($1, $2, $3, $4, now() - interval '3 days')",
        )
        .bind(tenant.as_uuid())
        .bind(Uuid::now_v7())
        .bind(Uuid::now_v7())
        .bind(&idempotency::hash_request(b"x")[..])
        .execute(&owner)
        .await?;
    }
    let pool = app_pool(&owner).await?;
    let purged: i64 =
        sqlx::query_scalar("SELECT app.purge_idempotency_records(interval '48 hours')")
            .fetch_one(&pool)
            .await?;
    assert_eq!(purged, 2);
    Ok(())
}

/// 0042: the purge never goes below 48 hours, whatever it is asked (a retried payment must find
/// its record). Red before: `interval '0'` deleted the hour-old record.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn purge_never_shortens_the_48_hours(owner: PgPool) -> TestResult {
    let tenant = seed_tenant(&owner, "spa-a").await?;
    sqlx::query(
        "INSERT INTO app.idempotency_record (tenant_id, actor_user_id, key, request_hash, created_at)
         VALUES ($1, $2, $3, $4, now() - interval '1 hour')",
    )
    .bind(tenant.as_uuid())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(&idempotency::hash_request(b"x")[..])
    .execute(&owner)
    .await?;
    let pool = app_pool(&owner).await?;
    for asked in ["0", "-1 day", "1 minute"] {
        let purged: i64 = sqlx::query_scalar("SELECT app.purge_idempotency_records($1::interval)")
            .bind(asked)
            .fetch_one(&pool)
            .await?;
        assert_eq!(purged, 0, "{asked}");
    }
    Ok(())
}

/// 0042: app_rw may change only the columns the code changes. Red before: whole-table UPDATE on
/// role / membership / branch and DELETE on role.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn app_rw_updates_only_the_columns_the_code_changes(owner: PgPool) -> TestResult {
    let pool = app_pool(&owner).await?;
    let (role_delete, role_update, branch_update, membership_update): (bool, bool, bool, bool) =
        sqlx::query_as(
            "SELECT has_table_privilege('app.role', 'DELETE'), has_table_privilege('app.role', 'UPDATE'),
                    has_table_privilege('app.branch', 'UPDATE'),
                    has_table_privilege('app.membership', 'UPDATE')",
        )
        .fetch_one(&pool)
        .await?;
    assert!(!role_delete && !role_update && !branch_update && !membership_update);
    let (follows, role_name, status, user_id): (bool, bool, bool, bool) = sqlx::query_as(
        "SELECT has_column_privilege('app.role', 'follows_template', 'UPDATE'),
                has_column_privilege('app.role', 'name', 'UPDATE'),
                has_column_privilege('app.membership', 'status', 'UPDATE'),
                has_column_privilege('app.membership', 'user_id', 'UPDATE')",
    )
    .fetch_one(&pool)
    .await?;
    assert!(follows && status, "the columns the code updates");
    assert!(!role_name && !user_id, "nothing else");
    // The two integrity keys of 0042 are validated by 0043.
    let validated: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_constraint
         WHERE conname IN ('visit_line_item_of_package_fkey', 'appointment_setting_updated_by_fkey')
           AND convalidated",
    )
    .fetch_one(&owner)
    .await?;
    assert_eq!(validated, 2);
    Ok(())
}
