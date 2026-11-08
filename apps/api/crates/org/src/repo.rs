//! Staff SQL. Tenant-scoped explicitly; RLS is the second layer.

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use kernel::{AppError, TenantTx};
use uuid::Uuid;

pub struct StaffRow {
    pub id: Uuid,
    pub full_name: String,
    pub position: String,
    pub status: String,
}

pub async fn list(tx: &mut TenantTx, status: Option<&str>) -> Result<Vec<StaffRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        StaffRow,
        r#"SELECT id, full_name, position, status FROM app.staff
           WHERE tenant_id = $1 AND ($2::text IS NULL OR status = $2)
           ORDER BY status, full_name"#,
        tenant,
        status
    )
    .fetch_all(tx.conn())
    .await?)
}

/// Locks the row (`FOR SHARE`) so the staff member cannot be marked as left while the caller's
/// transaction records them as closer or technician.
pub async fn get_for_share(tx: &mut TenantTx, id: Uuid) -> Result<Option<StaffRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        StaffRow,
        r#"SELECT id, full_name, position, status FROM app.staff
           WHERE tenant_id = $1 AND id = $2 FOR SHARE"#,
        tenant,
        id
    )
    .fetch_optional(tx.conn())
    .await?)
}

// ------------------------------------------------------------------------------ profiles

pub struct ProfileRow {
    pub id: Uuid,
    pub full_name: String,
    pub position: String,
    pub phone_e164: Option<String>,
    pub status: String,
    pub joined_on: Option<NaiveDate>,
    pub left_on: Option<NaiveDate>,
    pub user_id: Option<Uuid>,
    pub user_name: Option<String>,
    pub shift_start: Option<NaiveTime>,
    pub note: Option<String>,
    pub version: i64,
}

pub async fn profiles(tx: &mut TenantTx) -> Result<Vec<ProfileRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        ProfileRow,
        r#"SELECT s.id, s.full_name, s.position, s.phone_e164, s.status, s.joined_on, s.left_on,
                  s.user_id, u.full_name AS "user_name?", s.shift_start, s.note, s.version
           FROM app.staff s LEFT JOIN app.user_account u ON u.id = s.user_id
           WHERE s.tenant_id = $1
           ORDER BY s.status, s.full_name"#,
        tenant
    )
    .fetch_all(tx.conn())
    .await?)
}

/// One profile; `lock` takes `FOR UPDATE` on the staff row first (updates).
pub async fn profile(
    tx: &mut TenantTx,
    id: Uuid,
    lock: bool,
) -> Result<Option<ProfileRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    if lock {
        sqlx::query_scalar!(
            "SELECT id FROM app.staff WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
            tenant,
            id
        )
        .fetch_optional(tx.conn())
        .await?;
    }
    Ok(sqlx::query_as!(
        ProfileRow,
        r#"SELECT s.id, s.full_name, s.position, s.phone_e164, s.status, s.joined_on, s.left_on,
                  s.user_id, u.full_name AS "user_name?", s.shift_start, s.note, s.version
           FROM app.staff s LEFT JOIN app.user_account u ON u.id = s.user_id
           WHERE s.tenant_id = $1 AND s.id = $2"#,
        tenant,
        id
    )
    .fetch_optional(tx.conn())
    .await?)
}

pub struct ProfileWrite<'a> {
    pub full_name: &'a str,
    pub position: &'a str,
    pub phone_e164: Option<&'a str>,
    /// Start of the current (latest) period, mirrored on the staff row.
    pub joined_on: Option<NaiveDate>,
    pub shift_start: Option<NaiveTime>,
    pub note: Option<&'a str>,
}

/// Inserts an active profile; the 0025 trigger opens its first employment period.
pub async fn insert_profile(
    tx: &mut TenantTx,
    id: Uuid,
    branch: Uuid,
    w: &ProfileWrite<'_>,
    user_id: Option<Uuid>,
) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        r#"INSERT INTO app.staff (id, tenant_id, branch_id, full_name, position, phone_e164,
                                  joined_on, user_id, shift_start, note)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#,
        id,
        tenant,
        branch,
        w.full_name,
        w.position,
        w.phone_e164,
        w.joined_on,
        user_id,
        w.shift_start,
        w.note
    )
    .execute(tx.conn())
    .await?;
    Ok(())
}

