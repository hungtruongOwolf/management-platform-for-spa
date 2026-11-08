//! Customer package use cases and the in-transaction API other contexts build on:
//!
//! - [`create`]: a package with its items, from a sale (S6) or from the old paper books (S9,
//!   `Origin::OpeningBalance`, no sale and no payment);
//! - [`consume_session`]: take sessions from one item atomically (S7 visits);
//! - [`default_item`]: which item a visit uses when none was picked (S7, AC4.3).
//!
//! Both run inside the caller's `TenantTx` and leave authorization to the caller.

use crate::domain::{self, PackageStatus, RefundMethod};
use crate::repo::{self, ItemInsert, ItemRow, PackageInsert, PackageRow, RefundRow};
use chrono::{DateTime, NaiveDate, Utc};
use kernel::audit::{self, AuditEntry};
use kernel::{Actor, AppError, FieldError, Money, Permission, TenantTx};
use serde_json::json;
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// Sold in `sale_id`, closed by an active staff member (validated by the caller), or by
    /// nobody (`None`: "khách tự chọn", the customer chose it themselves).
    Sale {
        sale_id: Uuid,
        closer_staff_id: Option<Uuid>,
    },
    /// Carried over from the paper books: no sale, no payment, no revenue (US8).
    OpeningBalance,
}

impl Origin {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sale { .. } => "sale",
            Self::OpeningBalance => "opening_balance",
        }
    }
}

#[derive(Debug, Clone)]
pub struct NewItem {
    pub service_id: Uuid,
    /// Service name at this moment (snapshot).
    pub name: String,
    pub sessions_purchased: u32,
    pub sessions_bonus: u32,
    /// Usually purchased + bonus; less for an opening balance that is part-used.
    pub sessions_remaining: u32,
}

#[derive(Debug, Clone)]
pub struct NewPackage {
    pub customer_id: Uuid,
    pub plan_id: Option<Uuid>,
    pub name: String,
    pub gift_note: Option<String>,
    /// Price actually paid, after discount.
    pub sold_price: Money,
    pub origin: Origin,
    pub starts_on: NaiveDate,
    pub expires_on: Option<NaiveDate>,
    pub items: Vec<NewItem>,
}

const MAX_SESSIONS: u32 = 500;

