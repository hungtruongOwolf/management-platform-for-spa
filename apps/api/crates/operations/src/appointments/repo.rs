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

/// The first open (booked / arrived) appointment of `staff` overlapping `[from, to)`, other than
/// `except`.
pub async fn staff_overlap(
    tx: &mut TenantTx,
    staff: Uuid,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    except: Option<Uuid>,
) -> Result<Option<Uuid>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_scalar!(
        r#"SELECT id FROM app.appointment
           WHERE tenant_id = $1 AND staff_id = $2 AND status IN ('booked', 'arrived')
             AND starts_at < $4 AND ends_at > $3
             AND ($5::uuid IS NULL OR id <> $5)
           ORDER BY starts_at, id
           LIMIT 1"#,
        tenant,
        staff,
        from,
        to,
        except
    )
    .fetch_optional(tx.conn())
    .await?)
}

/// An open booking of the same person overlapping `[from, to)`: the same customer record, or the
/// same newcomer phone outside `group` (one family may share a phone, spec Q11).
pub async fn customer_overlap(
    tx: &mut TenantTx,
    customer: Option<Uuid>,
    guest_phone: Option<&str>,
    group: Option<Uuid>,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    except: Option<Uuid>,
) -> Result<Option<Uuid>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_scalar!(
        r#"SELECT id FROM app.appointment
           WHERE tenant_id = $1 AND status IN ('booked', 'arrived')
             AND starts_at < $5 AND ends_at > $4
             AND ($6::uuid IS NULL OR id <> $6)
             AND (customer_id = $2
                  OR (guest_phone = $3 AND ($7::uuid IS NULL OR group_id IS DISTINCT FROM $7)))
           ORDER BY starts_at, id
           LIMIT 1"#,
        tenant,
        customer,
        guest_phone,
        from,
        to,
        except,
        group
    )
    .fetch_optional(tx.conn())
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
    pub group_id: Option<Uuid>,
    /// Companions known only by a note ("chồng của chị Lan").
    pub companions: &'a [String],
    pub created_by: Uuid,
}

pub async fn insert(tx: &mut TenantTx, a: &AppointmentInsert<'_>) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        r#"INSERT INTO app.appointment
             (id, tenant_id, branch_id, customer_id, guest_name, guest_phone, business_date, starts_at,
              ends_at, staff_id, note, created_by, group_id, companions)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)"#,
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
        a.created_by,
        a.group_id,
        a.companions
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
    pub guest_phone: Option<String>,
    pub staff_id: Option<Uuid>,
    pub arrived_at: Option<DateTime<Utc>>,
    pub note: Option<String>,
    pub companions: Vec<String>,
    pub group_id: Option<Uuid>,
    pub version: i64,
}

/// The appointment row, locked for a change; `None` = not in this tenant.
pub async fn lock(tx: &mut TenantTx, id: Uuid) -> Result<Option<Locked>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        Locked,
        r#"SELECT status, business_date, starts_at, ends_at, customer_id, guest_phone, staff_id,
                  arrived_at, note, companions, group_id, version
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
    /// Note-only companions (the full list).
    pub companions: &'a [String],
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
               companions = $10, version = version + 1, updated_at = now()
           WHERE tenant_id = $1 AND id = $2 AND version = $3 AND status IN ('booked', 'arrived')"#,
        tenant,
        id,
        version,
        r.business_date,
        r.starts_at,
        r.ends_at,
        r.staff_id,
        r.note,
        r.customer_id,
        r.companions
    )
    .execute(tx.conn())
    .await?
    .rows_affected()
        == 1)
}

/// Opens a group on a booking that had none (a served companion joins it, spec §10.10).
pub async fn open_group(tx: &mut TenantTx, id: Uuid, group: Uuid) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        r#"UPDATE app.appointment
           SET group_id = $3, version = version + 1, updated_at = now()
           WHERE tenant_id = $1 AND id = $2 AND group_id IS NULL"#,
        tenant,
        id,
        group
    )
    .execute(tx.conn())
    .await?;
    Ok(())
}

