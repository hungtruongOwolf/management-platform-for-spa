//! Perf budget of the management views (spec management-views §6: p95 read < 300 ms) on three
//! years of one busy spa: ~44.000 visits, ~31.000 sales, 3.000 customers, 3.300 packages.
//! Ignored by default (seeding takes a while); run by hand:
//!   cargo test -p app --test reports_bench -- --ignored --nocapture
//! It prints p50/p95 per endpoint and EXPLAIN (ANALYZE) of the heaviest queries.

mod common;

use axum::http::StatusCode;
use common::{Harness, OWNER_PHONE, TestResult, call, call_with_headers, harness, session_for};
use serde_json::json;
use sqlx::PgPool;
use std::time::Instant;

const DAYS: i32 = 1100;
const SAMPLES: usize = 20;
const BUDGET_MS: u128 = 300;
/// Owner-only 3–5 year reports (`report.long_range`, EM Q5): rare and computed on demand.
const LONG_RANGE_BUDGET_MS: u128 = 1500;

/// Bulk rows straight into the tables (owner pool), shaped like the use cases write them.
const SEED: &str = r#"
CREATE TEMP TABLE b_ctx AS
SELECT t.id AS tenant, br.id AS branch, s.id AS service, p.id AS plan, st.id AS staff,
       (SELECT m.user_id FROM app.membership m WHERE m.tenant_id = t.id LIMIT 1) AS usr
FROM app.tenant t
JOIN app.branch br ON br.tenant_id = t.id
JOIN app.service s ON s.tenant_id = t.id
JOIN app.package_plan p ON p.tenant_id = t.id
JOIN app.staff st ON st.tenant_id = t.id
WHERE t.slug = 'vpyd' LIMIT 1;

CREATE TEMP TABLE b_c AS
SELECT g AS n, gen_random_uuid() AS id FROM generate_series(1, 3000) g;
INSERT INTO app.customer (id, tenant_id, full_name, search_text, phone_e164)
SELECT c.id, x.tenant, 'Khách ' || c.n, 'khach ' || c.n, '+8491' || lpad(c.n::text, 7, '0')
FROM b_c c, b_ctx x;

-- 3 package sales a day.
CREATE TEMP TABLE b_ps AS
SELECT row_number() OVER () AS n, gen_random_uuid() AS sale, gen_random_uuid() AS pkg,
       gen_random_uuid() AS item, d::date AS day, c.id AS customer
FROM generate_series(current_date - $DAYS, current_date, interval '1 day') d
CROSS JOIN generate_series(1, 3) k
JOIN b_c c ON c.n = 1 + abs(hashtext(d::text || k)) % 3000;
INSERT INTO app.sale (id, tenant_id, branch_id, customer_id, business_date, subtotal, total, amount_paid, closer_staff_id, created_by, created_at)
SELECT p.sale, x.tenant, x.branch, p.customer, p.day, 1200000, 1200000, 1200000, x.staff, x.usr, p.day + time '09:00'
FROM b_ps p, b_ctx x;
INSERT INTO app.customer_package (id, tenant_id, customer_id, plan_id, name_snapshot, sold_price, origin, sale_id, closer_staff_id, starts_on, created_by, created_at)
SELECT p.pkg, x.tenant, p.customer, x.plan, 'Gói 10 buổi', 1200000, 'sale', p.sale, x.staff, p.day, x.usr, p.day + time '09:00'
FROM b_ps p, b_ctx x;
INSERT INTO app.sale_line (id, tenant_id, sale_id, line_type, package_plan_id, customer_package_id, description_snapshot, qty, unit_price, line_total, position)
SELECT gen_random_uuid(), x.tenant, p.sale, 'package', x.plan, p.pkg, 'Gói 10 buổi', 1, 1200000, 1200000, 0
FROM b_ps p, b_ctx x;
INSERT INTO app.customer_package_item (id, tenant_id, customer_package_id, service_id, name_snapshot, sessions_purchased, sessions_remaining, per_session_value)
SELECT p.item, x.tenant, p.pkg, x.service, 'Gội đầu', 10, 5, 120000 FROM b_ps p, b_ctx x;
INSERT INTO app.payment (id, tenant_id, branch_id, sale_id, business_date, method, amount, created_by, paid_at)
SELECT gen_random_uuid(), x.tenant, x.branch, p.sale, p.day, 'bank_transfer', 1200000, x.usr, p.day + time '09:00'
FROM b_ps p, b_ctx x;
-- Every 10th package cancelled with a refund.
INSERT INTO app.refund (id, tenant_id, branch_id, customer_id, customer_package_id, business_date, method, amount, reason, created_by)
SELECT gen_random_uuid(), x.tenant, x.branch, p.customer, p.pkg, p.day, 'cash', 100000, 'Khách chuyển nhà', x.usr
FROM b_ps p, b_ctx x WHERE p.n % 10 = 0;

