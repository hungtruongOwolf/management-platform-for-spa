//! `app reset-demo --tenant <slug>`: wipes one spa's business data (customers, money, packages,
//! appointments, staff books, its audit trail) so `seed-demo` can fill it again, and keeps what
//! makes the spa work: the spa, branch, roles and permissions, feature flags, the catalog and
//! package plans, calendar and payroll *settings*, and every login with its MFA. Platform tables
//! are never touched. Development and staging only (the CLI refuses `APP_ENV=production`).
//!
//! Runs in one transaction as the schema owner. Business tables are append-only by trigger
//! (payments, refunds, audit, closed payroll months…); those triggers are disabled for the
//! transaction with `ALTER TABLE … DISABLE TRIGGER USER` (the owner owns the tables; no superuser,
//! no `session_replication_role`, which Supabase does not allow) and enabled again before commit.
//! Foreign keys stay enforced: the tables are emptied children first. Any failure rolls the whole
//! reset back, triggers included.

use anyhow::Context;
use sqlx::PgPool;
use uuid::Uuid;

/// Tenant-scoped business tables, in an order where every row is deleted before the rows it
/// references (`sale.visit_id` is deferred, which breaks the visit ↔ sale cycle).
pub const DELETED: &[&str] = &[
    "audit_log",
    "idempotency_record",
    "password_reset_request",
    "customer_follow_up",
    "appointment_service",
    "appointment",
    "payslip",
    "payroll_period",
    "staff_adjustment",
    "staff_evaluation",
    "staff_daily_review",
    "attendance_day",
    "refund",
    "payment",
    "visit_line",
    "visit",
    "customer_package_extension",
    "sale_line",
    "customer_package_item",
    "customer_package",
    "sale",
    "staff_pay",
    "staff_employment",
    "staff",
    "customer",
];

/// Tenant-scoped tables that are configuration, not history, and stay. `payroll_rules` and
/// `appointment_setting` are the owner's settings (pay rules, opening hours, slot length);
/// `platform_audit_log` belongs to the platform console and is never touched by a spa tool.
pub const KEPT: &[&str] = &[
    "appointment_setting",
    "branch",
    "membership",
    "membership_role",
    "package_plan",
    "package_plan_item",
    "payroll_rules",
    "platform_audit_log",
    "role",
    "role_permission",
    "service",
    "service_category",
    "service_price",
    "tenant_feature",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResetReport {
    /// Rows deleted per table, in [`DELETED`] order.
    pub deleted: Vec<(&'static str, u64)>,
}

impl ResetReport {
    pub fn total(&self) -> u64 {
        self.deleted.iter().map(|(_, n)| n).sum()
    }
}

/// Deletes every business row of the spa `slug` (see the module docs), as the schema owner.
pub async fn reset(owner: &PgPool, slug: &str) -> anyhow::Result<ResetReport> {
    let mut tx = owner.begin().await?;
    // Waiting forever behind a busy API transaction would hold every lock taken so far.
    sqlx::query("SET LOCAL lock_timeout = '15s'")
        .execute(&mut *tx)
        .await?;
    let tenant: Uuid = sqlx::query_scalar("SELECT id FROM app.tenant WHERE slug = $1 FOR UPDATE")
        .bind(slug)
        .fetch_optional(&mut *tx)
        .await?
        .with_context(|| format!("no spa with slug {slug:?}"))?;
    // Table names come from the constants above, never from input: safe to splice. DDL and a
    // table chosen at run time cannot go through the compile-time checked `query!`.
    for table in DELETED {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "ALTER TABLE app.{table} DISABLE TRIGGER USER"
        )))
        .execute(&mut *tx)
        .await
        .with_context(|| format!("disable the triggers of {table}"))?;
    }
    let mut deleted = Vec::with_capacity(DELETED.len());
    for table in DELETED {
        let n = sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM app.{table} WHERE tenant_id = $1"
        )))
        .bind(tenant)
        .execute(&mut *tx)
        .await
        .with_context(|| format!("empty {table}"))?
        .rows_affected();
        deleted.push((*table, n));
    }
    // The deferred `sale.visit_id` checks are queued as trigger events, and Postgres refuses
    // `ALTER TABLE` on a table with pending ones: run them now (every row they check is gone).
    sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
        .execute(&mut *tx)
        .await?;
    for table in DELETED {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "ALTER TABLE app.{table} ENABLE TRIGGER USER"
        )))
        .execute(&mut *tx)
        .await
        .with_context(|| format!("enable the triggers of {table}"))?;
    }
    tx.commit().await?;
    Ok(ResetReport { deleted })
}