/// The customers already booked in a group (any status), to refuse a duplicate.
pub async fn group_customers(tx: &mut TenantTx, group: Uuid) -> Result<Vec<Uuid>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_scalar!(
        r#"SELECT customer_id AS "customer_id!" FROM app.appointment
           WHERE tenant_id = $1 AND group_id = $2 AND customer_id IS NOT NULL"#,
        tenant,
        group
    )
    .fetch_all(tx.conn())
    .await?)
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
    pub customer_phone: Option<String>,
    pub customer_gender: Option<String>,
    pub staff_id: Option<Uuid>,
    pub staff_name: Option<String>,
    pub note: Option<String>,
    pub status_reason: Option<String>,
    pub arrived_at: Option<DateTime<Utc>>,
    pub closed_at: Option<DateTime<Utc>>,
    pub visit_id: Option<Uuid>,
    pub visit_status: Option<String>,
    pub visit_check_in_at: Option<DateTime<Utc>>,
    pub sale_total: Option<i64>,
    pub group_id: Option<Uuid>,
    pub companions: Vec<String>,
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
                  CASE WHEN c.id IS NULL THEN a.guest_name
                       ELSE app.customer_label(c.full_name) END AS "customer_name!",
                  coalesce(c.phone_e164, a.guest_phone) AS "customer_phone?",
                  c.gender AS "customer_gender?", a.staff_id, st.full_name AS "staff_name?",
                  a.note, a.status_reason, a.arrived_at, a.closed_at, a.visit_id,
                  v.status AS "visit_status?", v.check_in_at AS "visit_check_in_at?",
                  (SELECT sum(s.total) FROM app.sale s
                   WHERE s.tenant_id = v.tenant_id AND s.visit_id = v.id AND s.status = 'open')::bigint AS "sale_total?", a.group_id, a.companions, a.version, a.created_at
           FROM app.appointment a
           LEFT JOIN app.customer c ON c.tenant_id = a.tenant_id AND c.id = a.customer_id
           LEFT JOIN app.staff st ON st.tenant_id = a.tenant_id AND st.id = a.staff_id
           LEFT JOIN app.visit v ON v.tenant_id = a.tenant_id AND v.id = a.visit_id
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

pub struct WalkInRow {
    pub id: Uuid,
    pub business_date: NaiveDate,
    pub check_in_at: DateTime<Utc>,
    pub customer_id: Uuid,
    pub customer_name: String,
    pub customer_gender: Option<String>,
    pub status: String,
    pub sale_total: Option<i64>,
}

/// Visits of `from..=to` in this branch that no appointment points at (walk-ins), earliest first.
pub async fn walk_ins(
    tx: &mut TenantTx,
    branch: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<WalkInRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        WalkInRow,
        r#"SELECT v.id, v.business_date, v.check_in_at, v.customer_id,
                  app.customer_label(c.full_name) AS "customer_name!",

                  c.gender AS "customer_gender?", v.status, (SELECT sum(s.total) FROM app.sale s
                   WHERE s.tenant_id = v.tenant_id AND s.visit_id = v.id AND s.status = 'open')::bigint AS "sale_total?"
           FROM app.visit v
           JOIN app.customer c ON c.tenant_id = v.tenant_id AND c.id = v.customer_id
           WHERE v.tenant_id = $1 AND v.branch_id = $2 AND v.business_date BETWEEN $3 AND $4
             AND NOT EXISTS (
               SELECT 1 FROM app.appointment a WHERE a.tenant_id = v.tenant_id AND a.visit_id = v.id)
             -- A visit replaced by "Sửa lượt" is not a second customer: only its newest version shows.
             AND NOT EXISTS (
               SELECT 1 FROM app.visit n WHERE n.tenant_id = v.tenant_id AND n.corrects_visit_id = v.id)
           ORDER BY v.check_in_at, v.id
           LIMIT 2000"#,
        tenant,
        branch,
        from,
        to
    )
    .fetch_all(tx.conn())
    .await?)
}

