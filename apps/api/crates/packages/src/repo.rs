//! Customer package SQL. Tenant-scoped explicitly in every query; RLS is the second layer.

use chrono::{DateTime, NaiveDate, Utc};
use kernel::{AppError, TenantTx};
use uuid::Uuid;

pub struct PackageRow {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub plan_id: Option<Uuid>,
    pub name_snapshot: String,
    pub gift_note: Option<String>,
    pub sold_price: i64,
    pub origin: String,
    pub sale_id: Option<Uuid>,
    pub closer_staff_id: Option<Uuid>,
    pub closer_name: Option<String>,
    pub starts_on: NaiveDate,
    pub expires_on: Option<NaiveDate>,
    pub cancelled_at: Option<DateTime<Utc>>,
    pub cancel_reason: Option<String>,
    pub version: i64,
    pub created_at: DateTime<Utc>,
}

pub struct ItemRow {
    pub id: Uuid,
    pub customer_package_id: Uuid,
    pub service_id: Uuid,
    pub name_snapshot: String,
    pub sessions_purchased: i32,
    pub sessions_bonus: i32,
    pub sessions_total: i32,
    pub sessions_remaining: i32,
    pub per_session_value: i64,
}

pub struct PackageInsert<'a> {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub plan_id: Option<Uuid>,
    pub name: &'a str,
    pub gift_note: Option<&'a str>,
    pub sold_price: i64,
    pub origin: &'a str,
    pub sale_id: Option<Uuid>,
    pub closer_staff_id: Option<Uuid>,
    pub starts_on: NaiveDate,
    pub expires_on: Option<NaiveDate>,
    pub created_by: Uuid,
}

pub async fn insert_package(tx: &mut TenantTx, p: &PackageInsert<'_>) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        r#"INSERT INTO app.customer_package
             (id, tenant_id, customer_id, plan_id, name_snapshot, gift_note, sold_price, origin, sale_id,
              closer_staff_id, starts_on, expires_on, created_by)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)"#,
        p.id,
        tenant,
        p.customer_id,
        p.plan_id,
        p.name,
        p.gift_note,
        p.sold_price,
        p.origin,
        p.sale_id,
        p.closer_staff_id,
        p.starts_on,
        p.expires_on,
        p.created_by
    )
    .execute(tx.conn())
    .await?;
    Ok(())
}

pub struct ItemInsert<'a> {
    pub package_id: Uuid,
    pub service_id: Uuid,
    pub name: &'a str,
    pub sessions_purchased: i32,
    pub sessions_bonus: i32,
    pub sessions_remaining: i32,
    pub per_session_value: i64,
}

pub async fn insert_item(tx: &mut TenantTx, i: &ItemInsert<'_>) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        r#"INSERT INTO app.customer_package_item
             (id, tenant_id, customer_package_id, service_id, name_snapshot, sessions_purchased, sessions_bonus,
              sessions_remaining, per_session_value)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
        Uuid::now_v7(),
        tenant,
        i.package_id,
        i.service_id,
        i.name,
        i.sessions_purchased,
        i.sessions_bonus,
        i.sessions_remaining,
        i.per_session_value
    )
    .execute(tx.conn())
    .await?;
    Ok(())
}

/// Packages of one customer (`customer`), one package (`id`), or the packages of one sale (`sale`).
pub async fn packages(
    tx: &mut TenantTx,
    id: Option<Uuid>,
    customer: Option<Uuid>,
    sale: Option<Uuid>,
) -> Result<Vec<PackageRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        PackageRow,
        r#"SELECT p.id, p.customer_id, p.plan_id, p.name_snapshot, p.gift_note, p.sold_price, p.origin, p.sale_id,
                  p.closer_staff_id, s.full_name AS "closer_name?", p.starts_on, p.expires_on, p.cancelled_at,
                  p.cancel_reason, p.version, p.created_at
           FROM app.customer_package p
           LEFT JOIN app.staff s ON s.tenant_id = p.tenant_id AND s.id = p.closer_staff_id
           WHERE p.tenant_id = $1
             AND ($2::uuid IS NULL OR p.id = $2)
             AND ($3::uuid IS NULL OR p.customer_id = $3)
             AND ($4::uuid IS NULL OR p.sale_id = $4)
           ORDER BY p.created_at DESC, p.id DESC
           LIMIT 200"#,
        tenant,
        id,
        customer,
        sale
    )
    .fetch_all(tx.conn())
    .await?)
}

