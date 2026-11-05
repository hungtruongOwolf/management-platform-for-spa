//! Visit SQL. Tenant-scoped explicitly in every query; RLS is the second layer.
//! `visit_line` is insert-only except its technician (column-level grant).

use chrono::{DateTime, NaiveDate, Utc};
use kernel::{AppError, TenantTx};
use uuid::Uuid;

pub struct VisitInsert<'a> {
    pub id: Uuid,
    pub branch_id: Uuid,
    pub customer_id: Uuid,
    pub business_date: NaiveDate,
    pub check_in_at: DateTime<Utc>,
    pub note_ct: Option<&'a [u8]>,
    pub sale_id: Option<Uuid>,
    pub created_by: Uuid,
}

pub async fn insert_visit(tx: &mut TenantTx, v: &VisitInsert<'_>) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        r#"INSERT INTO app.visit
             (id, tenant_id, branch_id, customer_id, business_date, check_in_at, note_ct, sale_id, created_by)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
        v.id,
        tenant,
        v.branch_id,
        v.customer_id,
        v.business_date,
        v.check_in_at,
        v.note_ct,
        v.sale_id,
        v.created_by
    )
    .execute(tx.conn())
    .await?;
    Ok(())
}

pub struct LineInsert<'a> {
    pub visit_id: Uuid,
    pub position: i32,
    pub service_id: Uuid,
    pub service_name: &'a str,
    pub source: &'a str,
    pub customer_package_id: Option<Uuid>,
    pub customer_package_item_id: Option<Uuid>,
    pub sessions_left_after: Option<i32>,
    pub staff_id: Uuid,
    pub qty: i32,
    pub price_variant: Option<&'a str>,
    pub unit_price: i64,
    pub line_value: i64,
    pub expiry_override_reason: Option<&'a str>,
}

pub async fn insert_line(tx: &mut TenantTx, l: &LineInsert<'_>) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        r#"INSERT INTO app.visit_line
             (id, tenant_id, visit_id, position, service_id, service_name_snapshot, source, customer_package_id,
              customer_package_item_id, sessions_left_after, staff_id, qty, price_variant, unit_price, line_value,
              expiry_override_reason)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)"#,
        Uuid::now_v7(),
        tenant,
        l.visit_id,
        l.position,
        l.service_id,
        l.service_name,
        l.source,
        l.customer_package_id,
        l.customer_package_item_id,
        l.sessions_left_after,
        l.staff_id,
        l.qty,
        l.price_variant,
        l.unit_price,
        l.line_value,
        l.expiry_override_reason
    )
    .execute(tx.conn())
    .await?;
    Ok(())
}

pub struct VisitRow {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub customer_name: String,
    pub customer_gender: Option<String>,
    pub business_date: NaiveDate,
    pub check_in_at: DateTime<Utc>,
    pub note_ct: Option<Vec<u8>>,
    pub status: String,
    pub void_reason: Option<String>,
    pub voided_at: Option<DateTime<Utc>>,
    pub sale_id: Option<Uuid>,
    pub version: i64,
    pub created_at: DateTime<Utc>,
}

/// One visit (`id`) or the visits of a business day (`day`), latest check-in first.
pub async fn visits(
    tx: &mut TenantTx,
    id: Option<Uuid>,
    day: Option<NaiveDate>,
) -> Result<Vec<VisitRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        VisitRow,
        r#"SELECT v.id, v.customer_id, c.full_name AS customer_name, c.gender AS customer_gender, v.business_date, v.check_in_at, v.note_ct,
                  v.status, v.void_reason, v.voided_at, v.sale_id, v.version, v.created_at
           FROM app.visit v
           JOIN app.customer c ON c.tenant_id = v.tenant_id AND c.id = v.customer_id
           WHERE v.tenant_id = $1
             AND ($2::uuid IS NULL OR v.id = $2)
             AND ($3::date IS NULL OR v.business_date = $3)
           ORDER BY v.check_in_at DESC, v.id DESC
           LIMIT 500"#,
        tenant,
        id,
        day
    )
    .fetch_all(tx.conn())
    .await?)
}

/// Locks the visit row for a correction; `None` = not in this tenant.
pub async fn lock_visit(
    tx: &mut TenantTx,
    id: Uuid,
) -> Result<Option<(NaiveDate, String, i64, Uuid)>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query!(
        "SELECT business_date, status, version, customer_id FROM app.visit WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
        tenant,
        id
    )
    .fetch_optional(tx.conn())
    .await?
    .map(|r| (r.business_date, r.status, r.version, r.customer_id)))
}

pub struct LineRow {
    pub id: Uuid,
    pub visit_id: Uuid,
    pub service_id: Uuid,
    pub service_name_snapshot: String,
    pub category_name: String,
    pub source: String,
    pub customer_package_id: Option<Uuid>,
    pub customer_package_item_id: Option<Uuid>,
    pub package_name: Option<String>,
    pub sessions_left_after: Option<i32>,
    pub sessions_total: Option<i32>,
    pub staff_id: Uuid,
    pub staff_name: String,
    pub qty: i32,
    pub price_variant: Option<String>,
    pub unit_price: i64,
    pub line_value: i64,
    pub expiry_override_reason: Option<String>,
}