pub struct VisitLineRow {
    pub visit_id: Uuid,
    pub service_id: Uuid,
    pub service_name: String,
    pub category_name: String,
    pub duration_min: Option<i32>,
    pub staff_id: Uuid,
    pub staff_name: String,
}

/// Lines of the given visits, in order (no N+1).
pub async fn visit_lines(tx: &mut TenantTx, ids: &[Uuid]) -> Result<Vec<VisitLineRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        VisitLineRow,
        r#"SELECT l.visit_id, l.service_id, l.service_name_snapshot AS service_name,
                  sc.name AS category_name, sv.duration_min, l.staff_id, st.full_name AS staff_name
           FROM app.visit_line l
           JOIN app.service sv ON sv.tenant_id = l.tenant_id AND sv.id = l.service_id
           JOIN app.service_category sc ON sc.tenant_id = sv.tenant_id AND sc.id = sv.category_id
           JOIN app.staff st ON st.tenant_id = l.tenant_id AND st.id = l.staff_id
           WHERE l.tenant_id = $1 AND l.visit_id = ANY($2)
           ORDER BY l.visit_id, l.position"#,
        tenant,
        ids
    )
    .fetch_all(tx.conn())
    .await?)
}

// ------------------------------------------------------------- a technician's own calendar

pub struct MyAppointmentRow {
    pub id: Uuid,
    pub status: String,
    pub business_date: NaiveDate,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub customer_name: String,
    pub visit_status: Option<String>,
    pub visit_check_in_at: Option<DateTime<Utc>>,
    pub group_size: Option<i64>,
}

/// Appointments of `from..=to` assigned to `staff` (every status), earliest first. Only the
/// allowlisted columns of `MyCalendarEntry` are selected: no phone, note or money.
pub async fn my_appointments(
    tx: &mut TenantTx,
    staff: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<MyAppointmentRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        MyAppointmentRow,
        r#"SELECT a.id, a.status, a.business_date, a.starts_at, a.ends_at,
                  CASE WHEN c.id IS NULL THEN a.guest_name
                       ELSE app.customer_label(c.full_name) END AS "customer_name!",
                  v.status AS "visit_status?", v.check_in_at AS "visit_check_in_at?",
                  CASE WHEN a.group_id IS NULL THEN NULL ELSE
                    (SELECT count(*) FROM app.appointment g
                     WHERE g.tenant_id = a.tenant_id AND g.group_id = a.group_id
                       AND g.status <> 'cancelled') END AS "group_size?"
           FROM app.appointment a
           LEFT JOIN app.customer c ON c.tenant_id = a.tenant_id AND c.id = a.customer_id
           LEFT JOIN app.visit v ON v.tenant_id = a.tenant_id AND v.id = a.visit_id
           WHERE a.tenant_id = $1 AND a.staff_id = $2 AND a.business_date BETWEEN $3 AND $4
           ORDER BY a.starts_at, a.id
           LIMIT 2000"#,
        tenant,
        staff,
        from,
        to
    )
    .fetch_all(tx.conn())
    .await?)
}

pub struct MyVisitRow {
    pub id: Uuid,
    pub business_date: NaiveDate,
    pub check_in_at: DateTime<Utc>,
    pub customer_name: String,
    pub walk_in: bool,
}

