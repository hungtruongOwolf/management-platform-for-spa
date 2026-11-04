//! Catalog SQL. Every query runs in a `TenantTx` and filters by its tenant explicitly (RLS is layer two).

use crate::domain::{PriceVariant, ServicePrices};
use kernel::{AppError, Money, TenantTx};
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
    pub items: Vec<PlanItemRow>,
}

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

pub async fn services(tx: &mut TenantTx, active_only: bool) -> Result<Vec<ServiceRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    let rows = sqlx::query!(
        r#"SELECT s.id, s.category_id, s.name, s.description, s.duration_min, s.recommended, s.active
           FROM app.service s JOIN app.service_category c ON c.tenant_id = s.tenant_id AND c.id = s.category_id
           WHERE s.tenant_id = $1 AND (NOT $2 OR s.active)
           ORDER BY c.sort_order, s.sort_order, s.name"#,
        tenant,
        active_only
    )
    .fetch_all(tx.conn())
    .await?;
    let price_rows = sqlx::query!(
        "SELECT service_id, variant, amount FROM app.service_price WHERE tenant_id = $1",
        tenant
    )
    .fetch_all(tx.conn())
    .await?;

    let mut prices: HashMap<Uuid, ServicePrices> = HashMap::new();
    for p in price_rows {
        let (Some(variant), Ok(amount)) = (PriceVariant::parse(&p.variant), Money::vnd(p.amount))
        else {
            return Err(AppError::Internal(format!(
                "corrupt price row for service {}",
                p.service_id
            )));
        };
        prices.entry(p.service_id).or_default().set(variant, amount);
    }
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
        })
        .collect())
}

pub async fn package_plans(tx: &mut TenantTx, active_only: bool) -> Result<Vec<PlanRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    let plans = sqlx::query!(
        r#"SELECT id, name, price, validity_months, gift_note, active
           FROM app.package_plan WHERE tenant_id = $1 AND (NOT $2 OR active) ORDER BY name"#,
        tenant,
        active_only
    )
    .fetch_all(tx.conn())
    .await?;
    let items = sqlx::query!(
        "SELECT plan_id, service_id, sessions, bonus_sessions FROM app.package_plan_item WHERE tenant_id = $1",
        tenant
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
            })
        })
        .collect()
}
