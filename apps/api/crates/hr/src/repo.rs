//! HR SQL. Every query filters by `tx.tenant()` explicitly; RLS is the second layer.

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use kernel::{AppError, TenantTx};
use serde_json::Value;
use uuid::Uuid;

// ----------------------------------------------------------------------------- record scope

/// Staff linked to a login whose roles in this spa grant any of `permissions` (decision C:
/// people who supervise, whose records only the owner writes).
pub async fn supervising_staff(
    tx: &mut TenantTx,
    permissions: &[String],
) -> Result<Vec<Uuid>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_scalar!(
        r#"SELECT DISTINCT s.id AS "id!"
           FROM app.staff s
           JOIN app.membership m ON m.tenant_id = s.tenant_id AND m.user_id = s.user_id
           JOIN app.membership_role mr ON mr.tenant_id = m.tenant_id AND mr.membership_id = m.id
           JOIN app.role_permission rp ON rp.tenant_id = mr.tenant_id AND rp.role_id = mr.role_id
           WHERE s.tenant_id = $1 AND rp.permission = ANY($2)"#,
        tenant,
        permissions
    )
    .fetch_all(tx.conn())
    .await?)
}

pub struct VisitDayRow {
    pub staff_id: Uuid,
    pub day: NaiveDate,
}

/// Days on which each person performed a service of a completed visit (the attendance warning:
/// "absent" on a day with visits recorded is likely a mistake).
pub async fn visit_days(
    tx: &mut TenantTx,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<VisitDayRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        VisitDayRow,
        r#"SELECT DISTINCT vl.staff_id, v.business_date AS day
           FROM app.visit_line vl
           JOIN app.visit v ON v.tenant_id = vl.tenant_id AND v.id = vl.visit_id
           WHERE vl.tenant_id = $1 AND v.status = 'completed'
             AND v.business_date BETWEEN $2 AND $3
           ORDER BY 1, 2"#,
        tenant,
        from,
        to
    )
    .fetch_all(tx.conn())
    .await?)
}

// ----------------------------------------------------------------------------------- rules

pub async fn rules(tx: &mut TenantTx) -> Result<Option<(Value, i64)>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query!(
        "SELECT rules, version FROM app.payroll_rules WHERE tenant_id = $1",
        tenant
    )
    .fetch_optional(tx.conn())
    .await?
    .map(|r| (r.rules, r.version)))
}

/// Saves the rules if they are still at `expected` (0 = never saved): the new version, or `None`
/// when another save got there first (optimistic lock, ADR-0019 D10). One statement each way,
/// so two first saves cannot both win.
pub async fn put_rules(
    tx: &mut TenantTx,
    rules: &Value,
    by: Uuid,
    expected: i64,
) -> Result<Option<i64>, AppError> {
    let tenant = tx.tenant().as_uuid();
    let saved = if expected == 0 {
        sqlx::query_scalar!(
            r#"INSERT INTO app.payroll_rules (tenant_id, rules, updated_by) VALUES ($1, $2, $3)
               ON CONFLICT (tenant_id) DO NOTHING
               RETURNING version"#,
            tenant,
            rules,
            by
        )
        .fetch_optional(tx.conn())
        .await?
    } else {
        sqlx::query_scalar!(
            r#"UPDATE app.payroll_rules SET rules = $2, updated_by = $3, version = version + 1,
                 updated_at = now()
               WHERE tenant_id = $1 AND version = $4
               RETURNING version"#,
            tenant,
            rules,
            by,
            expected
        )
        .fetch_optional(tx.conn())
        .await?
    };
    Ok(saved)
}

// ---------------------------------------------------------------------------------- periods

pub struct PeriodRow {
    pub status: String,
    pub closed_at: Option<DateTime<Utc>>,
    pub closed_by_name: Option<String>,
}

