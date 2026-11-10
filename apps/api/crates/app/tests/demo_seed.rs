//! `app seed-demo` and `app reset-demo`: a spa gets one complete scenario (months of front desk,
//! packages in every state, appointments, staff books, closed payroll) through the real use cases,
//! a second run refuses to touch it, and `reset-demo` empties exactly the spa's business data so
//! the seed runs again.

mod common;

use app::demo::{
    DELETED, DemoOptions, FORMER_TECHNICIAN_PHONE, KEPT, MANAGER_PHONE, TECHNICIAN_PHONE, reset,
    seed, summary,
};
use axum::http::StatusCode;
use chrono::Datelike;
use common::{PASSWORD, TestResult, call, harness, session_for};
use serde_json::Value;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

fn options(slug: &str, months: u32) -> DemoOptions<'_> {
    DemoOptions {
        tenant_slug: slug,
        manager_password: Some(PASSWORD),
        technician_password: Some(PASSWORD),
        former_technician_password: Some(PASSWORD),
        months,
        workers: 6,
    }
}

/// `app_rw` with room for the seed's workers (the shared helper keeps 4 connections).
async fn wide_app_pool(owner: &PgPool) -> Result<PgPool, sqlx::Error> {
    let password =
        std::env::var("APP_RW_PASSWORD").unwrap_or_else(|_| "app_rw_dev_password".to_owned());
    let options = (*owner.connect_options())
        .clone()
        .username("app_rw")
        .password(&password);
    PgPoolOptions::new()
        .max_connections(8)
        .connect_with(options)
        .await
}

async fn count(owner: &PgPool, sql: &'static str) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(sql).fetch_one(owner).await
}

fn list(reply: &Value) -> Vec<Value> {
    reply.as_array().cloned().unwrap_or_default()
}

fn spa_today() -> chrono::NaiveDate {
    chrono::Utc::now()
        .with_timezone(&chrono_tz::Asia::Ho_Chi_Minh)
        .date_naive()
}

