//! Round 6 (spec R1-1 §10, EM answers 2026-11-06): one checkout sells packages too (US9), a
//! package bought for someone else (US13), services added to a saved visit (US12), a paid one-off
//! visit converted to a package (US11), the upsell credit ("người giới thiệu / chốt") in payroll,
//! companions known only by a note (Q-U5) and the reports.

mod common;

use axum::http::StatusCode;
use catalog::service::seed_menu;
use chrono::{Datelike, FixedOffset, NaiveDate, Utc};
use common::{
    Harness, OWNER_PHONE, Reply, TestResult, add_member, call, call_with_headers, harness,
    session_for,
};
use identity::SystemRole;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const MANAGER_PHONE: &str = "0912000111";
const TECHNICIAN_PHONE: &str = "0912000222";
/// "Cơ Bản – Dầu thảo dược": flat 49.000đ, the package's service.
const SHAMPOO: i64 = 49_000;
/// "Cơ Bản – Dầu cặp": flat 39.000đ, not in the package.
const CHEAPER: i64 = 39_000;
const PLAN_PRICE: i64 = 1_500_000;
/// 1.500.000đ ÷ 10 paid sessions.
const SESSION_VALUE: i64 = 150_000;

type Setup<T> = Result<T, Box<dyn std::error::Error>>;

struct Spa {
    h: Harness,
    owner: String,
    manager: String,
    technician: String,
    ha: String,
    binh: String,
    lan: String,
    mai: String,
    shampoo: String,
    cheaper: String,
    plan: String,
}

fn str_of(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_owned()
}

fn code(reply: &Reply) -> (StatusCode, Option<&str>) {
    (reply.status, reply.body["code"].as_str())
}

fn field(reply: &Reply) -> (StatusCode, Option<&str>, Option<&str>) {
    (
        reply.status,
        reply.body["errors"][0]["field"].as_str(),
        reply.body["errors"][0]["code"].as_str(),
    )
}

fn key() -> String {
    Uuid::now_v7().to_string()
}

fn spa_today() -> NaiveDate {
    let vn = FixedOffset::east_opt(7 * 3600).unwrap_or_else(|| panic!("offset"));
    Utc::now().with_timezone(&vn).date_naive()
}

async fn service(h: &Harness, name: &str) -> Setup<String> {
    let id: Uuid = sqlx::query_scalar(
        "SELECT s.id FROM app.service s JOIN app.service_category c ON c.id = s.category_id
         WHERE c.name = 'Gội đầu dưỡng sinh' AND s.name = $1",
    )
    .bind(name)
    .fetch_one(&h.owner)
    .await?;
    Ok(id.to_string())
}

async fn spa(pool: PgPool) -> Setup<Spa> {
    let h = harness(pool).await?;
    seed_menu(&h.owner, "vpyd").await?;
    add_member(
        &h.owner,
        "vpyd",
        MANAGER_PHONE,
        "Quản lý Ngọc",
        SystemRole::Manager,
    )
    .await?;
    add_member(
        &h.owner,
        "vpyd",
        TECHNICIAN_PHONE,
        "KTV Hoa",
        SystemRole::Technician,
    )
    .await?;
    let owner = session_for(&h, OWNER_PHONE).await;
    let manager = session_for(&h, MANAGER_PHONE).await;
    let technician = session_for(&h, TECHNICIAN_PHONE).await;
    let shampoo = service(&h, "Cơ Bản – Dầu thảo dược").await?;
    let cheaper = service(&h, "Cơ Bản – Dầu cặp").await?;
    let lan = org::provisioning::add_staff(&h.owner, "vpyd", "KTV Lan", "Kỹ thuật viên").await?;
    let mai = org::provisioning::add_staff(&h.owner, "vpyd", "KTV Mai", "Kỹ thuật viên").await?;
    let customer = |name: &str, phone: &str| {
        let app = h.app.clone();
        let manager = manager.clone();
        let body = json!({"full_name": name, "phone": phone, "gender": "female"});
        async move { call(&app, "POST", "/customers", Some(&manager), Some(body)).await }
    };
    let ha = customer("Nguyễn Thị Hà", "0905111222").await;
    assert_eq!(ha.status, StatusCode::CREATED, "{}", ha.body);
    let binh = customer("Trần Thị Bình", "0905333444").await;
    assert_eq!(binh.status, StatusCode::CREATED, "{}", binh.body);
    let plan = call(
        &h.app,
        "POST",
        "/package-plans",
        Some(&owner),
        Some(json!({
            "name": "Gội đầu 10 buổi + tặng 2", "price": PLAN_PRICE, "validity_months": 6,
            "gift_note": null, "active": true,
            "items": [{"service_id": shampoo, "sessions": 10, "bonus_sessions": 2}],
        })),
    )
    .await;
    assert_eq!(plan.status, StatusCode::CREATED, "{}", plan.body);
    Ok(Spa {
        owner,
        manager,
        technician,
        ha: str_of(&ha.body["id"]),
        binh: str_of(&binh.body["id"]),
        lan: lan.to_string(),
        mai: mai.to_string(),
        shampoo,
        cheaper,
        plan: str_of(&plan.body["id"]),
        h,
    })
}

impl Spa {
    async fn post_with(&self, who: &str, path: &str, body: Value, k: &str) -> Reply {
        call_with_headers(
            &self.h.app,
            "POST",
            path,
            Some(who),
            Some(body),
            &[("idempotency-key", k)],
        )
        .await
    }

    async fn post(&self, who: &str, path: &str, body: Value) -> Reply {
        self.post_with(who, path, body, &key()).await
    }

    async fn get(&self, who: &str, path: &str) -> Reply {
        call(&self.h.app, "GET", path, Some(who), None).await
    }