pub async fn period(tx: &mut TenantTx, period: NaiveDate) -> Result<Option<PeriodRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        PeriodRow,
        r#"SELECT p.status, p.closed_at, u.full_name AS "closed_by_name?"
           FROM app.payroll_period p LEFT JOIN app.user_account u ON u.id = p.closed_by
           WHERE p.tenant_id = $1 AND p.period = $2"#,
        tenant,
        period
    )
    .fetch_optional(tx.conn())
    .await?)
}

pub struct DatedPeriodRow {
    pub period: NaiveDate,
    pub status: String,
    pub closed_at: Option<DateTime<Utc>>,
    pub closed_by_name: Option<String>,
}

/// The months `from..=to` that have a row (a month without one is open), in one query.
pub async fn periods_between(
    tx: &mut TenantTx,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<DatedPeriodRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        DatedPeriodRow,
        r#"SELECT p.period, p.status, p.closed_at, u.full_name AS "closed_by_name?"
           FROM app.payroll_period p LEFT JOIN app.user_account u ON u.id = p.closed_by
           WHERE p.tenant_id = $1 AND p.period BETWEEN $2 AND $3"#,
        tenant,
        from,
        to
    )
    .fetch_all(tx.conn())
    .await?)
}

/// Creates the month's row if missing and locks it: `FOR SHARE` for writers of the month's
/// records (many at once), `FOR UPDATE` for the close (exclusive). Returns the status.
pub async fn lock_period(
    tx: &mut TenantTx,
    period: NaiveDate,
    exclusive: bool,
) -> Result<String, AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        r#"INSERT INTO app.payroll_period (tenant_id, period) VALUES ($1, $2)
           ON CONFLICT (tenant_id, period) DO NOTHING"#,
        tenant,
        period
    )
    .execute(tx.conn())
    .await?;
    let status = if exclusive {
        sqlx::query_scalar!(
            "SELECT status FROM app.payroll_period WHERE tenant_id = $1 AND period = $2 FOR UPDATE",
            tenant,
            period
        )
        .fetch_one(tx.conn())
        .await?
    } else {
        sqlx::query_scalar!(
            "SELECT status FROM app.payroll_period WHERE tenant_id = $1 AND period = $2 FOR SHARE",
            tenant,
            period
        )
        .fetch_one(tx.conn())
        .await?
    };
    Ok(status)
}

pub async fn close_period(tx: &mut TenantTx, period: NaiveDate, by: Uuid) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        r#"UPDATE app.payroll_period SET status = 'closed', closed_at = now(), closed_by = $3
           WHERE tenant_id = $1 AND period = $2 AND status = 'open'"#,
        tenant,
        period,
        by
    )
    .execute(tx.conn())
    .await?;
    Ok(())
}

// ------------------------------------------------------------------------------- attendance

#[derive(Debug, Clone)]
pub struct AttendanceRow {
    pub staff_id: Uuid,
    pub work_date: NaiveDate,
    pub status: String,
    pub check_in: Option<NaiveTime>,
    pub check_out: Option<NaiveTime>,
    pub late_minutes: i32,
    pub note: Option<String>,
}

pub async fn attendance_between(
    tx: &mut TenantTx,
    from: NaiveDate,
    to: NaiveDate,
    staff: Option<Uuid>,
) -> Result<Vec<AttendanceRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        AttendanceRow,
        r#"SELECT staff_id, work_date, status, check_in, check_out, late_minutes, note
           FROM app.attendance_day
           WHERE tenant_id = $1 AND work_date BETWEEN $2 AND $3
             AND ($4::uuid IS NULL OR staff_id = $4)
           ORDER BY staff_id, work_date"#,
        tenant,
        from,
        to,
        staff
    )
    .fetch_all(tx.conn())
    .await?)
}

/// One (person, day) cell of a batch write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Cell {
    pub staff_id: Uuid,
    pub day: NaiveDate,
}

fn columns(cells: &[Cell]) -> (Vec<Uuid>, Vec<NaiveDate>) {
    cells.iter().map(|c| (c.staff_id, c.day)).unzip()
}

