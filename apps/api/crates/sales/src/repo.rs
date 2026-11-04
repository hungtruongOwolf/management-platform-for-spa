//! Sales SQL. `payment` and `sale_line` are insert-only (no grant, plus a trigger).

use chrono::{DateTime, NaiveDate, Utc};
use kernel::{AppError, TenantTx};
use uuid::Uuid;

pub struct SaleInsert<'a> {
    pub id: Uuid,
    pub branch_id: Uuid,
    pub customer_id: Uuid,
    pub business_date: NaiveDate,
    pub subtotal: i64,
    pub discount_amount: i64,
    pub discount_reason: Option<&'a str>,
    pub total: i64,
    pub amount_paid: i64,
    pub closer_staff_id: Option<Uuid>,
    pub created_by: Uuid,
}

pub async fn insert_sale(tx: &mut TenantTx, s: &SaleInsert<'_>) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        r#"INSERT INTO app.sale
             (id, tenant_id, branch_id, customer_id, business_date, subtotal, discount_amount, discount_reason,
              total, amount_paid, closer_staff_id, created_by)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)"#,
        s.id,
        tenant,
        s.branch_id,
        s.customer_id,
        s.business_date,
        s.subtotal,
        s.discount_amount,
        s.discount_reason,
        s.total,
        s.amount_paid,
        s.closer_staff_id,
        s.created_by
    )
    .execute(tx.conn())
    .await?;
    Ok(())
}

pub struct PackageLineInsert<'a> {
    pub sale_id: Uuid,
    pub position: i32,
    pub package_plan_id: Uuid,
    pub customer_package_id: Uuid,
    pub description: &'a str,
    pub unit_price: i64,
}

pub async fn insert_package_line(
    tx: &mut TenantTx,
    l: &PackageLineInsert<'_>,
) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        r#"INSERT INTO app.sale_line
             (id, tenant_id, sale_id, line_type, package_plan_id, customer_package_id, description_snapshot,
              qty, unit_price, line_total, position)
           VALUES ($1, $2, $3, 'package', $4, $5, $6, 1, $7, $7, $8)"#,
        Uuid::now_v7(),
        tenant,
        l.sale_id,
        l.package_plan_id,
        l.customer_package_id,
        l.description,
        l.unit_price,
        l.position
    )
    .execute(tx.conn())
    .await?;
    Ok(())
}

pub struct ServiceLineInsert<'a> {
    pub sale_id: Uuid,
    pub position: i32,
    pub service_id: Uuid,
    pub description: &'a str,
    pub qty: i32,
    pub unit_price: i64,
    pub line_total: i64,
}

pub async fn insert_service_line(
    tx: &mut TenantTx,
    l: &ServiceLineInsert<'_>,
) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        r#"INSERT INTO app.sale_line
             (id, tenant_id, sale_id, line_type, service_id, description_snapshot, qty, unit_price, line_total, position)
           VALUES ($1, $2, $3, 'service', $4, $5, $6, $7, $8, $9)"#,
        Uuid::now_v7(),
        tenant,
        l.sale_id,
        l.service_id,
        l.description,
        l.qty,
        l.unit_price,
        l.line_total,
        l.position
    )
    .execute(tx.conn())
    .await?;
    Ok(())
}

pub struct SalePaymentRow {
    pub sale_id: Uuid,
    pub id: Uuid,
    pub kind: String,
    pub method: String,
    pub amount: i64,
    pub reference: Option<String>,
    pub paid_at: DateTime<Utc>,
}

/// Payments of several sales in one query (read models such as the day's visit list).
pub async fn payments_of(
    tx: &mut TenantTx,
    sale_ids: &[Uuid],
) -> Result<Vec<SalePaymentRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        SalePaymentRow,
        r#"SELECT sale_id, id, kind, method, amount, reference, paid_at
           FROM app.payment WHERE tenant_id = $1 AND sale_id = ANY($2) ORDER BY paid_at, id"#,
        tenant,
        sale_ids
    )
    .fetch_all(tx.conn())
    .await?)
}

pub struct PaymentInsert<'a> {
    pub branch_id: Uuid,
    pub sale_id: Uuid,
    pub business_date: NaiveDate,
    pub method: &'a str,
    pub amount: i64,
    pub reference: Option<&'a str>,
    pub created_by: Uuid,
}

pub async fn insert_payment(tx: &mut TenantTx, p: &PaymentInsert<'_>) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        r#"INSERT INTO app.payment
             (id, tenant_id, branch_id, sale_id, business_date, kind, method, amount, reference, created_by)
           VALUES ($1, $2, $3, $4, $5, 'payment', $6, $7, $8, $9)"#,
        Uuid::now_v7(),
        tenant,
        p.branch_id,
        p.sale_id,
        p.business_date,
        p.method,
        p.amount,
        p.reference,
        p.created_by
    )
    .execute(tx.conn())
    .await?;
    Ok(())
}

pub struct SaleRow {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub business_date: NaiveDate,
    pub subtotal: i64,
    pub discount_amount: i64,
    pub discount_reason: Option<String>,
    pub total: i64,
    pub amount_paid: i64,
    pub status: String,
    pub closer_staff_id: Option<Uuid>,
    pub closer_name: Option<String>,
    pub created_at: DateTime<Utc>,
}

pub async fn sale(tx: &mut TenantTx, id: Uuid) -> Result<Option<SaleRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        SaleRow,
        r#"SELECT s.id, s.customer_id, s.business_date, s.subtotal, s.discount_amount, s.discount_reason, s.total,
                  s.amount_paid, s.status, s.closer_staff_id, st.full_name AS "closer_name?", s.created_at
           FROM app.sale s
           LEFT JOIN app.staff st ON st.tenant_id = s.tenant_id AND st.id = s.closer_staff_id
           WHERE s.tenant_id = $1 AND s.id = $2"#,
        tenant,
        id
    )
    .fetch_optional(tx.conn())
    .await?)
}

pub struct LineRow {
    pub id: Uuid,
    pub line_type: String,
    pub package_plan_id: Option<Uuid>,
    pub customer_package_id: Option<Uuid>,
    pub service_id: Option<Uuid>,
    pub description_snapshot: String,
    pub qty: i32,
    pub unit_price: i64,
    pub line_total: i64,
}

pub async fn lines(tx: &mut TenantTx, sale_id: Uuid) -> Result<Vec<LineRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        LineRow,
        r#"SELECT id, line_type, package_plan_id, customer_package_id, service_id, description_snapshot, qty,
                  unit_price, line_total
           FROM app.sale_line WHERE tenant_id = $1 AND sale_id = $2 ORDER BY position"#,
        tenant,
        sale_id
    )
    .fetch_all(tx.conn())
    .await?)
}

pub struct PaymentRow {
    pub id: Uuid,
    pub kind: String,
    pub method: String,
    pub amount: i64,
    pub reference: Option<String>,
    pub paid_at: DateTime<Utc>,
}

pub async fn payments(tx: &mut TenantTx, sale_id: Uuid) -> Result<Vec<PaymentRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        PaymentRow,
        r#"SELECT id, kind, method, amount, reference, paid_at
           FROM app.payment WHERE tenant_id = $1 AND sale_id = $2 ORDER BY paid_at, id"#,
        tenant,
        sale_id
    )
    .fetch_all(tx.conn())
    .await?)
}
