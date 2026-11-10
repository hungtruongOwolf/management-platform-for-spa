//! What a spa holds after `seed-demo`, counted straight from the tables (one query, as the schema
//! owner, filtered by tenant), and the report the CLI prints. The test asserts every scenario
//! category from these counts, so they describe the data, not what the seeder believes it wrote.

use super::DemoReport;
use anyhow::Context;
use sqlx::{PgPool, Row};
use std::fmt::Write;
use std::time::Duration;

/// Each scenario category and how many rows the spa has of it, in report order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary(pub Vec<(&'static str, i64)>);

impl Summary {
    pub fn get(&self, name: &str) -> i64 {
        self.0
            .iter()
            .find(|(n, _)| *n == name)
            .map_or(0, |(_, v)| *v)
    }
}

/// `(name, SQL expression)`; `$1` is the tenant, `today` the spa's business day.
const COUNTS: &[(&str, &str)] = &[
    (
        "customers",
        "(SELECT count(*) FROM app.customer c WHERE c.tenant_id = $1)",
    ),
    (
        "walk_ins_without_name",
        "(SELECT count(*) FROM app.customer c WHERE c.tenant_id = $1 AND c.full_name IS NULL)",
    ),
    (
        "named_without_phone",
        "(SELECT count(*) FROM app.customer c WHERE c.tenant_id = $1 AND c.full_name IS NOT NULL AND c.phone_e164 IS NULL)",
    ),
    (
        "without_gender",
        "(SELECT count(*) FROM app.customer c WHERE c.tenant_id = $1 AND c.gender IS NULL)",
    ),
    (
        "birthdays_this_month",
        "(SELECT count(*) FROM app.customer c WHERE c.tenant_id = $1 AND extract(month FROM c.birth_date) = extract(month FROM d.today))",
    ),
    (
        "health_notes",
        "(SELECT count(*) FROM app.customer c WHERE c.tenant_id = $1 AND c.health_note_ct IS NOT NULL)",
    ),
    (
        "lapsed_60_to_180_days",
        "(SELECT count(*) FROM (SELECT max(v.business_date) AS last FROM app.visit v WHERE v.tenant_id = $1 AND v.status = 'completed' GROUP BY v.customer_id) l WHERE l.last BETWEEN d.today - 180 AND d.today - 60)",
    ),
    (
        "follow_ups",
        "(SELECT count(*) FROM app.customer_follow_up f WHERE f.tenant_id = $1)",
    ),
    (
        "follow_ups_with_callback",
        "(SELECT count(*) FROM app.customer_follow_up f WHERE f.tenant_id = $1 AND f.next_follow_up_on IS NOT NULL)",
    ),
    (
        "packages_active",
        "(SELECT count(*) FROM app.customer_package_state p WHERE p.tenant_id = $1 AND app.package_status(p.cancelled_at, p.sessions_remaining, p.expires_on, d.today) = 'active')",
    ),
    (
        "packages_expiring_within_7_days",
        "(SELECT count(*) FROM app.customer_package_state p WHERE p.tenant_id = $1 AND app.package_status(p.cancelled_at, p.sessions_remaining, p.expires_on, d.today) = 'active' AND p.expires_on <= d.today + 7)",
    ),
    (
        "packages_expired",
        "(SELECT count(*) FROM app.customer_package_state p WHERE p.tenant_id = $1 AND app.package_status(p.cancelled_at, p.sessions_remaining, p.expires_on, d.today) = 'expired')",
    ),
    (
        "packages_used_up",
        "(SELECT count(*) FROM app.customer_package_state p WHERE p.tenant_id = $1 AND app.package_status(p.cancelled_at, p.sessions_remaining, p.expires_on, d.today) = 'exhausted')",
    ),
    (
        "packages_cancelled",
        "(SELECT count(*) FROM app.customer_package p WHERE p.tenant_id = $1 AND p.cancelled_at IS NOT NULL)",
    ),
    (
        "refunds",
        "(SELECT count(*) FROM app.refund r WHERE r.tenant_id = $1)",
    ),
    (
        "extensions",
        "(SELECT count(*) FROM app.customer_package_extension e WHERE e.tenant_id = $1)",
    ),
    (
        "paper_book_packages",
        "(SELECT count(*) FROM app.customer_package p WHERE p.tenant_id = $1 AND p.origin = 'opening_balance')",
    ),
    (
        "visits",
        "(SELECT count(*) FROM app.visit v WHERE v.tenant_id = $1)",
    ),
    (
        "visits_completed",
        "(SELECT count(*) FROM app.visit v WHERE v.tenant_id = $1 AND v.status = 'completed')",
    ),
    (
        "visits_today",
        "(SELECT count(*) FROM app.visit v WHERE v.tenant_id = $1 AND v.business_date = d.today)",
    ),
    (
        "visits_voided",
        "(SELECT count(*) FROM app.visit v WHERE v.tenant_id = $1 AND v.status = 'voided' AND NOT EXISTS (SELECT 1 FROM app.visit n WHERE n.tenant_id = v.tenant_id AND n.corrects_visit_id = v.id))",
    ),
    (
        "visits_corrected",
        "(SELECT count(*) FROM app.visit v WHERE v.tenant_id = $1 AND v.corrects_visit_id IS NOT NULL)",
    ),
    (
        "visits_edited",
        "(SELECT count(*) FROM app.audit_log a WHERE a.tenant_id = $1 AND a.action = 'visit.update')",
    ),
    (
        "visits_with_session_note",
        "(SELECT count(*) FROM app.visit v WHERE v.tenant_id = $1 AND v.note_ct IS NOT NULL)",
    ),
    (
        "months_with_visits",
        "(SELECT count(DISTINCT date_trunc('month', v.business_date)) FROM app.visit v WHERE v.tenant_id = $1)",
    ),
    (
        "package_sales",
        "(SELECT count(DISTINCT l.sale_id) FROM app.sale_line l WHERE l.tenant_id = $1 AND l.line_type = 'package')",
    ),
    (
        "package_upsells_in_visit",
        "(SELECT count(DISTINCT s.id) FROM app.sale s JOIN app.sale_line l ON l.tenant_id = s.tenant_id AND l.sale_id = s.id WHERE s.tenant_id = $1 AND s.visit_id IS NOT NULL AND l.line_type = 'package')",
    ),
    (
        "sales_discounted_with_reason",
        "(SELECT count(*) FROM app.sale s WHERE s.tenant_id = $1 AND s.discount_amount > 0 AND s.discount_reason IS NOT NULL)",
    ),
    (
        "sales_voided",
        "(SELECT count(*) FROM app.sale s WHERE s.tenant_id = $1 AND s.status = 'voided')",
    ),
    (
        "payments_cash",
        "(SELECT count(*) FROM app.payment p WHERE p.tenant_id = $1 AND p.kind = 'payment' AND p.method = 'cash')",
    ),
    (
        "payments_transfer",
        "(SELECT count(*) FROM app.payment p WHERE p.tenant_id = $1 AND p.kind = 'payment' AND p.method = 'bank_transfer')",
    ),
    (
        "sales_paid_mixed",
        "(SELECT count(*) FROM (SELECT p.sale_id FROM app.payment p WHERE p.tenant_id = $1 AND p.kind = 'payment' GROUP BY p.sale_id HAVING count(DISTINCT p.method) > 1) m)",
    ),
    (
        "payment_reversals",
        "(SELECT count(*) FROM app.payment p WHERE p.tenant_id = $1 AND p.kind = 'reversal')",
    ),
    (
        "appointments_done",
        "(SELECT count(*) FROM app.appointment a WHERE a.tenant_id = $1 AND a.status = 'done')",
    ),
    (
        "appointments_no_show",
        "(SELECT count(*) FROM app.appointment a WHERE a.tenant_id = $1 AND a.status = 'no_show')",
    ),
    (
        "appointments_cancelled",
        "(SELECT count(*) FROM app.appointment a WHERE a.tenant_id = $1 AND a.status = 'cancelled')",
    ),
    (
        "appointments_today_open",
        "(SELECT count(*) FROM app.appointment a WHERE a.tenant_id = $1 AND a.business_date = d.today AND a.status IN ('booked', 'arrived'))",
    ),
    (
        "appointments_today_overdue",
        "(SELECT count(*) FROM app.appointment a WHERE a.tenant_id = $1 AND a.business_date = d.today AND a.status = 'booked' AND a.starts_at < now())",
    ),
    (
        "appointments_today_arrived",
        "(SELECT count(*) FROM app.appointment a WHERE a.tenant_id = $1 AND a.business_date = d.today AND a.status = 'arrived')",
    ),
    (
        "appointments_next_14_days",
        "(SELECT count(*) FROM app.appointment a WHERE a.tenant_id = $1 AND a.business_date BETWEEN d.today + 1 AND d.today + 14 AND a.status = 'booked')",
    ),
    (
        "appointment_groups",
        "(SELECT count(DISTINCT a.group_id) FROM app.appointment a WHERE a.tenant_id = $1 AND a.group_id IS NOT NULL)",
    ),
    (
        "staff_active",
        "(SELECT count(*) FROM app.staff s WHERE s.tenant_id = $1 AND s.status = 'active')",
    ),
    (
        "staff_left_with_login",
        "(SELECT count(*) FROM app.staff s WHERE s.tenant_id = $1 AND s.status = 'left' AND s.user_id IS NOT NULL)",
    ),
    (
        "attendance_days",
        "(SELECT count(*) FROM app.attendance_day a WHERE a.tenant_id = $1)",
    ),
    (
        "attendance_leave_days",
        "(SELECT count(*) FROM app.attendance_day a WHERE a.tenant_id = $1 AND a.status <> 'present')",
    ),
    (
        "attendance_late_days",
        "(SELECT count(*) FROM app.attendance_day a WHERE a.tenant_id = $1 AND a.late_minutes > 0)",
    ),
    (
        "daily_reviews",
        "(SELECT count(*) FROM app.staff_daily_review r WHERE r.tenant_id = $1)",
    ),
    (
        "evaluations",
        "(SELECT count(*) FROM app.staff_evaluation e WHERE e.tenant_id = $1)",
    ),
    (
        "adjustments",
        "(SELECT count(*) FROM app.staff_adjustment a WHERE a.tenant_id = $1)",
    ),
    (
        "payroll_months_closed",
        "(SELECT count(*) FROM app.payroll_period p WHERE p.tenant_id = $1 AND p.status = 'closed')",
    ),
    (
        "payslips",
        "(SELECT count(*) FROM app.payslip p WHERE p.tenant_id = $1)",
    ),
    (
        "password_reset_requests_open",
        "(SELECT count(*) FROM app.password_reset_request r WHERE r.tenant_id = $1 AND r.closed_at IS NULL)",
    ),
    (
        "audit_entries",
        "(SELECT count(*) FROM app.audit_log a WHERE a.tenant_id = $1)",
    ),
];