/// The recorded days among `cells`, in one query (a batch write's "before").
pub async fn attendance_cells(
    tx: &mut TenantTx,
    cells: &[Cell],
) -> Result<Vec<AttendanceRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    let (staff, days) = columns(cells);
    Ok(sqlx::query_as!(
        AttendanceRow,
        r#"SELECT a.staff_id, a.work_date, a.status, a.check_in, a.check_out, a.late_minutes, a.note
           FROM app.attendance_day a
           JOIN UNNEST($2::uuid[], $3::date[]) AS c(staff_id, work_date)
             ON c.staff_id = a.staff_id AND c.work_date = a.work_date
           WHERE a.tenant_id = $1"#,
        tenant,
        &staff[..],
        &days[..]
    )
    .fetch_all(tx.conn())
    .await?)
}

/// Records `rows` (JSON array of `{staff_id, work_date, status, check_in, check_out,
/// late_minutes, note}`, no (person, day) twice) in one statement.
pub async fn upsert_attendance_many(
    tx: &mut TenantTx,
    rows: &Value,
    by: Uuid,
) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        r#"INSERT INTO app.attendance_day
             (tenant_id, staff_id, work_date, status, check_in, check_out, late_minutes, note, recorded_by)
           SELECT $1::uuid, r.staff_id, r.work_date, r.status, r.check_in, r.check_out, r.late_minutes, r.note, $3::uuid
           FROM jsonb_to_recordset($2::jsonb) AS r(staff_id uuid, work_date date, status text, check_in time,
                                            check_out time, late_minutes integer, note text)
           ON CONFLICT (tenant_id, staff_id, work_date) DO UPDATE SET
             status = EXCLUDED.status, check_in = EXCLUDED.check_in, check_out = EXCLUDED.check_out,
             late_minutes = EXCLUDED.late_minutes, note = EXCLUDED.note,
             recorded_by = EXCLUDED.recorded_by, updated_at = now()"#,
        tenant,
        rows,
        by
    )
    .execute(tx.conn())
    .await?;
    Ok(())
}

/// Clears `cells` in one statement.
pub async fn delete_attendance_many(tx: &mut TenantTx, cells: &[Cell]) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    let (staff, days) = columns(cells);
    sqlx::query!(
        r#"DELETE FROM app.attendance_day a
           USING UNNEST($2::uuid[], $3::date[]) AS c(staff_id, work_date)
           WHERE a.tenant_id = $1 AND a.staff_id = c.staff_id AND a.work_date = c.work_date"#,
        tenant,
        &staff[..],
        &days[..]
    )
    .execute(tx.conn())
    .await?;
    Ok(())
}

// ----------------------------------------------------------------------------- daily review

#[derive(Debug, Clone)]
pub struct ReviewRow {
    pub staff_id: Uuid,
    pub work_date: NaiveDate,
    pub compliments: i32,
    pub complaints: i32,
    pub note: Option<String>,
}

pub async fn reviews_between(
    tx: &mut TenantTx,
    from: NaiveDate,
    to: NaiveDate,
    staff: Option<Uuid>,
) -> Result<Vec<ReviewRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        ReviewRow,
        r#"SELECT staff_id, work_date, compliments, complaints, note
           FROM app.staff_daily_review
           WHERE tenant_id = $1 AND work_date BETWEEN $2 AND $3
             AND ($4::uuid IS NULL OR staff_id = $4)
           ORDER BY staff_id, work_date"#,
        tenant,
        from,
        to,
        staff
    )
    .fetch_all(tx.conn())
    .await?)
}

/// The reviewed days among `cells`, in one query (a batch write's "before").
pub async fn review_cells(tx: &mut TenantTx, cells: &[Cell]) -> Result<Vec<ReviewRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    let (staff, days) = columns(cells);
    Ok(sqlx::query_as!(
        ReviewRow,
        r#"SELECT r.staff_id, r.work_date, r.compliments, r.complaints, r.note
           FROM app.staff_daily_review r
           JOIN UNNEST($2::uuid[], $3::date[]) AS c(staff_id, work_date)
             ON c.staff_id = r.staff_id AND c.work_date = r.work_date
           WHERE r.tenant_id = $1"#,
        tenant,
        &staff[..],
        &days[..]
    )
    .fetch_all(tx.conn())
    .await?)
}

