//! Test support shared by every context crate (enabled with the `testing` feature).
//! `#[sqlx::test]` hands out a pool connected as the schema owner; real code runs as `app_rw`,
//! so tests must use `app_pool` to exercise row-level security exactly as production does.

use crate::ids::TenantId;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

/// Pool on the same test database, connected as the runtime role `app_rw` (RLS applies).
pub async fn app_pool(owner_pool: &PgPool) -> Result<PgPool, sqlx::Error> {
    let password =
        std::env::var("APP_RW_PASSWORD").unwrap_or_else(|_| "app_rw_dev_password".to_owned());
    let options = (*owner_pool.connect_options())
        .clone()
        .username("app_rw")
        .password(&password);
    PgPoolOptions::new()
        .max_connections(4)
        .connect_with(options)
        .await
}

/// Inserts a tenant with the owner pool (bypassing RLS, like a platform provisioning step).
pub async fn seed_tenant(owner_pool: &PgPool, slug: &str) -> Result<TenantId, sqlx::Error> {
    let id = TenantId::new();
    sqlx::query("INSERT INTO app.tenant (id, name, slug) VALUES ($1, $2, $3)")
        .bind(id.as_uuid())
        .bind(format!("Spa {slug}"))
        .bind(slug)
        .execute(owner_pool)
        .await?;
    Ok(id)
}
