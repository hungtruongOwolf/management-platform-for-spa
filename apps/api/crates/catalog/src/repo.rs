//! Catalog SQL. Every query runs in a `TenantTx` and filters by its tenant explicitly (RLS is layer two).

use crate::domain::{PriceVariant, ServicePrices};
use kernel::{AppError, FieldError, Money, TenantTx};
use std::collections::HashMap;
use uuid::Uuid;

pub struct CategoryRow {
    pub id: Uuid,
    pub name: String,
    pub sort_order: i32,
}

pub struct ServiceRow {
    pub id: Uuid,
    pub category_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub duration_min: Option<i32>,
    pub recommended: bool,
    pub active: bool,
    pub version: i64,
    pub prices: ServicePrices,
}

pub struct PlanItemRow {
    pub service_id: Uuid,
    pub sessions: i32,
    pub bonus_sessions: i32,
}

pub struct PlanRow {
    pub id: Uuid,
    pub name: String,
    pub price: Money,
    pub validity_months: Option<i32>,
    pub gift_note: Option<String>,
    pub active: bool,
    pub version: i64,
    pub items: Vec<PlanItemRow>,
}

/// Maps a unique violation on `name` to a field error (AC1.5), anything else to a database error.
fn name_taken(error: sqlx::Error) -> AppError {
    let unique = error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .is_some_and(|c| c == "23505");
    if unique {
        AppError::Validation(vec![FieldError::new("name", "taken")])
    } else {
        AppError::Database(error)
    }
}

// ---------------------------------------------------------------- categories

pub async fn categories(tx: &mut TenantTx) -> Result<Vec<CategoryRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    let rows = sqlx::query!(
        "SELECT id, name, sort_order FROM app.service_category WHERE tenant_id = $1 ORDER BY sort_order, name",
        tenant
    )
    .fetch_all(tx.conn())
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| CategoryRow {
            id: r.id,
            name: r.name,
            sort_order: r.sort_order,
        })
        .collect())
}

/// Whether the group exists, holding it (`FOR KEY SHARE`, what the service's foreign key takes)
/// until the transaction ends: a delete of the group waits for this write and then finds the
/// service; a write that waited on a delete finds no group (`422`, not a foreign-key error).
/// A rename does not conflict with it.
pub async fn category_exists(tx: &mut TenantTx, id: Uuid) -> Result<bool, AppError> {
    let tenant = tx.tenant().as_uuid();
    let found = sqlx::query_scalar!(
        "SELECT 1 AS one FROM app.service_category WHERE tenant_id = $1 AND id = $2 FOR KEY SHARE",
        tenant,
        id
    )
    .fetch_optional(tx.conn())
    .await?;
    Ok(found.is_some())
}

/// The group, locked for its delete (`FOR UPDATE`): waits for a write that holds it
/// ([`category_exists`]), so the services counted after it include that write's.
pub async fn lock_category(tx: &mut TenantTx, id: Uuid) -> Result<Option<CategoryRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    let row = sqlx::query!(
        "SELECT id, name, sort_order FROM app.service_category WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
        tenant,
        id
    )
    .fetch_optional(tx.conn())
    .await?;
    Ok(row.map(|r| CategoryRow {
        id: r.id,
        name: r.name,
        sort_order: r.sort_order,
    }))
}

/// Whether any service, on sale or not, is in the group.
pub async fn category_in_use(tx: &mut TenantTx, id: Uuid) -> Result<bool, AppError> {
    let tenant = tx.tenant().as_uuid();
    let used = sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM app.service WHERE tenant_id = $1 AND category_id = $2) AS "used!""#,
        tenant,
        id
    )
    .fetch_one(tx.conn())
    .await?;
    Ok(used)
}

/// Deletes the group. A row still referencing it (a foreign-key violation, the guarantee behind
/// [`category_in_use`]) is `409 catalog.category_in_use`.
pub async fn delete_category(tx: &mut TenantTx, id: Uuid) -> Result<bool, AppError> {
    let tenant = tx.tenant().as_uuid();
    let deleted = sqlx::query!(
        "DELETE FROM app.service_category WHERE tenant_id = $1 AND id = $2",
        tenant,
        id
    )
    .execute(tx.conn())
    .await
    .map_err(|error| {
        let referenced = error
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::code)
            .is_some_and(|c| c == "23503");
        if referenced {
            AppError::Conflict {
                code: CATEGORY_IN_USE,
            }
        } else {
            AppError::Database(error)
        }
    })?;
    Ok(deleted.rows_affected() == 1)
}

/// `409`: the group still has services (on sale or not), or another row references it.
pub const CATEGORY_IN_USE: &str = "catalog.category_in_use";