/// Records `rows` (JSON array of `{staff_id, work_date, compliments, complaints, note}`, no
/// (person, day) twice) in one statement.
pub async fn upsert_reviews_many(
    tx: &mut TenantTx,
    rows: &Value,
    by: Uuid,
) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        r#"INSERT INTO app.staff_daily_review
             (tenant_id, staff_id, work_date, compliments, complaints, note, recorded_by)
           SELECT $1::uuid, r.staff_id, r.work_date, r.compliments, r.complaints, r.note, $3::uuid
           FROM jsonb_to_recordset($2::jsonb) AS r(staff_id uuid, work_date date, compliments integer,
                                            complaints integer, note text)
           ON CONFLICT (tenant_id, staff_id, work_date) DO UPDATE SET
             compliments = EXCLUDED.compliments, complaints = EXCLUDED.complaints,
             note = EXCLUDED.note, recorded_by = EXCLUDED.recorded_by, updated_at = now()"#,
        tenant,
        rows,
        by
    )
    .execute(tx.conn())
    .await?;
    Ok(())
}

/// Clears `cells` in one statement.
pub async fn delete_reviews_many(tx: &mut TenantTx, cells: &[Cell]) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    let (staff, days) = columns(cells);
    sqlx::query!(
        r#"DELETE FROM app.staff_daily_review r
           USING UNNEST($2::uuid[], $3::date[]) AS c(staff_id, work_date)
           WHERE r.tenant_id = $1 AND r.staff_id = c.staff_id AND r.work_date = c.work_date"#,
        tenant,
        &staff[..],
        &days[..]
    )
    .execute(tx.conn())
    .await?;
    Ok(())
}

// ------------------------------------------------------------------------------ evaluations

#[derive(Debug, Clone)]
pub struct EvaluationRow {
    pub staff_id: Uuid,
    /// First day of the evaluated month.
    pub work_date: NaiveDate,
    pub criteria: Value,
    pub serious_issue: bool,
    pub total_points: i32,
    pub result_level: String,
    pub coaching_plan: Option<String>,
    pub comment: Option<String>,
    pub by_name: String,
    pub updated_at: DateTime<Utc>,
    pub version: i64,
}

/// Evaluations of the months `from..=to` (first days), of one person or everyone, in one query.
pub async fn evaluations(
    tx: &mut TenantTx,
    from: NaiveDate,
    to: NaiveDate,
    staff: Option<Uuid>,
) -> Result<Vec<EvaluationRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        EvaluationRow,
        r#"SELECT e.staff_id, e.work_date, e.criteria, e.serious_issue, e.total_points,
                  e.result_level, e.coaching_plan, e.comment, u.full_name AS by_name,
                  e.updated_at, e.version
           FROM app.staff_evaluation e JOIN app.user_account u ON u.id = e.recorded_by
           WHERE e.tenant_id = $1 AND e.work_date BETWEEN $2 AND $3
             AND ($4::uuid IS NULL OR e.staff_id = $4)"#,
        tenant,
        from,
        to,
        staff
    )
    .fetch_all(tx.conn())
    .await?)
}

pub struct EvaluationWrite<'a> {
    pub staff_id: Uuid,
    pub month: NaiveDate,
    pub criteria: &'a Value,
    pub serious_issue: bool,
    pub total_points: i32,
    pub result_level: &'a str,
    pub coaching_plan: Option<&'a str>,
    pub comment: Option<&'a str>,
}