/// Shape rules for a new package (also run by [`create`]); field names are relative to the package.
pub fn validate(p: &NewPackage) -> Result<(), AppError> {
    let mut errors = Vec::new();
    let name_len = p.name.trim().chars().count();
    if name_len == 0 || name_len > 200 {
        errors.push(FieldError::new("name", "invalid"));
    }
    if p.gift_note
        .as_ref()
        .is_some_and(|g| g.chars().count() > 500)
    {
        errors.push(FieldError::new("gift_note", "too_long"));
    }
    if p.expires_on.is_some_and(|e| e < p.starts_on) {
        errors.push(FieldError::new("expires_on", "before_start"));
    }
    if p.items.is_empty() {
        errors.push(FieldError::new("items", "required"));
    }
    let mut seen = HashSet::new();
    for (i, item) in p.items.iter().enumerate() {
        let total = item.sessions_purchased + item.sessions_bonus;
        if item.sessions_purchased > MAX_SESSIONS
            || item.sessions_bonus > MAX_SESSIONS
            || total == 0
        {
            errors.push(FieldError::new(
                format!("items[{i}].sessions"),
                "out_of_range",
            ));
        }
        if item.sessions_remaining > total {
            errors.push(FieldError::new(
                format!("items[{i}].sessions_remaining"),
                "exceeds_total",
            ));
        }
        if !seen.insert(item.service_id) {
            errors.push(FieldError::new(
                format!("items[{i}].service_id"),
                "duplicate",
            ));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(AppError::Validation(errors))
    }
}

fn to_i32(v: u32) -> Result<i32, AppError> {
    i32::try_from(v).map_err(|_| AppError::Internal("session count overflow".into()))
}

/// Creates a package and its items, snapshotting names and the per-session value
/// (sold price ÷ purchased sessions across all items, D4), and audits it. Customer existence is
/// enforced by the tenant-scoped foreign key; callers should check it first for a clean 404.
pub async fn create(
    tx: &mut TenantTx,
    actor: &Actor,
    p: &NewPackage,
) -> Result<CustomerPackage, AppError> {
    validate(p)?;
    let purchased: u32 = p.items.iter().map(|i| i.sessions_purchased).sum();
    let per_session = domain::per_session_value(p.sold_price, purchased);
    let id = Uuid::now_v7();
    let (sale_id, closer) = match p.origin {
        Origin::Sale {
            sale_id,
            closer_staff_id,
        } => (Some(sale_id), closer_staff_id),
        Origin::OpeningBalance => (None, None),
    };
    repo::insert_package(
        tx,
        &PackageInsert {
            id,
            customer_id: p.customer_id,
            plan_id: p.plan_id,
            name: p.name.trim(),
            gift_note: p.gift_note.as_deref(),
            sold_price: p.sold_price.as_i64(),
            origin: p.origin.as_str(),
            sale_id,
            closer_staff_id: closer,
            starts_on: p.starts_on,
            expires_on: p.expires_on,
            created_by: actor.user_id.as_uuid(),
        },
    )
    .await?;
    for item in &p.items {
        repo::insert_item(
            tx,
            &ItemInsert {
                package_id: id,
                service_id: item.service_id,
                name: &item.name,
                sessions_purchased: to_i32(item.sessions_purchased)?,
                sessions_bonus: to_i32(item.sessions_bonus)?,
                sessions_remaining: to_i32(item.sessions_remaining)?,
                per_session_value: per_session.as_i64(),
            },
        )
        .await?;
    }
    audit::record(
        tx,
        actor,
        AuditEntry {
            action: "customer_package.create",
            entity_type: "customer_package",
            entity_id: id,
            before: None,
            after: Some(json!({
                "customer_id": p.customer_id, "plan_id": p.plan_id, "name": p.name.trim(),
                "origin": p.origin.as_str(), "sale_id": sale_id, "sold_price": p.sold_price.as_i64(),
                "per_session_value": per_session.as_i64(), "starts_on": p.starts_on, "expires_on": p.expires_on,
                "items": p.items.iter().map(|i| json!({
                    "service_id": i.service_id, "purchased": i.sessions_purchased,
                    "bonus": i.sessions_bonus, "remaining": i.sessions_remaining,
                })).collect::<Vec<_>>(),
            })),
            reason: None,
        },
    )
    .await?;
    load(tx, id, p.starts_on).await?.ok_or(AppError::NotFound)
}

/// One session request against a package item (S7).
#[derive(Debug, Clone, Copy)]
pub struct Consume {
    pub item_id: Uuid,
    /// The visit's customer: a package of anyone else is rejected (AC4.8).
    pub customer_id: Uuid,
    pub qty: u32,
    /// The visit's business day, for the expiry check.
    pub today: NaiveDate,
    /// The caller verified `package.use_expired_override` and recorded a reason (AC4.6).
    pub allow_expired: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Consumed {
    pub customer_package_id: Uuid,
    pub service_id: Uuid,
    pub service_name: String,
    pub sessions_total: i32,
    pub sessions_remaining: i32,
    /// Value of the sessions taken: paid sessions at `per_session_value`, bonus ones at 0.
    pub value: Money,
    pub per_session_value: Money,
    /// Package name (snapshot), for receipts and toasts.
    pub package_name: String,
    /// The package was past its expiry and went through only because `allow_expired` was set.
    pub used_expired: bool,
}

/// Takes `qty` sessions from one item in a single atomic `UPDATE … WHERE remaining >= qty`
/// inside the caller's transaction. Two requests racing for the last session: exactly one
/// succeeds, the other gets `409 package.no_sessions_left` (AC4.7).
///
/// Errors: unknown item / other customer's item → `422 package.item_not_found` (AC4.8);
/// cancelled → `409 package.cancelled`; expired without override → `409 package.expired`;
/// not enough sessions → `409 package.no_sessions_left`; `qty` 0 → 422.
/// Call once, before the first `consume_session` of a transaction, with every item it will
/// consume: share-locks their packages in id order (lock order package → item, like voids and
/// cancellations), so consuming never deadlocks with a concurrent void or cancel of the package.
pub async fn lock_for_consumption(tx: &mut TenantTx, item_ids: &[Uuid]) -> Result<(), AppError> {
    repo::share_lock_packages_of_items(tx, item_ids).await
}

pub async fn consume_session(tx: &mut TenantTx, c: Consume) -> Result<Consumed, AppError> {
    if c.qty == 0 || c.qty > MAX_SESSIONS {
        return Err(AppError::Validation(vec![FieldError::new(
            "qty",
            "out_of_range",
        )]));
    }
    let qty = to_i32(c.qty)?;
    if let Some(row) =
        repo::consume(tx, c.item_id, c.customer_id, qty, c.allow_expired, c.today).await?
    {
        let as_u32 = |v: i32| u32::try_from(v).unwrap_or(0);
        let used_before = as_u32(row.sessions_total - row.sessions_remaining - qty);
        let per_session = Money::vnd(row.per_session_value)
            .map_err(|_| AppError::Internal("corrupt per_session_value".into()))?;
        return Ok(Consumed {
            customer_package_id: row.customer_package_id,
            service_id: row.service_id,
            service_name: row.name_snapshot,
            sessions_total: row.sessions_total,
            sessions_remaining: row.sessions_remaining,
            value: domain::consumed_value(
                per_session,
                as_u32(row.sessions_purchased),
                used_before,
                c.qty,
            ),
            per_session_value: per_session,
            package_name: row.package_name,
            used_expired: row.expired,
        });
    }
    let code = match repo::item_state(tx, c.item_id).await? {
        Some(s) if s.customer_id != c.customer_id => None,
        None => None,
        Some(s) if s.cancelled => Some("package.cancelled"),
        Some(s) if !c.allow_expired && s.expires_on.is_some_and(|d| d < c.today) => {
            Some("package.expired")
        }
        Some(_) => Some("package.no_sessions_left"),
    };
    Err(match code {
        Some(code) => AppError::Conflict { code },
        None => AppError::Unprocessable {
            code: "package.item_not_found",
        },
    })
}

/// The item a visit uses when the front desk did not pick a package (AC4.3): among the
/// customer's usable packages containing `service_id`, the one expiring soonest, then the oldest.
/// `None` = no usable package (all exhausted, expired or cancelled — pick one explicitly to get
/// the precise 409, or to use an expired one with an override). Runs in the caller's transaction.
pub async fn default_item(
    tx: &mut TenantTx,
    customer_id: Uuid,
    service_id: Uuid,
    today: NaiveDate,
) -> Result<Option<Uuid>, AppError> {
    repo::default_item(tx, customer_id, service_id, today).await
}

// ---------------------------------------------------------------- read side

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageItem {
    pub id: Uuid,
    pub service_id: Uuid,
    pub name: String,
    /// Current catalog category of the service.
    pub category_name: String,
    pub sessions_purchased: i32,
    pub sessions_bonus: i32,
    pub sessions_total: i32,
    pub sessions_remaining: i32,
    pub per_session_value: Money,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomerPackage {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub plan_id: Option<Uuid>,
    pub name: String,
    pub gift_note: Option<String>,
    pub sold_price: Money,
    pub origin: &'static str,
    pub sale_id: Option<Uuid>,
    pub closer_staff_id: Option<Uuid>,
    pub closer_name: Option<String>,
    pub starts_on: NaiveDate,
    pub expires_on: Option<NaiveDate>,
    pub cancelled_at: Option<DateTime<Utc>>,
    pub cancel_reason: Option<String>,
    /// Money paid back when the package was cancelled (US7), if any.
    pub refund: Option<Refund>,
    pub status: PackageStatus,
    pub sessions_total: i64,
    pub sessions_remaining: i64,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub items: Vec<PackageItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refund {
    pub id: Uuid,
    pub amount: Money,
    pub method: RefundMethod,
    pub business_date: NaiveDate,
    pub created_at: DateTime<Utc>,
}

fn refund(r: RefundRow) -> Result<Refund, AppError> {
    Ok(Refund {
        amount: Money::vnd(r.amount)
            .map_err(|_| AppError::Internal(format!("corrupt refund {}", r.id)))?,
        method: RefundMethod::parse(&r.method)
            .ok_or_else(|| AppError::Internal(format!("refund method {}", r.method)))?,
        id: r.id,
        business_date: r.business_date,
        created_at: r.created_at,
    })
}

fn item(r: ItemRow) -> Result<PackageItem, AppError> {
    Ok(PackageItem {
        per_session_value: Money::vnd(r.per_session_value)
            .map_err(|_| AppError::Internal(format!("corrupt item {}", r.id)))?,
        id: r.id,
        service_id: r.service_id,
        name: r.name_snapshot,
        category_name: r.category_name,
        sessions_purchased: r.sessions_purchased,
        sessions_bonus: r.sessions_bonus,
        sessions_total: r.sessions_total,
        sessions_remaining: r.sessions_remaining,
    })
}

async fn assemble(
    tx: &mut TenantTx,
    rows: Vec<PackageRow>,
    today: NaiveDate,
) -> Result<Vec<CustomerPackage>, AppError> {
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let mut by_package: HashMap<Uuid, Vec<PackageItem>> = HashMap::new();
    for r in repo::items(tx, &ids).await? {
        by_package
            .entry(r.customer_package_id)
            .or_default()
            .push(item(r)?);
    }
    let mut refunds: HashMap<Uuid, Refund> = HashMap::new();
    for r in repo::refunds(tx, &ids).await? {
        refunds.insert(r.customer_package_id, refund(r)?);
    }
    rows.into_iter()
        .map(|r| {
            let items = by_package.remove(&r.id).unwrap_or_default();
            let total: i64 = items.iter().map(|i| i64::from(i.sessions_total)).sum();
            let remaining: i64 = items.iter().map(|i| i64::from(i.sessions_remaining)).sum();
            let origin = match r.origin.as_str() {
                "sale" => "sale",
                "opening_balance" => "opening_balance",
                other => return Err(AppError::Internal(format!("package origin {other}"))),
            };
            Ok(CustomerPackage {
                status: PackageStatus::derive(
                    r.cancelled_at.is_some(),
                    remaining,
                    r.expires_on,
                    today,
                ),
                sold_price: Money::vnd(r.sold_price)
                    .map_err(|_| AppError::Internal(format!("corrupt package {}", r.id)))?,
                sessions_total: total,
                sessions_remaining: remaining,
                id: r.id,
                customer_id: r.customer_id,
                plan_id: r.plan_id,
                name: r.name_snapshot,
                gift_note: r.gift_note,
                origin,
                sale_id: r.sale_id,
                closer_staff_id: r.closer_staff_id,
                closer_name: r.closer_name,
                starts_on: r.starts_on,
                expires_on: r.expires_on,
                cancelled_at: r.cancelled_at,
                refund: refunds.remove(&r.id),
                cancel_reason: r.cancel_reason,
                version: r.version,
                created_at: r.created_at,
                items,
            })
        })
        .collect()
}

pub async fn load(
    tx: &mut TenantTx,
    id: Uuid,
    today: NaiveDate,
) -> Result<Option<CustomerPackage>, AppError> {
    let rows = repo::packages(tx, Some(id), None, None).await?;
    Ok(assemble(tx, rows, today).await?.into_iter().next())
}

/// Packages created by one sale, inside the caller's transaction.
pub async fn of_sale(
    tx: &mut TenantTx,
    sale_id: Uuid,
    today: NaiveDate,
) -> Result<Vec<CustomerPackage>, AppError> {
    let rows = repo::packages(tx, None, None, Some(sale_id)).await?;
    assemble(tx, rows, today).await
}

/// `GET /customer-packages/{id}` (`customer.read`).
pub async fn get(pool: &PgPool, actor: &Actor, id: Uuid) -> Result<CustomerPackage, AppError> {
    actor.require(Permission::CustomerRead)?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let today = kernel::tenant::profile(&mut tx)
        .await?
        .business_date
        .as_naive();
    let found = load(&mut tx, id, today).await?;
    tx.commit().await?;
    found.ok_or(AppError::NotFound)
}

/// `GET /customers/{id}/packages` (`customer.read`): newest first, with remaining sessions.
pub async fn for_customer(
    pool: &PgPool,
    actor: &Actor,
    customer_id: Uuid,
) -> Result<Vec<CustomerPackage>, AppError> {
    actor.require(Permission::CustomerRead)?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    if !crm::service::exists_in(&mut tx, customer_id).await? {
        return Err(AppError::NotFound);
    }
    let today = kernel::tenant::profile(&mut tx)
        .await?
        .business_date
        .as_naive();
    let rows = repo::packages(&mut tx, None, Some(customer_id), None).await?;
    let packages = assemble(&mut tx, rows, today).await?;
    tx.commit().await?;
    Ok(packages)
}

/// The "Ghi chú gói" (F4) of `sale_ids`, decrypted (AAD "sale.note" + tenant + sale), for a
/// caller holding `customer.health_notes.read`; empty for anyone else (deny by default). The
/// caller has authorized the screen that shows them (`customer.read`).
pub async fn sale_notes(
    pool: &PgPool,
    cipher: &kernel::crypto::FieldCipher,
    actor: &Actor,
    sale_ids: &[Uuid],
) -> Result<HashMap<Uuid, String>, AppError> {
    if sale_ids.is_empty() || !actor.can(Permission::CustomerHealthNotesRead) {
        return Ok(HashMap::new());
    }
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let tenant = tx.tenant().as_uuid();
    let rows = sqlx::query!(
        r#"SELECT s.id, s.note_ct AS "note_ct!"
           FROM app.sale s
           WHERE s.tenant_id = $1 AND s.id = ANY($2) AND s.note_ct IS NOT NULL"#,
        tenant,
        sale_ids
    )
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    let mut notes = HashMap::with_capacity(rows.len());
    for r in rows {
        if let Some(text) = kernel::crypto::open_sale_note(cipher, tenant, r.id, Some(&r.note_ct))?
        {
            notes.insert(r.id, text);
        }
    }
    Ok(notes)
}

/// A paper-book package as `sales::paper` needs it to book the paper money.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaperPackage {
    pub customer_id: Uuid,
    pub plan_id: Option<Uuid>,
    pub name: String,
    pub from_paper_book: bool,
    pub sale_id: Option<Uuid>,
    pub cancelled: bool,
}

/// Locks the package (`FOR UPDATE`) inside the caller's transaction; `None` = not in this spa.
/// The caller authorizes.
pub async fn lock_paper_package(
    tx: &mut TenantTx,
    id: Uuid,
) -> Result<Option<PaperPackage>, AppError> {
    Ok(repo::lock_paper_package(tx, id)
        .await?
        .map(|r| PaperPackage {
            customer_id: r.customer_id,
            plan_id: r.plan_id,
            name: r.name_snapshot,
            from_paper_book: r.origin == "opening_balance",
            sale_id: r.sale_id,
            cancelled: r.cancelled,
        }))
}

/// Ties a paper-book package to its paper-money sale; `false` = already tied (or not paper).
pub async fn attach_paper_sale(
    tx: &mut TenantTx,
    id: Uuid,
    sale_id: Uuid,
) -> Result<bool, AppError> {
    repo::attach_paper_sale(tx, id, sale_id).await
}