    async fn convert(&self, who: &str, visit: &str, version: i64, body: Value) -> Reply {
        let etag = format!("\"{version}\"");
        call_with_headers(
            &self.h.app,
            "POST",
            &format!("/visits/{visit}/convert-to-package"),
            Some(who),
            Some(body),
            &[("idempotency-key", &key()), ("If-Match", &etag)],
        )
        .await
    }

    fn package_sale(&self, closer: Option<&str>) -> Value {
        json!({"ref": "p1", "package_plan_id": self.plan, "closer_staff_id": closer})
    }

    /// Hà: shampoo one-off (49.000đ) by Lan, paid cash.
    async fn one_off_visit(&self, extra: Option<Value>) -> Reply {
        let mut lines =
            vec![json!({"service_id": self.shampoo, "source": "one_off", "staff_id": self.lan})];
        let mut total = SHAMPOO;
        if let Some(l) = extra {
            lines.push(l);
            total += CHEAPER;
        }
        let v = self
            .post(
                &self.manager,
                "/visits",
                json!({"customer_id": self.ha, "lines": lines,
                       "payments": [{"method": "cash", "amount": total}]}),
            )
            .await;
        assert_eq!(v.status, StatusCode::CREATED, "{}", v.body);
        v
    }

    async fn count(&self, sql: &str) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
            .fetch_one(&self.h.owner)
            .await
    }

    async fn exec(&self, sql: &str) -> Result<(), sqlx::Error> {
        sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()))
            .execute(&self.h.owner)
            .await
            .map(|_| ())
    }

    async fn pay(&self, staff: &str, rate_bp: i32) {
        let r = call(
            &self.h.app,
            "PUT",
            &format!("/staff/{staff}/pay"),
            Some(&self.owner),
            Some(json!({
                "base_salary": 5_000_000, "commission_rate_bp": rate_bp,
                "loyalty_bonus_monthly": 0, "meal_allowance": 0
            })),
        )
        .await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    }
}

fn slip<'a>(payroll: &'a Value, staff: &str) -> &'a Value {
    payroll["payslips"]
        .as_array()
        .and_then(|a| a.iter().find(|s| s["staff_id"] == json!(staff)))
        .unwrap_or(&Value::Null)
}

// ------------------------------------------------------------------------------------- US9

/// AC9.1, AC9.2, AC9.10: the cart sells a package and takes today's session from it in one
/// request; one sale per kind, both linked to the visit; payments split cash-first.
#[sqlx::test(migrations = "../../migrations")]
async fn a_package_sold_in_the_cart_pays_for_todays_session(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let v = s
        .post(
            &s.manager,
            "/visits",
            json!({
                "customer_id": s.ha,
                "lines": [
                    {"service_id": s.shampoo, "source": "package", "package_ref": "p1", "staff_id": s.lan},
                    {"service_id": s.cheaper, "source": "one_off", "staff_id": s.lan, "referrer_staff_id": s.mai},
                ],
                "package_sales": [s.package_sale(Some(&s.lan))],
                "payments": [{"method": "cash", "amount": 100_000},
                             {"method": "bank_transfer", "amount": PLAN_PRICE + CHEAPER - 100_000}],
            }),
        )
        .await;
    assert_eq!(v.status, StatusCode::CREATED, "{}", v.body);
    let b = &v.body;
    assert_eq!(b["lines"][0]["source"], "package");
    assert_eq!(
        b["lines"][0]["sessions_left_after"], 11,
        "11/12 left (AC9.1)"
    );
    assert_eq!(b["lines"][0]["line_value"], SESSION_VALUE);
    assert_eq!(b["lines"][1]["referrer_name"], "KTV Mai");
    assert_eq!(b["total"], CHEAPER);
    // The one-off sale is paid cash first; the package gets the rest of the cash + the transfer.
    assert_eq!(b["payments"], json!([b["payments"][0].clone()]));
    assert_eq!(b["payments"][0]["amount"], CHEAPER);
    assert_eq!(b["payments"][0]["method"], "cash");
    let pkg = &b["package_sales"][0];
    assert_eq!(pkg["total"], PLAN_PRICE);
    assert_eq!(pkg["closer_name"], "KTV Lan");
    assert_eq!(
        pkg["payments"].as_array().map(|p| p
            .iter()
            .map(|x| (x["method"].clone(), x["amount"].clone()))
            .collect::<Vec<_>>()),
        Some(vec![
            (json!("cash"), json!(100_000 - CHEAPER)),
            (
                json!("bank_transfer"),
                json!(PLAN_PRICE + CHEAPER - 100_000)
            ),
        ])
    );
    let visit = str_of(&b["id"]);
    assert_eq!(
        s.count(&format!(
            "SELECT count(*) FROM app.sale WHERE visit_id = '{visit}'"
        ))
        .await?,
        2,
        "one sale per kind, both linked"
    );

    let report = s
        .get(&s.owner, &format!("/reports/daily?date={}", spa_today()))
        .await;
    assert_eq!(report.status, StatusCode::OK, "{}", report.body);
    assert_eq!(
        report.body["revenue"]["collected_by_kind"],
        json!({"package": PLAN_PRICE, "one_off": CHEAPER}),
        "AC9.10: each đồng once, by kind"
    );
    assert_eq!(report.body["counts"]["packages_sold"], 1);
    assert_eq!(report.body["counts"]["sessions_used"], 1);
    assert_eq!(report.body["counts"]["customers"], 1);

    // The revenue book shows who sold what.
    let book = s
        .get(&s.owner, &format!("/reports/ledger?from={}", spa_today()))
        .await;
    assert_eq!(book.status, StatusCode::OK, "{}", book.body);
    let rows = book.body["rows"].as_array().cloned().unwrap_or_default();
    let by_kind = |kind: &str| {
        rows.iter()
            .find(|r| r["kind"] == json!(kind))
            .map(|r| (r["upsell_by"].clone(), r["visit_id"].clone()))
    };
    assert_eq!(by_kind("package"), Some((json!("KTV Lan"), json!(visit))));
    assert_eq!(by_kind("one_off"), Some((json!("KTV Mai"), json!(visit))));
    Ok(())
}