pub async fn upsert_evaluation(
    tx: &mut TenantTx,
    w: &EvaluationWrite<'_>,
    by: Uuid,
) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        r#"INSERT INTO app.staff_evaluation
             (tenant_id, staff_id, work_date, criteria, serious_issue, total_points, result_level,
              coaching_plan, comment, recorded_by)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
           ON CONFLICT (tenant_id, staff_id, work_date) DO UPDATE SET
             criteria = EXCLUDED.criteria, serious_issue = EXCLUDED.serious_issue,
             total_points = EXCLUDED.total_points, result_level = EXCLUDED.result_level,
             coaching_plan = EXCLUDED.coaching_plan, comment = EXCLUDED.comment,
             recorded_by = EXCLUDED.recorded_by, version = app.staff_evaluation.version + 1,
             updated_at = now()"#,
        tenant,
        w.staff_id,
        w.month,
        w.criteria,
        w.serious_issue,
        w.total_points,
        w.result_level,
        w.coaching_plan,
        w.comment,
        by
    )
    .execute(tx.conn())
    .await?;
    Ok(())
}

// ------------------------------------------------------------------------------ adjustments

#[derive(Debug, Clone)]
pub struct AdjustmentRow {
    pub id: Uuid,
    pub staff_id: Uuid,
    pub work_date: NaiveDate,
    pub kind: String,
    pub amount: Option<i64>,
    pub points: Option<i32>,
    pub reason: String,
    pub corrects_period: Option<NaiveDate>,
    pub by_name: String,
    pub created_at: DateTime<Utc>,
}

pub async fn adjustments_between(
    tx: &mut TenantTx,
    from: NaiveDate,
    to: NaiveDate,
    staff: Option<Uuid>,
) -> Result<Vec<AdjustmentRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        AdjustmentRow,
        r#"SELECT a.id, a.staff_id, a.work_date, a.kind, a.amount, a.points, a.reason,
                  a.corrects_period, u.full_name AS by_name, a.created_at
           FROM app.staff_adjustment a JOIN app.user_account u ON u.id = a.created_by
           WHERE a.tenant_id = $1 AND a.work_date BETWEEN $2 AND $3 AND a.voided_at IS NULL
             AND ($4::uuid IS NULL OR a.staff_id = $4)
           ORDER BY a.work_date DESC, a.created_at DESC"#,
        tenant,
        from,
        to,
        staff
    )
    .fetch_all(tx.conn())
    .await?)
}

pub async fn adjustment(tx: &mut TenantTx, id: Uuid) -> Result<Option<AdjustmentRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        AdjustmentRow,
        r#"SELECT a.id, a.staff_id, a.work_date, a.kind, a.amount, a.points, a.reason,
                  a.corrects_period, u.full_name AS by_name, a.created_at
           FROM app.staff_adjustment a JOIN app.user_account u ON u.id = a.created_by
           WHERE a.tenant_id = $1 AND a.id = $2"#,
        tenant,
        id
    )
    .fetch_optional(tx.conn())
    .await?)
}

pub struct AdjustmentLock {
    pub staff_id: Uuid,
    pub work_date: NaiveDate,
    pub voided: bool,
}

/// Locks one entry (`FOR UPDATE`) so two removals serialize.
pub async fn lock_adjustment(
    tx: &mut TenantTx,
    id: Uuid,
) -> Result<Option<AdjustmentLock>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        AdjustmentLock,
        r#"SELECT staff_id, work_date, voided_at IS NOT NULL AS "voided!"
           FROM app.staff_adjustment WHERE tenant_id = $1 AND id = $2 FOR UPDATE"#,
        tenant,
        id
    )
    .fetch_optional(tx.conn())
    .await?)
}

pub async fn void_adjustment(tx: &mut TenantTx, id: Uuid, by: Uuid) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        r#"UPDATE app.staff_adjustment SET voided_at = now(), voided_by = $3
           WHERE tenant_id = $1 AND id = $2 AND voided_at IS NULL"#,
        tenant,
        id,
        by
    )
    .execute(tx.conn())
    .await?;
    Ok(())
}

pub struct AdjustmentInsert<'a> {
    pub id: Uuid,
    pub staff_id: Uuid,
    pub work_date: NaiveDate,
    pub kind: &'a str,
    pub amount: Option<i64>,
    pub points: Option<i32>,
    pub reason: &'a str,
    pub corrects_period: Option<NaiveDate>,
    pub created_by: Uuid,
}