-- 25 one-off visits a day, each with its own sale; 15 package visits.
CREATE TEMP TABLE b_v AS
SELECT gen_random_uuid() AS visit, gen_random_uuid() AS sale, d::date AS day, k,
       c.id AS customer, d::date + time '08:00' + k * interval '15 minutes' AS at
FROM generate_series(current_date - $DAYS, current_date, interval '1 day') d
CROSS JOIN generate_series(1, 25) k
JOIN b_c c ON c.n = 1 + abs(hashtext(d::text || 'v' || k)) % 3000;
INSERT INTO app.sale (id, tenant_id, branch_id, customer_id, business_date, subtotal, total, amount_paid, created_by, created_at)
SELECT v.sale, x.tenant, x.branch, v.customer, v.day, 150000, 150000, 150000, x.usr, v.at FROM b_v v, b_ctx x;
INSERT INTO app.sale_line (id, tenant_id, sale_id, line_type, service_id, description_snapshot, qty, unit_price, line_total, position)
SELECT gen_random_uuid(), x.tenant, v.sale, 'service', x.service, 'Gội đầu', 1, 150000, 150000, 0 FROM b_v v, b_ctx x;
INSERT INTO app.payment (id, tenant_id, branch_id, sale_id, business_date, method, amount, created_by, paid_at)
SELECT gen_random_uuid(), x.tenant, x.branch, v.sale, v.day, 'cash', 150000, x.usr, v.at FROM b_v v, b_ctx x;
INSERT INTO app.visit (id, tenant_id, branch_id, customer_id, business_date, check_in_at, sale_id, created_by)
SELECT v.visit, x.tenant, x.branch, v.customer, v.day, v.at, v.sale, x.usr FROM b_v v, b_ctx x;
INSERT INTO app.visit_line (id, tenant_id, visit_id, position, service_id, service_name_snapshot, source, staff_id, qty, price_variant, unit_price, line_value)
SELECT gen_random_uuid(), x.tenant, v.visit, 0, x.service, 'Gội đầu', 'one_off', x.staff, 1, 'all', 150000, 150000
FROM b_v v, b_ctx x;

CREATE TEMP TABLE b_pv AS
SELECT gen_random_uuid() AS visit, d::date AS day, p.customer, p.pkg, p.item,
       d::date + time '13:00' + k * interval '10 minutes' AS at
FROM generate_series(current_date - $DAYS, current_date, interval '1 day') d
CROSS JOIN generate_series(1, 15) k
JOIN b_ps p ON p.n = 1 + abs(hashtext(d::text || 'p' || k)) % (SELECT count(*) FROM b_ps);
INSERT INTO app.visit (id, tenant_id, branch_id, customer_id, business_date, check_in_at, created_by)
SELECT v.visit, x.tenant, x.branch, v.customer, v.day, v.at, x.usr FROM b_pv v, b_ctx x;
INSERT INTO app.visit_line (id, tenant_id, visit_id, position, service_id, service_name_snapshot, source, customer_package_id, customer_package_item_id, sessions_left_after, staff_id, qty, unit_price, line_value)
SELECT gen_random_uuid(), x.tenant, v.visit, 0, x.service, 'Gội đầu', 'package', v.pkg, v.item, 5, x.staff, 1, 120000, 120000
FROM b_pv v, b_ctx x;
ANALYZE;
"#;

fn p(samples: &mut [u128], q: f64) -> u128 {
    samples.sort_unstable();
    let i = ((samples.len() as f64 - 1.0) * q).round() as usize;
    samples.get(i).copied().unwrap_or_default()
}

