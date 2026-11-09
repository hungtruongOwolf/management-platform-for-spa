//! Management views (spec management-views US1–US5): money correctness with discounts, voids
//! and refunds, the 00:00 ICT day boundary, who may see money, tenant isolation of aggregates,
//! pagination, and reconciliation with the raw rows after `seed-demo`.

mod common;

use axum::http::StatusCode;
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::Asia::Ho_Chi_Minh;
use common::{
    Harness, OWNER_PHONE, Reply, TestResult, add_member, call, call_with_headers, harness,
    post_customer, session_for,
};
use identity::SystemRole;
use identity::provisioning::create_tenant;
use kernel::idempotency::hash_request;
use kernel::testing::app_pool;
use kernel::{Actor, TenantId, UserId};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::collections::BTreeSet;
use uuid::Uuid;

const MANAGER_PHONE: &str = "0912000111";
const TECH_PHONE: &str = "0912000222";
const OTHER_OWNER_PHONE: &str = "0913000333";
const PRICE: i64 = 150_000;
const PLAN_PRICE: i64 = 1_200_000;

type Setup<T> = Result<T, Box<dyn std::error::Error>>;

struct Spa {
    h: Harness,
    app: PgPool,
    owner: String,
    manager: String,
    tech: String,
    /// The owner acting directly on the use cases (back-dated records).
    actor: Actor,
    staff: Uuid,
    staff_b: Uuid,
    service: Uuid,
    plan: Uuid,
}

fn today() -> NaiveDate {
    Utc::now().with_timezone(&Ho_Chi_Minh).date_naive()
}

/// `HH:MM` Vietnam time on `day`, as an instant.
fn ict(day: NaiveDate, hour: u32, minute: u32) -> DateTime<Utc> {
    Ho_Chi_Minh
        .from_local_datetime(&day.and_hms_opt(hour, minute, 0).unwrap_or_default())
        .single()
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or_default()
}