/// AC9.3–AC9.8: any refusal leaves nothing behind (no visit, no sale, no package).
#[sqlx::test(migrations = "../../migrations")]
async fn a_refused_checkout_records_nothing(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let body = |lines: Value, sale: Value, paid: i64| {
        json!({"customer_id": s.ha, "lines": lines, "package_sales": [sale],
               "payments": [{"method": "cash", "amount": paid}]})
    };
    let today = json!([{"service_id": s.shampoo, "source": "package", "package_ref": "p1", "staff_id": s.lan}]);

    let short = s
        .post(
            &s.manager,
            "/visits",
            body(today.clone(), s.package_sale(None), PLAN_PRICE - 1),
        )
        .await;
    assert_eq!(
        code(&short),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("payment.amount_mismatch")
        )
    );

    let tech = s
        .post(
            &s.technician,
            "/visits",
            body(today.clone(), s.package_sale(None), PLAN_PRICE),
        )
        .await;
    assert_eq!(tech.status, StatusCode::FORBIDDEN);

    let unknown = s
        .post(
            &s.manager,
            "/visits",
            body(
                json!([{"service_id": s.shampoo, "source": "package", "package_ref": "p9", "staff_id": s.lan}]),
                s.package_sale(None),
                PLAN_PRICE,
            ),
        )
        .await;
    assert_eq!(
        field(&unknown),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("lines[0].package_ref"),
            Some("unknown")
        )
    );

    let not_in_plan = s
        .post(
            &s.manager,
            "/visits",
            body(
                json!([{"service_id": s.cheaper, "source": "package", "package_ref": "p1", "staff_id": s.lan}]),
                s.package_sale(None),
                PLAN_PRICE,
            ),
        )
        .await;
    assert_eq!(
        field(&not_in_plan),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("lines[0].service_id"),
            Some("not_in_package")
        )
    );

    s.exec(&format!(
        "UPDATE app.staff SET status = 'left', left_on = current_date WHERE id = '{}'",
        s.mai
    ))
    .await?;
    let gone = s
        .post(
            &s.manager,
            "/visits",
            body(today.clone(), s.package_sale(Some(&s.mai)), PLAN_PRICE),
        )
        .await;
    assert_eq!(
        code(&gone),
        (StatusCode::UNPROCESSABLE_ENTITY, Some("staff.inactive"))
    );

    let elsewhere = s
        .post(
            &s.manager,
            "/visits",
            body(
                today.clone(),
                json!({"ref": "p1", "package_plan_id": Uuid::now_v7()}),
                PLAN_PRICE,
            ),
        )
        .await;
    assert_eq!(elsewhere.status, StatusCode::NOT_FOUND);

    // AC9.8 / Q-U9: the manager's 10 % cap applies to the package sale on its own.
    let mut over = s.package_sale(None);
    over["discount"] = json!({"amount": 150_001, "reason": "Khách quen"});
    let capped = s
        .post(
            &s.manager,
            "/visits",
            body(today.clone(), over, PLAN_PRICE - 150_001),
        )
        .await;
    assert_eq!(
        code(&capped),
        (StatusCode::FORBIDDEN, Some("discount.limit_exceeded"))
    );
    let mut at_cap = s.package_sale(None);
    at_cap["discount"] = json!({"amount": 150_000, "reason": "Khách quen"});
    let ok = s
        .post(
            &s.manager,
            "/visits",
            body(today.clone(), at_cap, PLAN_PRICE - 150_000),
        )
        .await;
    assert_eq!(ok.status, StatusCode::CREATED, "{}", ok.body);

    assert_eq!(
        s.count("SELECT count(*) FROM app.visit").await?,
        1,
        "only the last one"
    );
    assert_eq!(
        s.count("SELECT count(*) FROM app.customer_package").await?,
        1
    );
    assert_eq!(s.count("SELECT count(*) FROM app.sale").await?, 1);

    // A package line credits the package's closer, never a line referrer.
    let referred = s
        .post(
            &s.manager,
            "/visits",
            json!({"customer_id": s.ha, "lines": [{"service_id": s.shampoo, "source": "package",
                    "staff_id": s.lan, "referrer_staff_id": s.lan}]}),
        )
        .await;
    assert_eq!(
        field(&referred),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("lines[0].referrer_staff_id"),
            Some("not_allowed")
        )
    );
    Ok(())
}

/// AC9.7, AC9.11: a retry with the same key replays the first response, one visit, one package.
#[sqlx::test(migrations = "../../migrations")]
async fn a_checkout_retry_never_sells_twice(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let body = json!({
        "customer_id": s.ha,
        "lines": [{"service_id": s.shampoo, "source": "package", "package_ref": "p1", "staff_id": s.lan}],
        "package_sales": [s.package_sale(Some(&s.lan))],
        "payments": [{"method": "cash", "amount": PLAN_PRICE}],
    });
    let k = key();
    let first = s.post_with(&s.manager, "/visits", body.clone(), &k).await;
    assert_eq!(first.status, StatusCode::CREATED, "{}", first.body);
    let again = s.post_with(&s.manager, "/visits", body.clone(), &k).await;
    assert_eq!(again.status, StatusCode::CREATED);
    assert_eq!(again.body, first.body);
    let mut other = body.clone();
    other["payments"][0]["method"] = json!("bank_transfer");
    let reused = s.post_with(&s.manager, "/visits", other, &k).await;
    assert_eq!(
        code(&reused),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("idempotency.key_reuse_mismatch")
        )
    );
    assert_eq!(s.count("SELECT count(*) FROM app.visit").await?, 1);
    assert_eq!(
        s.count("SELECT count(*) FROM app.customer_package").await?,
        1
    );
    Ok(())
}

