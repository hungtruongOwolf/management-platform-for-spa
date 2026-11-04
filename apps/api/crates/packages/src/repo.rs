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
                  p.version, p.created_at
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
                     i.sessions_total AS "sessions_total!", i.sessions_remaining, i.per_session_value"#,
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