pub async fn insert_adjustment(
    tx: &mut TenantTx,
    a: &AdjustmentInsert<'_>,
) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        r#"INSERT INTO app.staff_adjustment
             (id, tenant_id, staff_id, work_date, kind, amount, points, reason, corrects_period, created_by)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#,
        a.id,
        tenant,
        a.staff_id,
        a.work_date,
        a.kind,
        a.amount,
        a.points,
        a.reason,
        a.corrects_period,
        a.created_by
    )
    .execute(tx.conn())
    .await?;
    Ok(())
}

// ------------------------------------------------------------------------ commission inputs

pub struct CommissionRow {
    pub staff_id: Uuid,
    pub service_name: String,
    pub source: String,
    pub qty: i64,
    pub value: i64,
    pub full_value: i64,
}

/// Services performed in completed (not voided) visits of the period, per technician and service.
pub async fn commission_lines(
    tx: &mut TenantTx,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<CommissionRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        CommissionRow,
        r#"SELECT vl.staff_id, vl.service_name_snapshot AS service_name, vl.source,
                  SUM(vl.qty)::bigint AS "qty!", SUM(vl.line_value)::bigint AS "value!",
                  SUM(vl.qty::bigint * vl.unit_price)::bigint AS "full_value!"
           FROM app.visit_line vl
           JOIN app.visit v ON v.tenant_id = vl.tenant_id AND v.id = vl.visit_id
           WHERE vl.tenant_id = $1 AND v.status = 'completed'
             AND v.business_date BETWEEN $2 AND $3
           GROUP BY vl.staff_id, vl.service_name_snapshot, vl.source
           ORDER BY vl.staff_id, SUM(vl.line_value) DESC, vl.service_name_snapshot"#,
        tenant,
        from,
        to
    )
    .fetch_all(tx.conn())
    .await?)
}

pub struct UpsellRow {
    pub staff_id: Uuid,
    /// `package`, `package_refund` or `service` (as `calc::UpsellLine::kind`).
    pub kind: String,
    pub description: String,
    pub qty: i64,
    pub value: i64,
}

/// Upsell credits of the period ("người giới thiệu / chốt", compensation-rules §3.2, EM Q-U3),
/// per person and item:
/// - packages: the price paid (after discount) of every package sold in an open sale of the
///   period, credited to the sale's closer (Σ per sale = `sale.total`);
/// - package refunds (`package_refund`, negative): a package of such a sale cancelled with a
///   refund in the period takes the refunded amount back from its closer (ADR-0010: commission
///   follows the money the spa keeps), in the month of the refund — a closed month never changes;
/// - one-off services: `line_value` of the one-off lines of completed visits of the period,
///   credited to the line's referrer.
///
/// "Khách tự chọn" (no closer / referrer) credits nobody.
pub async fn upsell_lines(
    tx: &mut TenantTx,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<UpsellRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        UpsellRow,
        r#"SELECT staff_id AS "staff_id!", kind AS "kind!", description AS "description!",
                  qty AS "qty!", value AS "value!"
           FROM (
             SELECT s.closer_staff_id AS staff_id, 'package'::text AS kind,
                    cp.name_snapshot AS description, count(*)::bigint AS qty,
                    SUM(cp.sold_price)::bigint AS value
             FROM app.sale s
             JOIN app.customer_package cp ON cp.tenant_id = s.tenant_id AND cp.sale_id = s.id
             WHERE s.tenant_id = $1 AND s.status = 'open' AND s.closer_staff_id IS NOT NULL
               AND s.business_date BETWEEN $2 AND $3
             GROUP BY s.closer_staff_id, cp.name_snapshot
             UNION ALL
             SELECT s.closer_staff_id, 'package_refund', cp.name_snapshot, count(*)::bigint,
                    -SUM(r.amount)::bigint
             FROM app.refund r
             JOIN app.customer_package cp ON cp.tenant_id = r.tenant_id AND cp.id = r.customer_package_id
             JOIN app.sale s ON s.tenant_id = cp.tenant_id AND s.id = cp.sale_id
             WHERE r.tenant_id = $1 AND s.status = 'open' AND s.closer_staff_id IS NOT NULL
               AND r.business_date BETWEEN $2 AND $3
             GROUP BY s.closer_staff_id, cp.name_snapshot
             UNION ALL
             SELECT vl.referrer_staff_id, 'service', vl.service_name_snapshot,
                    SUM(vl.qty)::bigint, SUM(vl.line_value)::bigint
             FROM app.visit_line vl
             JOIN app.visit v ON v.tenant_id = vl.tenant_id AND v.id = vl.visit_id
             WHERE vl.tenant_id = $1 AND v.status = 'completed' AND vl.referrer_staff_id IS NOT NULL
               AND v.business_date BETWEEN $2 AND $3
             GROUP BY vl.referrer_staff_id, vl.service_name_snapshot
           ) u
           ORDER BY staff_id, kind, value DESC, description"#,
        tenant,
        from,
        to
    )
    .fetch_all(tx.conn())
    .await?)
}

