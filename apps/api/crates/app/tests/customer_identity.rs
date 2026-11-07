//! Customer identity (EM 2026-11-07, ADR-0011, spec R1-1 §10.11): an anonymous walk-in profile
//! ("Khách vãng lai"), the phone optional on every profile (decision E), an optional date of
//! birth, packages owned only by named customers, the beneficiary in the ledger and timelines,
//! and "Gia hạn" on the timeline.

mod common;

use axum::http::StatusCode;
use catalog::service::seed_menu;
use chrono::{Datelike, Duration, FixedOffset, NaiveDate, Utc};
use common::{
    Harness, OWNER_PHONE, Reply, TestResult, add_member, call, call_with_headers, harness,
    session_for,
};
use identity::SystemRole;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const MANAGER_PHONE: &str = "0912000111";
const PLAN_PRICE: i64 = 1_500_000;
const SHAMPOO: i64 = 49_000;

type Setup<T> = Result<T, Box<dyn std::error::Error>>;

struct Spa {
    h: Harness,
    manager: String,
    owner: String,
    staff: String,
    shampoo: String,
    plan: String,
}

fn str_of(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_owned()
}

fn first_error(reply: &Reply) -> (StatusCode, Option<&str>, Option<&str>) {
    (
        reply.status,
        reply.body["errors"][0]["field"].as_str(),
        reply.body["errors"][0]["code"].as_str(),
    )
}

fn spa_today() -> NaiveDate {
    let vn = FixedOffset::east_opt(7 * 3600).unwrap_or_else(|| panic!("offset"));
    Utc::now().with_timezone(&vn).date_naive()
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
    let owner = session_for(&h, OWNER_PHONE).await;
    let manager = session_for(&h, MANAGER_PHONE).await;
    let shampoo: Uuid = sqlx::query_scalar(
        "SELECT s.id FROM app.service s JOIN app.service_category c ON c.id = s.category_id
         WHERE c.name = 'Gội đầu dưỡng sinh' AND s.name = 'Cơ Bản – Dầu thảo dược'",
    )
    .fetch_one(&h.owner)
    .await?;
    let staff = org::provisioning::add_staff(&h.owner, "vpyd", "KTV Lan", "Kỹ thuật viên").await?;
    let plan = call(
        &h.app,
        "POST",
        "/package-plans",
        Some(&owner),
        Some(json!({
            "name": "Gội đầu 10 buổi", "price": PLAN_PRICE, "validity_months": 6,
            "gift_note": null, "active": true,
            "items": [{"service_id": shampoo, "sessions": 10, "bonus_sessions": 0}],
        })),
    )
    .await;
    assert_eq!(plan.status, StatusCode::CREATED, "{}", plan.body);
    Ok(Spa {
        manager,
        owner,
        staff: staff.to_string(),
        shampoo: shampoo.to_string(),
        plan: str_of(&plan.body["id"]),
        h,
    })
}

impl Spa {
    async fn post(&self, path: &str, body: Value) -> Reply {
        let key = Uuid::now_v7().to_string();
        call_with_headers(
            &self.h.app,
            "POST",
            path,
            Some(&self.manager),
            Some(body),
            &[("idempotency-key", &key)],
        )
        .await
    }

    /// As the owner: the ledger needs `report.revenue.read`.
    async fn get(&self, path: &str) -> Reply {
        call(&self.h.app, "GET", path, Some(&self.owner), None).await
    }

    async fn customer(&self, body: Value) -> Reply {
        call(
            &self.h.app,
            "POST",
            "/customers",
            Some(&self.manager),
            Some(body),
        )
        .await
    }

    async fn put_customer(&self, id: &str, version: i64, body: Value) -> Reply {
        let etag = format!("\"{version}\"");
        call_with_headers(
            &self.h.app,
            "PUT",
            &format!("/customers/{id}"),
            Some(&self.manager),
            Some(body),
            &[("If-Match", &etag)],
        )
        .await
    }

    fn package_sale(&self, payer: &str, beneficiary: Option<&str>) -> Value {
        json!({"customer_id": payer, "beneficiary_customer_id": beneficiary, "closer_staff_id": null,
               "lines": [{"package_plan_id": self.plan}],
               "payments": [{"method": "cash", "amount": PLAN_PRICE}]})
    }
}