/// Updates when `version` still matches; `false` = stale. Status, end date and login have their
/// own use cases (end / rehire / link).
pub async fn update_profile(
    tx: &mut TenantTx,
    id: Uuid,
    version: i64,
    w: &ProfileWrite<'_>,
) -> Result<bool, AppError> {
    let tenant = tx.tenant().as_uuid();
    let done = sqlx::query!(
        r#"UPDATE app.staff SET full_name = $4, position = $5, phone_e164 = $6,
                  joined_on = $7, shift_start = $8, note = $9,
                  version = version + 1, updated_at = now()
           WHERE tenant_id = $1 AND id = $2 AND version = $3"#,
        tenant,
        id,
        version,
        w.full_name,
        w.position,
        w.phone_e164,
        w.joined_on,
        w.shift_start,
        w.note
    )
    .execute(tx.conn())
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Links (or unlinks, `None`) a login; `false` = stale version.
pub async fn set_link(
    tx: &mut TenantTx,
    id: Uuid,
    version: i64,
    user_id: Option<Uuid>,
) -> Result<bool, AppError> {
    let tenant = tx.tenant().as_uuid();
    let done = sqlx::query!(
        r#"UPDATE app.staff SET user_id = $4, version = version + 1, updated_at = now()
           WHERE tenant_id = $1 AND id = $2 AND version = $3"#,
        tenant,
        id,
        version,
        user_id
    )
    .execute(tx.conn())
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Mirrors the latest period on the staff row (`status`, `joined_on`, `left_on`: what every
/// picker filters on); `false` = stale version.
pub async fn set_mirror(
    tx: &mut TenantTx,
    id: Uuid,
    version: i64,
    joined_on: Option<NaiveDate>,
    left_on: Option<NaiveDate>,
) -> Result<bool, AppError> {
    let tenant = tx.tenant().as_uuid();
    let done = sqlx::query!(
        r#"UPDATE app.staff
           SET status = CASE WHEN $5::date IS NULL THEN 'active' ELSE 'left' END,
               joined_on = $4, left_on = $5, version = version + 1, updated_at = now()
           WHERE tenant_id = $1 AND id = $2 AND version = $3"#,
        tenant,
        id,
        version,
        joined_on,
        left_on
    )
    .execute(tx.conn())
    .await?;
    Ok(done.rows_affected() == 1)
}

/// The staff member linked to `user_id` in this spa, if any.
pub async fn staff_of_user(tx: &mut TenantTx, user_id: Uuid) -> Result<Option<Uuid>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_scalar!(
        "SELECT id FROM app.staff WHERE tenant_id = $1 AND user_id = $2",
        tenant,
        user_id
    )
    .fetch_optional(tx.conn())
    .await?)
}

pub async fn is_active_member(tx: &mut TenantTx, user_id: Uuid) -> Result<bool, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM app.membership
                          WHERE tenant_id = $1 AND user_id = $2 AND status = 'active') AS "e!""#,
        tenant,
        user_id
    )
    .fetch_one(tx.conn())
    .await?)
}

pub struct AccountRow {
    pub user_id: Uuid,
    pub full_name: String,
    pub roles: Vec<String>,
    pub staff_id: Option<Uuid>,
    /// The login's roles grant `attendance.record` or `evaluation.record`.
    pub records_staff: bool,
    /// The login's roles grant `staff.manage` (the owner).
    pub manages_staff: bool,
}

/// Logins of this spa that a staff profile can be linked to, with what their roles let them do
/// to staff records (decision C).
pub async fn accounts(tx: &mut TenantTx) -> Result<Vec<AccountRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        AccountRow,
        r#"SELECT m.user_id, u.full_name,
                  COALESCE(array_agg(r.name ORDER BY r.name) FILTER (WHERE r.id IS NOT NULL), '{}') AS "roles!",
                  (SELECT s.id FROM app.staff s WHERE s.tenant_id = m.tenant_id AND s.user_id = m.user_id) AS staff_id,
                  EXISTS (SELECT 1 FROM app.membership_role smr
                          JOIN app.role_permission rp ON rp.tenant_id = smr.tenant_id AND rp.role_id = smr.role_id
                          WHERE smr.tenant_id = m.tenant_id AND smr.membership_id = m.id
                            AND rp.permission IN ('attendance.record', 'evaluation.record')) AS "records_staff!",
                  EXISTS (SELECT 1 FROM app.membership_role smr
                          JOIN app.role_permission rp ON rp.tenant_id = smr.tenant_id AND rp.role_id = smr.role_id
                          WHERE smr.tenant_id = m.tenant_id AND smr.membership_id = m.id
                            AND rp.permission = 'staff.manage') AS "manages_staff!"
           FROM app.membership m
           JOIN app.user_account u ON u.id = m.user_id
           LEFT JOIN app.membership_role mr ON mr.tenant_id = m.tenant_id AND mr.membership_id = m.id
           LEFT JOIN app.role r ON r.tenant_id = mr.tenant_id AND r.id = mr.role_id
           WHERE m.tenant_id = $1 AND m.status = 'active'
           GROUP BY m.tenant_id, m.id, m.user_id, u.full_name
           ORDER BY u.full_name"#,
        tenant
    )
    .fetch_all(tx.conn())
    .await?)
}