pub async fn lines(tx: &mut TenantTx, visit_ids: &[Uuid]) -> Result<Vec<LineRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        LineRow,
        r#"SELECT l.id, l.visit_id, l.service_id, l.service_name_snapshot, sc.name AS category_name,
                  l.source, l.customer_package_id,
                  l.customer_package_item_id, p.name_snapshot AS "package_name?", l.sessions_left_after,
                  i.sessions_total AS "sessions_total?", l.staff_id, s.full_name AS staff_name, l.qty,
                  l.price_variant, l.unit_price, l.line_value, l.expiry_override_reason
           FROM app.visit_line l
           JOIN app.staff s ON s.tenant_id = l.tenant_id AND s.id = l.staff_id
           JOIN app.service sv ON sv.tenant_id = l.tenant_id AND sv.id = l.service_id
           JOIN app.service_category sc ON sc.tenant_id = sv.tenant_id AND sc.id = sv.category_id
           LEFT JOIN app.customer_package p ON p.tenant_id = l.tenant_id AND p.id = l.customer_package_id
           LEFT JOIN app.customer_package_item i ON i.tenant_id = l.tenant_id AND i.id = l.customer_package_item_id
           WHERE l.tenant_id = $1 AND l.visit_id = ANY($2)
           ORDER BY l.visit_id, l.position"#,
        tenant,
        visit_ids
    )
    .fetch_all(tx.conn())
    .await?)
}

/// Bumps the version (and the note when `note` is `Some`) if the caller saw the current one.
pub async fn update_visit(
    tx: &mut TenantTx,
    id: Uuid,
    version: i64,
    note_ct: Option<Option<&[u8]>>,
) -> Result<bool, AppError> {
    let tenant = tx.tenant().as_uuid();
    let (set_note, note) = match note_ct {
        Some(n) => (true, n),
        None => (false, None),
    };
    Ok(sqlx::query!(
        r#"UPDATE app.visit
           SET note_ct = CASE WHEN $4 THEN $5 ELSE note_ct END, version = version + 1, updated_at = now()
           WHERE tenant_id = $1 AND id = $2 AND version = $3"#,
        tenant,
        id,
        version,
        set_note,
        note
    )
    .execute(tx.conn())
    .await?
    .rows_affected()
        == 1)
}

/// Technician of one line; the previous one, or `None` when the line is not on this visit.
pub async fn set_line_staff(
    tx: &mut TenantTx,
    visit_id: Uuid,
    line_id: Uuid,
    staff_id: Uuid,
) -> Result<Option<Uuid>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_scalar!(
        r#"UPDATE app.visit_line l SET staff_id = $4
           FROM app.visit_line old
           WHERE l.tenant_id = $1 AND l.visit_id = $2 AND l.id = $3
             AND old.tenant_id = l.tenant_id AND old.id = l.id
           RETURNING old.staff_id"#,
        tenant,
        visit_id,
        line_id,
        staff_id
    )
    .fetch_optional(tx.conn())
    .await?)
}

// ---------------------------------------------------------------- voids (S8)

pub struct LockedVisit {
    pub business_date: NaiveDate,
    pub status: String,
    pub sale_id: Option<Uuid>,
}

/// Locks a visit for a void (the first lock of `POST /visits/{id}/void`).
pub async fn lock_for_void(tx: &mut TenantTx, id: Uuid) -> Result<Option<LockedVisit>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        LockedVisit,
        "SELECT business_date, status, sale_id FROM app.visit WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
        tenant,
        id
    )
    .fetch_optional(tx.conn())
    .await?)
}

/// `completed` → `voided` with the reason. `false` = already voided: the `WHERE status =
/// 'completed'` alone makes a second void impossible.
pub async fn mark_voided(
    tx: &mut TenantTx,
    id: Uuid,
    reason: &str,
    by: Uuid,
) -> Result<bool, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query!(
        r#"UPDATE app.visit
           SET status = 'voided', void_reason = $3, voided_at = now(), voided_by = $4,
               version = version + 1, updated_at = now()
           WHERE tenant_id = $1 AND id = $2 AND status = 'completed'"#,
        tenant,
        id,
        reason,
        by
    )
    .execute(tx.conn())
    .await?
    .rows_affected()
        == 1)
}

pub struct PackageLine {
    pub customer_package_id: Uuid,
    pub customer_package_item_id: Uuid,
    pub qty: i32,
}

/// The package lines of a visit (sessions to give back), in lock order: package, then item.
pub async fn package_lines(
    tx: &mut TenantTx,
    visit_id: Uuid,
) -> Result<Vec<PackageLine>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        PackageLine,
        r#"SELECT customer_package_id AS "customer_package_id!", customer_package_item_id AS "customer_package_item_id!", qty
           FROM app.visit_line
           WHERE tenant_id = $1 AND visit_id = $2 AND source = 'package'
           ORDER BY customer_package_id, customer_package_item_id, position"#,
        tenant,
        visit_id
    )
    .fetch_all(tx.conn())
    .await?)
}