/// EM 2026-11-07: "khách vãng lai mà vẫn phải nhập tên với nhập số điện thoại lỡ đâu người ta
/// không muốn thì sao" — a visit is recorded for a profile with neither.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn an_anonymous_walk_in_is_recorded_without_name_or_phone(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let walk_in = s.customer(json!({})).await;
    assert_eq!(walk_in.status, StatusCode::CREATED, "{}", walk_in.body);
    assert_eq!(
        (
            walk_in.body["full_name"].clone(),
            walk_in.body["named"].clone(),
            walk_in.body["phone"].clone(),
            walk_in.body["phone_e164"].clone()
        ),
        (
            json!("Khách vãng lai"),
            json!(false),
            Value::Null,
            Value::Null
        )
    );
    // Blank strings mean the same as nothing.
    let blank = s.customer(json!({"full_name": "  ", "phone": " "})).await;
    assert_eq!(blank.status, StatusCode::CREATED, "{}", blank.body);
    assert_eq!(blank.body["named"], false);
    let id = str_of(&walk_in.body["id"]);

    let visit = s
        .post(
            "/visits",
            json!({"customer_id": id, "lines": [{"service_id": s.shampoo, "source": "one_off",
                   "staff_id": s.staff}], "payments": [{"method": "cash", "amount": SHAMPOO}]}),
        )
        .await;
    assert_eq!(visit.status, StatusCode::CREATED, "{}", visit.body);
    assert_eq!(visit.body["customer_name"], "Khách vãng lai");

    // Reports and the ledger count the visit and label the customer.
    let day = s.get("/reports/daily").await;
    assert_eq!(day.status, StatusCode::OK, "{}", day.body);
    assert_eq!(day.body["counts"]["visits"], 1, "{}", day.body);
    let ledger = s.get("/reports/ledger").await;
    assert_eq!(ledger.status, StatusCode::OK, "{}", ledger.body);
    assert_eq!(ledger.body["rows"][0]["customer_name"], "Khách vãng lai");
    let history = s.get(&format!("/customers/{id}/history")).await;
    assert_eq!(history.status, StatusCode::OK, "{}", history.body);
    assert_eq!(history.body["events"][0]["kind"], "visit");

    // Not in the recent list (the desk could not tell them apart), never in the call-back list
    // (nobody to call). A named customer without a phone is on it, with `phone: null`.
    let hoa = s.customer(json!({"full_name": "Chị Hoa"})).await;
    let hoa_id = str_of(&hoa.body["id"]);
    let hoa_visit = s
        .post(
            "/visits",
            json!({"customer_id": hoa_id, "lines": [{"service_id": s.shampoo, "source": "one_off",
                   "staff_id": s.staff}], "payments": [{"method": "cash", "amount": SHAMPOO}]}),
        )
        .await;
    assert_eq!(hoa_visit.status, StatusCode::CREATED, "{}", hoa_visit.body);
    let recent = s.get("/customers").await;
    assert_eq!(
        recent.body.as_array().map(Vec::len),
        Some(1),
        "{}",
        recent.body
    );
    assert_eq!(recent.body[0]["id"], json!(hoa_id));
    // Both last came 40 days ago.
    sqlx::raw_sql(
        "ALTER TABLE app.visit DISABLE TRIGGER USER; ALTER TABLE app.sale DISABLE TRIGGER USER;
         UPDATE app.visit SET business_date = business_date - 40;
         UPDATE app.sale SET business_date = business_date - 40;
         ALTER TABLE app.visit ENABLE TRIGGER USER; ALTER TABLE app.sale ENABLE TRIGGER USER;",
    )
    .execute(&s.h.owner)
    .await?;
    let inactive = s.get("/customers/inactive?days=30").await;
    assert_eq!(inactive.status, StatusCode::OK, "{}", inactive.body);
    let listed: Vec<&Value> = inactive.body["items"]
        .as_array()
        .map(|items| items.iter().map(|c| &c["customer_id"]).collect())
        .unwrap_or_default();
    assert_eq!(listed, vec![&json!(hoa_id)], "{}", inactive.body);
    assert_eq!(inactive.body["items"][0]["phone"], Value::Null);
    Ok(())
}