/// AC9.9 + Q-U5: completing a booking sells a package in the same checkout; the booking's
/// companion notes travel to the visit; a family with one person served is a valid group.
#[sqlx::test(migrations = "../../migrations")]
async fn a_booking_completes_with_a_package_and_keeps_its_companions(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let at = format!("{}T09:00:00+07:00", spa_today());
    let lonely_group = s
        .post(
            &s.owner,
            "/appointments/group",
            json!({"starts_at": at, "people": [{"customer_id": s.ha, "service_ids": [s.shampoo]}]}),
        )
        .await;
    assert_eq!(
        lonely_group.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        lonely_group.body
    );
    let group = s
        .post(
            &s.owner,
            "/appointments/group",
            json!({"starts_at": at, "people": [{"customer_id": s.ha, "service_ids": [s.shampoo]}],
                   "companions": ["Chồng của chị Hà"]}),
        )
        .await;
    assert_eq!(group.status, StatusCode::CREATED, "{}", group.body);
    let booking = &group.body["appointments"][0];
    assert_eq!(booking["companions"], json!(["Chồng của chị Hà"]));
    assert_eq!(
        booking["customer_id"],
        json!(s.ha),
        "the companion is not a customer"
    );
    let too_long = s
        .post(
            &s.owner,
            "/appointments",
            json!({"customer_id": s.binh, "service_ids": [s.shampoo], "starts_at": at,
                   "companions": ["x".repeat(101)]}),
        )
        .await;
    assert_eq!(
        field(&too_long),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("companions[0]"),
            Some("invalid")
        )
    );

    let done = s
        .post(
            &s.manager,
            &format!("/appointments/{}/complete", str_of(&booking["id"])),
            json!({
                "lines": [{"service_id": s.shampoo, "source": "package", "package_ref": "p1", "staff_id": s.lan}],
                "package_sales": [s.package_sale(Some(&s.lan))],
                "payments": [{"method": "cash", "amount": PLAN_PRICE}],
            }),
        )
        .await;
    assert_eq!(done.status, StatusCode::OK, "{}", done.body);
    assert_eq!(done.body["appointment"]["status"], "done");
    // The calendar counts every đồng taken in the visit, the package included.
    assert_eq!(done.body["appointment"]["payment_state"], "paid");
    assert_eq!(done.body["appointment"]["paid_amount"], PLAN_PRICE);
    let visit = &done.body["visit"];
    assert_eq!(visit["lines"][0]["sessions_left_after"], 11);
    assert_eq!(visit["package_sales"][0]["total"], PLAN_PRICE);
    assert_eq!(visit["companions"], json!(["Chồng của chị Hà"]));
    assert_eq!(
        s.count("SELECT count(*) FROM app.customer").await?,
        2,
        "no customer record for the note"
    );
    Ok(())
}

// ------------------------------------------------------------------------------------ US13

/// AC13.1–13.2: the payer pays, the package belongs to the user; packages are not shared.
#[sqlx::test(migrations = "../../migrations")]
async fn a_package_bought_for_someone_else_belongs_to_them(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let gift = s
        .post(
            &s.manager,
            "/sales",
            json!({"customer_id": s.ha, "beneficiary_customer_id": s.binh, "closer_staff_id": null,
                   "lines": [{"package_plan_id": s.plan}],
                   "payments": [{"method": "cash", "amount": PLAN_PRICE}]}),
        )
        .await;
    assert_eq!(gift.status, StatusCode::CREATED, "{}", gift.body);
    assert_eq!(gift.body["customer_id"], json!(s.ha), "the payer");
    assert_eq!(gift.body["closer_staff_id"], Value::Null, "khách tự chọn");
    assert_eq!(
        gift.body["customer_packages"][0]["customer_id"],
        json!(s.binh)
    );
    let item = str_of(&gift.body["customer_packages"][0]["items"][0]["id"]);

    let binh = s
        .get(&s.manager, &format!("/customers/{}/packages", s.binh))
        .await;
    assert_eq!(binh.status, StatusCode::OK, "{}", binh.body);
    assert_eq!(binh.body.as_array().map(Vec::len), Some(1));
    let history = s
        .get(&s.manager, &format!("/customers/{}/history", s.ha))
        .await;
    assert_eq!(history.status, StatusCode::OK, "{}", history.body);
    assert_eq!(
        history.body["events"][0]["purchase"]["beneficiary_name"], "Trần Thị Bình",
        "Mua tặng Gói … cho Bình"
    );

    // The payer cannot use it (D5).
    let shared = s
        .post(
            &s.manager,
            "/visits",
            json!({"customer_id": s.ha, "lines": [{"service_id": s.shampoo, "source": "package",
                    "customer_package_item_id": item, "staff_id": s.lan}]}),
        )
        .await;
    assert_eq!(
        code(&shared),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("package.item_not_found")
        )
    );
    // Nor take today's session from a package bought for someone else in the same cart.
    let mut cart_gift = s.package_sale(None);
    cart_gift["beneficiary_customer_id"] = json!(s.binh);
    let cart = s
        .post(
            &s.manager,
            "/visits",
            json!({"customer_id": s.ha,
                   "lines": [{"service_id": s.shampoo, "source": "package", "package_ref": "p1", "staff_id": s.lan}],
                   "package_sales": [cart_gift.clone()],
                   "payments": [{"method": "cash", "amount": PLAN_PRICE}]}),
        )
        .await;
    assert_eq!(
        field(&cart),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("lines[0].package_ref"),
            Some("not_beneficiary")
        )
    );
    cart_gift["beneficiary_customer_id"] = json!(Uuid::now_v7());
    let nobody = s
        .post(
            &s.manager,
            "/sales",
            json!({"customer_id": s.ha, "beneficiary_customer_id": Uuid::now_v7(),
                   "lines": [{"package_plan_id": s.plan}],
                   "payments": [{"method": "cash", "amount": PLAN_PRICE}]}),
        )
        .await;
    assert_eq!(nobody.status, StatusCode::NOT_FOUND);
    Ok(())
}