pub async fn insert_category(
    tx: &mut TenantTx,
    id: Uuid,
    name: &str,
    sort_order: Option<i32>,
) -> Result<CategoryRow, AppError> {
    let tenant = tx.tenant().as_uuid();
    let row = sqlx::query!(
        r#"INSERT INTO app.service_category (id, tenant_id, name, sort_order)
           VALUES ($1, $2, $3, COALESCE($4, (SELECT COALESCE(max(sort_order) + 1, 0) FROM app.service_category WHERE tenant_id = $2)))
           RETURNING id, name, sort_order"#,
        id,
        tenant,
        name,
        sort_order
    )
    .fetch_one(tx.conn())
    .await
    .map_err(name_taken)?;
    Ok(CategoryRow {
        id: row.id,
        name: row.name,
        sort_order: row.sort_order,
    })
}

pub async fn update_category(
    tx: &mut TenantTx,
    id: Uuid,
    name: &str,
    sort_order: i32,
) -> Result<Option<CategoryRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    let row = sqlx::query!(
        "UPDATE app.service_category SET name = $3, sort_order = $4 WHERE tenant_id = $1 AND id = $2 RETURNING id, name, sort_order",
        tenant,
        id,
        name,
        sort_order
    )
    .fetch_optional(tx.conn())
    .await
    .map_err(name_taken)?;
    Ok(row.map(|r| CategoryRow {
        id: r.id,
        name: r.name,
        sort_order: r.sort_order,
    }))
}

// ---------------------------------------------------------------- services

async fn prices_by_service(
    tx: &mut TenantTx,
    only: Option<Uuid>,
) -> Result<HashMap<Uuid, ServicePrices>, AppError> {
    let tenant = tx.tenant().as_uuid();
    let rows = sqlx::query!(
        "SELECT service_id, variant, amount FROM app.service_price WHERE tenant_id = $1 AND ($2::uuid IS NULL OR service_id = $2)",
        tenant,
        only
    )
    .fetch_all(tx.conn())
    .await?;
    let mut prices: HashMap<Uuid, ServicePrices> = HashMap::new();
    for p in rows {
        let (Some(variant), Ok(amount)) = (PriceVariant::parse(&p.variant), Money::vnd(p.amount))
        else {
            return Err(AppError::Internal(format!(
                "corrupt price row for service {}",
                p.service_id
            )));
        };
        prices.entry(p.service_id).or_default().set(variant, amount);
    }
    Ok(prices)
}

pub async fn services(
    tx: &mut TenantTx,
    active_only: bool,
    only: Option<Uuid>,
) -> Result<Vec<ServiceRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    let rows = sqlx::query!(
        r#"SELECT s.id, s.category_id, s.name, s.description, s.duration_min, s.recommended, s.active, s.version
           FROM app.service s JOIN app.service_category c ON c.tenant_id = s.tenant_id AND c.id = s.category_id
           WHERE s.tenant_id = $1 AND (NOT $2 OR s.active) AND ($3::uuid IS NULL OR s.id = $3)
           ORDER BY c.sort_order, s.sort_order, s.name"#,
        tenant,
        active_only,
        only
    )
    .fetch_all(tx.conn())
    .await?;
    let mut prices = prices_by_service(tx, only).await?;
    Ok(rows
        .into_iter()
        .map(|r| ServiceRow {
            prices: prices.remove(&r.id).unwrap_or_default(),
            id: r.id,
            category_id: r.category_id,
            name: r.name,
            description: r.description,
            duration_min: r.duration_min,
            recommended: r.recommended,
            active: r.active,
            version: r.version,
        })
        .collect())
}

pub async fn service(tx: &mut TenantTx, id: Uuid) -> Result<Option<ServiceRow>, AppError> {
    Ok(services(tx, false, Some(id)).await?.into_iter().next())
}

pub struct ServiceFields<'a> {
    pub category_id: Uuid,
    pub name: &'a str,
    pub description: Option<&'a str>,
    pub duration_min: Option<i32>,
    pub recommended: bool,
    pub active: bool,
}

pub async fn insert_service(
    tx: &mut TenantTx,
    id: Uuid,
    f: &ServiceFields<'_>,
) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        r#"INSERT INTO app.service (id, tenant_id, category_id, name, description, duration_min, recommended, active, sort_order)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8,
                   (SELECT COALESCE(max(sort_order) + 1, 0) FROM app.service WHERE tenant_id = $2 AND category_id = $3))"#,
        id,
        tenant,
        f.category_id,
        f.name,
        f.description,
        f.duration_min,
        f.recommended,
        f.active
    )
    .execute(tx.conn())
    .await
    .map_err(name_taken)?;
    Ok(())
}