pub async fn items(tx: &mut TenantTx, package_ids: &[Uuid]) -> Result<Vec<ItemRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        ItemRow,
        r#"SELECT id, customer_package_id, service_id, name_snapshot, sessions_purchased, sessions_bonus,
                  sessions_total AS "sessions_total!", sessions_remaining, per_session_value
           FROM app.customer_package_item
           WHERE tenant_id = $1 AND customer_package_id = ANY($2)
           ORDER BY customer_package_id, id"#,
        tenant,
        package_ids
    )
    .fetch_all(tx.conn())
    .await?)
}

pub struct ConsumedRow {
    pub customer_package_id: Uuid,
    pub service_id: Uuid,
    pub name_snapshot: String,
    pub sessions_purchased: i32,
    pub sessions_total: i32,
    pub sessions_remaining: i32,
    pub per_session_value: i64,
    pub package_name: String,
    pub expired: bool,
}

/// The atomic decrement. Concurrent callers on the same row serialize on its row lock and the
/// loser re-checks `sessions_remaining >= qty` against the winner's result, so the counter never
/// goes below zero and exactly as many requests succeed as there are sessions.
pub async fn consume(
    tx: &mut TenantTx,
    item_id: Uuid,
    customer_id: Uuid,
    qty: i32,
    allow_expired: bool,
    today: NaiveDate,
) -> Result<Option<ConsumedRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        ConsumedRow,
        r#"UPDATE app.customer_package_item i
           SET sessions_remaining = i.sessions_remaining - $3, version = i.version + 1
           FROM app.customer_package p
           WHERE i.tenant_id = $1 AND i.id = $2
             AND p.tenant_id = i.tenant_id AND p.id = i.customer_package_id
             AND p.customer_id = $4 AND p.cancelled_at IS NULL
             AND ($5 OR p.expires_on IS NULL OR p.expires_on >= $6)
             AND i.sessions_remaining >= $3
           RETURNING i.customer_package_id, i.service_id, i.name_snapshot, i.sessions_purchased,
                     i.sessions_total AS "sessions_total!", i.sessions_remaining, i.per_session_value,
                     p.name_snapshot AS package_name,
                     (p.expires_on IS NOT NULL AND p.expires_on < $6) AS "expired!""#,
        tenant,
        item_id,
        qty,
        customer_id,
        allow_expired,
        today
    )
    .fetch_optional(tx.conn())
    .await?)
}

/// The usable item for `service` that should be used first (AC4.3): not cancelled, not expired,
/// sessions left; soonest expiry first (no expiry = last), then oldest purchase.
pub async fn default_item(
    tx: &mut TenantTx,
    customer_id: Uuid,
    service_id: Uuid,
    today: NaiveDate,
) -> Result<Option<Uuid>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_scalar!(
        r#"SELECT i.id
           FROM app.customer_package_item i
           JOIN app.customer_package p ON p.tenant_id = i.tenant_id AND p.id = i.customer_package_id
           WHERE i.tenant_id = $1 AND p.customer_id = $2 AND i.service_id = $3
             AND p.cancelled_at IS NULL AND (p.expires_on IS NULL OR p.expires_on >= $4)
             AND i.sessions_remaining > 0
           ORDER BY p.expires_on ASC NULLS LAST, p.created_at ASC, p.id ASC
           LIMIT 1"#,
        tenant,
        customer_id,
        service_id,
        today
    )
    .fetch_optional(tx.conn())
    .await?)
}