pub struct PackageRefundRow {
    pub refund_id: Uuid,
    /// The closer of the refunded package's sale.
    pub staff_id: Uuid,
    pub description: String,
    pub amount: i64,
    pub business_date: NaiveDate,
    pub created_at: DateTime<Utc>,
}

/// Refunds dated `from..=to` of packages sold in an open sale with a closer, one row each, oldest
/// first: the `package_refund` credits of [`upsell_lines`], for settling a leaver's (ADR-0019
/// D2b).
pub async fn package_refunds_between(
    tx: &mut TenantTx,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<PackageRefundRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        PackageRefundRow,
        r#"SELECT r.id AS refund_id, s.closer_staff_id AS "staff_id!", cp.name_snapshot AS description,
                  r.amount, r.business_date, r.created_at
           FROM app.refund r
           JOIN app.customer_package cp ON cp.tenant_id = r.tenant_id AND cp.id = r.customer_package_id
           JOIN app.sale s ON s.tenant_id = cp.tenant_id AND s.id = cp.sale_id
           WHERE r.tenant_id = $1 AND s.status = 'open' AND s.closer_staff_id IS NOT NULL
             AND r.business_date BETWEEN $2 AND $3
           ORDER BY r.business_date, r.created_at, r.id"#,
        tenant,
        from,
        to
    )
    .fetch_all(tx.conn())
    .await?)
}

// --------------------------------------------------------------------------------- payslips

pub async fn payslips(
    tx: &mut TenantTx,
    period: NaiveDate,
    staff: Option<Uuid>,
) -> Result<Vec<Value>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_scalar!(
        r#"SELECT p.detail FROM app.payslip p JOIN app.staff s ON s.tenant_id = p.tenant_id AND s.id = p.staff_id
           WHERE p.tenant_id = $1 AND p.period = $2 AND ($3::uuid IS NULL OR p.staff_id = $3)
           ORDER BY s.full_name"#,
        tenant,
        period,
        staff
    )
    .fetch_all(tx.conn())
    .await?)
}

pub async fn insert_payslip(
    tx: &mut TenantTx,
    period: NaiveDate,
    staff: Uuid,
    net_pay: i64,
    detail: &Value,
) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        r#"INSERT INTO app.payslip (id, tenant_id, period, staff_id, net_pay, detail)
           VALUES ($1, $2, $3, $4, $5, $6)"#,
        Uuid::now_v7(),
        tenant,
        period,
        staff,
        net_pay,
        detail
    )
    .execute(tx.conn())
    .await?;
    Ok(())
}

// -------------------------------------------------------------------------- self-service day

pub struct DayLineRow {
    pub visit_id: Uuid,
    pub check_in_at: DateTime<Utc>,
    pub customer_name: String,
    pub service_name: String,
    pub source: String,
    pub qty: i64,
    pub value: i64,
    pub full_value: i64,
    /// The visit's encrypted session note; `note` is filled by the caller after decryption.
    pub note_ct: Option<Vec<u8>>,
    pub note: Option<String>,
}