/// Optimistic update. `Ok(false)` = no row with that id **and** version (caller decides 404 vs 412).
pub async fn update_service(
    tx: &mut TenantTx,
    id: Uuid,
    expected_version: i64,
    f: &ServiceFields<'_>,
) -> Result<bool, AppError> {
    let tenant = tx.tenant().as_uuid();
    let done = sqlx::query!(
        r#"UPDATE app.service
           SET category_id = $4, name = $5, description = $6, duration_min = $7, recommended = $8, active = $9,
               version = version + 1, updated_at = now()
           WHERE tenant_id = $1 AND id = $2 AND version = $3"#,
        tenant,
        id,
        expected_version,
        f.category_id,
        f.name,
        f.description,
        f.duration_min,
        f.recommended,
        f.active
    )
    .execute(tx.conn())
    .await
    .map_err(name_taken)?;
    Ok(done.rows_affected() == 1)
}

pub async fn replace_prices(
    tx: &mut TenantTx,
    service_id: Uuid,
    prices: &ServicePrices,
) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        "DELETE FROM app.service_price WHERE tenant_id = $1 AND service_id = $2",
        tenant,
        service_id
    )
    .execute(tx.conn())
    .await?;
    let variants = [
        (PriceVariant::All, prices.all),
        (PriceVariant::Female, prices.female),
        (PriceVariant::Male, prices.male),
    ];
    for (variant, amount) in variants {
        if let Some(amount) = amount {
            sqlx::query!(
                "INSERT INTO app.service_price (tenant_id, service_id, variant, amount) VALUES ($1, $2, $3, $4)",
                tenant,
                service_id,
                variant.as_str(),
                amount.as_i64()
            )
            .execute(tx.conn())
            .await?;
        }
    }
    Ok(())
}

/// Activity flag of each listed service in this tenant (missing ids are absent from the map).
pub async fn service_activity(
    tx: &mut TenantTx,
    ids: &[Uuid],
) -> Result<HashMap<Uuid, bool>, AppError> {
    let tenant = tx.tenant().as_uuid();
    let rows = sqlx::query!(
        "SELECT id, active FROM app.service WHERE tenant_id = $1 AND id = ANY($2)",
        tenant,
        ids
    )
    .fetch_all(tx.conn())
    .await?;
    Ok(rows.into_iter().map(|r| (r.id, r.active)).collect())
}

// ---------------------------------------------------------------- package plans

pub async fn package_plans(
    tx: &mut TenantTx,
    active_only: bool,
    only: Option<Uuid>,
) -> Result<Vec<PlanRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    let plans = sqlx::query!(
        r#"SELECT id, name, price, validity_months, gift_note, active, version
           FROM app.package_plan
           WHERE tenant_id = $1 AND (NOT $2 OR active) AND ($3::uuid IS NULL OR id = $3)
           ORDER BY name"#,
        tenant,
        active_only,
        only
    )
    .fetch_all(tx.conn())
    .await?;
    let items = sqlx::query!(
        r#"SELECT i.plan_id, i.service_id, i.sessions, i.bonus_sessions
           FROM app.package_plan_item i JOIN app.service s ON s.tenant_id = i.tenant_id AND s.id = i.service_id
           WHERE i.tenant_id = $1 AND ($2::uuid IS NULL OR i.plan_id = $2)
           ORDER BY s.sort_order, s.name"#,
        tenant,
        only
    )
    .fetch_all(tx.conn())
    .await?;
    let mut by_plan: HashMap<Uuid, Vec<PlanItemRow>> = HashMap::new();
    for i in items {
        by_plan.entry(i.plan_id).or_default().push(PlanItemRow {
            service_id: i.service_id,
            sessions: i.sessions,
            bonus_sessions: i.bonus_sessions,
        });
    }
    plans
        .into_iter()
        .map(|p| {
            Ok(PlanRow {
                price: Money::vnd(p.price)
                    .map_err(|_| AppError::Internal(format!("corrupt plan price {}", p.id)))?,
                items: by_plan.remove(&p.id).unwrap_or_default(),
                id: p.id,
                name: p.name,
                validity_months: p.validity_months,
                gift_note: p.gift_note,
                active: p.active,
                version: p.version,
            })
        })
        .collect()
}

pub async fn package_plan(tx: &mut TenantTx, id: Uuid) -> Result<Option<PlanRow>, AppError> {
    Ok(package_plans(tx, false, Some(id)).await?.into_iter().next())
}

pub struct PlanFields<'a> {
    pub name: &'a str,
    pub price: Money,
    pub validity_months: Option<i32>,
    pub gift_note: Option<&'a str>,
    pub active: bool,
}

pub async fn insert_plan(tx: &mut TenantTx, id: Uuid, f: &PlanFields<'_>) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        r#"INSERT INTO app.package_plan (id, tenant_id, name, price, validity_months, gift_note, active)
           VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
        id,
        tenant,
        f.name,
        f.price.as_i64(),
        f.validity_months,
        f.gift_note,
        f.active
    )
    .execute(tx.conn())
    .await
    .map_err(name_taken)?;
    Ok(())
}

