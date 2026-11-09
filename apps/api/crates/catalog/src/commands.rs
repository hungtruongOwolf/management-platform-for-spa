//! Catalog write use cases (US1, owner only): categories, services with price variants, package plans.
//! Every change is validated, optimistic-locked where versioned, and audited in the same transaction.

use crate::domain::{PricingError, ServicePrices};
use crate::repo::{
    self, CategoryRow, PlanFields, PlanItemRow, PlanRow, ServiceFields, ServiceRow, Versioned,
};
use kernel::audit::{self, AuditEntry};
use kernel::{Actor, AppError, FieldError, Money, Permission, TenantTx};
use serde_json::json;
use sqlx::PgPool;
use std::collections::HashSet;
use uuid::Uuid;

fn clean_name(name: &str, field: &'static str, errors: &mut Vec<FieldError>) -> String {
    let name: String = name.trim().chars().collect();
    let len = name.chars().count();
    if len == 0 {
        errors.push(FieldError::new(field, "required"));
    } else if len > 200 {
        errors.push(FieldError::new(field, "too_long"));
    }
    name
}

fn clean_optional(text: Option<&str>) -> Option<String> {
    text.map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
}

fn fail_if_any(errors: Vec<FieldError>) -> Result<(), AppError> {
    if errors.is_empty() {
        Ok(())
    } else {
        Err(AppError::Validation(errors))
    }
}

// ---------------------------------------------------------------- categories