// ------------------------------------------------------------------------------------ US12

/// AC12.1–12.3: "Thêm dịch vụ vào lượt" adds lines and a sale of its own, never touches the old
/// ones; a void of the visit later reverses every one-off sale but keeps a package sold in it.
#[sqlx::test(migrations = "../../migrations")]
async fn services_added_to_a_saved_visit_get_a_sale_of_their_own(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let v = s.one_off_visit(None).await;
    let id = str_of(&v.body["id"]);
    let path = format!("/visits/{id}/additions");
    let body = json!({
        "lines": [{"service_id": s.cheaper, "source": "one_off", "staff_id": s.lan, "referrer_staff_id": s.lan}],
        "package_sales": [{"package_plan_id": s.plan, "closer_staff_id": s.mai}],
        "payments": [{"method": "bank_transfer", "amount": CHEAPER + PLAN_PRICE}],
    });
    let k = key();
    let added = s.post_with(&s.manager, &path, body.clone(), &k).await;
    assert_eq!(added.status, StatusCode::CREATED, "{}", added.body);
    let b = &added.body;
    assert_eq!(b["lines"].as_array().map(Vec::len), Some(2));
    assert_eq!(
        b["lines"][0]["sale_id"], v.body["sale_id"],
        "the old line is untouched"
    );
    assert_ne!(b["lines"][1]["sale_id"], v.body["sale_id"]);
    assert_eq!(b["total"], SHAMPOO + CHEAPER);
    assert_eq!(b["payments"].as_array().map(Vec::len), Some(2));
    assert_eq!(b["package_sales"][0]["closer_name"], "KTV Mai");
    assert_eq!(b["version"], 2, "the visit moved on");
    assert_eq!(
        s.count("SELECT count(*) FROM app.payment WHERE kind = 'reversal'")
            .await?,
        0,
        "an addition is a sale, not a correction"
    );
    let replay = s.post_with(&s.manager, &path, body.clone(), &k).await;
    assert_eq!(replay.body, added.body);
    assert_eq!(s.count("SELECT count(*) FROM app.sale").await?, 3);
    let empty = s.post(&s.manager, &path, json!({"lines": []})).await;
    assert_eq!(field(&empty).1, Some("lines"));

    // The void reverses both one-off sales; the package stays the customer's.
    let void = s
        .post(
            &s.manager,
            &format!("/visits/{id}/void"),
            json!({"reason": "Ghi nhầm khách"}),
        )
        .await;
    assert_eq!(void.status, StatusCode::OK, "{}", void.body);
    assert_eq!(
        s.count("SELECT coalesce(sum(amount), 0)::bigint FROM app.payment WHERE kind = 'reversal'")
            .await?,
        SHAMPOO + CHEAPER
    );
    assert_eq!(
        s.count("SELECT count(*) FROM app.sale WHERE status = 'open'")
            .await?,
        1,
        "the package sale is voided on its own, if at all"
    );
    let after = s.post(&s.manager, &path, body).await;
    assert_eq!(code(&after), (StatusCode::CONFLICT, Some("visit.voided")));

    // Another day: the manager may not add, the owner may.
    let v2 = s.one_off_visit(None).await;
    let id2 = str_of(&v2.body["id"]);
    s.exec(&format!(
        "UPDATE app.visit SET business_date = business_date - 1, check_in_at = check_in_at - interval '1 day' WHERE id = '{id2}'"
    ))
    .await?;
    let more = json!({"lines": [{"service_id": s.cheaper, "source": "one_off", "staff_id": s.lan}],
                      "payments": [{"method": "cash", "amount": CHEAPER}]});
    let manager = s
        .post(
            &s.manager,
            &format!("/visits/{id2}/additions"),
            more.clone(),
        )
        .await;
    assert_eq!(code(&manager), (StatusCode::FORBIDDEN, Some("day.locked")));
    let owner = s
        .post(&s.owner, &format!("/visits/{id2}/additions"), more)
        .await;
    assert_eq!(owner.status, StatusCode::CREATED, "{}", owner.body);
    Ok(())
}

// ------------------------------------------------------------------------------------ US11

