//! Catalog use cases. Reading requires `catalog.read`; seeding is a provisioning step (owner pool).

use crate::repo::{self, CategoryRow, PlanRow, ServiceRow};
use crate::seed::{MENU, SeedPrice};
use kernel::{Actor, AppError, Permission, TenantTx};
use sqlx::PgPool;
use uuid::Uuid;

pub async fn categories(pool: &PgPool, actor: &Actor) -> Result<Vec<CategoryRow>, AppError> {
    actor.require(Permission::CatalogRead)?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let rows = repo::categories(&mut tx).await?;
    tx.commit().await?;
    Ok(rows)
}

pub async fn services(
    pool: &PgPool,
    actor: &Actor,
    active_only: bool,
) -> Result<Vec<ServiceRow>, AppError> {
    actor.require(Permission::CatalogRead)?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let rows = repo::services(&mut tx, active_only, None).await?;
    tx.commit().await?;
    Ok(rows)
}

pub async fn package_plans(
    pool: &PgPool,
    actor: &Actor,
    active_only: bool,
) -> Result<Vec<PlanRow>, AppError> {
    actor.require(Permission::CatalogRead)?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let rows = repo::package_plans(&mut tx, active_only, None).await?;
    tx.commit().await?;
    Ok(rows)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeedReport {
    pub categories: usize,
    pub services: usize,
    pub inactive_without_price: usize,
}

/// Loads the printed menu into a tenant that has no catalog yet (provisioning, schema-owner pool).
/// Services whose price is not on the menu are created inactive with no price.
pub async fn seed_menu(owner: &PgPool, tenant_slug: &str) -> Result<SeedReport, AppError> {
    let mut tx = owner.begin().await?;
    let tenant = sqlx::query_scalar!("SELECT id FROM app.tenant WHERE slug = $1", tenant_slug)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound)?;
    let existing = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM app.service_category WHERE tenant_id = $1"#,
        tenant
    )
    .fetch_one(&mut *tx)
    .await?;
    if existing > 0 {
        return Err(AppError::Conflict {
            code: "catalog.already_seeded",
        });
    }

    let mut report = SeedReport {
        categories: 0,
        services: 0,
        inactive_without_price: 0,
    };
    for (category_order, category) in (0_i32..).zip(MENU) {
        let category_id = Uuid::now_v7();
        sqlx::query!(
            "INSERT INTO app.service_category (id, tenant_id, name, sort_order) VALUES ($1, $2, $3, $4)",
            category_id,
            tenant,
            category.name,
            category_order
        )
        .execute(&mut *tx)
        .await?;
        report.categories += 1;

        for (service_order, item) in (0_i32..).zip(category.services) {
            let service_id = Uuid::now_v7();
            let active = !matches!(item.price, SeedPrice::Unknown);
            sqlx::query!(
                r#"INSERT INTO app.service (id, tenant_id, category_id, name, description, duration_min, recommended, active, sort_order)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
                service_id,
                tenant,
                category_id,
                item.name,
                item.description,
                item.duration_min,
                item.recommended,
                active,
                service_order
            )
            .execute(&mut *tx)
            .await?;
            let prices: Vec<(&str, i64)> = match item.price {
                SeedPrice::Flat(amount) => vec![("all", amount)],
                SeedPrice::Gendered(female, male) => [("female", female), ("male", male)]
                    .into_iter()
                    .filter_map(|(variant, amount)| amount.map(|a| (variant, a)))
                    .collect(),
                SeedPrice::Unknown => {
                    report.inactive_without_price += 1;
                    Vec::new()
                }
            };
            for (variant, amount) in prices {
                sqlx::query!(
                    "INSERT INTO app.service_price (tenant_id, service_id, variant, amount) VALUES ($1, $2, $3, $4)",
                    tenant,
                    service_id,
                    variant,
                    amount
                )
                .execute(&mut *tx)
                .await?;
            }
            report.services += 1;
        }
    }
    tx.commit().await?;
    Ok(report)
}