pub async fn create_category(
    pool: &PgPool,
    actor: &Actor,
    name: &str,
) -> Result<CategoryRow, AppError> {
    actor.require(Permission::CatalogManage)?;
    let mut errors = Vec::new();
    let name = clean_name(name, "name", &mut errors);
    fail_if_any(errors)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let row = repo::insert_category(&mut tx, Uuid::now_v7(), &name, None).await?;
    audit::record(
        &mut tx,
        actor,
        AuditEntry {
            action: "catalog.category_create",
            entity_type: "service_category",
            entity_id: row.id,
            before: None,
            after: Some(json!({"name": row.name, "sort_order": row.sort_order})),
            reason: None,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(row)
}

pub async fn update_category(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    name: &str,
    sort_order: i32,
) -> Result<CategoryRow, AppError> {
    actor.require(Permission::CatalogManage)?;
    let mut errors = Vec::new();
    let name = clean_name(name, "name", &mut errors);
    if !(0..=10_000).contains(&sort_order) {
        errors.push(FieldError::new("sort_order", "out_of_range"));
    }
    fail_if_any(errors)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let row = repo::update_category(&mut tx, id, &name, sort_order)
        .await?
        .ok_or(AppError::NotFound)?;
    audit::record(
        &mut tx,
        actor,
        AuditEntry {
            action: "catalog.category_update",
            entity_type: "service_category",
            entity_id: row.id,
            before: None,
            after: Some(json!({"name": row.name, "sort_order": row.sort_order})),
            reason: None,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(row)
}

// ---------------------------------------------------------------- services

pub struct ServiceInput {
    pub category_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub duration_min: Option<i32>,
    pub recommended: bool,
    pub active: bool,
    pub prices: ServicePrices,
}

/// Rules of AC1.8 and AC1.3: a sellable (active) service needs a valid price set; an inactive one may
/// have none yet (e.g. Ôn ấm tử cung waiting for its price).
fn validate_service(input: &ServiceInput) -> Result<(String, Option<String>), AppError> {
    let mut errors = Vec::new();
    let name = clean_name(&input.name, "name", &mut errors);
    if input.duration_min.is_some_and(|d| !(1..=600).contains(&d)) {
        errors.push(FieldError::new("duration_min", "out_of_range"));
    }
    match input.prices.validate() {
        Ok(_) => {}
        Err(PricingError::NoPrice) if !input.active => {}
        Err(PricingError::NoPrice) => errors.push(FieldError::new("prices", "required")),
        Err(PricingError::MixedVariants) => {
            errors.push(FieldError::new("prices", "mixed_variants"))
        }
    }
    let prices = &input.prices;
    if [prices.all, prices.female, prices.male]
        .into_iter()
        .flatten()
        .any(|p| !p.is_enterable())
    {
        errors.push(FieldError::new("prices", "out_of_range"));
    }
    fail_if_any(errors)?;
    Ok((name, clean_optional(input.description.as_deref())))
}

fn service_snapshot(row: &ServiceRow) -> serde_json::Value {
    json!({
        "name": row.name, "category_id": row.category_id, "duration_min": row.duration_min,
        "recommended": row.recommended, "active": row.active, "prices": row.prices,
    })
}

pub async fn create_service(
    pool: &PgPool,
    actor: &Actor,
    input: ServiceInput,
) -> Result<ServiceRow, AppError> {
    actor.require(Permission::CatalogManage)?;
    let (name, description) = validate_service(&input)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    if !repo::category_exists(&mut tx, input.category_id).await? {
        return Err(AppError::Validation(vec![FieldError::new(
            "category_id",
            "not_found",
        )]));
    }
    let id = Uuid::now_v7();
    let fields = ServiceFields {
        category_id: input.category_id,
        name: &name,
        description: description.as_deref(),
        duration_min: input.duration_min,
        recommended: input.recommended,
        active: input.active,
    };
    repo::insert_service(&mut tx, id, &fields).await?;
    repo::replace_prices(&mut tx, id, &input.prices).await?;
    let row = repo::service(&mut tx, id)
        .await?
        .ok_or(AppError::NotFound)?;
    audit::record(
        &mut tx,
        actor,
        AuditEntry {
            action: "catalog.service_create",
            entity_type: "service",
            entity_id: id,
            before: None,
            after: Some(service_snapshot(&row)),
            reason: None,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(row)
}

/// Full replacement of a service (PUT). Already-sold packages keep their own snapshots (AC1.2).
pub async fn update_service(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    expected_version: i64,
    input: ServiceInput,
) -> Result<ServiceRow, AppError> {
    actor.require(Permission::CatalogManage)?;
    let (name, description) = validate_service(&input)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let before = repo::service(&mut tx, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if !repo::category_exists(&mut tx, input.category_id).await? {
        return Err(AppError::Validation(vec![FieldError::new(
            "category_id",
            "not_found",
        )]));
    }
    let fields = ServiceFields {
        category_id: input.category_id,
        name: &name,
        description: description.as_deref(),
        duration_min: input.duration_min,
        recommended: input.recommended,
        active: input.active,
    };
    if !repo::update_service(&mut tx, id, expected_version, &fields).await? {
        return Err(if repo::exists(&mut tx, Versioned::Service, id).await? {
            AppError::StaleVersion
        } else {
            AppError::NotFound
        });
    }
    repo::replace_prices(&mut tx, id, &input.prices).await?;
    let after = repo::service(&mut tx, id)
        .await?
        .ok_or(AppError::NotFound)?;
    audit::record(
        &mut tx,
        actor,
        AuditEntry {
            action: "catalog.service_update",
            entity_type: "service",
            entity_id: id,
            before: Some(service_snapshot(&before)),
            after: Some(service_snapshot(&after)),
            reason: None,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(after)
}

// ---------------------------------------------------------------- package plans

pub struct PlanItemInput {
    pub service_id: Uuid,
    pub sessions: i32,
    pub bonus_sessions: i32,
}

pub struct PlanInput {
    pub name: String,
    pub price: Money,
    pub validity_months: Option<i32>,
    pub gift_note: Option<String>,
    pub active: bool,
    pub items: Vec<PlanItemInput>,
}

/// AC1.5: at least one item, positive sessions, no duplicate service, every service exists and is on sale.
async fn validate_plan(
    tx: &mut TenantTx,
    input: &PlanInput,
) -> Result<(String, Option<String>), AppError> {
    let mut errors = Vec::new();
    let name = clean_name(&input.name, "name", &mut errors);
    if !input.price.is_enterable() {
        errors.push(FieldError::new("price", "out_of_range"));
    }
    if input
        .validity_months
        .is_some_and(|m| !(1..=120).contains(&m))
    {
        errors.push(FieldError::new("validity_months", "out_of_range"));
    }
    if input.items.is_empty() {
        errors.push(FieldError::new("items", "required"));
    }
    let mut seen = HashSet::new();
    for (i, item) in input.items.iter().enumerate() {
        if !(1..=500).contains(&item.sessions) {
            errors.push(FieldError::new(
                format!("items[{i}].sessions"),
                "out_of_range",
            ));
        }
        if !(0..=500).contains(&item.bonus_sessions) {
            errors.push(FieldError::new(
                format!("items[{i}].bonus_sessions"),
                "out_of_range",
            ));
        }
        if !seen.insert(item.service_id) {
            errors.push(FieldError::new(
                format!("items[{i}].service_id"),
                "duplicate",
            ));
        }
    }
    let ids: Vec<Uuid> = input.items.iter().map(|i| i.service_id).collect();
    let activity = repo::service_activity(tx, &ids).await?;
    for (i, item) in input.items.iter().enumerate() {
        match activity.get(&item.service_id) {
            None => errors.push(FieldError::new(
                format!("items[{i}].service_id"),
                "not_found",
            )),
            Some(false) => errors.push(FieldError::new(
                format!("items[{i}].service_id"),
                "inactive",
            )),
            Some(true) => {}
        }
    }
    fail_if_any(errors)?;
    Ok((name, clean_optional(input.gift_note.as_deref())))
}

fn plan_items(input: &PlanInput) -> Vec<PlanItemRow> {
    input
        .items
        .iter()
        .map(|i| PlanItemRow {
            service_id: i.service_id,
            sessions: i.sessions,
            bonus_sessions: i.bonus_sessions,
        })
        .collect()
}

fn plan_snapshot(row: &PlanRow) -> serde_json::Value {
    json!({
        "name": row.name, "price": row.price, "validity_months": row.validity_months, "active": row.active,
        "items": row.items.iter().map(|i| json!({"service_id": i.service_id, "sessions": i.sessions, "bonus_sessions": i.bonus_sessions})).collect::<Vec<_>>(),
    })
}

pub async fn create_plan(
    pool: &PgPool,
    actor: &Actor,
    input: PlanInput,
) -> Result<PlanRow, AppError> {
    actor.require(Permission::CatalogManage)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let (name, gift_note) = validate_plan(&mut tx, &input).await?;
    let id = Uuid::now_v7();
    let fields = PlanFields {
        name: &name,
        price: input.price,
        validity_months: input.validity_months,
        gift_note: gift_note.as_deref(),
        active: input.active,
    };
    repo::insert_plan(&mut tx, id, &fields).await?;
    repo::replace_plan_items(&mut tx, id, &plan_items(&input)).await?;
    let row = repo::package_plan(&mut tx, id)
        .await?
        .ok_or(AppError::NotFound)?;
    audit::record(
        &mut tx,
        actor,
        AuditEntry {
            action: "catalog.plan_create",
            entity_type: "package_plan",
            entity_id: id,
            before: None,
            after: Some(plan_snapshot(&row)),
            reason: None,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(row)
}

pub async fn update_plan(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    expected_version: i64,
    input: PlanInput,
) -> Result<PlanRow, AppError> {
    actor.require(Permission::CatalogManage)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let before = repo::package_plan(&mut tx, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let (name, gift_note) = validate_plan(&mut tx, &input).await?;
    let fields = PlanFields {
        name: &name,
        price: input.price,
        validity_months: input.validity_months,
        gift_note: gift_note.as_deref(),
        active: input.active,
    };
    if !repo::update_plan(&mut tx, id, expected_version, &fields).await? {
        return Err(if repo::exists(&mut tx, Versioned::Plan, id).await? {
            AppError::StaleVersion
        } else {
            AppError::NotFound
        });
    }
    repo::replace_plan_items(&mut tx, id, &plan_items(&input)).await?;
    let after = repo::package_plan(&mut tx, id)
        .await?
        .ok_or(AppError::NotFound)?;
    audit::record(
        &mut tx,
        actor,
        AuditEntry {
            action: "catalog.plan_update",
            entity_type: "package_plan",
            entity_id: id,
            before: Some(plan_snapshot(&before)),
            after: Some(plan_snapshot(&after)),
            reason: None,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(after)
}