/// Locks (`disabled`) or unlocks (`active`) a login's membership of this spa. A disabled member
/// gets no permission on their next request (`identity::repo::member_permissions`).
pub async fn set_membership_active(
    tx: &mut TenantTx,
    user_id: Uuid,
    active: bool,
) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    let status = if active { "active" } else { "disabled" };
    sqlx::query!(
        "UPDATE app.membership SET status = $3 WHERE tenant_id = $1 AND user_id = $2",
        tenant,
        user_id,
        status
    )
    .execute(tx.conn())
    .await?;
    Ok(())
}

/// Whether the login holds `staff.manage` here: an owner's login is never locked by a roster
/// action (unlink it first).
pub async fn login_manages_staff(tx: &mut TenantTx, user_id: Uuid) -> Result<bool, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_scalar!(
        r#"SELECT EXISTS (
             SELECT 1 FROM app.membership m
             JOIN app.membership_role mr ON mr.tenant_id = m.tenant_id AND mr.membership_id = m.id
             JOIN app.role_permission rp ON rp.tenant_id = mr.tenant_id AND rp.role_id = mr.role_id
             WHERE m.tenant_id = $1 AND m.user_id = $2 AND rp.permission = 'staff.manage') AS "e!""#,
        tenant,
        user_id
    )
    .fetch_one(tx.conn())
    .await?)
}

// ------------------------------------------------------------------------ employment periods

pub struct PeriodRow {
    pub id: Uuid,
    pub staff_id: Uuid,
    pub started_on: Option<NaiveDate>,
    pub ended_on: Option<NaiveDate>,
    pub end_reason: Option<String>,
}

/// Periods of everyone (`None`) or one person, oldest first.
pub async fn periods(tx: &mut TenantTx, staff: Option<Uuid>) -> Result<Vec<PeriodRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        PeriodRow,
        r#"SELECT id, staff_id, started_on, ended_on, end_reason FROM app.staff_employment
           WHERE tenant_id = $1 AND ($2::uuid IS NULL OR staff_id = $2)
           ORDER BY staff_id, started_on NULLS FIRST"#,
        tenant,
        staff
    )
    .fetch_all(tx.conn())
    .await?)
}

pub async fn insert_period(
    tx: &mut TenantTx,
    staff: Uuid,
    started_on: NaiveDate,
) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        "INSERT INTO app.staff_employment (id, tenant_id, staff_id, started_on) VALUES ($1, $2, $3, $4)",
        Uuid::now_v7(),
        tenant,
        staff,
        started_on
    )
    .execute(tx.conn())
    .await?;
    Ok(())
}

/// Corrects the start of an open period.
pub async fn set_period_start(
    tx: &mut TenantTx,
    period: Uuid,
    started_on: Option<NaiveDate>,
) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        "UPDATE app.staff_employment SET started_on = $3 WHERE tenant_id = $1 AND id = $2 AND ended_on IS NULL",
        tenant,
        period,
        started_on
    )
    .execute(tx.conn())
    .await?;
    Ok(())
}

pub async fn end_period(
    tx: &mut TenantTx,
    period: Uuid,
    ended_on: NaiveDate,
    reason: &str,
    by: Uuid,
) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        r#"UPDATE app.staff_employment SET ended_on = $3, end_reason = $4, ended_by = $5
           WHERE tenant_id = $1 AND id = $2 AND ended_on IS NULL"#,
        tenant,
        period,
        ended_on,
        reason,
        by
    )
    .execute(tx.conn())
    .await?;
    Ok(())
}

/// Whether anything was recorded for the person after `day`: attendance or a completed visit
/// they served (ending employment before it would leave that record outside any period).
pub async fn worked_after(
    tx: &mut TenantTx,
    staff: Uuid,
    day: NaiveDate,
) -> Result<bool, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_scalar!(
        r#"SELECT (EXISTS (SELECT 1 FROM app.attendance_day
                           WHERE tenant_id = $1 AND staff_id = $2 AND work_date > $3)
                OR EXISTS (SELECT 1 FROM app.visit_line l
                           JOIN app.visit v ON v.tenant_id = l.tenant_id AND v.id = l.visit_id
                           WHERE l.tenant_id = $1 AND l.staff_id = $2 AND v.status = 'completed'
                             AND v.business_date > $3)) AS "e!""#,
        tenant,
        staff,
        day
    )
    .fetch_one(tx.conn())
    .await?)
}

