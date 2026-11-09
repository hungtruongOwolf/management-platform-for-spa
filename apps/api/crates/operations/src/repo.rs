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
    /// "Sửa lượt": the voided visit this one replaces.
    pub corrects_visit_id: Option<Uuid>,
    /// Companions known only by a note.
    pub companions: &'a [String],
}

pub async fn insert_visit(tx: &mut TenantTx, v: &VisitInsert<'_>) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        r#"INSERT INTO app.visit
             (id, tenant_id, branch_id, customer_id, business_date, check_in_at, note_ct, sale_id, created_by,
              corrects_visit_id, companions)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"#,
        v.id,
        tenant,
        v.branch_id,
        v.customer_id,
        v.business_date,
        v.check_in_at,
        v.note_ct,
        v.sale_id,
        v.created_by,
        v.corrects_visit_id,
        v.companions
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
    pub discount_amount: i64,
    /// `one_off` lines: the sale that charged them.
    pub sale_id: Option<Uuid>,
    /// `one_off` lines: who recommended them (upsell credit).
    pub referrer_staff_id: Option<Uuid>,
}

pub async fn insert_line(tx: &mut TenantTx, l: &LineInsert<'_>) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        r#"INSERT INTO app.visit_line
             (id, tenant_id, visit_id, position, service_id, service_name_snapshot, source, customer_package_id,
              customer_package_item_id, sessions_left_after, staff_id, qty, price_variant, unit_price, line_value,
              expiry_override_reason, discount_amount, sale_id, referrer_staff_id)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19)"#,
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
        l.expiry_override_reason,
        l.discount_amount,
        l.sale_id,
        l.referrer_staff_id
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
    pub discount_reason: Option<String>,
    pub corrects_visit_id: Option<Uuid>,
    pub corrected_by_visit_id: Option<Uuid>,
    pub companions: Vec<String>,
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
        r#"SELECT v.id, v.customer_id, app.customer_label(c.full_name) AS "customer_name!", c.gender AS customer_gender, v.business_date, v.check_in_at, v.note_ct,
                  v.status, v.void_reason, v.voided_at, v.sale_id,
                  s.discount_reason AS "discount_reason?",
                  v.corrects_visit_id, n.id AS "corrected_by_visit_id?", v.companions,
                  v.version, v.created_at
           FROM app.visit v
           JOIN app.customer c ON c.tenant_id = v.tenant_id AND c.id = v.customer_id
           LEFT JOIN app.sale s ON s.tenant_id = v.tenant_id AND s.id = v.sale_id
           LEFT JOIN app.visit n ON n.tenant_id = v.tenant_id AND n.corrects_visit_id = v.id
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

pub struct LockedVisit {
    pub customer_id: Uuid,
    pub business_date: NaiveDate,
    pub check_in_at: DateTime<Utc>,
    pub status: String,
    pub sale_id: Option<Uuid>,
    pub note_ct: Option<Vec<u8>>,
    pub version: i64,
    /// The visit that corrected this one, if any.
    pub corrected_by: Option<Uuid>,
}

/// Locks a visit row: the first lock of every change (`PATCH`, void, correction), so two changes
/// of one visit serialize here. `None` = not in this tenant.
pub async fn lock(tx: &mut TenantTx, id: Uuid) -> Result<Option<LockedVisit>, AppError> {
    let tenant = tx.tenant().as_uuid();
    let Some(row) = sqlx::query!(
        r#"SELECT customer_id, business_date, check_in_at, status, sale_id, note_ct, version
           FROM app.visit WHERE tenant_id = $1 AND id = $2 FOR UPDATE"#,
        tenant,
        id
    )
    .fetch_optional(tx.conn())
    .await?
    else {
        return Ok(None);
    };
    // A statement of its own, after the lock: when this request waited for a concurrent
    // correction, only a fresh snapshot sees the visit that correction inserted.
    let corrected_by = if row.status == "voided" {
        sqlx::query_scalar!(
            "SELECT id FROM app.visit WHERE tenant_id = $1 AND corrects_visit_id = $2",
            tenant,
            id
        )
        .fetch_optional(tx.conn())
        .await?
    } else {
        None
    };
    Ok(Some(LockedVisit {
        customer_id: row.customer_id,
        business_date: row.business_date,
        check_in_at: row.check_in_at,
        status: row.status,
        sale_id: row.sale_id,
        note_ct: row.note_ct,
        version: row.version,
        corrected_by,
    }))
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
    pub discount_amount: i64,
    pub expiry_override_reason: Option<String>,
    pub sale_id: Option<Uuid>,
    pub referrer_staff_id: Option<Uuid>,
    pub referrer_name: Option<String>,
}