/// AC11.1–11.3, 11.8: "Tính buổi hôm nay vào gói": the one-off is reversed, the package sold and
/// today's session taken from it; lines the package does not cover are recorded again unchanged.
#[sqlx::test(migrations = "../../migrations")]
async fn a_paid_one_off_visit_converts_to_its_first_package_session(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let extra = json!({"service_id": s.cheaper, "source": "one_off", "staff_id": s.mai, "referrer_staff_id": s.lan});
    let v = s.one_off_visit(Some(extra)).await;
    let id = str_of(&v.body["id"]);
    // Paid 88.000đ cash for both lines; the package is topped up by transfer: only the rest.
    let body = json!({
        "package_sale": {"package_plan_id": s.plan, "closer_staff_id": s.lan},
        "payments": [{"method": "bank_transfer", "amount": PLAN_PRICE - SHAMPOO}],
    });
    let r = s.convert(&s.manager, &id, 1, body.clone()).await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);
    let voided = &r.body["voided"];
    assert_eq!(voided["status"], "voided");
    assert_eq!(
        voided["void_reason"],
        "Chuyển sang gói Gội đầu 10 buổi + tặng 2"
    );
    let visit = &r.body["visit"];
    assert_eq!(visit["corrects_visit_id"], json!(id));
    assert_eq!(
        visit["check_in_at"], v.body["check_in_at"],
        "same service time"
    );
    assert_eq!(visit["lines"][0]["source"], "package");
    assert_eq!(visit["lines"][0]["sessions_left_after"], 11, "11/12");
    assert_eq!(
        visit["lines"][0]["staff_id"],
        json!(s.lan),
        "same technician"
    );
    // AC11.2: the line the package does not cover is recorded again, same money and people.
    assert_eq!(visit["lines"][1]["source"], "one_off");
    assert_eq!(visit["lines"][1]["line_value"], CHEAPER);
    assert_eq!(visit["lines"][1]["staff_id"], json!(s.mai));
    assert_eq!(visit["lines"][1]["referrer_staff_id"], json!(s.lan));
    assert_eq!(visit["package_sales"][0]["total"], PLAN_PRICE);
    assert_eq!(
        r.body["difference"],
        PLAN_PRICE - SHAMPOO,
        "Khách đưa thêm = gói − tiền lẻ đã trả"
    );
    // Net cash of the day from this customer = the package + the kept one-off.
    let report = s
        .get(&s.owner, &format!("/reports/daily?date={}", spa_today()))
        .await;
    assert_eq!(report.body["revenue"]["net"]["total"], PLAN_PRICE + CHEAPER);
    // The cash already paid stays cash: only the top-up is a transfer.
    assert_eq!(report.body["revenue"]["net"]["cash"], SHAMPOO + CHEAPER);
    assert_eq!(
        report.body["revenue"]["net"]["bank_transfer"],
        PLAN_PRICE - SHAMPOO
    );
    assert_eq!(
        report.body["counts"]["voided_visits"], 0,
        "a conversion is not a void"
    );
    assert_eq!(report.body["counts"]["packages_sold"], 1);

    // AC11.3: once only.
    let again = s.convert(&s.manager, &id, 2, body.clone()).await;
    assert_eq!(
        code(&again),
        (StatusCode::CONFLICT, Some("visit.corrected"))
    );
    // Nothing the package covers.
    let other = s
        .post(
            &s.manager,
            "/visits",
            json!({"customer_id": s.ha, "lines": [{"service_id": s.cheaper, "source": "one_off", "staff_id": s.lan}],
                   "payments": [{"method": "cash", "amount": CHEAPER}]}),
        )
        .await;
    let nothing = s
        .convert(&s.manager, &str_of(&other.body["id"]), 1, body.clone())
        .await;
    assert_eq!(
        code(&nothing),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("visit.nothing_to_convert")
        )
    );
    assert_eq!(
        s.count(&format!(
            "SELECT count(*) FROM app.visit WHERE id = '{}' AND status = 'completed'",
            str_of(&other.body["id"])
        ))
        .await?,
        1,
        "the refusal rolled back the void"
    );
    Ok(())
}

/// Live test 2026-11-07 (regression): converting a paid single visit re-recorded the money paid
/// as "−cash, +full package by transfer". The money already paid keeps its method; only the
/// top-up takes the method chosen now, so the day's cash / transfer split does not move and the
/// visit row reads "Tiền mặt + CK".
#[sqlx::test(migrations = "../../migrations")]
async fn a_conversion_keeps_the_money_already_paid_in_its_method(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let path = format!("/reports/daily?date={}", spa_today());
    // 49.000đ paid as 29.000đ cash + 20.000đ transfer.
    let v = s
        .post(
            &s.manager,
            "/visits",
            json!({"customer_id": s.ha,
                   "lines": [{"service_id": s.shampoo, "source": "one_off", "staff_id": s.lan}],
                   "payments": [{"method": "cash", "amount": 29_000},
                                {"method": "bank_transfer", "amount": 20_000}]}),
        )
        .await;
    assert_eq!(v.status, StatusCode::CREATED, "{}", v.body);
    let before = s.get(&s.owner, &path).await.body["revenue"]["net"].clone();
    assert_eq!(before["cash"], 29_000);
    assert_eq!(before["bank_transfer"], 20_000);

    // The rest of the package is handed over in cash.
    let r = s
        .convert(
            &s.manager,
            &str_of(&v.body["id"]),
            1,
            json!({"package_sale": {"package_plan_id": s.plan},
                   "payments": [{"method": "cash", "amount": PLAN_PRICE - SHAMPOO}]}),
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);
    assert_eq!(r.body["difference"], PLAN_PRICE - SHAMPOO);
    let paid: Vec<(String, i64)> = r.body["visit"]["package_sales"][0]["payments"]
        .as_array()
        .map(|ps| {
            ps.iter()
                .map(|p| (str_of(&p["method"]), p["amount"].as_i64().unwrap_or(0)))
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        paid,
        vec![
            ("cash".to_owned(), 29_000 + PLAN_PRICE - SHAMPOO),
            ("bank_transfer".to_owned(), 20_000),
        ],
        "the transfer already made stays a transfer; the cash top-up joins the cash"
    );
    let after = s.get(&s.owner, &path).await.body["revenue"]["net"].clone();
    assert_eq!(after["total"], PLAN_PRICE);
    assert_eq!(after["cash"], PLAN_PRICE - 20_000);
    assert_eq!(after["bank_transfer"], 20_000, "the transfer did not move");

    // The old contract (full package price as the top-up) now collects too much: refused, and
    // nothing changes.
    let v2 = s.one_off_visit(None).await;
    let over = s
        .convert(
            &s.manager,
            &str_of(&v2.body["id"]),
            1,
            json!({"package_sale": {"package_plan_id": s.plan},
                   "payments": [{"method": "bank_transfer", "amount": PLAN_PRICE}]}),
        )
        .await;
    assert_eq!(
        code(&over),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("payment.amount_mismatch")
        )
    );
    let unchanged = s.get(&s.owner, &path).await.body["revenue"]["net"].clone();
    assert_eq!(unchanged["cash"], PLAN_PRICE - 20_000 + SHAMPOO);
    assert_eq!(unchanged["bank_transfer"], 20_000);
    Ok(())
}

