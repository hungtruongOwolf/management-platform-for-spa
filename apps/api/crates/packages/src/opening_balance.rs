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
use kernel::{Actor, AppError, FieldError, Money, Permission, TenantTx};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::ToSchema;
use uuid::Uuid;

pub const MAX_PACKAGES: usize = 10;
pub const MAX_ITEMS: usize = 20;

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
    /// Price of the package in the paper book, in đồng (per-session value, refund cap).
    pub sold_price: i64,
    /// Money the customer paid for it on paper, in đồng (0 – 10 000 000 000, default 0). Above 0,
    /// it is booked as an ordinary sale + payment on `paid_on`: revenue of that day, in the
    /// ledger, the reports, the history and "Đã chi", like any other sale.
    #[serde(default)]
    pub paid_amount: i64,
    /// Day it was paid (the paper date), required when `paid_amount` > 0; not in the future.
    pub paid_on: Option<NaiveDate>,
    /// Default `cash`.
    pub method: Option<PaperMethodDto>,
    /// Original registration date (not in the future).
    pub starts_on: NaiveDate,
    pub expires_on: Option<NaiveDate>,
    pub items: Vec<OpeningItemCreate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PaperMethodDto {
    Cash,
    BankTransfer,
}

impl PaperMethodDto {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cash => "cash",
            Self::BankTransfer => "bank_transfer",
        }
    }
}

/// What the customer paid on paper for one package, validated: booked as a sale by the caller
/// (`sales::paper`), which owns the money books.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaperPayment {
    pub amount: Money,
    pub paid_on: NaiveDate,
    pub method: PaperMethodDto,
}

/// Checks a paper payment; `None` = nothing was paid. Field names are relative to the package.
pub fn paper_payment(
    amount: i64,
    paid_on: Option<NaiveDate>,
    method: Option<PaperMethodDto>,
    today: NaiveDate,
) -> Result<Option<PaperPayment>, Vec<FieldError>> {
    let mut errors = Vec::new();
    let amount = match Money::vnd(amount) {
        Ok(m) if m.is_enterable() => m,
        Ok(_) => {
            errors.push(FieldError::new("paid_amount", "out_of_range"));
            Money::ZERO
        }
        Err(_) => {
            errors.push(FieldError::new("paid_amount", "negative"));
            Money::ZERO
        }
    };
    let paid = amount > Money::ZERO;
    match paid_on {
        None if paid => errors.push(FieldError::new("paid_on", "required")),
        Some(_) if !paid && errors.is_empty() => {
            errors.push(FieldError::new("paid_on", "not_allowed"));
        }
        Some(d) if d > today => errors.push(FieldError::new("paid_on", "in_future")),
        _ => {}
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(paid_on.filter(|_| paid).map(|paid_on| PaperPayment {
        amount,
        paid_on,
        method: method.unwrap_or(PaperMethodDto::Cash),
    }))
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
        birth_date: c.birth_date,
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
) -> Result<(NewPackage, Option<PaperPayment>), AppError> {
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
        Ok(m) if m.is_enterable() => m,
        Ok(_) => {
            errors.push(FieldError::new("sold_price", "out_of_range"));
            Money::ZERO
        }
        Err(_) => {
            errors.push(FieldError::new("sold_price", "negative"));
            Money::ZERO
        }
    };
    let paid = match paper_payment(p.paid_amount, p.paid_on, p.method, today) {
        Ok(paid) => paid,
        Err(more) => {
            errors.extend(more);
            None
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
        Ok((package, paid))
    } else {
        Err(AppError::Validation(errors))
    }
}

/// What [`import_in`] created: the customer and, per package in request order, its id and the
/// paper payment the caller must book (`sales::paper`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imported {
    pub customer_id: Uuid,
    pub customer_created: bool,
    pub packages: Vec<(Uuid, Option<PaperPayment>)>,
}

/// The opening-balance use case (`opening_balance.import`; `customer.write` too when creating
/// the customer) inside the caller's transaction: resolves or creates the customer and creates
/// the packages (`origin = opening_balance`, no sale). The paper money is validated here and
/// booked by the caller in the same transaction, so a package and its money land together.
/// `sales::paper::import_opening_balance` wraps it for `POST /opening-balances` and the CLI.
///
/// Errors: no/both customer fields → `422 customer_id exactly_one`; unknown customer or plan
/// (or another spa's) → `404`; duplicate phone → `409 customer.phone_taken` + `existing_id`;
/// field errors are prefixed `customer.` / `packages[j].` (e.g.
/// `packages[0].items[0].sessions_remaining exceeds_total`, AC8.4, `packages[0].paid_on required`).
pub async fn import_in(
    tx: &mut TenantTx,
    cipher: &FieldCipher,
    actor: &Actor,
    input: &OpeningBalanceCreate,
    today: NaiveDate,
) -> Result<Imported, AppError> {
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
    let (customer_id, customer_created) = match (&input.customer_id, &input.customer) {
        (Some(id), _) => {
            crm::service::brief_in(tx, *id)
                .await?
                .ok_or(AppError::NotFound)?
                .require_named()?;
            (*id, false)
        }
        (None, Some(c)) => {
            // Packages need an owner the spa can tell apart (no anonymous walk-in here).
            if c.full_name.as_deref().is_none_or(|n| n.trim().is_empty()) {
                // Report the profile's other mistakes in the same answer.
                let mut errors = vec![FieldError::new("customer.full_name", "required")];
                if c.phone
                    .as_deref()
                    .map(str::trim)
                    .is_some_and(|p| !p.is_empty() && kernel::PhoneE164::parse(p).is_err())
                {
                    errors.push(FieldError::new("customer.phone", "invalid_format"));
                }
                return Err(AppError::Validation(errors));
            }
            let created =
                crm::service::create_in(tx, cipher, actor, profile(c), c.health_note.as_deref())
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
        match build(tx, customer_id, p, today).await {
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
    for (j, (package, paid)) in built.iter().enumerate() {
        let p = service::create(tx, actor, package)
            .await
            .map_err(|e| prefixed(&format!("packages[{j}]."), e))?;
        created.push((p.id, *paid));
    }
    audit::record(
        tx,
        actor,
        AuditEntry {
            action: "opening_balance.import",
            entity_type: "customer",
            entity_id: customer_id,
            before: None,
            after: Some(json!({
                "customer_created": customer_created,
                "customer_package_ids": created.iter().map(|(id, _)| id).collect::<Vec<_>>(),
            })),
            reason: None,
        },
    )
    .await?;
    Ok(Imported {
        customer_id,
        customer_created,
        packages: created,
    })
}

/// The response of `POST /opening-balances`, read after the paper money is booked (so each
/// package's `sale_id` is set when it was paid).
pub async fn dto(
    tx: &mut TenantTx,
    imported: &Imported,
    today: NaiveDate,
) -> Result<OpeningBalanceDto, AppError> {
    let mut customer_packages = Vec::with_capacity(imported.packages.len());
    for (id, _) in &imported.packages {
        let p = service::load(tx, *id, today)
            .await?
            .ok_or_else(|| AppError::Internal("imported package vanished".into()))?;
        customer_packages.push(CustomerPackageDto::from(p));
    }
    Ok(OpeningBalanceDto {
        customer_id: imported.customer_id,
        customer_created: imported.customer_created,
        customer_packages,
    })
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