/// The audit trail of those three years, shaped like the writers store it (ADR-0022 bench,
/// architecture/record-history §6): ~60.000 rows a year — every visit's `visit.create` and its
/// sale's `sale.create` at the same instant, each package sale's two rows, a technician change on
/// one visit in twenty, eight people's attendance and daily reviews (one save of the book a day:
/// the eight cells at one instant, on the person, each with its own `date`; one cell in ten written
/// again), and 25 bookings a day (real `app.appointment` rows, the next 14 days included) with their
/// arrival and completion, one in ten moved an hour.
const AUDIT_SEED: &str = r#"
-- Each one-off sale was made in its visit (the revenue book's `visit_id`, what `edited` looks up).
-- The link is set once at insert by the writers; the bulk seed sets it after the visits exist.
SET session_replication_role = replica;
UPDATE app.sale s SET visit_id = v.visit FROM b_v v WHERE s.id = v.sale;
SET session_replication_role = DEFAULT;
-- The bookings, no technician (the overlap exclusion is not what is measured).
CREATE TEMP TABLE b_a AS
SELECT gen_random_uuid() AS id, d::date AS day, k, c.id AS customer,
       d::date + time '08:00' + k * interval '20 minutes' AS at
FROM generate_series(current_date - $DAYS, current_date + 13, interval '1 day') d
CROSS JOIN generate_series(1, 25) k
JOIN b_c c ON c.n = 1 + abs(hashtext(d::text || 'a' || k)) % 3000;
INSERT INTO app.appointment (id, tenant_id, branch_id, customer_id, business_date, starts_at, ends_at, created_by)
SELECT a.id, x.tenant, x.branch, a.customer, a.day, a.at, a.at + interval '1 hour', x.usr FROM b_a a, b_ctx x;
-- Eight people: the bench's technician and seven more (ids only: the trail has no foreign key).
CREATE TEMP TABLE b_people AS
SELECT person, CASE WHEN person = 1 THEN x.staff ELSE md5('staff' || person)::uuid END AS staff
FROM generate_series(1, 8) person, b_ctx x;
INSERT INTO app.audit_log (id, tenant_id, actor_user_id, actor_kind, action, entity_type, entity_id, at, before, after)
SELECT gen_random_uuid(), x.tenant, x.usr, 'user', 'appointment.update', 'appointment', a.id,
       a.at - interval '1 day', jsonb_build_object('starts_at', a.at - interval '1 hour', 'staff_id', null),
       jsonb_build_object('starts_at', a.at, 'staff_id', null)
FROM b_a a, b_ctx x WHERE a.k % 10 = 0
UNION ALL
SELECT gen_random_uuid(), x.tenant, x.usr, 'user', 'attendance.record', 'staff', p.staff,
       d::date + time '20:00', jsonb_build_object('date', d::date, 'status', 'present', 'has_note', false),
       jsonb_build_object('date', d::date, 'status', 'present', 'check_in', '09:10', 'has_note', false)
FROM generate_series(current_date - $DAYS, current_date, interval '1 day') d
CROSS JOIN b_people p, b_ctx x
WHERE abs(hashtext(d::text || p.person)) % 10 = 0;
INSERT INTO app.audit_log (id, tenant_id, actor_user_id, actor_kind, action, entity_type, entity_id, at, after)
SELECT gen_random_uuid(), x.tenant, x.usr, 'user', 'visit.create', 'visit', v.visit, v.at,
       jsonb_build_object('sale_id', v.sale, 'total', 150000, 'discount_amount', 0)
FROM b_v v, b_ctx x
UNION ALL
SELECT gen_random_uuid(), x.tenant, x.usr, 'user', 'sale.create', 'sale', v.sale, v.at,
       jsonb_build_object('total', 150000, 'discount_amount', 0)
FROM b_v v, b_ctx x
UNION ALL
SELECT gen_random_uuid(), x.tenant, x.usr, 'user', 'visit.create', 'visit', v.visit, v.at,
       jsonb_build_object('total', 0)
FROM b_pv v, b_ctx x
UNION ALL
SELECT gen_random_uuid(), x.tenant, x.usr, 'user', 'sale.create', 'sale', p.sale, p.day + time '09:00',
       jsonb_build_object('total', 1200000, 'discount_amount', 0)
FROM b_ps p, b_ctx x
UNION ALL
SELECT gen_random_uuid(), x.tenant, x.usr, 'user', 'customer_package.create', 'customer_package', p.pkg,
       p.day + time '09:00', jsonb_build_object('sale_id', p.sale, 'origin', 'sale', 'sold_price', 1200000)
FROM b_ps p, b_ctx x
UNION ALL
SELECT gen_random_uuid(), x.tenant, x.usr, 'user', 'visit.update', 'visit', v.visit, v.at + interval '1 hour',
       jsonb_build_object('staff', jsonb_build_array(jsonb_build_object(
         'line_id', gen_random_uuid(), 'from', x.staff, 'to', gen_random_uuid())))
FROM b_v v, b_ctx x WHERE v.k % 20 = 0
UNION ALL
SELECT gen_random_uuid(), x.tenant, x.usr, 'user', book.action, 'staff', p.staff,
       d::date + time '18:00', jsonb_build_object('date', d::date, 'status', 'present')
FROM generate_series(current_date - $DAYS, current_date, interval '1 day') d
CROSS JOIN b_people p
CROSS JOIN (VALUES ('attendance.record'), ('evaluation.daily_review')) book(action), b_ctx x
UNION ALL
SELECT gen_random_uuid(), x.tenant, x.usr, 'user', step.action, 'appointment', a.id,
       a.at + step.after_minutes * interval '1 minute', '{}'::jsonb
FROM b_a a
CROSS JOIN (VALUES ('appointment.create', -1440), ('appointment.arrive', 0),
                   ('appointment.complete', 60)) step(action, after_minutes), b_ctx x;
ANALYZE app.appointment;
ANALYZE app.audit_log;
"#;

/// The bench spa: one owner, a technician, one service, one plan, then three years of books.
async fn books(pool: PgPool) -> Result<(Harness, String), Box<dyn std::error::Error>> {
    let h = harness(pool).await?;
    let owner = session_for(&h, OWNER_PHONE).await;
    org::provisioning::add_staff(&h.owner, "vpyd", "KTV Lan", "Kỹ thuật viên").await?;
    let category = call(
        &h.app,
        "POST",
        "/service-categories",
        Some(&owner),
        Some(json!({"name": "Dưỡng sinh"})),
    )
    .await;
    let service = call(&h.app, "POST", "/services", Some(&owner), Some(json!({
        "category_id": category.body["id"], "name": "Gội đầu", "description": null, "duration_min": 60,
        "recommended": false, "active": true, "prices": {"all": 150_000}
    }))).await;
    call(&h.app, "POST", "/package-plans", Some(&owner), Some(json!({
        "name": "Gói 10 buổi", "price": 1_200_000, "validity_months": null, "gift_note": null,
        "active": true, "items": [{"service_id": service.body["id"], "sessions": 10, "bonus_sessions": 0}]
    }))).await;

    let started = Instant::now();
    sqlx::raw_sql(sqlx::AssertSqlSafe(
        SEED.replace("$DAYS", &DAYS.to_string()),
    ))
    .execute(&h.owner)
    .await?;
    let visits: i64 = sqlx::query_scalar("SELECT count(*) FROM app.visit")
        .fetch_one(&h.owner)
        .await?;
    let sales: i64 = sqlx::query_scalar("SELECT count(*) FROM app.sale")
        .fetch_one(&h.owner)
        .await?;
    println!(
        "seeded {visits} visits, {sales} sales in {:?}",
        started.elapsed()
    );
    Ok((h, owner))
}

/// `SAMPLES` timed GETs; `(p50, p95)` in ms. Each from its own client IP (the per-IP rate limit
/// is not what is measured).
async fn timed(h: &Harness, session: &str, path: &str, n: &mut u32) -> (u128, u128) {
    let mut ms = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        *n += 1;
        let ip = format!("10.8.{}.{}", *n / 250, *n % 250);
        let t = Instant::now();
        let reply = call_with_headers(
            &h.app,
            "GET",
            path,
            Some(session),
            None,
            &[("X-Forwarded-For", &ip)],
        )
        .await;
        ms.push(t.elapsed().as_millis());
        assert_eq!(reply.status, StatusCode::OK, "{path}: {}", reply.body);
    }
    (p(&mut ms, 0.5), p(&mut ms, 0.95))
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
#[ignore = "perf bench: seeds three years of data; run with --ignored"]
async fn three_years_of_books_read_within_budget(pool: PgPool) -> TestResult {
    let (h, owner) = books(pool).await?;
    let customer: String = sqlx::query_scalar(
        "SELECT customer_id::text FROM app.visit GROUP BY customer_id ORDER BY count(*) DESC LIMIT 1",
    )
    .fetch_one(&h.owner)
    .await?;
    let package: String = sqlx::query_scalar(
        "SELECT customer_package_id::text FROM app.visit_line WHERE customer_package_id IS NOT NULL
         GROUP BY 1 ORDER BY count(*) DESC LIMIT 1",
    )
    .fetch_one(&h.owner)
    .await?;

    let paths = [
        "/reports/daily".to_owned(),
        "/reports/range?granularity=day".to_owned(),
        "/reports/range?from=2025-11-06&to=2026-11-05&granularity=month".to_owned(),
        "/reports/ledger?limit=100".to_owned(),
        "/reports/ledger?from=2026-08-06&to=2026-11-05&limit=100".to_owned(),
        format!("/customers/{customer}/history?limit=50"),
        format!("/customers/{customer}/summary"),
        format!("/customer-packages/{package}/usage"),
        "/packages?limit=50".to_owned(),
        "/packages?expiring_within_days=30&limit=50".to_owned(),
        "/packages?q=khach%2012&limit=50".to_owned(),
        "/customers/inactive?days=30&limit=50".to_owned(),
        "/customers/inactive?days=30&scope=all&limit=50".to_owned(),
        // Round 3: presets, multi-year, history search, follow-up views, phone check.
        "/reports/range?preset=last_12_months".to_owned(),
        "/reports/range?preset=last_2_years".to_owned(),
        "/reports/range?from=2024-11-05&to=2026-11-05&granularity=year".to_owned(),
        format!("/customers/{customer}/history?q=goi&limit=50"),
        format!("/customers/{customer}/history?kind=visit&from=2026-01-01&to=2026-11-05&limit=50"),
        "/customers/inactive?days=7&follow_up=due&limit=50".to_owned(),
        "/customers/inactive?days=7&q=khach&kind=one_off&limit=50".to_owned(),
        "/customers/phone-check?phone=0905000012".to_owned(),
        // Owner-only long range: the whole 3-year seed, by month and by year.
        "/reports/range?preset=last_5_years".to_owned(),
        "/reports/range?preset=last_3_years".to_owned(),
        "/reports/range?from=2023-11-06&to=2026-11-05&granularity=month".to_owned(),
    ];
    let mut over = Vec::new();
    let mut n = 0_u32;
    for path in &paths {
        let mut ms = Vec::with_capacity(SAMPLES);
        for _ in 0..SAMPLES {
            let t = Instant::now();
            // A fresh client IP per request: the per-IP rate limit is not what is measured.
            n += 1;
            let ip = format!("10.9.{}.{}", n / 250, n % 250);
            let reply = call_with_headers(
                &h.app,
                "GET",
                path,
                Some(&owner),
                None,
                &[("X-Forwarded-For", &ip)],
            )
            .await;
            ms.push(t.elapsed().as_millis());
            assert_eq!(reply.status, StatusCode::OK, "{path}: {}", reply.body);
        }
        let (p50, p95) = (p(&mut ms, 0.5), p(&mut ms, 0.95));
        println!("p50 {p50:>4} ms  p95 {p95:>4} ms  {path}");
        let budget = if path.contains("last_5_years")
            || path.contains("last_3_years")
            || path.contains("from=2023-")
        {
            LONG_RANGE_BUDGET_MS
        } else {
            BUDGET_MS
        };
        if p95 >= budget {
            over.push(format!("{path}: p95 {p95} ms (budget {budget} ms)"));
        }
    }

    let mut tx = h.owner.begin().await?;
    for (label, sql) in [
        (
            "daily money",
            "EXPLAIN (ANALYZE, BUFFERS) SELECT b.entry, b.sale_kind, sum(b.cash) FROM app.revenue_book b WHERE b.tenant_id = (SELECT id FROM app.tenant WHERE slug = 'vpyd') AND b.business_date = ANY(ARRAY[current_date, current_date - 1, current_date - 7]) GROUP BY 1, 2",
        ),
        (
            "ledger days 92",
            "EXPLAIN (ANALYZE, BUFFERS) SELECT b.business_date, sum(b.cash) FROM app.revenue_book b WHERE b.tenant_id = (SELECT id FROM app.tenant WHERE slug = 'vpyd') AND b.business_date BETWEEN current_date - 91 AND current_date GROUP BY 1",
        ),
        (
            "3y visit counts",
            "EXPLAIN (ANALYZE, BUFFERS) SELECT date_trunc('month', v.business_date::timestamp)::date, count(*) FILTER (WHERE v.status = 'completed'), count(DISTINCT v.customer_id) FILTER (WHERE v.status = 'completed') FROM app.visit v WHERE v.tenant_id = (SELECT id FROM app.tenant WHERE slug = 'vpyd') AND v.business_date BETWEEN current_date - 1095 AND current_date GROUP BY 1",
        ),
        (
            "3y lines",
            "EXPLAIN (ANALYZE, BUFFERS) SELECT date_trunc('month', v.business_date::timestamp)::date, sum(l.qty) FROM app.visit v JOIN app.visit_line l ON l.tenant_id = v.tenant_id AND l.visit_id = v.id WHERE v.tenant_id = (SELECT id FROM app.tenant WHERE slug = 'vpyd') AND v.business_date BETWEEN current_date - 1095 AND current_date AND v.status = 'completed' GROUP BY 1",
        ),
        (
            "3y money",
            "EXPLAIN (ANALYZE, BUFFERS) SELECT date_trunc('month', p.business_date::timestamp)::date, p.kind, p.method, CASE WHEN pk.sale_id IS NULL THEN 'one_off' ELSE 'package' END, sum(p.amount) FROM app.payment p LEFT JOIN (SELECT DISTINCT sl.sale_id FROM app.sale_line sl WHERE sl.tenant_id = (SELECT id FROM app.tenant WHERE slug = 'vpyd') AND sl.line_type = 'package') pk ON pk.sale_id = p.sale_id WHERE p.tenant_id = (SELECT id FROM app.tenant WHERE slug = 'vpyd') AND p.business_date BETWEEN current_date - 1095 AND current_date GROUP BY 1, 2, 3, 4",
        ),
        (
            "3y distinct customers",
            "EXPLAIN (ANALYZE, BUFFERS) SELECT count(DISTINCT customer_id) FROM app.visit WHERE tenant_id = (SELECT id FROM app.tenant WHERE slug = 'vpyd') AND business_date BETWEEN current_date - 1095 AND current_date AND status = 'completed'",
        ),
        (
            "3y services/staff/customers",
            "EXPLAIN (ANALYZE, BUFFERS) SELECT GROUPING(l.service_id, l.staff_id), l.service_id, min(s.name), min(sc.name), l.staff_id, min(st.full_name), count(DISTINCT v.customer_id), count(DISTINCT v.id), sum(l.qty) FILTER (WHERE l.source = 'package'), sum(l.qty) FILTER (WHERE l.source = 'one_off'), sum(l.line_value) FILTER (WHERE l.source = 'one_off') FROM app.visit v JOIN app.visit_line l ON l.tenant_id = v.tenant_id AND l.visit_id = v.id JOIN app.service s ON s.tenant_id = l.tenant_id AND s.id = l.service_id JOIN app.service_category sc ON sc.tenant_id = s.tenant_id AND sc.id = s.category_id JOIN app.staff st ON st.tenant_id = l.tenant_id AND st.id = l.staff_id WHERE v.tenant_id = (SELECT id FROM app.tenant WHERE slug = 'vpyd') AND v.business_date BETWEEN current_date - 1095 AND current_date AND v.status = 'completed' GROUP BY GROUPING SETS ((l.service_id), (l.staff_id), ())",
        ),
        (
            "3y top packages",
            "EXPLAIN (ANALYZE, BUFFERS) SELECT cp.plan_id, min(cp.name_snapshot), count(*), sum(cp.sold_price) FROM app.sale s JOIN app.customer_package cp ON cp.tenant_id = s.tenant_id AND cp.sale_id = s.id WHERE s.tenant_id = (SELECT id FROM app.tenant WHERE slug = 'vpyd') AND s.business_date BETWEEN current_date - 1095 AND current_date AND s.status = 'open' GROUP BY cp.plan_id",
        ),
        (
            "3y packages sold",
            "EXPLAIN (ANALYZE, BUFFERS) SELECT date_trunc('month', s.business_date::timestamp)::date, count(*) FROM app.sale s JOIN app.customer_package cp ON cp.tenant_id = s.tenant_id AND cp.sale_id = s.id WHERE s.tenant_id = (SELECT id FROM app.tenant WHERE slug = 'vpyd') AND s.business_date BETWEEN current_date - 1095 AND current_date AND s.status = 'open' GROUP BY 1",
        ),
        (
            "visits of a day",
            "EXPLAIN (ANALYZE, BUFFERS) SELECT count(*) FROM app.visit v WHERE v.tenant_id = (SELECT id FROM app.tenant WHERE slug = 'vpyd') AND v.business_date = ANY(ARRAY[current_date])",
        ),
    ] {
        let plan: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
            .fetch_all(&mut *tx)
            .await?;
        println!("--- {label}\n{}", plan.join("\n"));
    }
    tx.rollback().await?;
    // Which indexes the reads above actually used (0013 adds the *_tenant_day_* ones).
    let used: Vec<(String, i64)> = sqlx::query_as(
        "SELECT indexrelname::text, idx_scan FROM pg_stat_user_indexes
         WHERE schemaname = 'app' AND relname IN ('payment', 'sale', 'visit', 'refund', 'visit_line', 'customer_package')
         ORDER BY relname, indexrelname",
    )
    .fetch_all(&h.owner)
    .await?;
    for (index, scans) in used {
        println!("index {index}: {scans} scans");
    }
    assert!(over.is_empty(), "over budget: {over:?}");
    Ok(())
}

/// Budgets of "ai ghi, ai sửa" (ADR-0022, architecture/record-history §6) with a real-sized audit
/// trail (~60.000 rows a year × 3): one record's history p95 ≤ 100 ms (a three-version "Sửa lượt"
/// chain, the busiest package, the busiest customer); a staff-book cell ≤ 50 ms (wave 2, 0052's
/// day index); the day's visits and a 92-day revenue book with `edited` within +10 ms of the same
/// lists without it, and 14 days of bookings with `marks=true` within +10 ms of the grids;
/// "Nhật ký thao tác" after the `rows_by_ids` refactor within the management views' 300 ms.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
#[ignore = "perf bench: seeds three years of data and audit; run with --ignored"]
async fn record_history_reads_within_budget(pool: PgPool) -> TestResult {
    const HISTORY_BUDGET_MS: u128 = 100;
    const STAFF_DAY_BUDGET_MS: u128 = 50;
    const EDITED_BUDGET_MS: u128 = 10;
    const EDITED_ROUNDS: usize = 60;
    let (h, owner) = books(pool).await?;
    let tenant: uuid::Uuid = sqlx::query_scalar("SELECT id FROM app.tenant WHERE slug = 'vpyd'")
        .fetch_one(&h.owner)
        .await?;
    let started = Instant::now();
    sqlx::raw_sql(sqlx::AssertSqlSafe(
        AUDIT_SEED.replace("$DAYS", &DAYS.to_string()),
    ))
    .execute(&h.owner)
    .await?;
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM app.audit_log")
        .fetch_one(&h.owner)
        .await?;
    println!("seeded {rows} audit rows in {:?}", started.elapsed());
    common::enable_feature(&h, tenant, "activity_log").await?;

    // A real three-version chain through the API (today), on the busiest customer.
    let customer: String = sqlx::query_scalar(
        "SELECT customer_id::text FROM app.visit GROUP BY customer_id ORDER BY count(*) DESC LIMIT 1",
    )
    .fetch_one(&h.owner)
    .await?;
    let package: String = sqlx::query_scalar(
        "SELECT customer_package_id::text FROM app.visit_line WHERE customer_package_id IS NOT NULL
         GROUP BY 1 ORDER BY count(*) DESC LIMIT 1",
    )
    .fetch_one(&h.owner)
    .await?;
    let (service, staff): (String, String) = sqlx::query_as(
        "SELECT (SELECT id::text FROM app.service LIMIT 1), (SELECT id::text FROM app.staff LIMIT 1)",
    )
    .fetch_one(&h.owner)
    .await?;
    let line = json!({"service_id": service, "source": "one_off", "staff_id": staff});
    let first = call_with_headers(
        &h.app,
        "POST",
        "/visits",
        Some(&owner),
        Some(json!({"customer_id": customer, "lines": [line],
                    "payments": [{"method": "cash", "amount": 150_000}]})),
        &[("idempotency-key", &uuid::Uuid::now_v7().to_string())],
    )
    .await;
    assert_eq!(first.status, StatusCode::CREATED, "{}", first.body);
    let mut visit = first.body["id"].as_str().unwrap_or_default().to_owned();
    for _ in 0..2 {
        let corrected = call_with_headers(
            &h.app,
            "POST",
            &format!("/visits/{visit}/correct"),
            Some(&owner),
            Some(
                json!({"reason": "ghi nhầm", "customer_id": customer, "lines": [line],
                        "payments": [{"method": "cash", "amount": 150_000}]}),
            ),
            &[
                ("idempotency-key", &uuid::Uuid::now_v7().to_string()),
                ("If-Match", "\"1\""),
            ],
        )
        .await;
        assert_eq!(corrected.status, StatusCode::CREATED, "{}", corrected.body);
        visit = corrected.body["visit"]["id"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
    }

    let mut n = 0_u32;
    let mut over = Vec::new();
    for path in [
        format!("/record-history/visit/{visit}"),
        format!("/record-history/customer-package/{package}"),
        format!("/record-history/customer/{customer}"),
    ] {
        let (p50, p95) = timed(&h, &owner, &path, &mut n).await;
        println!("p50 {p50:>4} ms  p95 {p95:>4} ms  {path}");
        if p95 > HISTORY_BUDGET_MS {
            over.push(format!(
                "{path}: p95 {p95} ms (budget {HISTORY_BUDGET_MS} ms)"
            ));
        }
    }
    let audit = "/audit-log?limit=50";
    let (p50, p95) = timed(&h, &owner, audit, &mut n).await;
    println!("p50 {p50:>4} ms  p95 {p95:>4} ms  {audit}");
    if p95 >= BUDGET_MS {
        over.push(format!("{audit}: p95 {p95} ms (budget {BUDGET_MS} ms)"));
    }
    // Wave 2: one staff-book cell written twice (the person's three years of book rows around
    // it) through 0052's `audit_log_staff_day_idx`, ≤ 50 ms.
    let cell_day: String = sqlx::query_scalar(
        "SELECT after ->> 'date' FROM app.audit_log
         WHERE entity_id = $1::uuid AND action = 'attendance.record' AND before IS NOT NULL
         ORDER BY at DESC LIMIT 1",
    )
    .bind(&staff)
    .fetch_one(&h.owner)
    .await?;
    let cell = format!("/record-history/staff-day/{staff}/{cell_day}?book=attendance");
    let (p50, p95) = timed(&h, &owner, &cell, &mut n).await;
    println!("p50 {p50:>4} ms  p95 {p95:>4} ms  {cell}");
    if p95 > STAFF_DAY_BUDGET_MS {
        over.push(format!(
            "{cell}: p95 {p95} ms (budget {STAFF_DAY_BUDGET_MS} ms)"
        ));
    }
    // `/appointments` over 14 days: the list view's `marks=true` within +10 ms of the grids.
    let (from, to): (String, String) =
        sqlx::query_as("SELECT current_date::text, (current_date + 13)::text")
            .fetch_one(&h.owner)
            .await?;
    let grid = format!("/appointments?from={from}&to={to}");
    let (mut with, mut without) = (Vec::new(), Vec::new());
    for round in 0..(EDITED_ROUNDS * 2) {
        let marks = round % 2 == 0;
        let path = if marks {
            format!("{grid}&marks=true")
        } else {
            grid.clone()
        };
        n += 1;
        let ip = format!("10.6.{}.{}", n / 250, n % 250);
        let t = Instant::now();
        let reply = call_with_headers(
            &h.app,
            "GET",
            &path,
            Some(&owner),
            None,
            &[("X-Forwarded-For", &ip)],
        )
        .await;
        let ms = t.elapsed().as_millis();
        assert_eq!(reply.status, StatusCode::OK, "{path}: {}", reply.body);
        assert_eq!(
            reply.body.to_string().contains("\"edited\""),
            marks,
            "{path}"
        );
        if marks {
            with.push(ms)
        } else {
            without.push(ms)
        }
    }
    let (with_p50, with_p95) = (p(&mut with, 0.5), p(&mut with, 0.95));
    let (p50, p95) = (p(&mut without, 0.5), p(&mut without, 0.95));
    println!("marks: p50 {with_p50:>4} / {p50:>4} ms  p95 {with_p95:>4} / {p95:>4} ms  {grid}");
    if with_p95 > p95 + EDITED_BUDGET_MS {
        over.push(format!("{grid}: marks add {} ms at p95", with_p95 - p95));
    }

    // `edited` costs: the same lists with the right (the owner's default) and without it, in
    // alternating rounds so a busy machine slows both sides alike.
    let lists = [
        "/visits".to_owned(),
        "/reports/ledger?from=2026-08-06&to=2026-11-05&limit=100".to_owned(),
        format!("/customers/{customer}/history?limit=50"),
    ];
    for path in &lists {
        let (mut with, mut without) = (Vec::new(), Vec::new());
        for round in 0..(EDITED_ROUNDS * 2) {
            let shown = round % 2 == 0;
            let sql = if shown {
                "INSERT INTO app.role_permission (tenant_id, role_id, permission)
                 SELECT r.tenant_id, r.id, 'record.history.read' FROM app.role r
                 WHERE r.tenant_id = $1 AND r.key = 'owner' ON CONFLICT DO NOTHING"
            } else {
                "DELETE FROM app.role_permission
                 WHERE tenant_id = $1 AND permission = 'record.history.read'"
            };
            sqlx::query(sql).bind(tenant).execute(&h.owner).await?;
            n += 1;
            let ip = format!("10.7.{}.{}", n / 250, n % 250);
            let t = Instant::now();
            let reply = call_with_headers(
                &h.app,
                "GET",
                path,
                Some(&owner),
                None,
                &[("X-Forwarded-For", &ip)],
            )
            .await;
            let ms = t.elapsed().as_millis();
            assert_eq!(reply.status, StatusCode::OK, "{path}: {}", reply.body);
            assert_eq!(
                reply.body.to_string().contains("\"edited\""),
                shown,
                "{path}"
            );
            if shown {
                with.push(ms)
            } else {
                without.push(ms)
            }
        }
        let (with_p50, with_p95) = (p(&mut with, 0.5), p(&mut with, 0.95));
        let (p50, p95) = (p(&mut without, 0.5), p(&mut without, 0.95));
        println!(
            "edited: p50 {with_p50:>4} / {p50:>4} ms  p95 {with_p95:>4} / {p95:>4} ms  {path}"
        );
        if with_p95 > p95 + EDITED_BUDGET_MS {
            over.push(format!("{path}: edited adds {} ms at p95", with_p95 - p95));
        }
    }

    let mut tx = h.owner.begin().await?;
    for (label, sql) in [
        (
            "edited probe (partial index)",
            "EXPLAIN (ANALYZE, BUFFERS) SELECT v.id FROM app.visit v WHERE v.tenant_id = (SELECT id FROM app.tenant WHERE slug = 'vpyd') AND v.business_date = current_date - 3 AND EXISTS (SELECT 1 FROM app.audit_log a WHERE a.tenant_id = v.tenant_id AND a.entity_type = 'visit' AND a.entity_id = v.id AND a.action IN ('visit.update', 'appointment.update') AND a.action = 'visit.update')",
        ),
        (
            "package members (entity index)",
            "EXPLAIN (ANALYZE, BUFFERS) SELECT a.id FROM app.audit_log a WHERE a.tenant_id = (SELECT id FROM app.tenant WHERE slug = 'vpyd') AND a.entity_type = 'customer_package' AND a.entity_id = (SELECT customer_package_id FROM app.visit_line WHERE customer_package_id IS NOT NULL GROUP BY 1 ORDER BY count(*) DESC LIMIT 1)",
        ),
        (
            "staff-day cell (audit_log_staff_day_idx)",
            "EXPLAIN (ANALYZE, BUFFERS) SELECT a.id FROM app.audit_log a WHERE a.tenant_id = (SELECT id FROM app.tenant WHERE slug = 'vpyd') AND a.entity_id = (SELECT id FROM app.staff LIMIT 1) AND a.action IN ('attendance.record', 'evaluation.daily_review') AND coalesce(a.after ->> 'date', a.before ->> 'date') = (current_date - 3)::text AND a.action = 'attendance.record' AND a.entity_type = 'staff'",
        ),
        (
            "booking edited probe (partial index)",
            "EXPLAIN (ANALYZE, BUFFERS) SELECT ap.id FROM app.appointment ap WHERE ap.tenant_id = (SELECT id FROM app.tenant WHERE slug = 'vpyd') AND ap.business_date BETWEEN current_date AND current_date + 13 AND EXISTS (SELECT 1 FROM app.audit_log l WHERE l.tenant_id = ap.tenant_id AND l.entity_type = 'appointment' AND l.entity_id = ap.id AND l.action IN ('visit.update', 'appointment.update') AND l.action = 'appointment.update' AND (l.before -> 'starts_at' IS DISTINCT FROM l.after -> 'starts_at' OR l.before -> 'staff_id' IS DISTINCT FROM l.after -> 'staff_id'))",
        ),
    ] {
        let plan: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
            .fetch_all(&mut *tx)
            .await?;
        println!("--- {label}\n{}", plan.join("\n"));
    }
    tx.rollback().await?;
    assert!(over.is_empty(), "over budget: {over:?}");
    Ok(())
}