/// AC11.4–11.7 + Q-U4: manager same day, owner later days unless the month's payroll is closed;
/// stale `If-Match`; permissions.
#[sqlx::test(migrations = "../../migrations")]
async fn converting_follows_the_closed_day_and_payroll_rules(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let body = json!({
        "package_sale": {"package_plan_id": s.plan, "closer_staff_id": s.lan},
        "payments": [{"method": "cash", "amount": PLAN_PRICE - SHAMPOO}],
    });
    let v = s.one_off_visit(None).await;
    let id = str_of(&v.body["id"]);
    let stale = s.convert(&s.manager, &id, 7, body.clone()).await;
    assert_eq!(stale.status, StatusCode::PRECONDITION_FAILED);
    let tech = s.convert(&s.technician, &id, 1, body.clone()).await;
    assert_eq!(tech.status, StatusCode::FORBIDDEN);
    let missing = s
        .convert(&s.manager, &Uuid::now_v7().to_string(), 1, body.clone())
        .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
    let short = s
        .convert(
            &s.manager,
            &id,
            1,
            // The full package price would collect today's 49.000đ a second time.
            json!({"package_sale": {"package_plan_id": s.plan},
                   "payments": [{"method": "cash", "amount": PLAN_PRICE}]}),
        )
        .await;
    assert_eq!(
        code(&short),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("payment.amount_mismatch")
        )
    );

    // Yesterday's visit: manager refused, owner allowed.
    s.exec("UPDATE app.visit SET business_date = business_date - 1, check_in_at = check_in_at - interval '1 day'")
        .await?;
    s.exec("UPDATE app.sale SET business_date = business_date - 1 WHERE visit_id IS NOT NULL")
        .await?;
    let manager = s.convert(&s.manager, &id, 1, body.clone()).await;
    assert_eq!(code(&manager), (StatusCode::FORBIDDEN, Some("day.locked")));

    // A visit of a month whose payroll is closed.
    let v2 = s.one_off_visit(None).await;
    let id2 = str_of(&v2.body["id"]);
    let today = spa_today();
    let first_of_month = today.with_day(1).unwrap_or(today);
    let last_month = first_of_month - chrono::Duration::days(1);
    let back = (today - last_month).num_days();
    s.exec(&format!(
        "UPDATE app.visit SET business_date = business_date - {back}, check_in_at = check_in_at - interval '{back} days' WHERE id = '{id2}'"
    ))
    .await?;
    let owner_id: Uuid =
        sqlx::query_scalar("SELECT id FROM app.user_account WHERE full_name = 'Chủ Spa'")
            .fetch_one(&s.h.owner)
            .await?;
    s.exec(&format!(
        "INSERT INTO app.payroll_period (tenant_id, period, status, closed_at, closed_by)
         SELECT id, '{}', 'closed', now(), '{owner_id}' FROM app.tenant WHERE slug = 'vpyd'",
        last_month.with_day(1).unwrap_or(last_month)
    ))
    .await?;
    let closed = s.convert(&s.owner, &id2, 1, body.clone()).await;
    assert_eq!(
        code(&closed),
        (StatusCode::CONFLICT, Some("payroll.period_closed"))
    );
    let closed_add = s
        .post(
            &s.owner,
            &format!("/visits/{id2}/additions"),
            json!({"lines": [{"service_id": s.cheaper, "source": "one_off", "staff_id": s.lan}],
                   "payments": [{"method": "cash", "amount": CHEAPER}]}),
        )
        .await;
    assert_eq!(
        code(&closed_add),
        (StatusCode::CONFLICT, Some("payroll.period_closed"))
    );

    let owner = s.convert(&s.owner, &id, 1, body).await;
    assert_eq!(owner.status, StatusCode::CREATED, "{}", owner.body);
    assert_eq!(
        owner.body["visit"]["business_date"],
        json!((today - chrono::Duration::days(1)).to_string()),
        "the visit keeps its day"
    );
    // The package money changes hands today, like the reversal (Q-E1).
    assert_eq!(
        owner.body["visit"]["package_sales"][0]["business_date"],
        json!(today.to_string())
    );
    Ok(())
}

// ------------------------------------------------------------------------------ payroll Q-U3