pub async fn lines(tx: &mut TenantTx, visit_ids: &[Uuid]) -> Result<Vec<LineRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        LineRow,
        r#"SELECT l.id, l.visit_id, l.service_id, l.service_name_snapshot, sc.name AS category_name,
                  l.source, l.customer_package_id,
                  l.customer_package_item_id, p.name_snapshot AS "package_name?", l.sessions_left_after,
                  i.sessions_total AS "sessions_total?", l.staff_id, s.full_name AS staff_name, l.qty,
                  l.price_variant, l.unit_price, l.line_value, l.discount_amount, l.expiry_override_reason,
                  l.sale_id, l.referrer_staff_id, r.full_name AS "referrer_name?"
           FROM app.visit_line l
           JOIN app.staff s ON s.tenant_id = l.tenant_id AND s.id = l.staff_id
           LEFT JOIN app.staff r ON r.tenant_id = l.tenant_id AND r.id = l.referrer_staff_id
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

/// Bumps the version (and the note when `note` is `Some`, the service time when `check_in_at` is
/// `Some`) if the caller saw the current one.
pub async fn update_visit(
    tx: &mut TenantTx,
    id: Uuid,
    version: i64,
    note_ct: Option<Option<&[u8]>>,
    check_in_at: Option<DateTime<Utc>>,
    companions: Option<&[String]>,
) -> Result<bool, AppError> {
    let tenant = tx.tenant().as_uuid();
    let (set_note, note) = match note_ct {
        Some(n) => (true, n),
        None => (false, None),
    };
    Ok(sqlx::query!(
        r#"UPDATE app.visit
           SET note_ct = CASE WHEN $4 THEN $5 ELSE note_ct END,
               check_in_at = coalesce($6, check_in_at),
               companions = coalesce($7, companions),
               version = version + 1, updated_at = now()
           WHERE tenant_id = $1 AND id = $2 AND version = $3"#,
        tenant,
        id,
        version,
        set_note,
        note,
        check_in_at,
        companions
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

// ---------------------------------------------------------------- round 6: sales of a visit

/// Position for the next line appended to a visit (additions).
pub async fn next_position(tx: &mut TenantTx, visit_id: Uuid) -> Result<i32, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_scalar!(
        r#"SELECT coalesce(max(position) + 1, 0) AS "next!" FROM app.visit_line
           WHERE tenant_id = $1 AND visit_id = $2"#,
        tenant,
        visit_id
    )
    .fetch_one(tx.conn())
    .await?)
}

/// A new version of the visit (an addition changed it): a desk holding the old `ETag` gets `412`.
pub async fn bump_version(tx: &mut TenantTx, id: Uuid) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        "UPDATE app.visit SET version = version + 1, updated_at = now() WHERE tenant_id = $1 AND id = $2",
        tenant,
        id
    )
    .execute(tx.conn())
    .await?;
    Ok(())
}