/// Completed visits of `from..=to` in which `staff` performed a line and that no booking assigned
/// to `staff` stands for (that booking is the entry), earliest first.
pub async fn my_visits(
    tx: &mut TenantTx,
    staff: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<MyVisitRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        MyVisitRow,
        r#"SELECT v.id, v.business_date, v.check_in_at,
                  app.customer_label(c.full_name) AS "customer_name!",
                  NOT EXISTS (SELECT 1 FROM app.appointment a
                              WHERE a.tenant_id = v.tenant_id AND a.visit_id = v.id) AS "walk_in!"
           FROM app.visit v
           JOIN app.customer c ON c.tenant_id = v.tenant_id AND c.id = v.customer_id
           WHERE v.tenant_id = $1 AND v.status = 'completed' AND v.business_date BETWEEN $3 AND $4
             AND EXISTS (SELECT 1 FROM app.visit_line l
                         WHERE l.tenant_id = v.tenant_id AND l.visit_id = v.id AND l.staff_id = $2)
             AND NOT EXISTS (SELECT 1 FROM app.appointment a
                             WHERE a.tenant_id = v.tenant_id AND a.visit_id = v.id AND a.staff_id = $2)
           ORDER BY v.check_in_at, v.id
           LIMIT 2000"#,
        tenant,
        staff,
        from,
        to
    )
    .fetch_all(tx.conn())
    .await?)
}

pub struct MyVisitLineRow {
    pub visit_id: Uuid,
    pub service_name: String,
    pub category_name: String,
    pub duration_min: Option<i32>,
    pub source: String,
}

/// `staff`'s own lines of the given visits, in order (no N+1; another technician's lines of a
/// shared visit are never read).
pub async fn my_visit_lines(
    tx: &mut TenantTx,
    staff: Uuid,
    ids: &[Uuid],
) -> Result<Vec<MyVisitLineRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        MyVisitLineRow,
        r#"SELECT l.visit_id, l.service_name_snapshot AS service_name, sc.name AS category_name,
                  sv.duration_min, l.source
           FROM app.visit_line l
           JOIN app.service sv ON sv.tenant_id = l.tenant_id AND sv.id = l.service_id
           JOIN app.service_category sc ON sc.tenant_id = sv.tenant_id AND sc.id = sv.category_id
           WHERE l.tenant_id = $1 AND l.staff_id = $2 AND l.visit_id = ANY($3)
           ORDER BY l.visit_id, l.position"#,
        tenant,
        staff,
        ids
    )
    .fetch_all(tx.conn())
    .await?)
}

/// The owner's advisory "max customers per slot"; `None` = no limit (or never set).
pub async fn max_per_slot(tx: &mut TenantTx) -> Result<Option<i32>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_scalar!(
        "SELECT max_per_slot FROM app.appointment_setting WHERE tenant_id = $1",
        tenant
    )
    .fetch_optional(tx.conn())
    .await?
    .flatten())
}

/// Sets (or clears with `None`) the spa's "max customers per slot"; returns the previous value.
pub async fn set_max_per_slot(
    tx: &mut TenantTx,
    value: Option<i32>,
    by: Uuid,
) -> Result<Option<i32>, AppError> {
    let before = max_per_slot(tx).await?;
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        r#"INSERT INTO app.appointment_setting (tenant_id, max_per_slot, updated_by)
           VALUES ($1, $2, $3)
           ON CONFLICT (tenant_id) DO UPDATE
             SET max_per_slot = EXCLUDED.max_per_slot, updated_by = EXCLUDED.updated_by,
                 updated_at = now()"#,
        tenant,
        value,
        by
    )
    .execute(tx.conn())
    .await?;
    Ok(before)
}

/// "Sửa lượt": the appointment completed by visit `from` now points at `to`, the visit correcting
/// it (0020 lets a done appointment change only this). Returns the appointment moved, if any.
pub async fn move_to_correction(
    tx: &mut TenantTx,
    from: Uuid,
    to: Uuid,
) -> Result<Option<Uuid>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_scalar!(
        r#"UPDATE app.appointment SET visit_id = $3, version = version + 1, updated_at = now()
           WHERE tenant_id = $1 AND visit_id = $2
           RETURNING id"#,
        tenant,
        from,
        to
    )
    .fetch_optional(tx.conn())
    .await?)
}