pub struct ItemState {
    pub customer_id: Uuid,
    pub cancelled: bool,
    pub expires_on: Option<NaiveDate>,
}

/// Why a consume matched nothing: read after the failed update, inside the same transaction.
pub async fn item_state(tx: &mut TenantTx, item_id: Uuid) -> Result<Option<ItemState>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        ItemState,
        r#"SELECT p.customer_id, (p.cancelled_at IS NOT NULL) AS "cancelled!", p.expires_on
           FROM app.customer_package_item i
           JOIN app.customer_package p ON p.tenant_id = i.tenant_id AND p.id = i.customer_package_id
           WHERE i.tenant_id = $1 AND i.id = $2"#,
        tenant,
        item_id
    )
    .fetch_optional(tx.conn())
    .await?)
}

// ---------------------------------------------------------------- lifecycle (S8)

pub struct LockedPackage {
    pub customer_id: Uuid,
    pub sold_price: i64,
    pub expires_on: Option<NaiveDate>,
    pub cancelled: bool,
    pub version: i64,
}

/// Locks one package for an extension or a cancellation. Lock order everywhere in S8: package
/// rows (by id) before their item rows, so concurrent lifecycle changes cannot deadlock.
pub async fn lock_package(tx: &mut TenantTx, id: Uuid) -> Result<Option<LockedPackage>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        LockedPackage,
        r#"SELECT customer_id, sold_price, expires_on, (cancelled_at IS NOT NULL) AS "cancelled!", version
           FROM app.customer_package WHERE tenant_id = $1 AND id = $2 FOR UPDATE"#,
        tenant,
        id
    )
    .fetch_optional(tx.conn())
    .await?)
}

pub struct LockedOfSale {
    pub id: Uuid,
    pub cancelled: bool,
}

/// Locks the packages created by one sale, in id order.
pub async fn lock_packages_of_sale(
    tx: &mut TenantTx,
    sale_id: Uuid,
) -> Result<Vec<LockedOfSale>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        LockedOfSale,
        r#"SELECT id, (cancelled_at IS NOT NULL) AS "cancelled!"
           FROM app.customer_package WHERE tenant_id = $1 AND sale_id = $2
           ORDER BY id FOR UPDATE"#,
        tenant,
        sale_id
    )
    .fetch_all(tx.conn())
    .await?)
}

pub struct LockedItem {
    pub sessions_total: i32,
    pub sessions_remaining: i32,
}

/// Locks the items of `package_ids` (after their packages), in id order.
pub async fn lock_items(
    tx: &mut TenantTx,
    package_ids: &[Uuid],
) -> Result<Vec<LockedItem>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        LockedItem,
        r#"SELECT sessions_total AS "sessions_total!", sessions_remaining
           FROM app.customer_package_item WHERE tenant_id = $1 AND customer_package_id = ANY($2)
           ORDER BY id FOR UPDATE"#,
        tenant,
        package_ids
    )
    .fetch_all(tx.conn())
    .await?)
}

/// Cancels a locked package: remaining sessions go to 0 so nothing can be consumed any more
/// (a concurrent consume re-checks `sessions_remaining >= qty` on the item row and fails).
pub async fn cancel_package(
    tx: &mut TenantTx,
    id: Uuid,
    reason: &str,
    by: Uuid,
) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        r#"UPDATE app.customer_package_item SET sessions_remaining = 0, version = version + 1
           WHERE tenant_id = $1 AND customer_package_id = $2 AND sessions_remaining > 0"#,
        tenant,
        id
    )
    .execute(tx.conn())
    .await?;
    let done = sqlx::query!(
        r#"UPDATE app.customer_package
           SET cancelled_at = now(), cancel_reason = $3, cancelled_by = $4, version = version + 1, updated_at = now()
           WHERE tenant_id = $1 AND id = $2 AND cancelled_at IS NULL"#,
        tenant,
        id,
        reason,
        by
    )
    .execute(tx.conn())
    .await?
    .rows_affected();
    if done == 1 {
        Ok(())
    } else {
        Err(AppError::Conflict {
            code: "package.cancelled",
        })
    }
}