/// A package needs an owner the spa can tell apart; the anonymous walk-in may still pay for a
/// named person, and gets packages once a name is added. A package owner keeps their name.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn only_a_named_customer_owns_a_package(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let walk_in = s.customer(json!({})).await;
    let id = str_of(&walk_in.body["id"]);
    let refused = s.post("/sales", s.package_sale(&id, None)).await;
    assert_eq!(
        (refused.status, refused.body["code"].as_str()),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("customer.name_required")
        ),
        "{}",
        refused.body
    );
    let cart = s
        .post(
            "/visits",
            json!({"customer_id": id, "lines": [], "payments": [{"method": "cash", "amount": PLAN_PRICE}],
                   "package_sales": [{"ref": "p1", "package_plan_id": s.plan, "closer_staff_id": null}]}),
        )
        .await;
    assert_eq!(
        (cart.status, cart.body["code"].as_str()),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("customer.name_required")
        ),
        "the visit checkout follows the same rule: {}",
        cart.body
    );

    let binh = s.customer(json!({"full_name": "Trần Thị Bình"})).await;
    assert_eq!(
        binh.status,
        StatusCode::CREATED,
        "a name without a phone: {}",
        binh.body
    );
    let binh_id = str_of(&binh.body["id"]);
    let gift = s.post("/sales", s.package_sale(&id, Some(&binh_id))).await;
    assert_eq!(
        gift.status,
        StatusCode::CREATED,
        "anonymous payer, named owner: {}",
        gift.body
    );

    let named = s
        .put_customer(&id, 1, json!({"full_name": "Chị Hoa", "phone": null}))
        .await;
    assert_eq!(named.status, StatusCode::OK, "{}", named.body);
    assert_eq!(named.body["named"], true);
    let own = s.post("/sales", s.package_sale(&id, None)).await;
    assert_eq!(own.status, StatusCode::CREATED, "{}", own.body);

    let unnamed = s.put_customer(&id, 2, json!({"full_name": ""})).await;
    assert_eq!(
        (unnamed.status, unnamed.body["code"].as_str()),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("customer.name_required")
        ),
        "{}",
        unnamed.body
    );
    Ok(())
}

/// Decision E: no phone is fine for any number of profiles; a phone, when given, is still
/// format-checked and unique.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn the_phone_is_optional_but_still_checked_and_unique(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    for _ in 0..2 {
        let no_phone = s.customer(json!({"full_name": "Chồng của chị Hà"})).await;
        assert_eq!(no_phone.status, StatusCode::CREATED, "{}", no_phone.body);
        assert_eq!(no_phone.body["phone"], Value::Null);
    }
    let ha = s
        .customer(json!({"full_name": "Nguyễn Thị Hà", "phone": "0905111222"}))
        .await;
    assert_eq!(ha.status, StatusCode::CREATED, "{}", ha.body);
    let dup = s.customer(json!({"phone": "+84 905 111 222"})).await;
    assert_eq!(
        (
            dup.status,
            dup.body["code"].as_str(),
            dup.body["existing_id"].clone()
        ),
        (
            StatusCode::CONFLICT,
            Some("customer.phone_taken"),
            ha.body["id"].clone()
        )
    );
    let bad = s
        .customer(json!({"full_name": "X", "phone": "12345"}))
        .await;
    assert_eq!(
        first_error(&bad),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("phone"),
            Some("invalid_format")
        )
    );
    // Removing the phone of a profile frees the number.
    let ha_id = str_of(&ha.body["id"]);
    let cleared = s
        .put_customer(
            &ha_id,
            1,
            json!({"full_name": "Nguyễn Thị Hà", "phone": null}),
        )
        .await;
    assert_eq!(cleared.status, StatusCode::OK, "{}", cleared.body);
    assert_eq!(cleared.body["phone"], Value::Null);
    let reuse = s
        .customer(json!({"full_name": "Hà mới", "phone": "0905111222"}))
        .await;
    assert_eq!(reuse.status, StatusCode::CREATED, "{}", reuse.body);
    // Search by name still finds the phone-less profile.
    let found = s.get("/customers?q=chong").await;
    assert_eq!(
        found.body.as_array().map(Vec::len),
        Some(2),
        "{}",
        found.body
    );
    assert_eq!(found.body[0]["phone"], Value::Null);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_date_of_birth_is_optional_and_plausible(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let today = spa_today();
    let created = s
        .customer(json!({"full_name": "Lê Thị Mai", "birth_date": "1990-03-14"}))
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    assert_eq!(
        (
            created.body["birth_date"].clone(),
            created.body["birth_year"].clone()
        ),
        (json!("1990-03-14"), json!(1990)),
        "the year follows the date"
    );
    let shown = s
        .get(&format!("/customers/{}", str_of(&created.body["id"])))
        .await;
    assert_eq!(shown.body["birth_date"], "1990-03-14");

    let tomorrow = today + Duration::days(1);
    let future = s
        .customer(json!({"full_name": "A", "birth_date": tomorrow}))
        .await;
    assert_eq!(
        first_error(&future),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("birth_date"),
            Some("in_future")
        )
    );
    let born_today = s
        .customer(json!({"full_name": "Bé mới sinh", "birth_date": today}))
        .await;
    assert_eq!(
        born_today.status,
        StatusCode::CREATED,
        "{}",
        born_today.body
    );
    let ancient = NaiveDate::from_ymd_opt(today.year() - 121, 1, 1).unwrap_or(today);
    let old = s
        .customer(json!({"full_name": "B", "birth_date": ancient}))
        .await;
    assert_eq!(
        first_error(&old),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("birth_date"),
            Some("too_old")
        )
    );
    let mismatch = s
        .customer(json!({"full_name": "C", "birth_date": "1990-03-14", "birth_year": 1991}))
        .await;
    assert_eq!(
        first_error(&mismatch),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("birth_year"),
            Some("conflicts_with_birth_date")
        )
    );
    // The database refuses a disagreeing pair too (second layer).
    let id = str_of(&created.body["id"]);
    let db = sqlx::query("UPDATE app.customer SET birth_year = 1991 WHERE id = $1::uuid")
        .bind(&id)
        .execute(&s.h.owner)
        .await;
    assert!(db.is_err(), "customer_birth_year_matches_date");
    Ok(())
}

