//! Catalog use cases. Reading requires `catalog.read`; seeding is a provisioning step (owner pool).

use crate::repo::{self, CategoryRow, PlanRow, ServiceRow};
use crate::seed::{MENU, SeedPrice};
use kernel::{Actor, AppError, Permission, TenantTx};
use sqlx::PgPool;
use uuid::Uuid;

pub async fn categories(pool: &PgPool, actor: &Actor) -> Result<Vec<CategoryRow>, AppError> {
    actor.require(Permission::CatalogRead)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
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
    let mut tx = TenantTx::for_actor(pool, actor).await?;
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
    let mut tx = TenantTx::for_actor(pool, actor).await?;
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

/// A service as a one-off visit line charges it: name and prices at this moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceSnapshot {
    pub id: Uuid,
    pub name: String,
    pub active: bool,
    pub prices: crate::domain::ServicePrices,
}

/// Reads a service of the caller's tenant inside the caller's transaction (`None` = no such
/// service here), locked `FOR SHARE` against a concurrent price edit. The caller authorizes.
pub async fn service_snapshot(
    tx: &mut TenantTx,
    id: Uuid,
) -> Result<Option<ServiceSnapshot>, AppError> {
    Ok(repo::lock_service(tx, id).await?.map(|s| ServiceSnapshot {
        id: s.id,
        name: s.name,
        active: s.active,
        prices: s.prices,
    }))
}

/// One line of a plan as it is sold: the service and its name at this moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanItemSnapshot {
    pub service_id: Uuid,
    pub service_name: String,
    pub sessions: i32,
    pub bonus_sessions: i32,
}

/// A package plan with everything a sale must copy (AC1.2): later catalog edits never reach
/// packages already sold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanSnapshot {
    pub id: Uuid,
    pub name: String,
    pub price: kernel::Money,
    pub validity_months: Option<i32>,
    pub gift_note: Option<String>,
    pub active: bool,
    pub items: Vec<PlanItemSnapshot>,
}

/// Reads a plan of the caller's tenant inside the caller's transaction (`None` = no such plan here).
/// Locks the plan row `FOR SHARE` so a concurrent price edit waits until the sale commits.
/// The caller authorizes (e.g. `package.sell`); catalog read access is not required to sell.
pub async fn plan_snapshot(tx: &mut TenantTx, id: Uuid) -> Result<Option<PlanSnapshot>, AppError> {
    let Some(plan) = repo::lock_plan(tx, id).await? else {
        return Ok(None);
    };
    let items = repo::plan_items_with_names(tx, id)
        .await?
        .into_iter()
        .map(
            |(service_id, service_name, sessions, bonus_sessions)| PlanItemSnapshot {
                service_id,
                service_name,
                sessions,
                bonus_sessions,
            },
        )
        .collect();
    Ok(Some(PlanSnapshot {
        id: plan.id,
        name: plan.name,
        price: plan.price,
        validity_months: plan.validity_months,
        gift_note: plan.gift_note,
        active: plan.active,
        items,
    }))
}

/// Name of a service of the caller's tenant (active or not), read inside the caller's
/// transaction for snapshotting (`None` = no such service here). The caller authorizes.
pub async fn service_name(tx: &mut TenantTx, id: Uuid) -> Result<Option<String>, AppError> {
    Ok(repo::service(tx, id).await?.map(|s| s.name))
}