pub struct UpcomingBookingRow {
    pub id: Uuid,
    pub business_date: NaiveDate,
    pub starts_at: DateTime<Utc>,
    /// The customer's name, "Khách vãng lai" for a nameless record, the guest's name for a newcomer.
    pub customer: String,
}

/// Open bookings (booked / arrived) assigned to the person after `day`, earliest first (at most
/// `limit`): what blocks ending their employment, listed so the owner can reassign them.
pub async fn bookings_after(
    tx: &mut TenantTx,
    staff: Uuid,
    day: NaiveDate,
    limit: i64,
) -> Result<Vec<UpcomingBookingRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        UpcomingBookingRow,
        r#"SELECT a.id, a.business_date, a.starts_at,
                  CASE WHEN c.id IS NULL THEN coalesce(a.guest_name, 'Khách mới')
                       ELSE app.customer_label(c.full_name) END AS "customer!"
           FROM app.appointment a
           LEFT JOIN app.customer c ON c.tenant_id = a.tenant_id AND c.id = a.customer_id
           WHERE a.tenant_id = $1 AND a.staff_id = $2 AND a.status IN ('booked', 'arrived')
             AND a.business_date > $3
           ORDER BY a.starts_at, a.id
           LIMIT $4"#,
        tenant,
        staff,
        day,
        limit
    )
    .fetch_all(tx.conn())
    .await?)
}

// --------------------------------------------------------------------------------- history

pub struct HistoryRow {
    pub at: DateTime<Utc>,
    pub action: String,
    pub actor_name: Option<String>,
    pub before: Option<serde_json::Value>,
    pub after: Option<serde_json::Value>,
    pub reason: Option<String>,
}

/// The audit trail of one profile, newest first.
pub async fn history(tx: &mut TenantTx, staff: Uuid) -> Result<Vec<HistoryRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        HistoryRow,
        r#"SELECT a.at, a.action, u.full_name AS "actor_name?", a.before, a.after, a.reason
           FROM app.audit_log a LEFT JOIN app.user_account u ON u.id = a.actor_user_id
           WHERE a.tenant_id = $1 AND a.entity_type = 'staff' AND a.entity_id = $2
             AND a.action IN ('staff.create', 'staff.update', 'staff.account_link', 'staff.end',
                              'staff.rehire', 'staff.pay_update')
           ORDER BY a.at DESC, a.id DESC
           LIMIT 200"#,
        tenant,
        staff
    )
    .fetch_all(tx.conn())
    .await?)
}

// ------------------------------------------------------------------------------- pay terms

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayRow {
    pub staff_id: Uuid,
    pub base_salary: i64,
    pub commission_rate_bp: i32,
    pub loyalty_bonus_monthly: i64,
    pub meal_allowance: i64,
    pub version: i64,
}

pub async fn pays(tx: &mut TenantTx) -> Result<Vec<PayRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        PayRow,
        r#"SELECT staff_id, base_salary, commission_rate_bp, loyalty_bonus_monthly, meal_allowance, version
           FROM app.staff_pay WHERE tenant_id = $1"#,
        tenant
    )
    .fetch_all(tx.conn())
    .await?)
}

/// One staff member's pay terms, locked for update.
pub async fn pay_for_update(tx: &mut TenantTx, staff_id: Uuid) -> Result<Option<PayRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        PayRow,
        r#"SELECT staff_id, base_salary, commission_rate_bp, loyalty_bonus_monthly, meal_allowance, version
           FROM app.staff_pay WHERE tenant_id = $1 AND staff_id = $2 FOR UPDATE"#,
        tenant,
        staff_id
    )
    .fetch_optional(tx.conn())
    .await?)
}

pub async fn upsert_pay(tx: &mut TenantTx, p: &PayRow) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        r#"INSERT INTO app.staff_pay (tenant_id, staff_id, base_salary, commission_rate_bp,
                                      loyalty_bonus_monthly, meal_allowance)
           VALUES ($1, $2, $3, $4, $5, $6)
           ON CONFLICT (tenant_id, staff_id) DO UPDATE SET
             base_salary = EXCLUDED.base_salary, commission_rate_bp = EXCLUDED.commission_rate_bp,
             loyalty_bonus_monthly = EXCLUDED.loyalty_bonus_monthly,
             meal_allowance = EXCLUDED.meal_allowance,
             version = app.staff_pay.version + 1, updated_at = now()"#,
        tenant,
        p.staff_id,
        p.base_salary,
        p.commission_rate_bp,
        p.loyalty_bonus_monthly,
        p.meal_allowance
    )
    .execute(tx.conn())
    .await?;
    Ok(())
}
