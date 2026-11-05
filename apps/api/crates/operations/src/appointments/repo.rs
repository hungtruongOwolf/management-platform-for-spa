//! Appointment SQL. Tenant-scoped explicitly in every query; RLS is the second layer.

use chrono::{DateTime, NaiveDate, Utc};
use kernel::{AppError, TenantTx};
use serde_json::Value;
use uuid::Uuid;

/// `tenant.settings` keys of the calendar (missing = defaults).
pub async fn calendar_settings(
    tx: &mut TenantTx,
) -> Result<(Option<Value>, Option<Value>), AppError> {
    let tenant = tx.tenant().as_uuid();
    let row = sqlx::query!(
        r#"SELECT settings -> 'opening_hours' AS opening_hours, settings -> 'slot_minutes' AS slot_minutes
           FROM app.tenant WHERE id = $1"#,
        tenant
    )
    .fetch_optional(tx.conn())
    .await?
    .ok_or(AppError::NotFound)?;
    Ok((row.opening_hours, row.slot_minutes))
}

/// Serializes every booking change of one spa's business day until the transaction ends, so the
/// overlap and capacity checks below cannot both pass for two racing requests.
pub async fn lock_day(tx: &mut TenantTx, day: NaiveDate) -> Result<(), AppError> {
    let key = format!("appointment:{}:{day}", tx.tenant().as_uuid());
    sqlx::query!("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))", key)
        .execute(tx.conn())
        .await?;
    Ok(())
}

pub struct OpenSlot {
    pub id: Uuid,
    pub staff_id: Option<Uuid>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
}

/// Open (booked / arrived) appointments overlapping `[from, to)`, other than `except`.
pub async fn open_overlapping(
    tx: &mut TenantTx,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    except: Option<Uuid>,
) -> Result<Vec<OpenSlot>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        OpenSlot,
        r#"SELECT id, staff_id, starts_at, ends_at FROM app.appointment
           WHERE tenant_id = $1 AND status IN ('booked', 'arrived')
             AND starts_at < $3 AND ends_at > $2
             AND ($4::uuid IS NULL OR id <> $4)
           ORDER BY starts_at, id"#,
        tenant,
        from,
        to,
        except
    )
    .fetch_all(tx.conn())
    .await?)
}

pub struct ServiceInfo {
    pub id: Uuid,
    pub name: String,
    pub duration_min: Option<i32>,
    pub active: bool,
}

/// Catalog facts of the booked services (only this tenant's; missing ids are simply absent).
pub async fn services(tx: &mut TenantTx, ids: &[Uuid]) -> Result<Vec<ServiceInfo>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        ServiceInfo,
        "SELECT id, name, duration_min, active FROM app.service WHERE tenant_id = $1 AND id = ANY($2)",
        tenant,
        ids
    )
    .fetch_all(tx.conn())
    .await?)
}

pub struct AppointmentInsert<'a> {
    pub id: Uuid,
    pub branch_id: Uuid,
    pub customer_id: Option<Uuid>,
    pub guest_name: Option<&'a str>,
    pub guest_phone: Option<&'a str>,
    pub business_date: NaiveDate,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub staff_id: Option<Uuid>,
    pub note: Option<&'a str>,
    pub created_by: Uuid,
}

pub async fn insert(tx: &mut TenantTx, a: &AppointmentInsert<'_>) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        r#"INSERT INTO app.appointment
             (id, tenant_id, branch_id, customer_id, guest_name, guest_phone, business_date, starts_at,
              ends_at, staff_id, note, created_by)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)"#,
        a.id,
        tenant,
        a.branch_id,
        a.customer_id,
        a.guest_name,
        a.guest_phone,
        a.business_date,
        a.starts_at,
        a.ends_at,
        a.staff_id,
        a.note,
        a.created_by
    )
    .execute(tx.conn())
    .await?;
    Ok(())
}

/// Replaces the booked services (position = order given).
pub async fn set_services(
    tx: &mut TenantTx,
    appointment: Uuid,
    services: &[(Uuid, String)],
) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        "DELETE FROM app.appointment_service WHERE tenant_id = $1 AND appointment_id = $2",
        tenant,
        appointment
    )
    .execute(tx.conn())
    .await?;
    for (position, (id, name)) in (0_i32..).zip(services) {
        sqlx::query!(
            r#"INSERT INTO app.appointment_service (tenant_id, appointment_id, position, service_id, service_name_snapshot)
               VALUES ($1, $2, $3, $4, $5)"#,
            tenant,
            appointment,
            position,
            id,
            name
        )
        .execute(tx.conn())
        .await?;
    }
    Ok(())
}

pub struct Locked {
    pub status: String,
    pub business_date: NaiveDate,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub customer_id: Option<Uuid>,
    pub staff_id: Option<Uuid>,
    pub arrived_at: Option<DateTime<Utc>>,
    pub note: Option<String>,
    pub version: i64,
}

/// The appointment row, locked for a change; `None` = not in this tenant.
pub async fn lock(tx: &mut TenantTx, id: Uuid) -> Result<Option<Locked>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        Locked,
        r#"SELECT status, business_date, starts_at, ends_at, customer_id, staff_id, arrived_at, note, version
           FROM app.appointment WHERE tenant_id = $1 AND id = $2 FOR UPDATE"#,
        tenant,
        id
    )
    .fetch_optional(tx.conn())
    .await?)
}

pub struct Reschedule<'a> {
    pub business_date: NaiveDate,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub staff_id: Option<Uuid>,
    pub note: Option<&'a str>,
    /// `Some` links a newcomer to a customer record (guest fields cleared).
    pub customer_id: Option<Uuid>,
}