/// Buying for someone else names them in the ledger, and the beneficiary's timeline shows who
/// paid. "Gia hạn" is an entry on the owner's timeline.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn ledger_and_timelines_name_the_beneficiary_and_show_extensions(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let ha = s
        .customer(json!({"full_name": "Nguyễn Thị Hà", "phone": "0905111222"}))
        .await;
    let binh = s.customer(json!({"full_name": "Trần Thị Bình"})).await;
    let (ha, binh) = (str_of(&ha.body["id"]), str_of(&binh.body["id"]));
    let gift = s.post("/sales", s.package_sale(&ha, Some(&binh))).await;
    assert_eq!(gift.status, StatusCode::CREATED, "{}", gift.body);

    let ledger = s.get("/reports/ledger").await;
    assert_eq!(
        (
            ledger.body["rows"][0]["customer_name"].clone(),
            ledger.body["rows"][0]["beneficiary_name"].clone()
        ),
        (json!("Nguyễn Thị Hà"), json!("Trần Thị Bình")),
        "{}",
        ledger.body
    );
    let payer = s.get(&format!("/customers/{ha}/history")).await;
    assert_eq!(
        (
            payer.body["events"][0]["purchase"]["beneficiary_name"].clone(),
            payer.body["events"][0]["purchase"]["payer_name"].clone()
        ),
        (json!("Trần Thị Bình"), Value::Null)
    );
    let owner_view = s.get(&format!("/customers/{binh}/history")).await;
    assert_eq!(owner_view.status, StatusCode::OK, "{}", owner_view.body);
    assert_eq!(
        (
            owner_view.body["events"][0]["kind"].clone(),
            owner_view.body["events"][0]["purchase"]["payer_name"].clone()
        ),
        (json!("purchase"), json!("Nguyễn Thị Hà")),
        "{}",
        owner_view.body
    );

    let package = &gift.body["customer_packages"][0];
    let package_id = str_of(&package["id"]);
    let expires: NaiveDate = package["expires_on"]
        .as_str()
        .and_then(|d| d.parse().ok())
        .unwrap_or_else(|| panic!("expiry in {package}"));
    let later = expires + Duration::days(90);
    let etag = format!("\"{}\"", package["version"]);
    let key = Uuid::now_v7().to_string();
    let extended = call_with_headers(
        &s.h.app,
        "POST",
        &format!("/customer-packages/{package_id}/extend"),
        Some(&s.owner),
        Some(json!({"expires_on": later, "reason": "Khách đi công tác"})),
        &[("idempotency-key", &key), ("If-Match", &etag)],
    )
    .await;
    assert_eq!(extended.status, StatusCode::OK, "{}", extended.body);
    let timeline = s.get(&format!("/customers/{binh}/history")).await;
    let event = &timeline.body["events"][0];
    assert_eq!(event["kind"], "package_extended", "{}", timeline.body);
    assert_eq!(
        (
            event["extension"]["previous_expires_on"].clone(),
            event["extension"]["expires_on"].clone(),
            event["extension"]["reason"].clone(),
            event["extension"]["package_name"].clone()
        ),
        (
            json!(expires),
            json!(later),
            json!("Khách đi công tác"),
            json!("Gội đầu 10 buổi")
        )
    );
    let only = s
        .get(&format!(
            "/customers/{binh}/history?kind=package_extended&q=goi"
        ))
        .await;
    assert_eq!(
        only.body["events"].as_array().map(Vec::len),
        Some(1),
        "{}",
        only.body
    );
    Ok(())
}

/// `app.customer_label` (SQL) and `crm::service::WALK_IN_LABEL` (Rust) spell the label alike.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn the_walk_in_label_has_one_spelling(pool: PgPool) -> TestResult {
    let label: String = sqlx::query_scalar("SELECT app.customer_label(NULL)")
        .fetch_one(&pool)
        .await?;
    assert_eq!(label, crm::service::WALK_IN_LABEL);
    let named: String = sqlx::query_scalar("SELECT app.customer_label('Hà')")
        .fetch_one(&pool)
        .await?;
    assert_eq!(named, "Hà");
    Ok(())
}