/// The lines `staff` performed in completed visits of one business day, earliest visit first.
/// Only the allowlisted columns of `day::MyDayVisit` are selected (no phone, customer notes or
/// customer ids); the session note comes with the visits the person worked on (item 3).
pub async fn day_lines(
    tx: &mut TenantTx,
    staff: Uuid,
    day: NaiveDate,
) -> Result<Vec<DayLineRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        DayLineRow,
        r#"SELECT v.id AS visit_id, v.check_in_at,
                  app.customer_label(c.full_name) AS "customer_name!",
                  vl.service_name_snapshot AS service_name, vl.source,
                  vl.qty::bigint AS "qty!", vl.line_value AS value,
                  (vl.qty::bigint * vl.unit_price)::bigint AS "full_value!",
                  v.note_ct, NULL::text AS note
           FROM app.visit v
           JOIN app.visit_line vl ON vl.tenant_id = v.tenant_id AND vl.visit_id = v.id
           JOIN app.customer c ON c.tenant_id = v.tenant_id AND c.id = v.customer_id
           WHERE v.tenant_id = $1 AND vl.staff_id = $2 AND v.status = 'completed'
             AND v.business_date = $3
           ORDER BY v.check_in_at, v.id, vl.position"#,
        tenant,
        staff,
        day
    )
    .fetch_all(tx.conn())
    .await?)
}

pub struct DayUpsellRow {
    pub kind: String,
    pub description: String,
    pub customer_name: String,
    pub qty: i64,
    pub value: i64,
}

/// Upsell credits of `staff` on one business day, item by item, with the rules of
/// [`upsell_lines`]: packages they closed (price paid), refunds of packages they closed
/// (negative), one-off services they recommended (`line_value`).
pub async fn day_upsell(
    tx: &mut TenantTx,
    staff: Uuid,
    day: NaiveDate,
) -> Result<Vec<DayUpsellRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        DayUpsellRow,
        r#"SELECT kind AS "kind!", description AS "description!", customer_name AS "customer_name!",
                  qty AS "qty!", value AS "value!"
           FROM (
             SELECT 'package'::text AS kind, cp.name_snapshot AS description,
                    app.customer_label(c.full_name) AS customer_name, 1::bigint AS qty,
                    cp.sold_price AS value, s.created_at AS at
             FROM app.sale s
             JOIN app.customer_package cp ON cp.tenant_id = s.tenant_id AND cp.sale_id = s.id
             JOIN app.customer c ON c.tenant_id = cp.tenant_id AND c.id = cp.customer_id
             WHERE s.tenant_id = $1 AND s.status = 'open' AND s.closer_staff_id = $2
               AND s.business_date = $3
             UNION ALL
             SELECT 'package_refund', cp.name_snapshot, app.customer_label(c.full_name), 1::bigint,
                    -r.amount, r.created_at
             FROM app.refund r
             JOIN app.customer_package cp ON cp.tenant_id = r.tenant_id AND cp.id = r.customer_package_id
             JOIN app.sale s ON s.tenant_id = cp.tenant_id AND s.id = cp.sale_id
             JOIN app.customer c ON c.tenant_id = cp.tenant_id AND c.id = cp.customer_id
             WHERE r.tenant_id = $1 AND s.status = 'open' AND s.closer_staff_id = $2
               AND r.business_date = $3
             UNION ALL
             SELECT 'service', vl.service_name_snapshot, app.customer_label(c.full_name),
                    vl.qty::bigint, vl.line_value, v.check_in_at
             FROM app.visit_line vl
             JOIN app.visit v ON v.tenant_id = vl.tenant_id AND v.id = vl.visit_id
             JOIN app.customer c ON c.tenant_id = v.tenant_id AND c.id = v.customer_id
             WHERE vl.tenant_id = $1 AND v.status = 'completed' AND vl.referrer_staff_id = $2
               AND v.business_date = $3
           ) u
           ORDER BY at, kind, description"#,
        tenant,
        staff,
        day
    )
    .fetch_all(tx.conn())
    .await?)
}
