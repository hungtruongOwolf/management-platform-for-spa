//! Opening balance (US8, S9): carry packages a customer is part-way through over from the paper
//! books. One transaction resolves or creates the customer and creates every package with
//! `origin = opening_balance` — no sale, no payment, so revenue and the cash book never see it
//! (AC8.1, AC8.3).

use crate::http::CustomerPackageDto;
use crate::service::{self, NewItem, NewPackage, Origin};
use chrono::NaiveDate;
use crm::FieldCipher;
use crm::http::CustomerCreate;
use crm::service::ProfileInput;
use kernel::audit::{self, AuditEntry};
use kernel::idempotency::{self, Claim};
use kernel::{Actor, AppError, FieldError, Money, Permission, TenantTx};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

pub const MAX_PACKAGES: usize = 10;
pub const MAX_ITEMS: usize = 20;
/// Ten billion đồng: far above any real package, low enough to catch a run of typo'd zeros.
const MAX_SOLD_PRICE: i64 = 10_000_000_000;

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OpeningItemCreate {
    pub service_id: Uuid,
    /// Paid sessions in the original package.
    pub sessions_purchased: u32,
    /// Bonus sessions in the original package (default 0).
    #[serde(default)]
    pub sessions_bonus: u32,
    /// Sessions still unused today, ≤ purchased + bonus.
    pub sessions_remaining: u32,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OpeningPackageCreate {
    /// The current plan this old package matches, if any (reference only: items and price are
    /// what the paper book says).
    pub package_plan_id: Option<Uuid>,
    /// Package name; defaults to the plan's name. Required without a plan.
    pub name: Option<String>,
    pub gift_note: Option<String>,
    /// Price the customer originally paid, in đồng (for the per-session value; not revenue).
    pub sold_price: i64,
    /// Original registration date (not in the future).
    pub starts_on: NaiveDate,
    pub expires_on: Option<NaiveDate>,
    pub items: Vec<OpeningItemCreate>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OpeningBalanceCreate {
    /// An existing customer of this spa. Send exactly one of `customer_id` and `customer`.
    pub customer_id: Option<Uuid>,
    /// A new customer profile, created in the same transaction (needs `customer.write`).
    pub customer: Option<CustomerCreate>,
    /// 1–10 packages.
    pub packages: Vec<OpeningPackageCreate>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OpeningBalanceDto {
    pub customer_id: Uuid,
    /// True when `customer` was sent and a new profile was created.
    pub customer_created: bool,
    pub customer_packages: Vec<CustomerPackageDto>,
}

fn prefixed(prefix: &str, error: AppError) -> AppError {
    match error {
        AppError::Validation(errors) => AppError::Validation(
            errors
                .into_iter()
                .map(|e| FieldError::new(format!("{prefix}{}", e.field), e.code))
                .collect(),
        ),
        other => other,
    }
}

fn trimmed(s: Option<&String>) -> Option<String> {
    s.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty())
}

fn profile(c: &CustomerCreate) -> ProfileInput {
    ProfileInput {
        full_name: c.full_name.clone(),
        phone: c.phone.clone(),
        gender: c.gender.map(Into::into),
        birth_year: c.birth_year,
        address: c.address.clone(),
        source: c.source.clone(),
        note: c.note.clone(),
    }
}

/// Turns one requested package into a `NewPackage`, reading plan and service names from the
/// catalog inside the transaction. Field errors are relative to the package.
async fn build(
    tx: &mut TenantTx,
    customer_id: Uuid,
    p: &OpeningPackageCreate,
    today: NaiveDate,
) -> Result<NewPackage, AppError> {
    let mut errors = Vec::new();
    let plan = match p.package_plan_id {
        Some(id) => Some(
            catalog::service::plan_snapshot(tx, id)
                .await?
                .ok_or(AppError::NotFound)?,
        ),
        None => None,
    };
    let name = trimmed(p.name.as_ref())
        .or_else(|| plan.as_ref().map(|plan| plan.name.clone()))
        .unwrap_or_else(|| {
            errors.push(FieldError::new("name", "required"));
            String::new()
        });
    let sold_price = match Money::vnd(p.sold_price) {
        Ok(m) if p.sold_price <= MAX_SOLD_PRICE => m,
        Ok(_) => {
            errors.push(FieldError::new("sold_price", "out_of_range"));
            Money::ZERO
        }
        Err(_) => {
            errors.push(FieldError::new("sold_price", "negative"));
            Money::ZERO
        }
    };
    if p.starts_on > today {
        errors.push(FieldError::new("starts_on", "in_future"));
    }
    // Bounds the catalog lookups below (one per item); real packages have a handful of services.
    if p.items.len() > MAX_ITEMS {
        return Err(AppError::Validation(vec![FieldError::new(
            "items",
            "count_out_of_range",
        )]));
    }
    let mut items = Vec::with_capacity(p.items.len());
    for (i, item) in p.items.iter().enumerate() {
        let name = catalog::service::service_name(tx, item.service_id)
            .await?
            .unwrap_or_else(|| {
                errors.push(FieldError::new(
                    format!("items[{i}].service_id"),
                    "not_found",
                ));
                String::new()
            });
        items.push(NewItem {
            service_id: item.service_id,
            name,
            sessions_purchased: item.sessions_purchased,
            sessions_bonus: item.sessions_bonus,
            sessions_remaining: item.sessions_remaining,
        });
    }
    let package = NewPackage {
        customer_id,
        plan_id: plan.map(|plan| plan.id),
        name,
        gift_note: trimmed(p.gift_note.as_ref()),
        sold_price,
        origin: Origin::OpeningBalance,
        starts_on: p.starts_on,
        expires_on: p.expires_on,
        items,
    };
    // Shape rules (remaining ≤ total, duplicate service, ranges, expiry ≥ start) come from the
    // packages context so a sale and an import can never disagree on them.
    if let Err(AppError::Validation(more)) = service::validate(&package) {
        let name_reported = errors.iter().any(|e| e.field == "name");
        errors.extend(
            more.into_iter()
                .filter(|e| !(name_reported && e.field == "name")),
        );
    }
    if errors.is_empty() {
        Ok(package)
    } else {
        Err(AppError::Validation(errors))
    }
}

/// `POST /opening-balances` (`opening_balance.import`; `customer.write` too when creating the
/// customer). Returns the HTTP status and body, also stored under the idempotency key so a
/// double submit replays them without importing twice.
///
/// Errors: no/both customer fields → `422 customer_id exactly_one`; unknown customer or plan
/// (or another spa's) → `404`; duplicate phone → `409 customer.phone_taken` + `existing_id`;
/// field errors are prefixed `customer.` / `packages[j].` (e.g.
/// `packages[0].items[0].sessions_remaining exceeds_total`, AC8.4).
pub async fn import(
    pool: &PgPool,
    cipher: &FieldCipher,
    actor: &Actor,
    key: Uuid,
    request_hash: [u8; 32],
    input: &OpeningBalanceCreate,
) -> Result<(u16, Value), AppError> {
    actor.require(Permission::OpeningBalanceImport)?;
    let mut shape = Vec::new();
    if input.customer_id.is_some() == input.customer.is_some() {
        shape.push(FieldError::new("customer_id", "exactly_one"));
    }
    if input.packages.is_empty() || input.packages.len() > MAX_PACKAGES {
        shape.push(FieldError::new("packages", "count_out_of_range"));
    }
    if !shape.is_empty() {
        return Err(AppError::Validation(shape));
    }

    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    if let Claim::Replay { status, body } =
        idempotency::claim(&mut tx, actor, key, request_hash).await?
    {
        tx.commit().await?;
        return Ok((status, body));
    }
    let today = kernel::tenant::profile(&mut tx)
        .await?
        .business_date
        .as_naive();

    let (customer_id, customer_created) = match (&input.customer_id, &input.customer) {
        (Some(id), _) => {
            if !crm::service::exists_in(&mut tx, *id).await? {
                return Err(AppError::NotFound);
            }
            (*id, false)
        }
        (None, Some(c)) => {
            let created = crm::service::create_in(
                &mut tx,
                cipher,
                actor,
                profile(c),
                c.health_note.as_deref(),
            )
            .await
            .map_err(|e| prefixed("customer.", e))?;
            (created.row.id, true)
        }
        (None, None) => return Err(AppError::Internal("customer shape unchecked".into())),
    };

    // Build and check every package before writing any, so one response lists all mistakes.
    let mut built = Vec::with_capacity(input.packages.len());
    let mut errors = Vec::new();
    for (j, p) in input.packages.iter().enumerate() {
        match build(&mut tx, customer_id, p, today).await {
            Ok(package) => built.push(package),
            Err(e) => match prefixed(&format!("packages[{j}]."), e) {
                AppError::Validation(more) => errors.extend(more),
                other => return Err(other),
            },
        }
    }
    if !errors.is_empty() {
        return Err(AppError::Validation(errors));
    }

    let mut created = Vec::with_capacity(built.len());
    for (j, package) in built.iter().enumerate() {
        let p = service::create(&mut tx, actor, package)
            .await
            .map_err(|e| prefixed(&format!("packages[{j}]."), e))?;
        created.push(p);
    }
    audit::record(
        &mut tx,
        actor,
        AuditEntry {
            action: "opening_balance.import",
            entity_type: "customer",
            entity_id: customer_id,
            before: None,
            after: Some(json!({
                "customer_created": customer_created,
                "customer_package_ids": created.iter().map(|p| p.id).collect::<Vec<_>>(),
            })),
            reason: None,
        },
    )
    .await?;

    let dto = OpeningBalanceDto {
        customer_id,
        customer_created,
        customer_packages: created.into_iter().map(CustomerPackageDto::from).collect(),
    };
    let body = serde_json::to_value(&dto).map_err(|e| AppError::Internal(e.to_string()))?;
    idempotency::complete(&mut tx, actor, key, 201, &body).await?;
    tx.commit().await?;
    Ok((201, body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_only_field_errors() {
        let e = prefixed(
            "packages[1].",
            AppError::Validation(vec![FieldError::new(
                "items[0].sessions_remaining",
                "exceeds_total",
            )]),
        );
        let AppError::Validation(errors) = e else {
            panic!("expected validation");
        };
        assert_eq!(errors[0].field, "packages[1].items[0].sessions_remaining");
        assert!(matches!(
            prefixed("customer.", AppError::NotFound),
            AppError::NotFound
        ));
    }
}
