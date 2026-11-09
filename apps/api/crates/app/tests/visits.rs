//! S7: recording visits at the front desk (spec R1-1 US4: AC4.1–4.8, 4.10, 4.2b, 4.2c).

mod common;

use axum::http::StatusCode;
use catalog::service::seed_menu;
use common::{
    Harness, OWNER_PHONE, Reply, TEST_DATA_KEY, TestResult, add_member, call, call_with_headers,
    harness, post_customer, session_for,
};
use identity::SystemRole;
use kernel::idempotency::hash_request;
use kernel::testing::app_pool;
use kernel::{Actor, AppError, Permission, TenantId, UserId};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const MANAGER_PHONE: &str = "0912000111";
const TECH_PHONE: &str = "0912000222";

type Setup<T> = Result<T, Box<dyn std::error::Error>>;

struct Desk {
    h: Harness,
    manager: String,
    /// Female customer with a "10 + 2" shampoo package (12/12).
    ha: String,
    /// Male customer, no package.
    duc: String,
    /// Customer without a recorded gender.
    khoi: String,
    lan: String,
    tuan: String,
    shampoo: String,
    dai_thong_lac: String,
    gac_hoi: String,
    plan: String,
}

fn id_of(reply: &Reply) -> String {
    reply.body["id"].as_str().unwrap_or_default().to_owned()
}

fn service_id(services: &Value, name: &str) -> String {
    services
        .as_array()
        .and_then(|l| l.iter().find(|s| s["name"] == name))
        .and_then(|s| s["id"].as_str())
        .unwrap_or_default()
        .to_owned()
}

async fn desk(pool: PgPool) -> Setup<Desk> {
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
        TECH_PHONE,
        "KTV Mai",
        SystemRole::Technician,
    )
    .await?;
    let owner = session_for(&h, OWNER_PHONE).await;
    let manager = session_for(&h, MANAGER_PHONE).await;
    let services = call(&h.app, "GET", "/services", Some(&owner), None)
        .await
        .body;
    let shampoo = service_id(&services, "Cơ Bản – Dầu thảo dược");
    let plan = call(
        &h.app,
        "POST",
        "/package-plans",
        Some(&owner),
        Some(json!({
            "name": "Gội đầu dưỡng sinh 10 buổi + tặng 2", "price": 1_500_000, "validity_months": 6,
            "gift_note": null, "active": true,
            "items": [{"service_id": shampoo, "sessions": 10, "bonus_sessions": 2}],
        })),
    )
    .await;
    assert_eq!(plan.status, StatusCode::CREATED, "{}", plan.body);
    let mut customers = Vec::new();
    for (name, phone, gender) in [
        ("Nguyễn Thị Hà", "0905111222", json!("female")),
        ("Trần Văn Đức", "0905333444", json!("male")),
        ("Phạm Minh Khôi", "0905555666", Value::Null),
    ] {
        let c = post_customer(
            &h.app,
            Some(&manager),
            Some(json!({"full_name": name, "phone": phone, "gender": gender})),
        )
        .await;
        assert_eq!(c.status, StatusCode::CREATED, "{}", c.body);
        customers.push(id_of(&c));
    }
    let lan = org::provisioning::add_staff(&h.owner, "vpyd", "KTV Lan", "Kỹ thuật viên").await?;
    let tuan = org::provisioning::add_staff(&h.owner, "vpyd", "KTV Tuấn", "Kỹ thuật viên").await?;
    let d = Desk {
        manager,
        ha: customers[0].clone(),
        duc: customers[1].clone(),
        khoi: customers[2].clone(),
        lan: lan.to_string(),
        tuan: tuan.to_string(),
        dai_thong_lac: service_id(&services, "Đại Thông Lạc"),
        gac_hoi: service_id(&services, "Giác hơi + Đắp thuốc bắc"),
        shampoo,
        plan: id_of(&plan),
        h,
    };
    d.sell(&d.ha).await?;
    Ok(d)
}

fn code(reply: &Reply) -> (StatusCode, Option<&str>) {
    (reply.status, reply.body["code"].as_str())
}

impl Desk {
    /// Sells the 12-session shampoo package to `customer`; returns (package id, item id).
    async fn sell(&self, customer: &str) -> Setup<(String, String)> {
        let sold = call_with_headers(
            &self.h.app,
            "POST",
            "/sales",
            Some(&self.manager),
            Some(json!({
                "customer_id": customer, "closer_staff_id": self.lan,
                "lines": [{"package_plan_id": self.plan}],
                "payments": [{"method": "cash", "amount": 1_500_000}],
            })),
            &[("idempotency-key", &Uuid::now_v7().to_string())],
        )
        .await;
        assert_eq!(sold.status, StatusCode::CREATED, "{}", sold.body);
        let p = &sold.body["customer_packages"][0];
        Ok((
            p["id"].as_str().unwrap_or_default().to_owned(),
            p["items"][0]["id"].as_str().unwrap_or_default().to_owned(),
        ))
    }