/// Five months: long enough for every category (a lapsed customer 60+ days away, the technician
/// who joined four months ago), short enough for CI.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_full_scenario_covers_every_category_through_the_use_cases(pool: PgPool) -> TestResult {
    let h = harness(pool).await?;
    let app = wide_app_pool(&h.owner).await?;

    let report = seed(&h.owner, &app, &h.cipher, &options("vpyd", 5)).await?;
    let s = summary(&h.owner, "vpyd").await?;

    for category in [
        "walk_ins_without_name",
        "named_without_phone",
        "without_gender",
        "birthdays_this_month",
        "health_notes",
        "lapsed_60_to_180_days",
        "follow_ups_with_callback",
        "packages_active",
        "packages_expiring_within_7_days",
        "packages_expired",
        "packages_used_up",
        "packages_cancelled",
        "refunds",
        "extensions",
        "visits_today",
        "visits_voided",
        "visits_corrected",
        "visits_edited",
        "visits_with_session_note",
        "package_upsells_in_visit",
        "sales_discounted_with_reason",
        "sales_voided",
        "payments_cash",
        "payments_transfer",
        "sales_paid_mixed",
        "payment_reversals",
        "appointments_done",
        "appointments_no_show",
        "appointments_cancelled",
        "appointments_next_14_days",
        "appointment_groups",
        "attendance_leave_days",
        "attendance_late_days",
        "daily_reviews",
        "adjustments",
    ] {
        assert!(s.get(category) > 0, "no {category}: {s:?}");
    }
    assert_eq!(s.get("customers"), 40 + 15 * 5, "{s:?}");
    assert_eq!(s.get("paper_book_packages"), 5, "four paper-book customers");
    assert_eq!(s.get("months_with_visits"), 5);
    assert!(s.get("visits") >= 5 * 28 * 4, "{s:?}");
    assert_eq!(s.get("payroll_months_closed"), 4, "every ended month");
    assert_eq!(s.get("staff_active"), 6, "five technicians and the manager");
    assert_eq!(s.get("staff_left_with_login"), 1);
    assert_eq!(s.get("password_reset_requests_open"), 1);
    assert_eq!(report.cancelled_with_refund, 2);
    assert!(report.staff.weak_evaluations > 0, "{:?}", report.staff);
    // Every completed visit left its audit entry (corrections and completed bookings included).
    assert!(
        count(
            &h.owner,
            "SELECT count(*) FROM app.audit_log WHERE action = 'visit.create'"
        )
        .await?
            >= s.get("visits") - s.get("visits_corrected")
    );

    // The owner sees closed payroll; partial months follow ADR-0019 D4.
    let owner = session_for(&h, common::OWNER_PHONE).await;
    let today = spa_today();
    let this_month = today.with_day(1).unwrap_or(today);
    let last_month = (this_month - chrono::Duration::days(1))
        .with_day(1)
        .unwrap_or(this_month);
    let payroll = |month: chrono::NaiveDate| {
        let (app, owner) = (&h.app, &owner);
        async move {
            call(
                app,
                "GET",
                &format!("/payroll?month={}", month.format("%Y-%m")),
                Some(owner),
                None,
            )
            .await
        }
    };
    let last = payroll(last_month).await;
    assert_eq!(last.status, StatusCode::OK, "{}", last.body);
    assert_eq!(last.body["closed"], true);
    let slips = list(&last.body["payslips"]);
    assert_eq!(
        slips.len(),
        7,
        "six technicians (one left mid-month) and the manager"
    );
    let leaver = slips
        .iter()
        .find(|p| p["full_name"] == "Đỗ Thị Ngân")
        .cloned()
        .unwrap_or_default();
    assert_eq!(leaver["employed_days"], 18, "{leaver}");
    assert!(leaver["base_pay"].as_i64() < leaver["base_salary"].as_i64());
    assert_eq!(leaver["loyalty_bonus_accrued"], 500_000, "18 days ≥ 15");
    let mut four_ago = this_month;
    for _ in 0..4 {
        four_ago = (four_ago - chrono::Duration::days(1))
            .with_day(1)
            .unwrap_or(four_ago);
    }
    let joined = payroll(four_ago).await;
    let joiner = list(&joined.body["payslips"])
        .into_iter()
        .find(|p| p["full_name"] == "Lê Thị Hoa")
        .unwrap_or_default();
    assert!(
        joiner["base_pay"].as_i64() < joiner["base_salary"].as_i64(),
        "{joiner}"
    );
    assert_eq!(
        joiner["loyalty_bonus_accrued"], 0,
        "joined on the 20th: under 15 days"
    );

    // Commission reconciles with the visits, closed month and open month alike.
    for (from, to) in [
        (last_month, this_month - chrono::Duration::days(1)),
        (this_month, today),
    ] {
        let reply = payroll(from).await;
        let mut total = 0_i64;
        for slip in list(&reply.body["payslips"]) {
            let staff = Uuid::parse_str(slip["staff_id"].as_str().unwrap_or_default())?;
            let base: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(vl.line_value), 0)::bigint FROM app.visit_line vl
                 JOIN app.visit v ON v.tenant_id = vl.tenant_id AND v.id = vl.visit_id
                 WHERE vl.staff_id = $1 AND v.status = 'completed'
                   AND v.business_date BETWEEN $2 AND $3",
            )
            .bind(staff)
            .bind(from)
            .bind(to)
            .fetch_one(&h.owner)
            .await?;
            assert_eq!(
                slip["service_commission_base"], base,
                "{}",
                slip["full_name"]
            );
            total += base;
        }
        let visits: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(vl.line_value), 0)::bigint FROM app.visit_line vl
             JOIN app.visit v ON v.tenant_id = vl.tenant_id AND v.id = vl.visit_id
             WHERE v.status = 'completed' AND v.business_date BETWEEN $1 AND $2",
        )
        .bind(from)
        .bind(to)
        .fetch_one(&h.owner)
        .await?;
        assert_eq!(total, visits, "every completed line belongs to one payslip");
    }

    // Mistakes are fixed on their own day: every reversal sits on its payment's business day, so
    // past corrections do not pile up on the run date (live test 2026-11-10: today was −989.000đ).
    assert_eq!(
        count(
            &h.owner,
            "SELECT count(*) FROM app.payment r
             JOIN app.payment p ON p.tenant_id = r.tenant_id AND p.id = r.reversal_of
             WHERE r.kind = 'reversal' AND r.business_date <> p.business_date"
        )
        .await?,
        0,
        "a reversal booked on another day than its payment"
    );
    assert!(
        count(
            &h.owner,
            "SELECT count(*) FROM app.payment r
             JOIN app.payment p ON p.tenant_id = r.tenant_id AND p.id = r.reversal_of
             WHERE r.kind = 'reversal' AND r.business_date < (now() AT TIME ZONE 'Asia/Ho_Chi_Minh')::date"
        )
        .await?
            > 0,
        "past days keep their corrections"
    );
    // Both refunds on past days (about three weeks and three months back), after the package was
    // sold and after its last session: no visit takes a session from a cancelled package.
    let refund_days: Vec<chrono::NaiveDate> =
        sqlx::query_scalar("SELECT business_date FROM app.refund ORDER BY business_date")
            .fetch_all(&h.owner)
            .await?;
    assert_eq!(refund_days.len(), 2, "{refund_days:?}");
    assert!(refund_days.iter().all(|d| *d < today), "{refund_days:?}");
    assert_ne!(refund_days[0], refund_days[1]);
    assert_eq!(
        count(
            &h.owner,
            "SELECT count(*) FROM app.refund f
             JOIN app.customer_package cp ON cp.tenant_id = f.tenant_id AND cp.id = f.customer_package_id
             JOIN app.sale s ON s.tenant_id = cp.tenant_id AND s.id = cp.sale_id
             WHERE f.business_date < s.business_date
                OR EXISTS (SELECT 1 FROM app.visit_line vl
                           JOIN app.visit v ON v.tenant_id = vl.tenant_id AND v.id = vl.visit_id
                           WHERE vl.tenant_id = f.tenant_id
                             AND vl.customer_package_id = f.customer_package_id
                             AND v.status = 'completed' AND v.business_date >= f.business_date)"
        )
        .await?,
        0,
        "a refund before its sale, or a session used on or after it"
    );
    // Today keeps only today's own corrections: the owner's "Thực thu hôm nay" is not negative.
    let daily = call(&h.app, "GET", "/reports/daily", Some(&owner), None).await;
    assert_eq!(daily.status, StatusCode::OK, "{}", daily.body);
    assert!(
        daily.body["revenue"]["net"]["total"]
            .as_i64()
            .is_some_and(|net| net >= 0),
        "today's net takings: {}",
        daily.body["revenue"]
    );
    assert_eq!(
        daily.body["revenue"]["net"]["total"].as_i64(),
        Some(s.get("net_takings_today_vnd")),
        "the CLI's count is the report's figure"
    );

    // Each login has its screens: the desk today, the technician's own month, the leaver's past.
    let manager = session_for(&h, MANAGER_PHONE).await;
    let desk = list(
        &call(&h.app, "GET", "/visits", Some(&manager), None)
            .await
            .body,
    );
    assert!(!desk.is_empty());
    assert!(desk.iter().any(|v| v["status"] == "voided"));
    let tech = session_for(&h, TECHNICIAN_PHONE).await;
    let mine = call(&h.app, "GET", "/me/staff", Some(&tech), None).await;
    assert_eq!(mine.body["linked"], true, "{}", mine.body);
    let former = session_for(&h, FORMER_TECHNICIAN_PHONE).await;
    let past = call(
        &h.app,
        "GET",
        &format!("/me/staff?month={}", last_month.format("%Y-%m")),
        Some(&former),
        None,
    )
    .await;
    assert_eq!(past.status, StatusCode::OK, "{}", past.body);
    assert!(
        past.body["data"]["payslip"]["base_salary"].is_number(),
        "{}",
        past.body
    );
    assert_eq!(
        call(&h.app, "GET", "/visits", Some(&former), None)
            .await
            .status,
        StatusCode::FORBIDDEN,
        "former staff only read their own past"
    );

    // A second run changes nothing.
    let visits = s.get("visits");
    let again = seed(&h.owner, &app, &h.cipher, &options("vpyd", 5)).await;
    let message = again.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(message.contains("already has demo data"), "{message}");
    assert_eq!(
        count(&h.owner, "SELECT count(*) FROM app.visit").await?,
        visits
    );
    Ok(())
}

