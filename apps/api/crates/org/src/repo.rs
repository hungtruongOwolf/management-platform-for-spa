//! Staff SQL. Tenant-scoped explicitly; RLS is the second layer.

use chrono::{NaiveDate, NaiveTime};
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
    pub version: i64,
}

pub async fn profiles(tx: &mut TenantTx) -> Result<Vec<ProfileRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        ProfileRow,
        r#"SELECT s.id, s.full_name, s.position, s.phone_e164, s.status, s.joined_on, s.left_on,
                  s.user_id, u.full_name AS "user_name?", s.shift_start, s.version
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
                  s.user_id, u.full_name AS "user_name?", s.shift_start, s.version
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
    pub status: &'a str,
    pub joined_on: Option<NaiveDate>,
    pub left_on: Option<NaiveDate>,
    pub user_id: Option<Uuid>,
    pub shift_start: Option<NaiveTime>,
}

pub async fn insert_profile(
    tx: &mut TenantTx,
    id: Uuid,
    branch: Uuid,
    w: &ProfileWrite<'_>,
) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        r#"INSERT INTO app.staff (id, tenant_id, branch_id, full_name, position, phone_e164, status,
                                  joined_on, left_on, user_id, shift_start)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"#,
        id,
        tenant,
        branch,
        w.full_name,
        w.position,
        w.phone_e164,
        w.status,
        w.joined_on,
        w.left_on,
        w.user_id,
        w.shift_start
    )
    .execute(tx.conn())
    .await?;
    Ok(())
}

/// Updates when `version` still matches; `false` = stale.
pub async fn update_profile(
    tx: &mut TenantTx,
    id: Uuid,
    version: i64,
    w: &ProfileWrite<'_>,
) -> Result<bool, AppError> {
    let tenant = tx.tenant().as_uuid();
    let done = sqlx::query!(
        r#"UPDATE app.staff SET full_name = $4, position = $5, phone_e164 = $6, status = $7,
                  joined_on = $8, left_on = $9, user_id = $10, shift_start = $11,
                  version = version + 1, updated_at = now()
           WHERE tenant_id = $1 AND id = $2 AND version = $3"#,
        tenant,
        id,
        version,
        w.full_name,
        w.position,
        w.phone_e164,
        w.status,
        w.joined_on,
        w.left_on,
        w.user_id,
        w.shift_start
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
}

/// Logins of this spa that a staff profile can be linked to.
pub async fn accounts(tx: &mut TenantTx) -> Result<Vec<AccountRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        AccountRow,
        r#"SELECT m.user_id, u.full_name,
                  COALESCE(array_agg(r.name ORDER BY r.name) FILTER (WHERE r.id IS NOT NULL), '{}') AS "roles!",
                  (SELECT s.id FROM app.staff s WHERE s.tenant_id = m.tenant_id AND s.user_id = m.user_id) AS staff_id
           FROM app.membership m
           JOIN app.user_account u ON u.id = m.user_id
           LEFT JOIN app.membership_role mr ON mr.tenant_id = m.tenant_id AND mr.membership_id = m.id
           LEFT JOIN app.role r ON r.tenant_id = mr.tenant_id AND r.id = mr.role_id
           WHERE m.tenant_id = $1 AND m.status = 'active'
           GROUP BY m.tenant_id, m.user_id, u.full_name
           ORDER BY u.full_name"#,
        tenant
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