pub async fn reschedule(
    tx: &mut TenantTx,
    id: Uuid,
    version: i64,
    r: &Reschedule<'_>,
) -> Result<bool, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query!(
        r#"UPDATE app.appointment
           SET business_date = $4, starts_at = $5, ends_at = $6, staff_id = $7, note = $8,
               customer_id = coalesce($9, customer_id),
               guest_name = CASE WHEN $9::uuid IS NULL THEN guest_name END,
               guest_phone = CASE WHEN $9::uuid IS NULL THEN guest_phone END,
               version = version + 1, updated_at = now()
           WHERE tenant_id = $1 AND id = $2 AND version = $3 AND status IN ('booked', 'arrived')"#,
        tenant,
        id,
        version,
        r.business_date,
        r.starts_at,
        r.ends_at,
        r.staff_id,
        r.note,
        r.customer_id
    )
    .execute(tx.conn())
    .await?
    .rows_affected()
        == 1)
}

pub async fn mark_arrived(tx: &mut TenantTx, id: Uuid, staff_id: Uuid) -> Result<bool, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query!(
        r#"UPDATE app.appointment
           SET status = 'arrived', staff_id = $3, arrived_at = now(), version = version + 1, updated_at = now()
           WHERE tenant_id = $1 AND id = $2 AND status = 'booked'"#,
        tenant,
        id,
        staff_id
    )
    .execute(tx.conn())
    .await?
    .rows_affected()
        == 1)
}

/// booked / arrived → `status` (done with its visit, or no_show / cancelled with a reason).
pub async fn close(
    tx: &mut TenantTx,
    id: Uuid,
    status: &str,
    reason: Option<&str>,
    visit_id: Option<Uuid>,
    by: Uuid,
) -> Result<bool, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query!(
        r#"UPDATE app.appointment
           SET status = $3, status_reason = $4, visit_id = $5, closed_at = now(), closed_by = $6,
               version = version + 1, updated_at = now()
           WHERE tenant_id = $1 AND id = $2 AND status IN ('booked', 'arrived')"#,
        tenant,
        id,
        status,
        reason,
        visit_id,
        by
    )
    .execute(tx.conn())
    .await?
    .rows_affected()
        == 1)
}

pub struct AppointmentRow {
    pub id: Uuid,
    pub status: String,
    pub business_date: NaiveDate,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub customer_id: Option<Uuid>,
    pub customer_name: String,
    pub customer_phone: String,
    pub customer_gender: Option<String>,
    pub staff_id: Option<Uuid>,
    pub staff_name: Option<String>,
    pub note: Option<String>,
    pub status_reason: Option<String>,
    pub arrived_at: Option<DateTime<Utc>>,
    pub closed_at: Option<DateTime<Utc>>,
    pub visit_id: Option<Uuid>,
    pub visit_status: Option<String>,
    pub sale_total: Option<i64>,
    pub version: i64,
    pub created_at: DateTime<Utc>,
}

/// One appointment (`id`) or those of `from..=to`, earliest first.
pub async fn appointments(
    tx: &mut TenantTx,
    id: Option<Uuid>,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
) -> Result<Vec<AppointmentRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        AppointmentRow,
        r#"SELECT a.id, a.status, a.business_date, a.starts_at, a.ends_at, a.customer_id,
                  coalesce(c.full_name, a.guest_name) AS "customer_name!",
                  coalesce(c.phone_e164, a.guest_phone) AS "customer_phone!",
                  c.gender AS "customer_gender?", a.staff_id, st.full_name AS "staff_name?",
                  a.note, a.status_reason, a.arrived_at, a.closed_at, a.visit_id,
                  v.status AS "visit_status?", s.total AS "sale_total?", a.version, a.created_at
           FROM app.appointment a
           LEFT JOIN app.customer c ON c.tenant_id = a.tenant_id AND c.id = a.customer_id
           LEFT JOIN app.staff st ON st.tenant_id = a.tenant_id AND st.id = a.staff_id
           LEFT JOIN app.visit v ON v.tenant_id = a.tenant_id AND v.id = a.visit_id
           LEFT JOIN app.sale s ON s.tenant_id = v.tenant_id AND s.id = v.sale_id
           WHERE a.tenant_id = $1
             AND ($2::uuid IS NULL OR a.id = $2)
             AND ($3::date IS NULL OR a.business_date BETWEEN $3 AND $4)
           ORDER BY a.starts_at, a.id
           LIMIT 2000"#,
        tenant,
        id,
        from,
        to
    )
    .fetch_all(tx.conn())
    .await?)
}

pub struct ServiceRow {
    pub appointment_id: Uuid,
    pub service_id: Uuid,
    pub service_name: String,
    pub category_name: String,
    pub duration_min: Option<i32>,
}

pub async fn services_of(tx: &mut TenantTx, ids: &[Uuid]) -> Result<Vec<ServiceRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        ServiceRow,
        r#"SELECT x.appointment_id, x.service_id, x.service_name_snapshot AS service_name,
                  sc.name AS category_name, sv.duration_min
           FROM app.appointment_service x
           JOIN app.service sv ON sv.tenant_id = x.tenant_id AND sv.id = x.service_id
           JOIN app.service_category sc ON sc.tenant_id = sv.tenant_id AND sc.id = sv.category_id
           WHERE x.tenant_id = $1 AND x.appointment_id = ANY($2)
           ORDER BY x.appointment_id, x.position"#,
        tenant,
        ids
    )
    .fetch_all(tx.conn())
    .await?)
}