/// The one-off sales of a visit still open (its first sale and every addition), oldest first:
/// what a void of the visit reverses. Packages sold in the visit are not among them.
pub async fn open_service_sales(tx: &mut TenantTx, visit_id: Uuid) -> Result<Vec<Uuid>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_scalar!(
        r#"SELECT s.id FROM app.sale s
           WHERE s.tenant_id = $1 AND s.status = 'open'
             AND s.id IN (SELECT l.sale_id FROM app.visit_line l
                          WHERE l.tenant_id = $1 AND l.visit_id = $2 AND l.sale_id IS NOT NULL
                          UNION
                          SELECT v.sale_id FROM app.visit v
                          WHERE v.tenant_id = $1 AND v.id = $2 AND v.sale_id IS NOT NULL)
           ORDER BY s.id"#,
        tenant,
        visit_id
    )
    .fetch_all(tx.conn())
    .await?)
}

/// Sale ids charging the one-off lines of these visits (for their payments).
pub async fn service_sales_of(
    tx: &mut TenantTx,
    visit_ids: &[Uuid],
) -> Result<Vec<(Uuid, Uuid)>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query!(
        r#"SELECT DISTINCT l.visit_id, l.sale_id AS "sale_id!" FROM app.visit_line l
           WHERE l.tenant_id = $1 AND l.visit_id = ANY($2) AND l.sale_id IS NOT NULL
           UNION
           SELECT v.id, v.sale_id AS "sale_id!" FROM app.visit v
           WHERE v.tenant_id = $1 AND v.id = ANY($2) AND v.sale_id IS NOT NULL"#,
        tenant,
        visit_ids
    )
    .fetch_all(tx.conn())
    .await?
    .into_iter()
    .filter_map(|r| Some((r.visit_id?, r.sale_id)))
    .collect())
}

pub struct PackageSaleRow {
    pub visit_id: Uuid,
    pub sale_id: Uuid,
    pub status: String,
    pub business_date: NaiveDate,
    pub description: String,
    pub customer_package_ids: Vec<Uuid>,
    pub total: i64,
    pub closer_staff_id: Option<Uuid>,
    pub closer_name: Option<String>,
    pub beneficiary_customer_id: Option<Uuid>,
    pub beneficiary_name: Option<String>,
    /// "Ghi chú gói" (F4), still encrypted.
    pub note_ct: Option<Vec<u8>>,
    pub version: i64,
}

/// Package sales made in these visits (`sale.visit_id`), oldest first. The beneficiary is set only
/// when the packages belong to someone other than the payer.
pub async fn package_sales(
    tx: &mut TenantTx,
    visit_ids: &[Uuid],
) -> Result<Vec<PackageSaleRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        PackageSaleRow,
        r#"SELECT s.visit_id AS "visit_id!", s.id AS sale_id, s.status, s.business_date,
                  d.description AS "description!", d.packages AS "customer_package_ids!",
                  s.total, s.closer_staff_id, st.full_name AS "closer_name?",
                  b.id AS "beneficiary_customer_id?",
                  -- customer_label(NULL) reads "Khách vãng lai": only label a beneficiary that exists.
                  CASE WHEN b.id IS NOT NULL THEN app.customer_label(b.full_name) END AS "beneficiary_name?",
                  s.note_ct, s.version
           FROM app.sale s
           CROSS JOIN LATERAL (
             SELECT string_agg(sl.description_snapshot, ', ' ORDER BY sl.position) AS description,
                    array_agg(sl.customer_package_id ORDER BY sl.position) AS packages
             FROM app.sale_line sl
             WHERE sl.tenant_id = s.tenant_id AND sl.sale_id = s.id AND sl.line_type = 'package'
           ) d
           LEFT JOIN app.staff st ON st.tenant_id = s.tenant_id AND st.id = s.closer_staff_id
           LEFT JOIN LATERAL (
             SELECT c.id, c.full_name FROM app.customer_package cp
             JOIN app.customer c ON c.tenant_id = cp.tenant_id AND c.id = cp.customer_id
             WHERE cp.tenant_id = s.tenant_id AND cp.sale_id = s.id AND cp.customer_id <> s.customer_id
             LIMIT 1
           ) b ON true
           WHERE s.tenant_id = $1 AND s.visit_id = ANY($2) AND d.description IS NOT NULL
           ORDER BY s.id"#,
        tenant,
        visit_ids
    )
    .fetch_all(tx.conn())
    .await?)
}