async fn tenant_id(owner: &PgPool, slug: &str) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM app.tenant WHERE slug = $1")
        .bind(slug)
        .fetch_one(owner)
        .await
}

/// Rows of `table` for the spa, and a fingerprint of their content.
async fn rows(owner: &PgPool, table: &str, tenant: Uuid) -> Result<(i64, String), sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT count(*), coalesce(md5(string_agg(t::text, '|' ORDER BY t::text)), '')
         FROM app.{table} t WHERE tenant_id = $1"
    )))
    .bind(tenant)
    .fetch_one(owner)
    .await
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn reset_empties_one_spas_business_data_and_the_seed_runs_again(pool: PgPool) -> TestResult {
    let h = harness(pool).await?;
    identity::provisioning::create_tenant(&h.owner, "khac", "Spa khác", "Cơ sở chính").await?;
    common::add_member(
        &h.owner,
        "khac",
        "0905123999",
        "Chủ spa khác",
        identity::SystemRole::Owner,
    )
    .await?;
    let app = wide_app_pool(&h.owner).await?;
    seed(&h.owner, &app, &h.cipher, &options("vpyd", 2)).await?;
    seed(&h.owner, &app, &h.cipher, &options("khac", 2)).await?;
    let (mine, other) = (
        tenant_id(&h.owner, "vpyd").await?,
        tenant_id(&h.owner, "khac").await?,
    );

    let mut kept = Vec::new();
    for table in KEPT {
        kept.push(rows(&h.owner, table, mine).await?);
    }
    let mut others = Vec::new();
    for table in DELETED.iter().chain(KEPT) {
        others.push(rows(&h.owner, table, other).await?);
    }
    let fingerprint = "SELECT md5(string_agg(t::text, '|' ORDER BY t::text)) FROM (
        SELECT row_to_json(x)::text AS t FROM app.tenant x
        UNION ALL SELECT row_to_json(u)::text FROM app.user_account u
        UNION ALL SELECT row_to_json(r)::text FROM app.mfa_recovery_code r) s";
    let identities: String = sqlx::query_scalar(fingerprint).fetch_one(&h.owner).await?;

    let report = reset(&h.owner, "vpyd").await?;

    assert!(report.total() > 1_000, "{report:?}");
    assert!(
        report
            .deleted
            .iter()
            .any(|(t, n)| *t == "payment" && *n > 0)
    );
    for table in DELETED {
        assert_eq!(
            rows(&h.owner, table, mine).await?.0,
            0,
            "{table} still has rows"
        );
    }
    for (table, before) in KEPT.iter().zip(&kept) {
        assert_eq!(
            &rows(&h.owner, table, mine).await?,
            before,
            "{table} changed"
        );
    }
    for (table, before) in DELETED.iter().chain(KEPT).zip(&others) {
        assert_eq!(
            &rows(&h.owner, table, other).await?,
            before,
            "other spa's {table} changed"
        );
    }
    let after: String = sqlx::query_scalar(fingerprint).fetch_one(&h.owner).await?;
    assert_eq!(after, identities, "spas, logins and MFA are kept");

    // The append-only triggers are back: the other spa's money still cannot be rewritten.
    let rewrite = sqlx::query("UPDATE app.payment SET amount = amount WHERE tenant_id = $1")
        .bind(other)
        .execute(&h.owner)
        .await;
    assert!(
        rewrite.is_err(),
        "payment_append_only must be enabled again"
    );

    // The seed runs again on the emptied spa and reuses the logins (no password needed).
    let again = seed(
        &h.owner,
        &app,
        &h.cipher,
        &DemoOptions {
            manager_password: None,
            technician_password: None,
            former_technician_password: None,
            ..options("vpyd", 2)
        },
    )
    .await?;
    assert!(again.desk.visits > 0);
    assert_eq!(
        count(
            &h.owner,
            "SELECT count(*) FROM app.user_account WHERE phone_e164 LIKE '+84905000%'"
        )
        .await?,
        3,
        "manager, technician and former technician logins, not duplicated"
    );
    Ok(())
}