/// Counts every scenario category of the spa `slug`.
pub async fn summary(owner: &PgPool, slug: &str) -> anyhow::Result<Summary> {
    let select = COUNTS
        .iter()
        .map(|(name, sql)| format!("{sql} AS {name}"))
        .collect::<Vec<_>>()
        .join(",\n");
    // Built from the constants above only (no input), so splicing is safe.
    let sql = format!(
        "WITH d AS (SELECT (now() AT TIME ZONE t.timezone)::date AS today FROM app.tenant t WHERE t.id = $1)
         SELECT {select} FROM d"
    );
    let tenant: uuid::Uuid = sqlx::query_scalar("SELECT id FROM app.tenant WHERE slug = $1")
        .bind(slug)
        .fetch_optional(owner)
        .await?
        .with_context(|| format!("no spa with slug {slug:?}"))?;
    let row = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(tenant)
        .fetch_one(owner)
        .await?;
    let mut out = Vec::with_capacity(COUNTS.len());
    for (name, _) in COUNTS {
        out.push((*name, row.try_get::<i64, _>(*name)?));
    }
    Ok(Summary(out))
}

/// The report `app seed-demo` prints: the window, each phase's time and SQL statements, and the
/// scenario counts.
pub fn render(report: &DemoReport, summary: &Summary, elapsed: Duration) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "seed-demo: {} → {} in {:.1} s",
        report
            .window_start
            .map(|d| d.to_string())
            .unwrap_or_default(),
        report.today.map(|d| d.to_string()).unwrap_or_default(),
        elapsed.as_secs_f64()
    );
    let _ = writeln!(out, "phases (seconds, SQL statements):");
    for p in &report.phases {
        let _ = writeln!(
            out,
            "  {:<70} {:>8.1} s {:>8}",
            p.name, p.seconds, p.statements
        );
    }
    let visits = report.desk.visits.max(1);
    let _ = writeln!(
        out,
        "front desk: {} visits recorded by the history, {:.1} statements per visit (incl. its sales, voids, corrections)",
        report.desk.visits,
        report.desk.statements_in_visits as f64 / visits as f64
    );
    let _ = writeln!(out, "scenario counts:");
    for (name, n) in &summary.0 {
        let _ = writeln!(out, "  {name:<34} {n:>8}");
    }
    out
}