pub async fn update_plan(
    tx: &mut TenantTx,
    id: Uuid,
    expected_version: i64,
    f: &PlanFields<'_>,
) -> Result<bool, AppError> {
    let tenant = tx.tenant().as_uuid();
    let done = sqlx::query!(
        r#"UPDATE app.package_plan
           SET name = $4, price = $5, validity_months = $6, gift_note = $7, active = $8, version = version + 1, updated_at = now()
           WHERE tenant_id = $1 AND id = $2 AND version = $3"#,
        tenant,
        id,
        expected_version,
        f.name,
        f.price.as_i64(),
        f.validity_months,
        f.gift_note,
        f.active
    )
    .execute(tx.conn())
    .await
    .map_err(name_taken)?;
    Ok(done.rows_affected() == 1)
}

pub async fn replace_plan_items(
    tx: &mut TenantTx,
    plan_id: Uuid,
    items: &[PlanItemRow],
) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        "DELETE FROM app.package_plan_item WHERE tenant_id = $1 AND plan_id = $2",
        tenant,
        plan_id
    )
    .execute(tx.conn())
    .await?;
    for item in items {
        sqlx::query!(
            r#"INSERT INTO app.package_plan_item (id, tenant_id, plan_id, service_id, sessions, bonus_sessions)
               VALUES ($1, $2, $3, $4, $5, $6)"#,
            Uuid::now_v7(),
            tenant,
            plan_id,
            item.service_id,
            item.sessions,
            item.bonus_sessions
        )
        .execute(tx.conn())
        .await?;
    }
    Ok(())
}

/// Distinguishes "gone" (404) from "changed by someone else" (412) after a failed optimistic update.
pub async fn exists(tx: &mut TenantTx, table: Versioned, id: Uuid) -> Result<bool, AppError> {
    let tenant = tx.tenant().as_uuid();
    let found = match table {
        Versioned::Service => {
            sqlx::query_scalar!(
                "SELECT 1 AS one FROM app.service WHERE tenant_id = $1 AND id = $2",
                tenant,
                id
            )
            .fetch_optional(tx.conn())
            .await?
        }
        Versioned::Plan => {
            sqlx::query_scalar!(
                "SELECT 1 AS one FROM app.package_plan WHERE tenant_id = $1 AND id = $2",
                tenant,
                id
            )
            .fetch_optional(tx.conn())
            .await?
        }
    };
    Ok(found.is_some())
}

#[derive(Debug, Clone, Copy)]
pub enum Versioned {
    Service,
    Plan,
}

/// One service with its prices, the service row locked `FOR SHARE` so a concurrent price edit
/// (which updates the row's version) waits until the caller's transaction ends.
pub async fn lock_service(tx: &mut TenantTx, id: Uuid) -> Result<Option<ServiceRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    let locked = sqlx::query_scalar!(
        "SELECT id FROM app.service WHERE tenant_id = $1 AND id = $2 FOR SHARE",
        tenant,
        id
    )
    .fetch_optional(tx.conn())
    .await?;
    match locked {
        Some(_) => service(tx, id).await,
        None => Ok(None),
    }
}

/// The plan row alone, locked against concurrent edits until the caller's transaction ends.
pub async fn lock_plan(tx: &mut TenantTx, id: Uuid) -> Result<Option<PlanRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    let row = sqlx::query!(
        r#"SELECT id, name, price, validity_months, gift_note, active, version
           FROM app.package_plan WHERE tenant_id = $1 AND id = $2 FOR SHARE"#,
        tenant,
        id
    )
    .fetch_optional(tx.conn())
    .await?;
    row.map(|p| {
        Ok(PlanRow {
            price: Money::vnd(p.price)
                .map_err(|_| AppError::Internal(format!("corrupt plan price {}", p.id)))?,
            items: Vec::new(),
            id: p.id,
            name: p.name,
            validity_months: p.validity_months,
            gift_note: p.gift_note,
            active: p.active,
            version: p.version,
        })
    })
    .transpose()
}

/// `(service_id, service name, sessions, bonus_sessions)` of each item, in menu order.
pub async fn plan_items_with_names(
    tx: &mut TenantTx,
    plan_id: Uuid,
) -> Result<Vec<(Uuid, String, i32, i32)>, AppError> {
    let tenant = tx.tenant().as_uuid();
    let rows = sqlx::query!(
        r#"SELECT i.service_id, s.name, i.sessions, i.bonus_sessions
           FROM app.package_plan_item i JOIN app.service s ON s.tenant_id = i.tenant_id AND s.id = i.service_id
           WHERE i.tenant_id = $1 AND i.plan_id = $2
           ORDER BY s.sort_order, s.name"#,
        tenant,
        plan_id
    )
    .fetch_all(tx.conn())
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.service_id, r.name, r.sessions, r.bonus_sessions))
        .collect())
}