fn id(reply: &Reply) -> Uuid {
    reply.body["id"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or_default()
}

fn key() -> String {
    Uuid::now_v7().to_string()
}

async fn spa(pool: PgPool) -> Setup<Spa> {
    let h = harness(pool).await?;
    add_member(
        &h.owner,
        "vpyd",
        MANAGER_PHONE,
        "Quản lý",
        SystemRole::Manager,
    )
    .await?;
    add_member(&h.owner, "vpyd", TECH_PHONE, "KTV", SystemRole::Technician).await?;
    let owner = session_for(&h, OWNER_PHONE).await;
    let manager = session_for(&h, MANAGER_PHONE).await;
    let tech = session_for(&h, TECH_PHONE).await;
    let staff = org::provisioning::add_staff(&h.owner, "vpyd", "KTV Lan", "Kỹ thuật viên").await?;
    let staff_b =
        org::provisioning::add_staff(&h.owner, "vpyd", "KTV Mai", "Kỹ thuật viên").await?;
    let category = call(
        &h.app,
        "POST",
        "/service-categories",
        Some(&owner),
        Some(json!({"name": "Dưỡng sinh"})),
    )
    .await;
    let service = call(
        &h.app,
        "POST",
        "/services",
        Some(&owner),
        Some(json!({
            "category_id": id(&category), "name": "Gội đầu dưỡng sinh", "description": null,
            "duration_min": 60, "recommended": false, "active": true, "prices": {"all": PRICE}
        })),
    )
    .await;
    assert_eq!(service.status, StatusCode::CREATED, "{}", service.body);
    let plan = call(&h.app, "POST", "/package-plans", Some(&owner), Some(json!({
        "name": "Gói 10 buổi", "price": PLAN_PRICE, "validity_months": null, "gift_note": null,
        "active": true, "items": [{"service_id": id(&service), "sessions": 10, "bonus_sessions": 0}]
    }))).await;
    assert_eq!(plan.status, StatusCode::CREATED, "{}", plan.body);
    let (tenant, user): (Uuid, Uuid) = sqlx::query_as(
        "SELECT m.tenant_id, m.user_id FROM app.membership m JOIN app.user_account u ON u.id = m.user_id
         WHERE u.phone_e164 = '+84905123456'",
    )
    .fetch_one(&h.owner)
    .await?;
    let app = app_pool(&h.owner).await?;
    let (tenant, user) = (TenantId::from_uuid(tenant), UserId::from_uuid(user));
    let permissions = identity::service::tenant_permissions(&app, user, tenant)
        .await?
        .ok_or("owner has no membership")?;
    Ok(Spa {
        actor: Actor::new(user, tenant, permissions, None),
        app,
        owner,
        manager,
        tech,
        staff,
        staff_b,
        service: id(&service),
        plan: id(&plan),
        h,
    })
}

impl Spa {
    async fn get(&self, session: &str, path: &str) -> Reply {
        call(&self.h.app, "GET", path, Some(session), None).await
    }

    async fn post(&self, session: &str, path: &str, body: Value) -> Reply {
        let k = key();
        call_with_headers(
            &self.h.app,
            "POST",
            path,
            Some(session),
            Some(body),
            &[("Idempotency-Key", &k)],
        )
        .await
    }

    async fn customer(&self, name: &str, phone: &str) -> Uuid {
        let reply = post_customer(
            &self.h.app,
            Some(&self.owner),
            Some(json!({"full_name": name, "phone": phone, "gender": "female"})),
        )
        .await;
        assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
        id(&reply)
    }

    fn sale_body(&self, customer: Uuid, discount: i64, method: &str) -> Value {
        let mut body = json!({
            "customer_id": customer, "closer_staff_id": self.staff,
            "lines": [{"package_plan_id": self.plan}],
            "payments": [{"method": method, "amount": PLAN_PRICE - discount}]
        });
        if discount > 0 {
            body["discount"] = json!({"amount": discount, "reason": "Khách quen"});
        }
        body
    }

    /// Sells the plan now (HTTP) or at `at` (use case with an explicit clock). Returns the sale.
    async fn sell(
        &self,
        customer: Uuid,
        discount: i64,
        method: &str,
        at: Option<DateTime<Utc>>,
    ) -> Setup<Value> {
        let body = self.sale_body(customer, discount, method);
        match at {
            None => {
                let reply = self.post(&self.owner, "/sales", body).await;
                assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
                Ok(reply.body)
            }
            Some(at) => {
                let k = Uuid::now_v7();
                let (_, sale) = sales::service::create_sale_as_of(
                    &self.app,
                    &self.h.cipher,
                    &self.actor,
                    k,
                    hash_request(k.as_bytes()),
                    &serde_json::from_value(body)?,
                    at,
                )
                .await
                .map_err(|e| e.code())?;
                Ok(sale)
            }
        }
    }

    fn one_off(&self, staff: Uuid) -> Value {
        json!({"service_id": self.service, "source": "one_off", "staff_id": staff})
    }

    fn session_line(&self, staff: Uuid) -> Value {
        json!({"service_id": self.service, "source": "package", "staff_id": staff})
    }

    async fn visit(
        &self,
        customer: Uuid,
        lines: Vec<Value>,
        cash: i64,
        at: Option<DateTime<Utc>>,
    ) -> Setup<Value> {
        let payments = if cash > 0 {
            json!([{"method": "cash", "amount": cash}])
        } else {
            json!([])
        };
        let body = json!({"customer_id": customer, "lines": lines, "payments": payments});
        match at {
            None => {
                let reply = self.post(&self.owner, "/visits", body).await;
                assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
                Ok(reply.body)
            }
            Some(at) => {
                let k = Uuid::now_v7();
                let (_, visit) = operations::service::record_as_of(
                    &self.app,
                    &self.h.cipher,
                    &self.actor,
                    k,
                    hash_request(k.as_bytes()),
                    &serde_json::from_value(body)?,
                    at,
                )
                .await
                .map_err(|e| e.code())?;
                Ok(visit)
            }
        }
    }

    async fn void(&self, path: String) {
        let reply = self
            .post(&self.owner, &path, json!({"reason": "Ghi nhầm"}))
            .await;
        assert_eq!(reply.status, StatusCode::OK, "{path}: {}", reply.body);
    }

    async fn cancel(&self, package: &str, refund: i64, method: &str) {
        let mut body = json!({"reason": "Khách chuyển nhà", "refund_amount": refund});
        if refund > 0 {
            body["refund_method"] = json!(method);
        }
        let reply = self
            .post(
                &self.owner,
                &format!("/customer-packages/{package}/cancel"),
                body,
            )
            .await;
        assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    }

    /// Imports a paper-book package (owner, transitional HTTP route) and returns its id.
    async fn import(
        &self,
        customer: Uuid,
        starts_on: NaiveDate,
        expires_on: Option<NaiveDate>,
        remaining: u32,
    ) -> String {
        let reply = self
            .post(
                &self.owner,
                "/opening-balances",
                json!({"customer_id": customer, "packages": [{
                    "name": "Gói sổ giấy", "sold_price": 1_000_000, "starts_on": starts_on,
                    "expires_on": expires_on,
                    "items": [{"service_id": self.service, "sessions_purchased": 10, "sessions_remaining": remaining}]
                }]}),
            )
            .await;
        assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
        reply.body["customer_packages"][0]["id"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    }
}

fn split(cash: i64, bank_transfer: i64) -> Value {
    json!({"cash": cash, "bank_transfer": bank_transfer, "total": cash + bank_transfer})
}

fn kinds(package: i64, one_off: i64) -> Value {
    json!({"package": package, "one_off": one_off})
}

// ------------------------------------------------------------------------------------- US1

/// AC1.1–1.5, 1.7–1.9: counts and money of one day, with a discount, a same-day visit void, a
/// sale from yesterday voided today and a refund.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn the_daily_dashboard_nets_discounts_voids_and_refunds(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let (day, yesterday) = (today(), today() - Duration::days(1));
    let ha = s.customer("Nguyễn Thị Hà", "0905111001").await;
    let lan = s.customer("Trần Thị Lan", "0905111002").await;

    // Yesterday: one package sale in cash (voided today) and one one-off visit.
    let old_sale = s.sell(lan, 0, "cash", Some(ict(yesterday, 10, 0))).await?;
    s.visit(
        lan,
        vec![s.one_off(s.staff)],
        PRICE,
        Some(ict(yesterday, 11, 0)),
    )
    .await?;

    // Today: Hà buys with a 100.000đ discount by transfer and uses a session twice (two
    // technicians); Lan has a one-off visit (cash) and a second one that is voided.
    let sale = s.sell(ha, 100_000, "bank_transfer", None).await?;
    let package = sale["customer_packages"][0]["id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    s.visit(ha, vec![s.session_line(s.staff)], 0, None).await?;
    s.visit(ha, vec![s.session_line(s.staff_b)], 0, None)
        .await?;
    s.visit(lan, vec![s.one_off(s.staff)], PRICE, None).await?;
    let wrong = s
        .visit(lan, vec![s.one_off(s.staff_b)], PRICE, None)
        .await?;
    s.void(format!(
        "/visits/{}/void",
        wrong["id"].as_str().unwrap_or_default()
    ))
    .await;
    s.void(format!(
        "/sales/{}/void",
        old_sale["id"].as_str().unwrap_or_default()
    ))
    .await;
    // Hà's package is cancelled with a 300.000đ cash refund.
    s.cancel(&package, 300_000, "cash").await;

    let report = s.get(&s.owner, &format!("/reports/daily?date={day}")).await;
    assert_eq!(report.status, StatusCode::OK, "{}", report.body);
    let b = &report.body;
    assert_eq!(
        b["counts"],
        json!({"customers": 2, "visits": 3, "voided_visits": 1, "corrected_visits": 0, "sessions_used": 2,
               "one_off_services": 1, "packages_sold": 1}),
        "the cancelled package still was sold today (its sale is not voided)"
    );
    let r = &b["revenue"];
    // Collected: 1.100.000 transfer (package) + 150.000 + 150.000 cash (one-off, one voided).
    assert_eq!(r["collected"], split(2 * PRICE, 1_100_000));
    assert_eq!(r["collected_by_kind"], kinds(1_100_000, 2 * PRICE));
    // Voids: the voided visit (one-off, cash) and yesterday's package sale (cash).
    assert_eq!(r["voids"], split(PRICE + PLAN_PRICE, 0));
    assert_eq!(r["voids_by_kind"], kinds(PLAN_PRICE, PRICE));
    assert_eq!(r["refunds"], split(300_000, 0));
    assert_eq!(
        r["net"],
        split(2 * PRICE - PRICE - PLAN_PRICE - 300_000, 1_100_000)
    );
    assert_eq!(
        r["net_by_kind"],
        kinds(1_100_000 - PLAN_PRICE - 300_000, PRICE)
    );

    // Two technicians: Lan did 2 visits (1 session + 1 one-off), Mai 1 session (her one-off
    // visit was voided).
    assert_eq!(
        b["technicians"],
        json!([
            {"staff_id": s.staff, "name": "KTV Lan", "visits": 2, "sessions_used": 1, "one_off_services": 1},
            {"staff_id": s.staff_b, "name": "KTV Mai", "visits": 1, "sessions_used": 1, "one_off_services": 0},
        ])
    );

    // Yesterday keeps its own money: the void did not rewrite it (AC1.4), but its package no
    // longer counts as sold.
    let y = &b["compare"]["yesterday"];
    assert_eq!(
        (
            y["date"].clone(),
            y["visits"].clone(),
            y["packages_sold"].clone(),
            y["net_revenue"].clone()
        ),
        (
            json!(yesterday),
            json!(1),
            json!(0),
            json!(PLAN_PRICE + PRICE)
        )
    );
    assert_eq!(b["compare"]["same_weekday_last_week"]["visits"], 0);

    // No date = today (AC1.9); a future day is all zeros, not an error.
    let default = s.get(&s.owner, "/reports/daily").await;
    assert_eq!(default.body["date"], json!(day));
    let future = s
        .get(
            &s.owner,
            &format!("/reports/daily?date={}", day + Duration::days(30)),
        )
        .await;
    assert_eq!(future.status, StatusCode::OK);
    assert_eq!(future.body["counts"]["visits"], 0);
    assert_eq!(future.body["revenue"]["net"]["total"], 0);
    Ok(())
}

/// AC1.6: 23:59 and 00:01 Vietnam time fall on different business days (16:59 / 17:01 UTC).
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn the_business_day_turns_at_midnight_in_vietnam(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let ha = s.customer("Nguyễn Thị Hà", "0905111001").await;
    let day = today() - Duration::days(10);
    let late = ict(day, 23, 59);
    let early = ict(day + Duration::days(1), 0, 1);
    assert_eq!(late.format("%H:%M").to_string(), "16:59", "UTC");
    s.visit(ha, vec![s.one_off(s.staff)], PRICE, Some(late))
        .await?;
    s.visit(ha, vec![s.one_off(s.staff)], PRICE, Some(early))
        .await?;

    for d in [day, day + Duration::days(1)] {
        let r = s.get(&s.owner, &format!("/reports/daily?date={d}")).await;
        assert_eq!(r.body["counts"]["visits"], 1, "{d}: {}", r.body);
        assert_eq!(r.body["revenue"]["net"]["cash"], PRICE, "{d}");
    }
    let range = s
        .get(
            &s.owner,
            &format!("/reports/range?from={day}&to={}", day + Duration::days(1)),
        )
        .await;
    let visits: Vec<i64> = range.body["series"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|b| b["counts"]["visits"].as_i64())
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(visits, [1, 1]);
    Ok(())
}

/// F17: the right is checked before the query string is read. A technician sending a malformed
/// query gets `403` (never learns the parameters); the owner hears `400 query.invalid` as
/// problem+json. Red before the fix: axum answered a plain-text 400 to both.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn report_routes_check_the_right_before_the_query(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let customer = Uuid::now_v7();
    for path in [
        "/reports/daily?date=hom-nay".to_owned(),
        "/reports/range?from=hom-nay".to_owned(),
        "/reports/ledger?limit=abc".to_owned(),
        "/customers/inactive?days=abc".to_owned(),
        format!("/customers/{customer}/history?limit=abc"),
        "/packages?limit=abc".to_owned(),
    ] {
        let tech = s.get(&s.tech, &path).await;
        assert_eq!(tech.status, StatusCode::FORBIDDEN, "{path}: {}", tech.body);
        let owner = s.get(&s.owner, &path).await;
        assert_eq!(
            owner.status,
            StatusCode::BAD_REQUEST,
            "{path}: {}",
            owner.body
        );
        assert_eq!(
            owner.body["code"], "query.invalid",
            "{path}: {}",
            owner.body
        );
    }
    Ok(())
}

/// AC1.10, AC2.5, AC3.4 + tenant isolation of aggregates (they have no {id} for the matrix).
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn money_is_only_sent_to_holders_of_report_revenue_read(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let ha = s.customer("Nguyễn Thị Hà", "0905111001").await;
    s.sell(ha, 0, "cash", None).await?;
    s.visit(ha, vec![s.one_off(s.staff)], PRICE, None).await?;

    // Default: the manager (cashier) sees money (spec Q1).
    let as_manager = s.get(&s.manager, "/reports/daily").await;
    assert_eq!(
        as_manager.body["revenue"]["net"]["total"],
        PLAN_PRICE + PRICE
    );

    // The owner turns it off for managers: counts stay, every money field disappears.
    sqlx::query(
        "DELETE FROM app.role_permission rp USING app.role r
         WHERE r.id = rp.role_id AND r.key = 'manager' AND rp.permission = 'report.revenue.read'",
    )
    .execute(&s.h.owner)
    .await?;
    let manager = session_for(&s.h, MANAGER_PHONE).await;
    let daily = s.get(&manager, "/reports/daily").await;
    assert_eq!(daily.status, StatusCode::OK);
    assert_eq!(daily.body["counts"]["visits"], 1);
    assert!(daily.body.get("revenue").is_none(), "{}", daily.body);
    for side in ["yesterday", "same_weekday_last_week"] {
        assert!(daily.body["compare"][side].get("net_revenue").is_none());
    }
    let range = s.get(&manager, "/reports/range").await;
    assert_eq!(range.status, StatusCode::OK);
    assert!(range.body.get("revenue").is_none());
    let text = range.body.to_string();
    for money_key in [
        "\"revenue\"",
        "one_off_revenue",
        "\"collected\"",
        "net_revenue",
    ] {
        assert!(!text.contains(money_key), "{money_key} leaked: {text}");
    }
    assert_eq!(range.body["top_packages"][0]["sold"], 1);
    let ledger = s.get(&manager, "/reports/ledger").await;
    assert_eq!(
        (ledger.status, ledger.body["code"].clone()),
        (StatusCode::FORBIDDEN, json!("auth.forbidden"))
    );

    // Technicians see no reports at all.
    for path in [
        "/reports/daily",
        "/reports/range",
        "/reports/ledger",
        "/packages",
        "/customers/inactive",
    ] {
        assert_eq!(
            s.get(&s.tech, path).await.status,
            StatusCode::FORBIDDEN,
            "{path}"
        );
    }
    // Anonymous: 401.
    let anon = call(&s.h.app, "GET", "/reports/daily", None, None).await;
    assert_eq!(anon.status, StatusCode::UNAUTHORIZED);

    // Another spa's owner sees only their own (empty) books.
    create_tenant(&s.h.owner, "other", "Spa khác", "Huế").await?;
    add_member(
        &s.h.owner,
        "other",
        OTHER_OWNER_PHONE,
        "Chủ khác",
        SystemRole::Owner,
    )
    .await?;
    let intruder = session_for(&s.h, OTHER_OWNER_PHONE).await;
    let theirs = s.get(&intruder, "/reports/daily").await;
    assert_eq!(theirs.body["counts"]["visits"], 0);
    assert_eq!(theirs.body["revenue"]["collected"]["total"], 0);
    let book = s.get(&intruder, "/reports/ledger").await;
    assert_eq!(book.body["rows"], json!([]));
    let packages = s.get(&intruder, "/packages").await;
    assert_eq!(packages.body["items"], json!([]));
    assert_eq!(
        s.get(&intruder, &format!("/customers/{ha}/history"))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    Ok(())
}

// ------------------------------------------------------------------------------------- US2

/// AC2.1–2.5: continuous series by day / week / month that add up to the totals and to the
/// daily dashboard.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn the_range_report_is_continuous_and_reconciles(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let ha = s.customer("Nguyễn Thị Hà", "0905111001").await;
    let lan = s.customer("Trần Thị Lan", "0905111002").await;
    // 2026-10-04 is a Sunday, 2026-10-05 a Monday: one ISO week boundary in between.
    let d = |s: &str| s.parse::<NaiveDate>().unwrap_or_default();
    s.sell(ha, 0, "bank_transfer", Some(ict(d("2026-10-04"), 9, 0)))
        .await?;
    s.visit(
        ha,
        vec![s.session_line(s.staff)],
        0,
        Some(ict(d("2026-10-04"), 10, 0)),
    )
    .await?;
    s.visit(
        lan,
        vec![s.one_off(s.staff)],
        PRICE,
        Some(ict(d("2026-10-05"), 10, 0)),
    )
    .await?;
    s.visit(
        ha,
        vec![s.one_off(s.staff), s.session_line(s.staff)],
        PRICE,
        Some(ict(d("2026-10-07"), 10, 0)),
    )
    .await?;

    let daily = s
        .get(
            &s.owner,
            "/reports/range?from=2026-10-03&to=2026-10-08&granularity=day",
        )
        .await;
    assert_eq!(daily.status, StatusCode::OK, "{}", daily.body);
    let series = daily.body["series"].as_array().cloned().unwrap_or_default();
    assert_eq!(series.len(), 6, "every day, empty ones included");
    let visits: Vec<i64> = series
        .iter()
        .filter_map(|b| b["counts"]["visits"].as_i64())
        .collect();
    assert_eq!(visits, [0, 1, 1, 0, 1, 0]);
    let t = &daily.body["totals"];
    assert_eq!(
        (
            t["visits"].clone(),
            t["customers"].clone(),
            t["sessions_used"].clone(),
            t["packages_sold"].clone()
        ),
        (json!(3), json!(2), json!(2), json!(1)),
        "customers = distinct over the range, not the sum of days"
    );
    assert_eq!(daily.body["revenue"]["net"], split(2 * PRICE, PLAN_PRICE));
    // Each day equals the daily dashboard of that day.
    for bucket in &series {
        let date = bucket["start"].as_str().unwrap_or_default();
        let day = s
            .get(&s.owner, &format!("/reports/daily?date={date}"))
            .await;
        assert_eq!(bucket["counts"], day.body["counts"], "{date}");
        assert_eq!(bucket["revenue"], day.body["revenue"], "{date}");
    }

    let weekly = s
        .get(
            &s.owner,
            "/reports/range?from=2026-10-03&to=2026-10-08&granularity=week",
        )
        .await;
    let weeks: Vec<(Value, Value, Value)> = weekly.body["series"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|b| {
                    (
                        b["start"].clone(),
                        b["end"].clone(),
                        b["counts"]["visits"].clone(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        weeks,
        [
            (json!("2026-10-03"), json!("2026-10-04"), json!(1)),
            (json!("2026-10-05"), json!("2026-10-08"), json!(2)),
        ],
        "ISO weeks start on Monday, clipped to the range"
    );
    assert_eq!(weekly.body["series"][1]["counts"]["customers"], 2);
    let monthly = s
        .get(
            &s.owner,
            "/reports/range?from=2026-09-20&to=2026-10-08&granularity=month",
        )
        .await;
    assert_eq!(monthly.body["series"].as_array().map(Vec::len), Some(2));
    assert_eq!(
        monthly.body["series"][1]["revenue"]["net"]["total"],
        2 * PRICE + PLAN_PRICE
    );

    assert_eq!(
        daily.body["top_services"][0],
        json!({"service_id": s.service, "name": "Gội đầu dưỡng sinh",
               "category_name": "Dưỡng sinh", "sessions_used": 2,
               "one_off_services": 2, "one_off_revenue": 2 * PRICE})
    );
    assert_eq!(
        daily.body["top_packages"],
        json!([{"plan_id": s.plan, "name": "Gói 10 buổi", "sold": 1, "revenue": PLAN_PRICE}])
    );

    for (query, field) in [
        ("from=2026-10-08&to=2026-10-03", "from"),
        ("from=2025-01-01&to=2026-10-03", "to"),
    ] {
        let bad = s.get(&s.owner, &format!("/reports/range?{query}")).await;
        assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY, "{query}");
        assert_eq!(bad.body["errors"][0]["field"], field);
    }
    Ok(())
}

// ------------------------------------------------------------------------------------- US3

/// AC3.1–3.3: the revenue book has one row per sale, a negative row per void (on the void day)
/// and per refund, day subtotals equal the dashboard, and pages cover every row exactly once.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn the_ledger_mirrors_the_paper_book(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let ha = s.customer("Nguyễn Thị Hà", "0905111001").await;
    let lan = s.customer("Trần Thị Lan", "0905111002").await;
    let (day, yesterday) = (today(), today() - Duration::days(1));
    let old = s.sell(lan, 0, "cash", Some(ict(yesterday, 9, 0))).await?;
    let sale = s.sell(ha, 200_000, "bank_transfer", None).await?;
    s.visit(lan, vec![s.one_off(s.staff)], PRICE, None).await?;
    s.void(format!(
        "/sales/{}/void",
        old["id"].as_str().unwrap_or_default()
    ))
    .await;
    let package = sale["customer_packages"][0]["id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    s.cancel(&package, 500_000, "bank_transfer").await;

    let book = s
        .get(
            &s.owner,
            &format!("/reports/ledger?from={yesterday}&to={day}&limit=100"),
        )
        .await;
    assert_eq!(book.status, StatusCode::OK, "{}", book.body);
    let rows = book.body["rows"].as_array().cloned().unwrap_or_default();
    let shape: Vec<(Value, Value, Value, Value, Value, Value)> = rows
        .iter()
        .map(|r| {
            (
                r["business_date"].clone(),
                r["entry"].clone(),
                r["kind"].clone(),
                r["cash"].clone(),
                r["bank_transfer"].clone(),
                r["voided"].clone(),
            )
        })
        .collect();
    let (y, t) = (json!(yesterday), json!(day));
    assert_eq!(
        shape,
        [
            (
                y.clone(),
                json!("sale"),
                json!("package"),
                json!(PLAN_PRICE),
                json!(0),
                json!(true)
            ),
            (
                t.clone(),
                json!("sale"),
                json!("package"),
                json!(0),
                json!(1_000_000),
                json!(false)
            ),
            (
                t.clone(),
                json!("sale"),
                json!("one_off"),
                json!(PRICE),
                json!(0),
                json!(false)
            ),
            (
                t.clone(),
                json!("void"),
                json!("package"),
                json!(-PLAN_PRICE),
                json!(0),
                json!(true)
            ),
            (
                t.clone(),
                json!("refund"),
                json!("package"),
                json!(0),
                json!(-500_000),
                json!(false)
            ),
        ]
    );
    assert_eq!(rows[1]["package"], "Gói 10 buổi");
    assert_eq!(rows[1]["note"], "Khách quen", "discount reason");
    assert_eq!(rows[1]["customer_name"], "Nguyễn Thị Hà");
    assert_eq!(rows[2]["one_off"], "Gội đầu dưỡng sinh");
    assert_eq!(rows[3]["note"], "Ghi nhầm");
    assert_eq!(rows[4]["note"], "Khách chuyển nhà");
    assert!(rows.iter().all(|r| r["unpaid"] == 0), "R1 collects in full");

    let days = book.body["days"].as_array().cloned().unwrap_or_default();
    assert_eq!(days.len(), 2);
    for d in &days {
        let date = d["date"].as_str().unwrap_or_default();
        let daily = s
            .get(&s.owner, &format!("/reports/daily?date={date}"))
            .await;
        assert_eq!(d["total"], daily.body["revenue"]["net"]["total"], "{date}");
        assert_eq!(d["cash"], daily.body["revenue"]["net"]["cash"], "{date}");
    }

    // Paging two rows at a time returns the same rows, in order, once each.
    let mut paged = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..10 {
        let path = match &cursor {
            Some(c) => format!("/reports/ledger?from={yesterday}&to={day}&limit=2&cursor={c}"),
            None => format!("/reports/ledger?from={yesterday}&to={day}&limit=2"),
        };
        let page = s.get(&s.owner, &path).await;
        assert_eq!(page.status, StatusCode::OK, "{}", page.body);
        assert_eq!(
            page.body["days"], book.body["days"],
            "subtotals cover the whole range"
        );
        paged.extend(page.body["rows"].as_array().cloned().unwrap_or_default());
        cursor = page.body["next_cursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(paged, rows);

    let tampered = s
        .get(&s.owner, "/reports/ledger?cursor=bm90LWEtY3Vyc29y")
        .await;
    assert_eq!(tampered.status, StatusCode::UNPROCESSABLE_ENTITY);
    let too_long = s
        .get(&s.owner, "/reports/ledger?from=2026-01-01&to=2026-06-01")
        .await;
    assert_eq!(too_long.status, StatusCode::UNPROCESSABLE_ENTITY);
    Ok(())
}

// ------------------------------------------------------------------------------------- US4

/// AC4.1–4.5: one customer's timeline, the paper package page and the summary.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_customer_history_shows_every_visit_purchase_void_and_refund(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let ha = s.customer("Nguyễn Thị Hà", "0905111001").await;
    let day = today() - Duration::days(3);
    let sale = s
        .sell(ha, 0, "cash", Some(ict(day - Duration::days(1), 9, 0)))
        .await?;
    let package = sale["customer_packages"][0]["id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    s.visit(ha, vec![s.session_line(s.staff)], 0, Some(ict(day, 10, 0)))
        .await?;
    s.visit(
        ha,
        vec![s.session_line(s.staff_b), s.one_off(s.staff)],
        PRICE,
        Some(ict(day + Duration::days(1), 10, 0)),
    )
    .await?;
    let wrong = s
        .visit(
            ha,
            vec![s.session_line(s.staff)],
            0,
            Some(Utc::now() - Duration::seconds(2)),
        )
        .await?;
    s.void(format!(
        "/visits/{}/void",
        wrong["id"].as_str().unwrap_or_default()
    ))
    .await;
    s.cancel(&package, 400_000, "cash").await;
    let paper = s.import(ha, day - Duration::days(30), None, 3).await;

    let history = s
        .get(&s.manager, &format!("/customers/{ha}/history?limit=100"))
        .await;
    assert_eq!(history.status, StatusCode::OK, "{}", history.body);
    let events = history.body["events"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let kinds: Vec<&str> = events.iter().filter_map(|e| e["kind"].as_str()).collect();
    assert_eq!(
        kinds,
        [
            "package_cancelled",
            "visit_voided",
            "visit",
            "visit",
            "purchase"
        ],
        "newest first; one event per voided visit (F16); the paper-book package has no purchase event"
    );
    let cancelled = &events[0]["cancellation"];
    assert_eq!(cancelled["refund"]["amount"], 400_000);
    assert_eq!(cancelled["reason"], "Khách chuyển nhà");
    assert_eq!(events[1]["void"]["sessions_restored"], 1);
    assert_eq!(events[1]["void"]["reason"], "Ghi nhầm");
    assert_eq!(
        events[1]["visit"]["voided"], true,
        "the voided visit is one event carrying the visit and its void"
    );
    assert_eq!(
        events[1]["at"], wrong["check_in_at"],
        "at the visit's own time"
    );
    let mixed = &events[2]["visit"];
    assert_eq!(mixed["paid"], split(PRICE, 0));
    assert_eq!(mixed["lines"][0]["staff_name"], "KTV Mai");
    assert_eq!(mixed["lines"][0]["package_name"], "Gói 10 buổi");
    assert_eq!(mixed["lines"][0]["sessions_left_after"], 8);
    assert_eq!(mixed["lines"][1]["amount"], PRICE);
    assert_eq!(events[4]["purchase"]["total"], PLAN_PRICE);
    assert_eq!(events[4]["purchase"]["closer_name"], "KTV Lan");
    let text = history.body.to_string();
    assert!(
        !text.contains("note\":"),
        "no visit/health notes in the timeline: {text}"
    );

    // Pages of 2 give the same events.
    let mut paged = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..10 {
        let path = match &cursor {
            Some(c) => format!("/customers/{ha}/history?limit=2&cursor={c}"),
            None => format!("/customers/{ha}/history?limit=2"),
        };
        let page = s.get(&s.manager, &path).await;
        paged.extend(page.body["events"].as_array().cloned().unwrap_or_default());
        cursor = page.body["next_cursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(paged, events);

    let usage = s
        .get(&s.manager, &format!("/customer-packages/{package}/usage"))
        .await;
    assert_eq!(usage.status, StatusCode::OK, "{}", usage.body);
    assert_eq!(usage.body["package"]["status"], "cancelled");
    let uses: Vec<(Value, Value, Value)> = usage.body["uses"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|u| {
                    (
                        u["staff_name"].clone(),
                        u["sessions_left_after"].clone(),
                        u["voided"].clone(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        uses,
        [
            (json!("KTV Lan"), json!(9), json!(false)),
            (json!("KTV Mai"), json!(8), json!(false)),
            (json!("KTV Lan"), json!(7), json!(true)),
        ],
        "oldest first, like the paper page"
    );
    let paper_usage = s
        .get(&s.manager, &format!("/customer-packages/{paper}/usage"))
        .await;
    assert!(paper_usage.body["package"].get("origin").is_none());
    assert_eq!(paper_usage.body["package"]["sessions_remaining"], 3);

    let summary = s.get(&s.manager, &format!("/customers/{ha}/summary")).await;
    assert_eq!(summary.status, StatusCode::OK, "{}", summary.body);
    let b = &summary.body;
    assert_eq!(b["first_visit_on"], json!(day));
    assert_eq!(b["last_visit_on"], json!(day + Duration::days(1)));
    assert_eq!(b["days_since_last_visit"], 2);
    assert_eq!(
        (b["visit_count"].clone(), b["voided_visit_count"].clone()),
        (json!(2), json!(1))
    );
    assert_eq!(b["sessions_used"], 2);
    assert_eq!(
        (
            b["paid"].clone(),
            b["voided"].clone(),
            b["refunded"].clone(),
            b["net_spent"].clone()
        ),
        (
            json!(PLAN_PRICE + PRICE),
            json!(0),
            json!(400_000),
            json!(PLAN_PRICE + PRICE - 400_000)
        )
    );
    assert_eq!(
        b["packages"],
        json!({"active": 1, "expired": 0, "exhausted": 0, "cancelled": 1})
    );
    assert_eq!(b["sessions_remaining"], 3);

    // Negative paths: technician 403, unknown customer 404, tampered cursor 422.
    assert_eq!(
        s.get(&s.tech, &format!("/customers/{ha}/history"))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        s.get(
            &s.manager,
            &format!("/customers/{}/summary", Uuid::now_v7())
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        s.get(&s.manager, &format!("/customers/{ha}/history?cursor=xyz"))
            .await
            .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    Ok(())
}

// ------------------------------------------------------------------------------------- US5

/// AC5.1–5.2, 5.4: every package by derived status, expiring soon first, search, counts, pages.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn the_packages_overview_filters_by_status_and_expiry(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let ha = s.customer("Nguyễn Thị Hà", "0905111001").await;
    let lan = s.customer("Trần Thị Lan", "0905111002").await;
    let day = today();
    let soon = s
        .import(
            ha,
            day - Duration::days(60),
            Some(day + Duration::days(10)),
            5,
        )
        .await;
    let later = s
        .import(
            lan,
            day - Duration::days(60),
            Some(day + Duration::days(20)),
            5,
        )
        .await;
    let far = s
        .import(
            lan,
            day - Duration::days(50),
            Some(day + Duration::days(200)),
            5,
        )
        .await;
    let expired = s
        .import(
            ha,
            day - Duration::days(90),
            Some(day - Duration::days(1)),
            5,
        )
        .await;
    let exhausted = s.import(ha, day - Duration::days(80), None, 0).await;
    let cancelled = s.import(lan, day - Duration::days(70), None, 5).await;
    s.cancel(&cancelled, 0, "cash").await;
    s.visit(ha, vec![s.session_line(s.staff)], 0, None).await?;

    let all = s.get(&s.manager, "/packages?limit=100").await;
    assert_eq!(all.status, StatusCode::OK, "{}", all.body);
    assert_eq!(
        all.body["counts"],
        json!({"active": 3, "expiring_soon": 2, "expired": 1, "exhausted": 1, "cancelled": 1})
    );
    assert_eq!(all.body["items"].as_array().map(Vec::len), Some(6));
    // Status matches the package endpoint (one rule, `app.package_status`).
    for item in all.body["items"].as_array().cloned().unwrap_or_default() {
        let one = s
            .get(
                &s.manager,
                &format!(
                    "/customer-packages/{}",
                    item["id"].as_str().unwrap_or_default()
                ),
            )
            .await;
        assert_eq!(item["status"], one.body["status"]);
        assert_eq!(item["sessions_remaining"], one.body["sessions_remaining"]);
    }

    let expiring = s.get(&s.manager, "/packages?expiring_within_days=30").await;
    let ids: Vec<&str> = expiring.body["items"]
        .as_array()
        .map(|a| a.iter().filter_map(|i| i["id"].as_str()).collect())
        .unwrap_or_default();
    assert_eq!(
        ids,
        [soon.as_str(), later.as_str()],
        "active only, soonest first"
    );
    assert_eq!(expiring.body["items"][0]["days_to_expiry"], 10);
    assert_eq!(
        expiring.body["items"][0]["last_used_on"],
        json!(day),
        "the visit used the soonest"
    );

    for (status, wanted) in [
        ("expired", &expired),
        ("exhausted", &exhausted),
        ("cancelled", &cancelled),
    ] {
        let page = s
            .get(&s.manager, &format!("/packages?status={status}"))
            .await;
        assert_eq!(
            page.body["items"].as_array().map(Vec::len),
            Some(1),
            "{status}"
        );
        assert_eq!(page.body["items"][0]["id"], json!(wanted), "{status}");
    }
    let lans = s.get(&s.manager, "/packages?q=lan").await;
    assert_eq!(lans.body["items"].as_array().map(Vec::len), Some(3));
    assert_eq!(lans.body["counts"]["active"], 2);
    let by_phone = s.get(&s.manager, "/packages?q=111001&status=active").await;
    assert_eq!(by_phone.body["items"][0]["id"], json!(soon));

    // Pages of 2: every package once.
    let mut seen = BTreeSet::new();
    let mut cursor: Option<String> = None;
    for _ in 0..10 {
        let path = match &cursor {
            Some(c) => format!("/packages?limit=2&cursor={c}"),
            None => "/packages?limit=2".to_owned(),
        };
        let page = s.get(&s.manager, &path).await;
        for i in page.body["items"].as_array().cloned().unwrap_or_default() {
            assert!(
                seen.insert(i["id"].as_str().unwrap_or_default().to_owned()),
                "duplicate"
            );
        }
        cursor = page.body["next_cursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(seen.len(), 6);
    let _ = far;

    for bad in [
        "/packages?expiring_within_days=0",
        "/packages?expiring_within_days=366",
        "/packages?q=a",
    ] {
        assert_eq!(
            s.get(&s.manager, bad).await.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{bad}"
        );
    }
    Ok(())
}

/// AC5.3: customers with sessions left who have not come back, longest absence first.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn customers_who_stopped_coming_are_listed_longest_absence_first(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let day = today();
    let absent = s.customer("Khách vắng 40 ngày", "0905111001").await;
    let regular = s.customer("Khách quen", "0905111002").await;
    let never = s.customer("Khách chưa đến", "0905111003").await;
    let done = s.customer("Khách hết buổi", "0905111004").await;
    s.import(absent, day - Duration::days(45), None, 4).await;
    s.visit(
        absent,
        vec![s.one_off(s.staff)],
        PRICE,
        Some(ict(day - Duration::days(40), 10, 0)),
    )
    .await?;
    s.import(regular, day - Duration::days(45), None, 4).await;
    s.visit(regular, vec![s.session_line(s.staff)], 0, None)
        .await?;
    s.import(never, day - Duration::days(60), None, 4).await;
    s.visit(
        done,
        vec![s.one_off(s.staff)],
        PRICE,
        Some(ict(day - Duration::days(50), 10, 0)),
    )
    .await?;

    // Default scope is `all` since EM round 3; this asks for the package-only view.
    let list = s
        .get(
            &s.manager,
            "/customers/inactive?days=30&scope=with_sessions",
        )
        .await;
    assert_eq!(list.status, StatusCode::OK, "{}", list.body);
    let rows: Vec<(Value, Value, Value, Value)> = list.body["items"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|r| {
                    (
                        r["full_name"].clone(),
                        r["days_absent"].clone(),
                        r["last_visit_on"].clone(),
                        r["sessions_remaining"].clone(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        rows,
        [
            (json!("Khách chưa đến"), json!(60), Value::Null, json!(4)),
            (
                json!("Khách vắng 40 ngày"),
                json!(40),
                json!(day - Duration::days(40)),
                json!(4)
            ),
        ]
    );
    let all = s
        .get(&s.manager, "/customers/inactive?days=30&scope=all")
        .await;
    let names: Vec<&str> = all.body["items"]
        .as_array()
        .map(|a| a.iter().filter_map(|r| r["full_name"].as_str()).collect())
        .unwrap_or_default();
    assert_eq!(
        names,
        ["Khách chưa đến", "Khách hết buổi", "Khách vắng 40 ngày"]
    );
    let page = s
        .get(&s.manager, "/customers/inactive?days=30&scope=all&limit=2")
        .await;
    let next = page.body["next_cursor"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let rest = s
        .get(
            &s.manager,
            &format!("/customers/inactive?days=30&scope=all&limit=2&cursor={next}"),
        )
        .await;
    assert_eq!(rest.body["items"][0]["full_name"], "Khách vắng 40 ngày");
    assert_eq!(rest.body["next_cursor"], Value::Null);
    let _ = regular;
    assert_eq!(
        s.get(&s.manager, "/customers/inactive?days=0").await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    Ok(())
}

// ------------------------------------------------------------------------------ AC1.11 demo

/// After `seed-demo`, every day's dashboard reconciles with sums taken straight from the rows.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn the_dashboard_reconciles_with_raw_rows_after_seed_demo(pool: PgPool) -> TestResult {
    let h = harness(pool).await?;
    let app = app_pool(&h.owner).await?;
    app::demo::seed(
        &h.owner,
        &app,
        &h.cipher,
        &app::demo::DemoOptions {
            tenant_slug: "vpyd",
            manager_password: common::PASSWORD,
            force_again: false,
        },
    )
    .await?;
    let owner = session_for(&h, OWNER_PHONE).await;
    let days: Vec<NaiveDate> = sqlx::query_scalar(
        "SELECT DISTINCT business_date FROM app.visit
         UNION SELECT DISTINCT business_date FROM app.payment
         UNION SELECT DISTINCT business_date FROM app.refund ORDER BY 1",
    )
    .fetch_all(&h.owner)
    .await?;
    assert!(days.len() >= 15, "three weeks of demo days: {}", days.len());
    let mut range_net = 0;
    for day in &days {
        let report = call(
            &h.app,
            "GET",
            &format!("/reports/daily?date={day}"),
            Some(&owner),
            None,
        )
        .await;
        assert_eq!(report.status, StatusCode::OK, "{}", report.body);
        let (visits, customers, voided): (i64, i64, i64) = sqlx::query_as(
            "SELECT count(*) FILTER (WHERE status = 'completed'),
                    count(DISTINCT customer_id) FILTER (WHERE status = 'completed'),
                    count(*) FILTER (WHERE status = 'voided')
             FROM app.visit WHERE business_date = $1",
        )
        .bind(day)
        .fetch_one(&h.owner)
        .await?;
        let (paid, reversed): (i64, i64) = sqlx::query_as(
            "SELECT coalesce(sum(amount) FILTER (WHERE kind = 'payment'), 0)::bigint,
                    coalesce(sum(amount) FILTER (WHERE kind = 'reversal'), 0)::bigint
             FROM app.payment WHERE business_date = $1",
        )
        .bind(day)
        .fetch_one(&h.owner)
        .await?;
        let refunded: i64 = sqlx::query_scalar(
            "SELECT coalesce(sum(amount), 0)::bigint FROM app.refund WHERE business_date = $1",
        )
        .bind(day)
        .fetch_one(&h.owner)
        .await?;
        let b = &report.body;
        assert_eq!(
            (
                b["counts"]["visits"].clone(),
                b["counts"]["customers"].clone(),
                b["counts"]["voided_visits"].clone()
            ),
            (json!(visits), json!(customers), json!(voided)),
            "{day}"
        );
        let r = &b["revenue"];
        assert_eq!(
            (
                r["collected"]["total"].clone(),
                r["voids"]["total"].clone(),
                r["refunds"]["total"].clone(),
                r["net"]["total"].clone()
            ),
            (
                json!(paid),
                json!(reversed),
                json!(refunded),
                json!(paid - reversed - refunded)
            ),
            "{day}"
        );
        range_net += paid - reversed - refunded;
        let book = call(
            &h.app,
            "GET",
            &format!("/reports/ledger?from={day}&to={day}"),
            Some(&owner),
            None,
        )
        .await;
        let book_total = book.body["days"][0]["total"].as_i64().unwrap_or_default();
        assert_eq!(book_total, paid - reversed - refunded, "ledger {day}");
    }
    let (first, last) = (
        days.first().copied().unwrap_or_default(),
        days.last().copied().unwrap_or_default(),
    );
    let range = call(
        &h.app,
        "GET",
        &format!("/reports/range?from={first}&to={last}"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(range.body["revenue"]["net"]["total"], range_net);
    let visits: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.visit WHERE status = 'completed'")
            .fetch_one(&h.owner)
            .await?;
    assert_eq!(range.body["totals"]["visits"], visits);
    Ok(())
}

/// ADR-0019 D6: a package bought for someone else and refunded gives the money back to who paid.
/// The payer's spend drops and their timeline carries the refund; the person who used the package
/// sees the cancellation without any negative line; the ledger names the payer.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_refund_of_a_package_bought_for_someone_else_goes_to_the_payer(
    pool: PgPool,
) -> TestResult {
    let s = spa(pool).await?;
    let day = today();
    let payer = s.customer("Nguyễn Thị Hà", "0905111222").await;
    let user = s.customer("Trần Thị Mẹ", "0905333444").await;
    let mut body = s.sale_body(payer, 0, "cash");
    body["beneficiary_customer_id"] = json!(user);
    let sale = s.post(&s.owner, "/sales", body).await;
    assert_eq!(sale.status, StatusCode::CREATED, "{}", sale.body);
    let package = sale.body["customer_packages"][0]["id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    s.cancel(&package, 400_000, "cash").await;

    let paid = s
        .get(&s.manager, &format!("/customers/{payer}/summary"))
        .await;
    assert_eq!(paid.status, StatusCode::OK, "{}", paid.body);
    assert_eq!(paid.body["refunded"], 400_000, "{}", paid.body);
    assert_eq!(paid.body["net_spent"], PLAN_PRICE - 400_000);
    let used = s
        .get(&s.manager, &format!("/customers/{user}/summary"))
        .await;
    assert_eq!(used.body["refunded"], 0, "{}", used.body);
    assert_eq!(used.body["net_spent"], 0);

    let cancellation_of = |history: &Value| -> Value {
        history["events"]
            .as_array()
            .and_then(|e| e.iter().find(|e| e["kind"] == "package_cancelled"))
            .map(|e| e["cancellation"].clone())
            .unwrap_or_default()
    };
    let payer_history = s
        .get(&s.manager, &format!("/customers/{payer}/history"))
        .await;
    assert_eq!(
        payer_history.status,
        StatusCode::OK,
        "{}",
        payer_history.body
    );
    let theirs = cancellation_of(&payer_history.body);
    assert_eq!(
        theirs["refund"]["amount"], 400_000,
        "{}",
        payer_history.body
    );
    assert_eq!(theirs["beneficiary_name"], "Trần Thị Mẹ");
    assert_eq!(theirs["payer_name"], Value::Null);
    let user_history = s
        .get(&s.manager, &format!("/customers/{user}/history"))
        .await;
    let mine = cancellation_of(&user_history.body);
    assert_eq!(mine["package_name"], "Gói 10 buổi", "{}", user_history.body);
    assert_eq!(mine["refund"], Value::Null, "no negative line for the user");
    assert_eq!(mine["payer_name"], "Nguyễn Thị Hà");
    assert_eq!(mine["beneficiary_name"], Value::Null);

    let book = s
        .get(
            &s.owner,
            &format!("/reports/ledger?from={day}&to={day}&limit=100"),
        )
        .await;
    assert_eq!(book.status, StatusCode::OK, "{}", book.body);
    let refund = book.body["rows"]
        .as_array()
        .and_then(|rows| rows.iter().find(|r| r["entry"] == "refund"))
        .cloned()
        .unwrap_or_default();
    assert_eq!(refund["customer_id"], json!(payer), "{}", book.body);
    assert_eq!(refund["customer_name"], "Nguyễn Thị Hà");
    assert_eq!(refund["beneficiary_name"], "Trần Thị Mẹ");
    assert_eq!(refund["cash"], -400_000);
    Ok(())
}

/// ADR-0019 D7: the range report's packages. Paper-book packages without a plan are one row per
/// name (not one "no plan" row), revenue is what was paid (the paper amount, the price after
/// discount), and a package refunded since counts net.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn the_range_report_ranks_packages_by_what_was_paid_net_of_refunds(
    pool: PgPool,
) -> TestResult {
    let s = spa(pool).await?;
    let day = today();
    let ha = s.customer("Nguyễn Thị Hà", "0905111222").await;
    let lan = s.customer("Lê Thị Lan", "0905333444").await;
    for (customer, name, paid) in [
        (ha, "Gói ngâm chân sổ cũ", 600_000),
        (lan, "Gói bấm huyệt sổ cũ", 250_000),
    ] {
        let reply = s
            .post(
                &s.owner,
                "/opening-balances",
                json!({"customer_id": customer, "packages": [{
                    "name": name, "sold_price": 1_000_000, "paid_amount": paid, "paid_on": day,
                    "starts_on": day, "expires_on": null,
                    "items": [{"service_id": s.service, "sessions_purchased": 10, "sessions_remaining": 10}]
                }]}),
            )
            .await;
        assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
    }
    s.sell(ha, 200_000, "cash", None).await?;
    let refunded = s.sell(lan, 0, "cash", None).await?;
    let package = refunded["customer_packages"][0]["id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    s.cancel(&package, 500_000, "cash").await;
    // Refunded in full: not sold at all (lead 2026-11-09 on D7, red before: sold 3).
    let returned = s.sell(lan, 0, "cash", None).await?;
    let returned = returned["customer_packages"][0]["id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    s.cancel(&returned, PLAN_PRICE, "cash").await;

    let range = s
        .get(&s.owner, &format!("/reports/range?from={day}&to={day}"))
        .await;
    assert_eq!(range.status, StatusCode::OK, "{}", range.body);
    assert_eq!(
        range.body["top_packages"],
        json!([
            {"plan_id": s.plan, "name": "Gói 10 buổi", "sold": 2,
             "revenue": (PLAN_PRICE - 200_000) + (PLAN_PRICE - 500_000)},
            {"plan_id": null, "name": "Gói bấm huyệt sổ cũ", "sold": 1, "revenue": 250_000},
            {"plan_id": null, "name": "Gói ngâm chân sổ cũ", "sold": 1, "revenue": 600_000},
        ]),
        "{}",
        range.body["top_packages"]
    );
    assert_eq!(
        range.body["totals"]["packages_sold"],
        json!(4),
        "the headline count is net of the package refunded in full, like the ranking"
    );
    Ok(())
}