/// Two separate credits: "làm" to whoever performs a service (§3.1), the upsell credit to the
/// person credited for the sale (§3.2): packages and one-off services they recommended.
#[sqlx::test(migrations = "../../migrations")]
async fn payroll_credits_upsell_and_performing_separately(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    s.pay(&s.lan, 500).await;
    s.pay(&s.mai, 1000).await;
    let v = s
        .post(
            &s.manager,
            "/visits",
            json!({
                "customer_id": s.ha,
                "lines": [
                    {"service_id": s.shampoo, "source": "package", "package_ref": "p1", "staff_id": s.lan},
                    {"service_id": s.cheaper, "source": "one_off", "staff_id": s.lan, "referrer_staff_id": s.mai},
                ],
                "package_sales": [s.package_sale(Some(&s.mai))],
                "payments": [{"method": "cash", "amount": PLAN_PRICE + CHEAPER}],
            }),
        )
        .await;
    assert_eq!(v.status, StatusCode::CREATED, "{}", v.body);
    // "Khách tự chọn" credits nobody.
    let own_choice = s
        .post(
            &s.manager,
            "/sales",
            json!({"customer_id": s.binh, "closer_staff_id": null, "lines": [{"package_plan_id": s.plan}],
                   "payments": [{"method": "cash", "amount": PLAN_PRICE}]}),
        )
        .await;
    assert_eq!(
        own_choice.status,
        StatusCode::CREATED,
        "{}",
        own_choice.body
    );

    let payroll = s.get(&s.owner, "/payroll").await;
    assert_eq!(payroll.status, StatusCode::OK, "{}", payroll.body);
    let lan = slip(&payroll.body, &s.lan);
    assert_eq!(
        lan["service_commission_base"],
        SESSION_VALUE + CHEAPER,
        "Lan performed both"
    );
    assert_eq!(lan["closing_commission_base"], 0, "Lan sold nothing");
    let mai = slip(&payroll.body, &s.mai);
    assert_eq!(mai["service_commission_base"], 0, "Mai performed nothing");
    assert_eq!(mai["upsell_package_base"], PLAN_PRICE);
    assert_eq!(mai["upsell_service_base"], CHEAPER);
    assert_eq!(mai["closing_commission_base"], PLAN_PRICE + CHEAPER);
    assert_eq!(mai["closing_commission"], (PLAN_PRICE + CHEAPER) / 10);
    assert_eq!(mai["upsell_lines"].as_array().map(Vec::len), Some(2));

    // A void takes the credits back.
    let void = s
        .post(
            &s.manager,
            &format!("/visits/{}/void", str_of(&v.body["id"])),
            json!({"reason": "Ghi nhầm"}),
        )
        .await;
    assert_eq!(void.status, StatusCode::OK, "{}", void.body);
    let payroll = s.get(&s.owner, "/payroll").await;
    let mai = slip(&payroll.body, &s.mai);
    assert_eq!(mai["upsell_service_base"], 0);
    assert_eq!(
        mai["upsell_package_base"], PLAN_PRICE,
        "the package sale stays until voided"
    );
    Ok(())
}

/// Live test 2026-11-07 (ADR-0010): a package cancelled with a refund takes the refunded amount
/// back from its closer's upsell base, in the month of the refund; a cancellation without refund
/// keeps the credit (the spa kept the money).
#[sqlx::test(migrations = "../../migrations")]
async fn a_refund_takes_its_share_of_the_upsell_credit_back(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    s.pay(&s.mai, 700).await;
    // Hà's package is refunded 1.000.000đ, Bình's cancelled without refund.
    for (customer, refund) in [(&s.ha, 1_000_000), (&s.binh, 0)] {
        let sale = s
            .post(
                &s.manager,
                "/sales",
                json!({"customer_id": customer, "closer_staff_id": s.mai,
                       "lines": [{"package_plan_id": s.plan}],
                       "payments": [{"method": "cash", "amount": PLAN_PRICE}]}),
            )
            .await;
        assert_eq!(sale.status, StatusCode::CREATED, "{}", sale.body);
        let package = str_of(&sale.body["customer_packages"][0]["id"]);
        let mut body = json!({"reason": "Khách chuyển nhà", "refund_amount": refund});
        if refund > 0 {
            body["refund_method"] = json!("cash");
        }
        let r = s
            .post(
                &s.owner,
                &format!("/customer-packages/{package}/cancel"),
                body,
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    }

    let payroll = s.get(&s.owner, "/payroll").await;
    assert_eq!(payroll.status, StatusCode::OK, "{}", payroll.body);
    let mai = slip(&payroll.body, &s.mai);
    assert_eq!(
        mai["upsell_package_base"],
        2 * PLAN_PRICE - 1_000_000,
        "both packages, minus the refund"
    );
    assert_eq!(
        mai["closing_commission"],
        (2 * PLAN_PRICE - 1_000_000) * 7 / 100
    );
    let refund_line = mai["upsell_lines"]
        .as_array()
        .and_then(|ls| ls.iter().find(|l| l["kind"] == "package_refund"))
        .cloned()
        .unwrap_or(Value::Null);
    assert_eq!(refund_line["value"], -1_000_000, "{}", mai["upsell_lines"]);
    Ok(())
}

/// Companions on a visit: recorded, edited directly, never charged.
#[sqlx::test(migrations = "../../migrations")]
async fn companion_notes_ride_along_on_a_visit(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let v = s
        .post(
            &s.manager,
            "/visits",
            json!({"customer_id": s.ha, "companions": ["  Gia đình của chị Hà "],
                   "lines": [{"service_id": s.cheaper, "source": "one_off", "staff_id": s.lan}],
                   "payments": [{"method": "cash", "amount": CHEAPER}]}),
        )
        .await;
    assert_eq!(v.status, StatusCode::CREATED, "{}", v.body);
    assert_eq!(v.body["companions"], json!(["Gia đình của chị Hà"]));
    assert_eq!(v.body["total"], CHEAPER, "never charged");
    let edited = call_with_headers(
        &s.h.app,
        "PATCH",
        &format!("/visits/{}", str_of(&v.body["id"])),
        Some(&s.manager),
        Some(json!({"companions": []})),
        &[("If-Match", "\"1\"")],
    )
    .await;
    assert_eq!(edited.status, StatusCode::OK, "{}", edited.body);
    assert_eq!(edited.body["companions"], json!([]));
    let six = s
        .post(
            &s.manager,
            "/visits",
            json!({"customer_id": s.ha, "companions": ["a", "b", "c", "d", "e", "f"],
                   "lines": [{"service_id": s.cheaper, "source": "one_off", "staff_id": s.lan}],
                   "payments": [{"method": "cash", "amount": CHEAPER}]}),
        )
        .await;
    assert_eq!(field(&six).1, Some("companions"));
    Ok(())
}