/// New expiry for a locked, live package. `false` = the version moved on (412).
pub async fn set_expiry(
    tx: &mut TenantTx,
    id: Uuid,
    version: i64,
    expires_on: NaiveDate,
) -> Result<bool, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query!(
        r#"UPDATE app.customer_package SET expires_on = $4, version = version + 1, updated_at = now()
           WHERE tenant_id = $1 AND id = $2 AND version = $3 AND cancelled_at IS NULL"#,
        tenant,
        id,
        version,
        expires_on
    )
    .execute(tx.conn())
    .await?
    .rows_affected()
        == 1)
}

pub struct ItemPackage {
    pub customer_package_id: Uuid,
    pub cancelled: bool,
}

/// Locks the package an item belongs to (package before item, as everywhere in S8).
pub async fn lock_package_of_item(
    tx: &mut TenantTx,
    item_id: Uuid,
) -> Result<Option<ItemPackage>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        ItemPackage,
        r#"SELECT p.id AS customer_package_id, (p.cancelled_at IS NOT NULL) AS "cancelled!"
           FROM app.customer_package p
           JOIN app.customer_package_item i ON i.tenant_id = p.tenant_id AND i.customer_package_id = p.id
           WHERE p.tenant_id = $1 AND i.id = $2
           FOR UPDATE OF p"#,
        tenant,
        item_id
    )
    .fetch_optional(tx.conn())
    .await?)
}

pub struct RestoredRow {
    pub sessions_remaining: i32,
    pub sessions_total: i32,
}

/// The atomic re-credit: `remaining + qty` only while it stays within the item's total.
pub async fn restore(
    tx: &mut TenantTx,
    item_id: Uuid,
    qty: i32,
) -> Result<Option<RestoredRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        RestoredRow,
        r#"UPDATE app.customer_package_item
           SET sessions_remaining = sessions_remaining + $3, version = version + 1
           WHERE tenant_id = $1 AND id = $2 AND sessions_remaining + $3 <= sessions_total
           RETURNING sessions_remaining, sessions_total AS "sessions_total!""#,
        tenant,
        item_id,
        qty
    )
    .fetch_optional(tx.conn())
    .await?)
}

pub struct RefundInsert<'a> {
    pub branch_id: Uuid,
    pub customer_id: Uuid,
    pub customer_package_id: Uuid,
    pub business_date: NaiveDate,
    pub method: &'a str,
    pub amount: i64,
    pub reason: &'a str,
    pub created_by: Uuid,
}

pub async fn insert_refund(tx: &mut TenantTx, r: &RefundInsert<'_>) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        r#"INSERT INTO app.refund
             (id, tenant_id, branch_id, customer_id, customer_package_id, business_date, method, amount, reason,
              created_by)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#,
        Uuid::now_v7(),
        tenant,
        r.branch_id,
        r.customer_id,
        r.customer_package_id,
        r.business_date,
        r.method,
        r.amount,
        r.reason,
        r.created_by
    )
    .execute(tx.conn())
    .await?;
    Ok(())
}

pub struct RefundRow {
    pub id: Uuid,
    pub customer_package_id: Uuid,
    pub business_date: NaiveDate,
    pub method: String,
    pub amount: i64,
    pub created_at: DateTime<Utc>,
}

pub async fn refunds(tx: &mut TenantTx, package_ids: &[Uuid]) -> Result<Vec<RefundRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        RefundRow,
        r#"SELECT id, customer_package_id, business_date, method, amount, created_at
           FROM app.refund WHERE tenant_id = $1 AND customer_package_id = ANY($2)"#,
        tenant,
        package_ids
    )
    .fetch_all(tx.conn())
    .await?)
}
