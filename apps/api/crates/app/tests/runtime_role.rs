//! Security review L6: the API refuses to serve with a database role that RLS does not bind.

use app::db_guard::check_runtime_role;
use kernel::testing::app_pool;
use sqlx::postgres::PgPoolOptions;
use sqlx::{AssertSqlSafe, PgPool};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn app_rw_is_accepted(owner: PgPool) -> TestResult {
    let pool = app_pool(&owner).await?;
    check_runtime_role(&pool).await.map_err(|e| e.to_string())?;
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn the_schema_owner_is_refused(owner: PgPool) -> TestResult {
    assert!(
        check_runtime_role(&owner).await.is_err(),
        "the owner (or a superuser) bypasses RLS"
    );
    Ok(())
}

async fn exec(pool: &PgPool, sql: String) -> Result<(), sqlx::Error> {
    sqlx::query(AssertSqlSafe(sql))
        .execute(pool)
        .await
        .map(|_| ())
}

async fn connect_as(owner: &PgPool, role: &str) -> Result<PgPool, sqlx::Error> {
    let options = (*owner.connect_options())
        .clone()
        .username(role)
        .password("probe-pw");
    PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_table_owning_or_bypassrls_role_is_refused(owner: PgPool) -> TestResult {
    // Roles are cluster-wide: a unique name keeps parallel runs apart.
    let role = format!("vpyd_probe_{}", uuid::Uuid::now_v7().simple());
    exec(
        &owner,
        format!("CREATE ROLE {role} LOGIN PASSWORD 'probe-pw' NOSUPERUSER NOBYPASSRLS"),
    )
    .await?;
    exec(
        &owner,
        format!("GRANT USAGE, CREATE ON SCHEMA app TO {role}"),
    )
    .await?;

    let probe = connect_as(&owner, &role).await?;
    check_runtime_role(&probe)
        .await
        .map_err(|e| e.to_string())?;
    exec(&probe, "CREATE TABLE app.probe_owned (id int)".to_owned()).await?;
    let owning = check_runtime_role(&probe).await;
    exec(&probe, "DROP TABLE app.probe_owned".to_owned()).await?;
    probe.close().await;
    assert!(
        owning.is_err_and(|e| e.to_string().contains("owns 1 table")),
        "a table owner is not bound by RLS"
    );

    exec(&owner, format!("ALTER ROLE {role} BYPASSRLS")).await?;
    let bypass = connect_as(&owner, &role).await?;
    let refused = check_runtime_role(&bypass).await;
    bypass.close().await;
    exec(&owner, format!("REVOKE ALL ON SCHEMA app FROM {role}")).await?;
    exec(&owner, format!("DROP ROLE {role}")).await?;
    assert!(refused.is_err_and(|e| e.to_string().contains("BYPASSRLS")));
    Ok(())
}