    async fn item_of(&self, customer: &str) -> String {
        let r = self
            .get(&format!("/customers/{customer}/packages"))
            .await
            .body;
        r[0]["items"][0]["id"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    }

    async fn post_with_key(&self, key: &str, body: Value) -> Reply {
        call_with_headers(
            &self.h.app,
            "POST",
            "/visits",
            Some(&self.manager),
            Some(body),
            &[("idempotency-key", key)],
        )
        .await
    }

    async fn record(&self, body: Value) -> Reply {
        self.post_with_key(&Uuid::now_v7().to_string(), body).await
    }

    async fn get(&self, path: &str) -> Reply {
        call(&self.h.app, "GET", path, Some(&self.manager), None).await
    }

    fn deduct(&self, customer: &str, item: Option<&str>) -> Value {
        json!({
            "customer_id": customer,
            "lines": [{"service_id": self.shampoo, "source": "package",
                       "customer_package_item_id": item, "staff_id": self.lan}],
        })
    }

    fn one_off(&self, customer: &str, service: &str, extra: Value, payments: Value) -> Value {
        let mut line = json!({"service_id": service, "source": "one_off", "staff_id": self.lan});
        if let (Some(l), Some(e)) = (line.as_object_mut(), extra.as_object()) {
            l.extend(e.clone());
        }
        json!({"customer_id": customer, "lines": [line], "payments": payments})
    }

    async fn remaining(&self, item: &str) -> Result<i32, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT sessions_remaining FROM app.customer_package_item WHERE id = $1::uuid",
        )
        .bind(item)
        .fetch_one(&self.h.owner)
        .await
    }

    async fn set_remaining(&self, item: &str, n: i32) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE app.customer_package_item SET sessions_remaining = $2 WHERE id = $1::uuid",
        )
        .bind(item)
        .bind(n)
        .execute(&self.h.owner)
        .await
        .map(|_| ())
    }

    /// Moves a package into the past: started 60 days ago, expires `expires_in_days` from today.
    async fn set_expiry(&self, package: &str, expires_in_days: i32) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE app.customer_package SET starts_on = current_date - 60, expires_on = current_date + $2 WHERE id = $1::uuid",
        )
        .bind(package)
        .bind(expires_in_days)
        .execute(&self.h.owner)
        .await
        .map(|_| ())
    }

    async fn count(&self, sql: &str) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
            .fetch_one(&self.h.owner)
            .await
    }
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn deducting_one_session_takes_5_of_12_to_4_of_12(pool: PgPool) -> TestResult {
    let d = desk(pool).await?;
    let item = d.item_of(&d.ha).await;
    d.set_remaining(&item, 5).await?;
    let v = d.record(d.deduct(&d.ha, Some(&item))).await;
    assert_eq!(v.status, StatusCode::CREATED, "{}", v.body);
    assert_eq!(d.remaining(&item).await?, 4, "AC4.1");
    let line = &v.body["lines"][0];
    assert_eq!(
        (
            line["sessions_left_after"].clone(),
            line["sessions_total"].clone()
        ),
        (json!(4), json!(12))
    );
    assert_eq!(line["staff_name"], "KTV Lan");
    // 7 of 10 paid sessions were already used: this one is the 8th paid session, worth 150.000đ.
    assert_eq!(line["line_value"], 150_000);
    assert_eq!(line["unit_price"], 150_000);
    assert_eq!(
        (v.body["total"].clone(), v.body["sale_id"].clone()),
        (json!(0), Value::Null)
    );
    assert!(v.headers.contains_key("etag"));
    assert_eq!(v.body.get("note"), None);

    let today = d.get("/visits").await;
    assert_eq!(today.status, StatusCode::OK);
    assert_eq!(today.body[0]["id"], v.body["id"]);
    assert_eq!(today.body[0]["customer_name"], "Nguyễn Thị Hà");
    let one = d.get(&format!("/visits/{}", id_of(&v))).await;
    assert_eq!(
        one.body["lines"][0]["package_name"],
        "Gội đầu dưỡng sinh 10 buổi + tặng 2"
    );
    let packages = d.get(&format!("/customers/{}/packages", d.ha)).await;
    assert_eq!(packages.body[0]["sessions_remaining"], 4);

    assert_eq!(
        d.count("SELECT count(*) FROM app.payment").await?,
        1,
        "only the package sale"
    );
    assert_eq!(
        d.count("SELECT count(*) FROM app.audit_log WHERE action = 'visit.create'")
            .await?,
        1
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn one_off_service_creates_a_sale_and_payment_tied_to_the_visit(pool: PgPool) -> TestResult {
    let d = desk(pool).await?;
    let item = d.item_of(&d.ha).await;
    let body = json!({
        "customer_id": d.ha,
        "lines": [
            {"service_id": d.shampoo, "source": "package", "customer_package_item_id": item, "staff_id": d.lan},
            {"service_id": d.gac_hoi, "source": "one_off", "staff_id": d.lan},
        ],
        "payments": [{"method": "bank_transfer", "amount": 100_000}],
    });
    let v = d.record(body).await;
    assert_eq!(v.status, StatusCode::CREATED, "{}", v.body);
    assert_eq!(v.body["total"], 100_000, "AC4.2");
    assert_eq!(v.body["payments"][0]["method"], "bank_transfer");
    assert_eq!(v.body["lines"][1]["price_variant"], "all");
    let sale = d
        .get(&format!(
            "/sales/{}",
            v.body["sale_id"].as_str().unwrap_or("")
        ))
        .await;
    assert_eq!(sale.status, StatusCode::OK);
    assert_eq!(
        (
            sale.body["total"].clone(),
            sale.body["lines"][0]["line_type"].clone()
        ),
        (json!(100_000), json!("service"))
    );
    assert_eq!(sale.body["closer_staff_id"], Value::Null);
    // AC4.4: two lines by the same technician are one customer for her that day.
    let per_tech = d
        .count(&format!(
            "SELECT count(DISTINCT visit_id) FROM app.visit_line WHERE staff_id = '{}'",
            d.lan
        ))
        .await?;
    assert_eq!(per_tech, 1);

    let mixed = d
        .record(d.one_off(
            &d.ha,
            &d.gac_hoi,
            json!({"qty": 2}),
            json!([{"method": "cash", "amount": 50_000}, {"method": "bank_transfer", "amount": 150_000}]),
        ))
        .await;
    assert_eq!(mixed.status, StatusCode::CREATED, "{}", mixed.body);
    assert_eq!(mixed.body["payments"].as_array().map(Vec::len), Some(2));
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn one_off_price_follows_the_customers_gender(pool: PgPool) -> TestResult {
    let d = desk(pool).await?;
    // The client's amounts are ignored: the male price comes from the catalog (AC4.2b).
    let mut body = d.one_off(
        &d.duc,
        &d.dai_thong_lac,
        json!({"unit_price": 1}),
        json!([{"method": "cash", "amount": 399_000}]),
    );
    body["total"] = json!(1);
    let male = d.record(body).await;
    assert_eq!(male.status, StatusCode::CREATED, "{}", male.body);
    assert_eq!(male.body["total"], 399_000);
    assert_eq!(male.body["lines"][0]["price_variant"], "male");
    assert_eq!(male.body["lines"][0]["unit_price"], 399_000);

    let female = d
        .record(d.one_off(
            &d.ha,
            &d.dai_thong_lac,
            json!({}),
            json!([{"method": "cash", "amount": 349_000}]),
        ))
        .await;
    assert_eq!(female.status, StatusCode::CREATED, "{}", female.body);
    assert_eq!(female.body["lines"][0]["unit_price"], 349_000);

    let wrong_amount = d
        .record(d.one_off(
            &d.duc,
            &d.dai_thong_lac,
            json!({}),
            json!([{"method": "cash", "amount": 349_000}]),
        ))
        .await;
    assert_eq!(
        code(&wrong_amount),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("payment.amount_mismatch")
        )
    );

    let conflicting = d
        .record(d.one_off(
            &d.duc,
            &d.dai_thong_lac,
            json!({"price_variant": "female"}),
            json!([{"method": "cash", "amount": 349_000}]),
        ))
        .await;
    assert_eq!(
        code(&conflicting),
        (StatusCode::UNPROCESSABLE_ENTITY, Some("validation_failed"))
    );
    assert_eq!(
        conflicting.body["errors"][0]["field"],
        "lines[0].price_variant"
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn customer_without_gender_needs_a_price_variant_for_gendered_services(
    pool: PgPool,
) -> TestResult {
    let d = desk(pool).await?;
    let missing = d
        .record(d.one_off(
            &d.khoi,
            &d.dai_thong_lac,
            json!({}),
            json!([{"method": "cash", "amount": 399_000}]),
        ))
        .await;
    assert_eq!(
        code(&missing),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("price.gender_required")
        )
    );
    let chosen = d
        .record(d.one_off(
            &d.khoi,
            &d.dai_thong_lac,
            json!({"price_variant": "male"}),
            json!([{"method": "cash", "amount": 399_000}]),
        ))
        .await;
    assert_eq!(chosen.status, StatusCode::CREATED, "{}", chosen.body);
    // F2 (live test 2026-11-08): the gender picked to price the visit is now the profile's, so the
    // next visit needs no pick, and a different one is refused rather than overwriting it.
    let profile = d.get(&format!("/customers/{}", d.khoi)).await;
    assert_eq!(profile.body["gender"], "male", "{}", profile.body);
    assert_eq!(chosen.body["customer_gender"], "male");
    let audited = d
        .count("SELECT count(*) FROM app.audit_log WHERE action = 'customer.update' AND after->>'gender' = 'male'")
        .await?;
    assert_eq!(audited, 1);
    let again = d
        .record(d.one_off(
            &d.khoi,
            &d.dai_thong_lac,
            json!({}),
            json!([{"method": "cash", "amount": 399_000}]),
        ))
        .await;
    assert_eq!(again.status, StatusCode::CREATED, "{}", again.body);
    let other = d
        .record(d.one_off(
            &d.khoi,
            &d.dai_thong_lac,
            json!({"price_variant": "female"}),
            json!([{"method": "cash", "amount": 349_000}]),
        ))
        .await;
    assert_eq!(
        other.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        other.body
    );
    let profile = d.get(&format!("/customers/{}", d.khoi)).await;
    assert_eq!(profile.body["gender"], "male", "never overwritten");
    let flat = d
        .record(d.one_off(
            &d.khoi,
            &d.gac_hoi,
            json!({}),
            json!([{"method": "cash", "amount": 100_000}]),
        ))
        .await;
    assert_eq!(
        flat.status,
        StatusCode::CREATED,
        "flat price needs no gender: {}",
        flat.body
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn female_only_service_for_a_male_customer_is_422(pool: PgPool) -> TestResult {
    let d = desk(pool).await?;
    // The owner prices Ôn ấm tử cung for women only (AC4.2c).
    let service: Uuid = sqlx::query_scalar(
        "UPDATE app.service SET active = true WHERE name = 'Ôn ấm tử cung' RETURNING id",
    )
    .fetch_one(&d.h.owner)
    .await?;
    sqlx::query(
        "INSERT INTO app.service_price (tenant_id, service_id, variant, amount)
         SELECT tenant_id, id, 'female', 399000 FROM app.service WHERE id = $1",
    )
    .bind(service)
    .execute(&d.h.owner)
    .await?;
    let service = service.to_string();
    let male = d
        .record(d.one_off(
            &d.duc,
            &service,
            json!({}),
            json!([{"method": "cash", "amount": 399_000}]),
        ))
        .await;
    assert_eq!(
        code(&male),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("price.variant_not_available")
        )
    );
    let female = d
        .record(d.one_off(
            &d.ha,
            &service,
            json!({}),
            json!([{"method": "cash", "amount": 399_000}]),
        ))
        .await;
    assert_eq!(female.status, StatusCode::CREATED, "{}", female.body);
    assert_eq!(d.count("SELECT count(*) FROM app.visit").await?, 1);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn default_package_is_the_one_expiring_soonest(pool: PgPool) -> TestResult {
    let d = desk(pool).await?;
    let (first, first_item) = (
        d.get(&format!("/customers/{}/packages", d.ha)).await.body[0]["id"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        d.item_of(&d.ha).await,
    );
    let (second, second_item) = d.sell(&d.ha).await?;
    // The newer package expires first, so it is used first (AC4.3).
    d.set_expiry(&second, 10).await?;
    d.set_expiry(&first, 100).await?;
    let v = d.record(d.deduct(&d.ha, None)).await;
    assert_eq!(v.status, StatusCode::CREATED, "{}", v.body);
    assert_eq!(
        v.body["lines"][0]["customer_package_id"].as_str(),
        Some(second.as_str())
    );
    assert_eq!(
        (
            d.remaining(&second_item).await?,
            d.remaining(&first_item).await?
        ),
        (11, 12)
    );

    // Once it is used up, the next one is chosen.
    d.set_remaining(&second_item, 0).await?;
    let next = d.record(d.deduct(&d.ha, None)).await;
    assert_eq!(
        next.body["lines"][0]["customer_package_id"].as_str(),
        Some(first.as_str())
    );

    // Nothing usable left → 422; the desk then sells a new package.
    d.set_remaining(&first_item, 0).await?;
    let none = d.record(d.deduct(&d.ha, None)).await;
    assert_eq!(
        code(&none),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("package.item_not_found")
        )
    );
    Ok(())
}

/// AC4.3 with quantities: the default pick skips a package that cannot cover the line, counting
/// what earlier lines of the same checkout take. Red before: the soonest-expiring package with
/// 1 session left was picked for qty 2 (and for a second line), and the visit failed
/// `409 package.no_sessions_left` although the other package had sessions.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn default_package_has_enough_sessions_for_the_line(pool: PgPool) -> TestResult {
    let d = desk(pool).await?;
    let (later, later_item) = (
        d.get(&format!("/customers/{}/packages", d.ha)).await.body[0]["id"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        d.item_of(&d.ha).await,
    );
    let (soon, soon_item) = d.sell(&d.ha).await?;
    d.set_expiry(&soon, 10).await?;
    d.set_expiry(&later, 100).await?;
    d.set_remaining(&soon_item, 1).await?;

    let line = |qty: u32| json!({"service_id": d.shampoo, "source": "package", "staff_id": d.lan, "qty": qty});
    let two = d
        .record(json!({"customer_id": d.ha, "lines": [line(2)]}))
        .await;
    assert_eq!(two.status, StatusCode::CREATED, "{}", two.body);
    assert_eq!(
        two.body["lines"][0]["customer_package_id"].as_str(),
        Some(later.as_str())
    );
    assert_eq!(d.remaining(&later_item).await?, 10);

    let pair = d
        .record(json!({"customer_id": d.ha, "lines": [line(1), line(1)]}))
        .await;
    assert_eq!(pair.status, StatusCode::CREATED, "{}", pair.body);
    let used: Vec<Option<&str>> = (0..2)
        .map(|i| pair.body["lines"][i]["customer_package_id"].as_str())
        .collect();
    assert_eq!(used, [Some(soon.as_str()), Some(later.as_str())]);
    assert_eq!(
        (
            d.remaining(&soon_item).await?,
            d.remaining(&later_item).await?
        ),
        (0, 9)
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn exhausted_and_cancelled_packages_are_409(pool: PgPool) -> TestResult {
    let d = desk(pool).await?;
    let item = d.item_of(&d.ha).await;
    d.set_remaining(&item, 0).await?;
    let empty = d.record(d.deduct(&d.ha, Some(&item))).await;
    assert_eq!(
        code(&empty),
        (StatusCode::CONFLICT, Some("package.no_sessions_left")),
        "AC4.5"
    );

    d.set_remaining(&item, 3).await?;
    sqlx::query(
        // A cancellation is complete only with who did it (0009). Sessions are left at 3 to prove
        // the consume refuses a cancelled package on its own, not because the counter is empty.
        "UPDATE app.customer_package SET cancelled_at = now(), cancel_reason = 'hoàn tiền', cancelled_by = created_by",
    )
    .execute(&d.h.owner)
    .await?;
    let cancelled = d.record(d.deduct(&d.ha, Some(&item))).await;
    assert_eq!(
        code(&cancelled),
        (StatusCode::CONFLICT, Some("package.cancelled")),
        "AC4.10"
    );
    assert_eq!(d.remaining(&item).await?, 3);
    assert_eq!(d.count("SELECT count(*) FROM app.visit").await?, 0);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn expired_package_needs_a_reason_and_the_override_permission(pool: PgPool) -> TestResult {
    let d = desk(pool).await?;
    let package = d.get(&format!("/customers/{}/packages", d.ha)).await.body[0]["id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let item = d.item_of(&d.ha).await;
    d.set_expiry(&package, -1).await?;
    let blocked = d.record(d.deduct(&d.ha, Some(&item))).await;
    assert_eq!(
        code(&blocked),
        (StatusCode::CONFLICT, Some("package.expired")),
        "AC4.6"
    );
    // The default never picks an expired package.
    let default = d.record(d.deduct(&d.ha, None)).await;
    assert_eq!(
        code(&default),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("package.item_not_found")
        )
    );

    let mut body = d.deduct(&d.ha, Some(&item));
    body["lines"][0]["expiry_override_reason"] = json!("Khách ốm nằm viện 2 tháng");
    let allowed = d.record(body.clone()).await;
    assert_eq!(allowed.status, StatusCode::CREATED, "{}", allowed.body);
    assert_eq!(
        allowed.body["lines"][0]["expiry_override_reason"],
        "Khách ốm nằm viện 2 tháng"
    );
    assert_eq!(d.remaining(&item).await?, 11);
    let audited: Option<String> = sqlx::query_scalar(
        "SELECT reason FROM app.audit_log WHERE action = 'customer_package.use_expired_override'",
    )
    .fetch_optional(&d.h.owner)
    .await?;
    assert_eq!(audited.as_deref(), Some("Khách ốm nằm viện 2 tháng"));

    // Without `package.use_expired_override` the reason does not help.
    let tenant: Uuid = sqlx::query_scalar("SELECT id FROM app.tenant WHERE slug = 'vpyd'")
        .fetch_one(&d.h.owner)
        .await?;
    let clerk = Actor::new(
        UserId::new(),
        TenantId::from_uuid(tenant),
        [Permission::VisitRecord],
        None,
    );
    let input: operations::dto::VisitCreate = serde_json::from_value(body)?;
    let cipher = crm::FieldCipher::from_base64(TEST_DATA_KEY)?;
    let denied = operations::service::record(
        &app_pool(&d.h.owner).await?,
        &cipher,
        &clerk,
        Uuid::now_v7(),
        hash_request(b"x"),
        &input,
    )
    .await;
    assert!(
        matches!(
            denied,
            Err(AppError::Forbidden {
                code: "auth.forbidden"
            })
        ),
        "{denied:?}"
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn two_devices_racing_for_the_last_session_exactly_one_wins(pool: PgPool) -> TestResult {
    let d = desk(pool).await?;
    let item = d.item_of(&d.ha).await;
    d.set_remaining(&item, 1).await?;
    let body = d.deduct(&d.ha, Some(&item));
    let (a, b) = tokio::join!(d.record(body.clone()), d.record(body.clone()));
    let mut statuses = [a.status, b.status];
    statuses.sort();
    assert_eq!(
        statuses,
        [StatusCode::CREATED, StatusCode::CONFLICT],
        "AC4.7: {} / {}",
        a.body,
        b.body
    );
    let loser = if a.status == StatusCode::CONFLICT {
        &a
    } else {
        &b
    };
    assert_eq!(loser.body["code"], "package.no_sessions_left");
    assert_eq!(d.remaining(&item).await?, 0, "never negative");
    assert_eq!(d.count("SELECT count(*) FROM app.visit").await?, 1);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn double_submit_with_the_same_key_records_one_visit(pool: PgPool) -> TestResult {
    let d = desk(pool).await?;
    let item = d.item_of(&d.ha).await;
    let key = Uuid::now_v7().to_string();
    let body = d.deduct(&d.ha, Some(&item));
    let (a, b) = tokio::join!(
        d.post_with_key(&key, body.clone()),
        d.post_with_key(&key, body.clone())
    );
    assert_eq!(
        (a.status, b.status),
        (StatusCode::CREATED, StatusCode::CREATED),
        "{} / {}",
        a.body,
        b.body
    );
    assert_eq!(a.body["id"], b.body["id"]);
    let again = d.post_with_key(&key, body).await;
    assert_eq!(again.body["id"], a.body["id"]);
    assert_eq!(d.remaining(&item).await?, 11, "consumed once");
    assert_eq!(d.count("SELECT count(*) FROM app.visit").await?, 1);

    let different = d.post_with_key(&key, d.deduct(&d.duc, None)).await;
    assert_eq!(
        code(&different),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("idempotency.key_reuse_mismatch")
        )
    );
    let no_key = call(
        &d.h.app,
        "POST",
        "/visits",
        Some(&d.manager),
        Some(d.deduct(&d.ha, None)),
    )
    .await;
    assert_eq!(
        code(&no_key),
        (StatusCode::BAD_REQUEST, Some("idempotency.key_required"))
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn technician_must_be_working_and_package_must_be_the_customers(pool: PgPool) -> TestResult {
    let d = desk(pool).await?;
    let item = d.item_of(&d.ha).await;
    let mut no_tech = d.deduct(&d.ha, Some(&item));
    no_tech["lines"][0]["staff_id"] = Value::Null;
    let missing = d.record(no_tech).await;
    assert_eq!(
        code(&missing),
        (StatusCode::UNPROCESSABLE_ENTITY, Some("validation_failed"))
    );
    assert_eq!(missing.body["errors"][0]["field"], "lines[0].staff_id");

    sqlx::query("UPDATE app.staff SET status = 'left', left_on = current_date WHERE id = $1::uuid")
        .bind(&d.tuan)
        .execute(&d.h.owner)
        .await?;
    let mut left = d.deduct(&d.ha, Some(&item));
    left["lines"][0]["staff_id"] = json!(d.tuan);
    assert_eq!(
        code(&d.record(left).await),
        (StatusCode::UNPROCESSABLE_ENTITY, Some("staff.inactive"))
    );
    let mut stranger = d.deduct(&d.ha, Some(&item));
    stranger["lines"][0]["staff_id"] = json!(Uuid::now_v7());
    assert_eq!(
        code(&d.record(stranger).await),
        (StatusCode::UNPROCESSABLE_ENTITY, Some("staff.inactive"))
    );

    // Đức tries to use Hà's package (AC4.8).
    let foreign = d.record(d.deduct(&d.duc, Some(&item))).await;
    assert_eq!(
        code(&foreign),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("package.item_not_found")
        )
    );
    // A package item that does not cover the line's service.
    let mut wrong_service = d.deduct(&d.ha, Some(&item));
    wrong_service["lines"][0]["service_id"] = json!(d.gac_hoi);
    let mismatch = d.record(wrong_service).await;
    assert_eq!(mismatch.body["errors"][0]["code"], "not_in_package");
    assert_eq!(d.remaining(&item).await?, 12, "every failure rolled back");
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn check_in_must_be_now_or_earlier_today(pool: PgPool) -> TestResult {
    let d = desk(pool).await?;
    let mut future = d.deduct(&d.ha, None);
    future["check_in_at"] =
        json!((chrono::Utc::now() + chrono::Duration::minutes(30)).to_rfc3339());
    assert_eq!(
        code(&d.record(future).await),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("visit.check_in_in_future")
        )
    );
    let mut old = d.deduct(&d.ha, None);
    old["check_in_at"] = json!((chrono::Utc::now() - chrono::Duration::days(2)).to_rfc3339());
    assert_eq!(
        code(&d.record(old).await),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("visit.check_in_not_today")
        )
    );
    let mut earlier = d.deduct(&d.ha, None);
    // A minute ago, but never before today's midnight (in the first minute of the day "a minute
    // ago" is yesterday, refused as `check_in_not_today`).
    let today = chrono::Utc::now()
        .with_timezone(&chrono_tz::Asia::Ho_Chi_Minh)
        .date_naive();
    let midnight = chrono::DateTime::parse_from_rfc3339(&format!("{today}T00:00:00+07:00"))?
        .with_timezone(&chrono::Utc);
    let at = (chrono::Utc::now() - chrono::Duration::minutes(1)).max(midnight);
    earlier["check_in_at"] = json!(at.to_rfc3339());
    let ok = d.record(earlier).await;
    assert_eq!(ok.status, StatusCode::CREATED, "{}", ok.body);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn payments_on_a_package_only_visit_are_rejected(pool: PgPool) -> TestResult {
    let d = desk(pool).await?;
    let mut body = d.deduct(&d.ha, None);
    body["payments"] = json!([{"method": "cash", "amount": 1}]);
    assert_eq!(
        code(&d.record(body).await),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("payment.amount_mismatch")
        )
    );
    let unpaid = d
        .record(d.one_off(&d.ha, &d.gac_hoi, json!({}), json!([])))
        .await;
    assert_eq!(
        code(&unpaid),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("payment.amount_mismatch")
        )
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn session_note_is_encrypted_and_shown_only_to_health_readers(pool: PgPool) -> TestResult {
    let d = desk(pool).await?;
    let mut body = d.deduct(&d.ha, None);
    body["note"] = json!("Đỏ da vùng cổ sau buổi");
    let v = d.record(body).await;
    assert_eq!(v.status, StatusCode::CREATED, "{}", v.body);
    assert_eq!(
        v.body.get("note"),
        None,
        "the stored response never holds the note"
    );
    let one = d.get(&format!("/visits/{}", id_of(&v))).await;
    assert_eq!(one.body["note"], "Đỏ da vùng cổ sau buổi");
    // EM 2026-11-07: the note is seen in the day list and in the customer's history too.
    let list = d.get("/visits").await;
    assert_eq!(list.body[0]["note"], "Đỏ da vùng cổ sau buổi");
    let history = d.get(&format!("/customers/{}/history", d.ha)).await;
    assert_eq!(history.status, StatusCode::OK, "{}", history.body);
    // Found by kind: same-day order mixes the API and database clocks (backend §3b).
    let visit_event = history.body["events"]
        .as_array()
        .and_then(|events| events.iter().find(|e| e["kind"] == "visit"))
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        visit_event["visit"]["note"], "Đỏ da vùng cổ sau buổi",
        "{}",
        history.body
    );

    let leaks = d
        .count(
            "SELECT (SELECT count(*) FROM app.audit_log WHERE after::text LIKE '%Đỏ da%')
                  + (SELECT count(*) FROM app.idempotency_record WHERE response_body::text LIKE '%Đỏ da%')",
        )
        .await?;
    assert_eq!(leaks, 0);
    let ct: Option<Vec<u8>> = sqlx::query_scalar("SELECT note_ct FROM app.visit")
        .fetch_one(&d.h.owner)
        .await?;
    let ct = ct.unwrap_or_default();
    assert!(!ct.is_empty());
    assert!(!String::from_utf8_lossy(&ct).contains("Đỏ da"));

    // A role without health-note access gets no key at all.
    let tenant: Uuid = sqlx::query_scalar("SELECT id FROM app.tenant WHERE slug = 'vpyd'")
        .fetch_one(&d.h.owner)
        .await?;
    let reader = Actor::new(
        UserId::new(),
        TenantId::from_uuid(tenant),
        [Permission::VisitRead],
        None,
    );
    let cipher = crm::FieldCipher::from_base64(TEST_DATA_KEY)?;
    let app = app_pool(&d.h.owner).await?;
    let seen =
        operations::service::get(&app, &cipher, &reader, Uuid::parse_str(&id_of(&v))?).await?;
    assert_eq!(seen.note, None);
    let day = operations::service::for_day(&app, &cipher, &reader, None).await?;
    assert_eq!(day.first().map(|v| v.note.clone()), Some(None));
    let history_reader = Actor::new(
        UserId::new(),
        TenantId::from_uuid(tenant),
        [
            Permission::CustomerRead,
            Permission::VisitRead,
            Permission::SaleRead,
        ],
        None,
    );
    let page = reporting::customer::history(
        &app,
        &cipher,
        &history_reader,
        Uuid::parse_str(&d.ha)?,
        &reporting::dto::HistoryQuery {
            q: None,
            kind: None,
            from: None,
            to: None,
            cursor: None,
            limit: None,
        },
    )
    .await?;
    let visit = page.events.first().and_then(|e| e.visit.as_ref());
    assert!(visit.is_some(), "the visit is on the timeline");
    assert_eq!(visit.and_then(|v| v.note.clone()), None);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn corrections_change_technician_and_note_with_if_match(pool: PgPool) -> TestResult {
    let d = desk(pool).await?;
    let v = d.record(d.deduct(&d.ha, None)).await;
    let id = id_of(&v);
    let line = v.body["lines"][0]["id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let patch = |version: &str, body: Value| {
        let app = d.h.app.clone();
        let manager = d.manager.clone();
        let path = format!("/visits/{id}");
        let header = format!("\"{version}\"");
        async move {
            call_with_headers(
                &app,
                "PATCH",
                &path,
                Some(&manager),
                Some(body),
                &[("If-Match", &header)],
            )
            .await
        }
    };
    let fixed = patch(
        "1",
        json!({"lines": [{"id": line, "staff_id": d.tuan}], "note": "Khách thích lực nhẹ"}),
    )
    .await;
    assert_eq!(fixed.status, StatusCode::OK, "{}", fixed.body);
    assert_eq!(fixed.body["lines"][0]["staff_name"], "KTV Tuấn");
    assert_eq!(fixed.body["note"], "Khách thích lực nhẹ");
    assert_eq!(fixed.body["version"], 2);
    assert_eq!(
        d.remaining(&d.item_of(&d.ha).await).await?,
        11,
        "sessions untouched"
    );

    let stale = patch("1", json!({"note": null})).await;
    assert_eq!(stale.status, StatusCode::PRECONDITION_FAILED);
    let cleared = patch("2", json!({"note": null})).await;
    assert_eq!(cleared.body["note"], Value::Null);

    sqlx::query("UPDATE app.staff SET status = 'left', left_on = current_date WHERE id = $1::uuid")
        .bind(&d.lan)
        .execute(&d.h.owner)
        .await?;
    let gone = patch("3", json!({"lines": [{"id": line, "staff_id": d.lan}]})).await;
    assert_eq!(
        code(&gone),
        (StatusCode::UNPROCESSABLE_ENTITY, Some("staff.inactive"))
    );
    let unknown_line = patch(
        "3",
        json!({"lines": [{"id": Uuid::now_v7(), "staff_id": d.tuan}]}),
    )
    .await;
    assert_eq!(unknown_line.body["errors"][0]["field"], "lines[0].id");
    let no_if_match = call(
        &d.h.app,
        "PATCH",
        &format!("/visits/{id}"),
        Some(&d.manager),
        Some(json!({})),
    )
    .await;
    assert_eq!(
        code(&no_if_match),
        (
            StatusCode::BAD_REQUEST,
            Some("precondition.if_match_required")
        )
    );

    // Yesterday's visit: a manager may no longer correct it.
    sqlx::query("UPDATE app.visit SET business_date = business_date - 1")
        .execute(&d.h.owner)
        .await?;
    let locked = patch("3", json!({"note": "x"})).await;
    assert_eq!(code(&locked), (StatusCode::FORBIDDEN, Some("day.locked")));
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn technician_role_cannot_record_or_read_visits(pool: PgPool) -> TestResult {
    let d = desk(pool).await?;
    let tech = session_for(&d.h, TECH_PHONE).await;
    let r = call_with_headers(
        &d.h.app,
        "POST",
        "/visits",
        Some(&tech),
        Some(d.deduct(&d.ha, None)),
        &[("idempotency-key", &Uuid::now_v7().to_string())],
    )
    .await;
    assert_eq!(code(&r), (StatusCode::FORBIDDEN, Some("auth.forbidden")));
    let list = call(&d.h.app, "GET", "/visits", Some(&tech), None).await;
    assert_eq!(list.status, StatusCode::FORBIDDEN);
    // F17 (live test 2026-11-08): the right is checked before the query, so a malformed query
    // tells someone without it nothing about the parameters (it used to be 400 first).
    for path in [
        "/visits?limit=abc",
        "/appointments?from=not-a-day",
        "/notes?kind=nope",
        "/sales?date=x",
    ] {
        let r = call(&d.h.app, "GET", path, Some(&tech), None).await;
        assert_eq!(r.status, StatusCode::FORBIDDEN, "{path}: {}", r.body);
    }
    // A caller who has the right still hears what is wrong with the query.
    let bad = call(&d.h.app, "GET", "/visits?limit=abc", Some(&d.manager), None).await;
    assert_eq!(
        code(&bad),
        (StatusCode::BAD_REQUEST, Some("query.invalid")),
        "{}",
        bad.body
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn day_list_can_be_limited_to_the_newest_visits(pool: PgPool) -> TestResult {
    let d = desk(pool).await?;
    for _ in 0..3 {
        let v = d.record(d.deduct(&d.ha, None)).await;
        assert_eq!(v.status, StatusCode::CREATED, "{}", v.body);
    }
    let all = d.get("/visits").await;
    assert_eq!(all.body.as_array().map(Vec::len), Some(3));
    let two = d.get("/visits?limit=2").await;
    assert_eq!(two.status, StatusCode::OK, "{}", two.body);
    let got: Vec<Value> = two.body.as_array().cloned().unwrap_or_default();
    assert_eq!(got.len(), 2);
    // Same order as the full list: newest first, so the cut keeps the latest visits.
    assert_eq!(
        (got[0]["id"].clone(), got[1]["id"].clone()),
        (all.body[0]["id"].clone(), all.body[1]["id"].clone())
    );
    let more = d.get("/visits?limit=500").await;
    assert_eq!(more.body.as_array().map(Vec::len), Some(3));
    for bad in ["/visits?limit=0", "/visits?limit=501"] {
        let r = d.get(bad).await;
        assert_eq!(
            r.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{bad}: {}",
            r.body
        );
    }
    Ok(())
}