/// Every tenant-scoped table is either business data (`reset-demo` deletes it) or configuration
/// (kept): a new table fails here until it is classified.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn every_tenant_table_is_classified_for_reset(pool: PgPool) -> TestResult {
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT c.table_name::text FROM information_schema.columns c
         JOIN information_schema.tables t
           ON t.table_schema = c.table_schema AND t.table_name = c.table_name
         WHERE c.table_schema = 'app' AND c.column_name = 'tenant_id' AND t.table_type = 'BASE TABLE'
         ORDER BY 1",
    )
    .fetch_all(&pool)
    .await?;
    let mut classified: Vec<String> = DELETED
        .iter()
        .chain(KEPT)
        .map(|t| (*t).to_owned())
        .collect();
    classified.sort();
    assert_eq!(tables, classified);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_new_demo_login_without_its_password_is_refused_before_anything_is_written(
    pool: PgPool,
) -> TestResult {
    let h = harness(pool).await?;
    let app = wide_app_pool(&h.owner).await?;
    let refused = seed(
        &h.owner,
        &app,
        &h.cipher,
        &DemoOptions {
            technician_password: None,
            ..options("vpyd", 2)
        },
    )
    .await;
    let message = refused.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(message.contains("APP_NEW_TECHNICIAN_PASSWORD"), "{message}");
    assert_eq!(
        count(&h.owner, "SELECT count(*) FROM app.customer").await?,
        0
    );
    assert_eq!(count(&h.owner, "SELECT count(*) FROM app.staff").await?, 0);
    assert_eq!(
        count(&h.owner, "SELECT count(*) FROM app.service").await?,
        0
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_spa_without_an_owner_is_refused_before_anything_is_written(pool: PgPool) -> TestResult {
    identity::provisioning::create_tenant(&pool, "trong", "Spa trống", "Cơ sở chính").await?;
    let app = wide_app_pool(&pool).await?;
    let cipher = kernel::crypto::FieldCipher::from_base64(common::TEST_DATA_KEY)?;

    let refused = seed(&pool, &app, &cipher, &options("trong", 2)).await;

    let message = refused.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(message.contains("no owner"), "{message}");
    assert_eq!(count(&pool, "SELECT count(*) FROM app.customer").await?, 0);
    assert_eq!(count(&pool, "SELECT count(*) FROM app.staff").await?, 0);
    Ok(())
}
