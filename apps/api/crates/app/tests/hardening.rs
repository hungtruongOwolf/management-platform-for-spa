//! S10 hardening: edge cases across R1-1 that no single slice owned — Unicode, business-day
//! boundaries, money limits and rounding, races between different routes, double submits on routes
//! without an Idempotency-Key, and the shape of 4xx responses.

mod common;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode, header};
use catalog::service::seed_menu;
use chrono::{Duration, TimeZone, Utc};
use common::{
    Harness, ORIGIN, OWNER_PHONE, Reply, TestResult, add_member, call, call_with_headers, harness,
    session_for,
};
use http_body_util::BodyExt;
use identity::SystemRole;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::net::SocketAddr;
use tower::ServiceExt;
use uuid::Uuid;

const MANAGER_PHONE: &str = "0912000222";

type Setup<T> = Result<T, Box<dyn std::error::Error>>;

struct Spa {
    h: Harness,
    owner: String,
    manager: String,
    ha: String,
    lan: String,
    shampoo: String,
    category: String,
}

fn str_of(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_owned()
}

fn code(reply: &Reply) -> (StatusCode, Option<&str>) {
    (reply.status, reply.body["code"].as_str())
}

fn field_codes(reply: &Reply) -> Vec<String> {
    reply.body["errors"]
        .as_array()
        .map(|errors| {
            errors
                .iter()
                .map(|e| format!("{}:{}", str_of(&e["field"]), str_of(&e["code"])))
                .collect()
        })
        .unwrap_or_default()
}

fn vn() -> chrono_tz::Tz {
    kernel::BusinessDate::parse_tz("Asia/Ho_Chi_Minh").unwrap_or(chrono_tz::UTC)
}

/// Percent-encodes a query-string value (UTF-8 bytes; unreserved characters kept).
fn encode(q: &str) -> String {
    q.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
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
    let (shampoo, category): (Uuid, Uuid) = sqlx::query_as(
        "SELECT id, category_id FROM app.service WHERE name = 'Cơ Bản – Dầu thảo dược'",
    )
    .fetch_one(&h.owner)
    .await?;
    let ha = call(
        &h.app,
        "POST",
        "/customers",
        Some(&manager),
        Some(json!({"full_name": "Nguyễn Thị Hà", "phone": "0905111222", "gender": "female"})),
    )
    .await;
    assert_eq!(ha.status, StatusCode::CREATED, "{}", ha.body);
    let lan = org::provisioning::add_staff(&h.owner, "vpyd", "KTV Lan", "Kỹ thuật viên").await?;
    Ok(Spa {
        owner,
        manager,
        ha: str_of(&ha.body["id"]),
        lan: lan.to_string(),
        shampoo: shampoo.to_string(),
        category: category.to_string(),
        h,
    })
}

impl Spa {
    async fn post(&self, who: &str, path: &str, body: Value) -> Reply {
        let key = Uuid::now_v7().to_string();
        call_with_headers(
            &self.h.app,
            "POST",
            path,
            Some(who),
            Some(body),
            &[("idempotency-key", &key)],
        )
        .await
    }

    async fn plan(&self, price: i64, sessions: i32, bonus: i32) -> String {
        let plan = call(
            &self.h.app,
            "POST",
            "/package-plans",
            Some(&self.owner),
            Some(json!({
                "name": format!("Gói {price} {sessions}+{bonus} {}", Uuid::now_v7()),
                "price": price, "validity_months": null, "gift_note": null, "active": true,
                "items": [{"service_id": self.shampoo, "sessions": sessions, "bonus_sessions": bonus}],
            })),
        )
        .await;
        assert_eq!(plan.status, StatusCode::CREATED, "{}", plan.body);
        str_of(&plan.body["id"])
    }

    async fn sell(&self, who: &str, plan: &str, payments: Value, discount: Option<Value>) -> Reply {
        let mut body = json!({
            "customer_id": self.ha, "closer_staff_id": self.lan,
            "lines": [{"package_plan_id": plan}], "payments": payments,
        });
        if let Some(d) = discount {
            body["discount"] = d;
        }
        self.post(who, "/sales", body).await
    }

    async fn search(&self, q: &str) -> Reply {
        let q = encode(q);
        call(
            &self.h.app,
            "GET",
            &format!("/customers?q={q}"),
            Some(&self.manager),
            None,
        )
        .await
    }

    async fn count(&self, sql: &str) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
            .fetch_one(&self.h.owner)
            .await
    }
}

/// A request whose body is sent verbatim (not necessarily valid JSON).
async fn raw(app: &axum::Router, method: &str, path: &str, session: &str, body: &str) -> Reply {
    let mut request = Request::builder()
        .method(method)
        .uri(format!("/api/v1{path}"))
        .header(header::ORIGIN, ORIGIN)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::COOKIE, format!("__Host-session={session}"))
        .header("idempotency-key", Uuid::now_v7().to_string())
        .header("If-Match", "\"1\"")
        .body(Body::from(body.to_owned()))
        .unwrap_or_default();
    request
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([10, 0, 0, 7], 50_000))));
    let response = app
        .clone()
        .oneshot(request)
        .await
        .unwrap_or_else(|never| match never {});
    let (status, headers) = (response.status(), response.headers().clone());
    let bytes = response
        .into_body()
        .collect()
        .await
        .map(|b| b.to_bytes())
        .unwrap_or_default();
    Reply {
        status,
        headers,
        body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    }
}

// ------------------------------------------------------------------------------------ Unicode

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn names_from_any_keyboard_are_stored_nfc_and_found_with_or_without_accents(
    pool: PgPool,
) -> TestResult {
    let s = spa(pool).await?;
    // "Đặng Thị Hà" typed decomposed (base letters + combining marks), as some keyboards do.
    let decomposed = "\u{110}a\u{323}\u{306}ng Thi\u{323} Ha\u{300}";
    let created = call(
        &s.h.app,
        "POST",
        "/customers",
        Some(&s.manager),
        Some(json!({"full_name": decomposed, "phone": "0905333444", "gender": "female"})),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    assert_eq!(created.body["full_name"], "Đặng Thị Hà", "AC2.7: NFC");

    for q in [
        "Đặng",
        "đặng",
        "ĐẶNG",
        "dang",
        "DANG THI",
        "dang thi ha",
        "Đặng Thị Hà",
        decomposed,
    ] {
        let found = s.search(q).await;
        assert_eq!(found.status, StatusCode::OK, "{q}: {}", found.body);
        let names: Vec<String> = found
            .body
            .as_array()
            .map(|a| a.iter().map(|c| str_of(&c["full_name"])).collect())
            .unwrap_or_default();
        assert!(names.contains(&"Đặng Thị Hà".to_owned()), "{q} → {names:?}");
    }
    // "d" folds "đ", never the other way round: "Dang" (no stroke) is a different spelling but the
    // same search key, which is what a manager typing without accents expects.
    let dung = s.search("dung").await;
    assert_eq!(dung.body.as_array().map(Vec::len), Some(0));

    // Emoji and other non-Vietnamese symbols are kept as typed and do not break the search key.
    let emoji = call(
        &s.h.app,
        "POST",
        "/customers",
        Some(&s.manager),
        Some(json!({"full_name": "Chị Mai 🌸", "phone": "0905333555"})),
    )
    .await;
    assert_eq!(emoji.status, StatusCode::CREATED, "{}", emoji.body);
    assert_eq!(emoji.body["full_name"], "Chị Mai 🌸");
    let found = s.search("chi mai").await;
    assert_eq!(found.body[0]["full_name"], "Chị Mai 🌸");
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn text_limits_count_characters_not_bytes(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    // "Ễ" is 3 bytes in UTF-8: 200 of them are 600 bytes but still a valid 200-character name.
    let create = |name: String, phone: &'static str| {
        let app = s.h.app.clone();
        let manager = s.manager.clone();
        async move {
            call(
                &app,
                "POST",
                "/customers",
                Some(&manager),
                Some(json!({"full_name": name, "phone": phone})),
            )
            .await
        }
    };
    let ok = create("Ễ".repeat(200), "0905333666").await;
    assert_eq!(ok.status, StatusCode::CREATED, "{}", ok.body);
    let long = create("Ễ".repeat(201), "0905333777").await;
    assert_eq!(long.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(field_codes(&long), ["full_name:too_long"]);
    let blank = create(" \u{3000} \t".to_owned(), "0905333777").await;
    assert_eq!(field_codes(&blank), ["full_name:required"]);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn nul_characters_are_rejected_as_422_not_500(pool: PgPool) -> TestResult {
    // Postgres `text` cannot hold U+0000; JSON can (`"\u0000"`). It must be a validation error.
    let s = spa(pool).await?;
    let customer = call(
        &s.h.app,
        "POST",
        "/customers",
        Some(&s.manager),
        Some(json!({"full_name": "Hà\u{0}", "phone": "0905444555"})),
    )
    .await;
    assert_eq!(
        code(&customer),
        (StatusCode::UNPROCESSABLE_ENTITY, Some("validation_failed")),
        "{}",
        customer.body
    );

    let service = call(
        &s.h.app,
        "POST",
        "/services",
        Some(&s.owner),
        Some(json!({
            "category_id": s.category, "name": "Gội\u{0}", "description": null,
            "duration_min": 60, "recommended": false, "active": true, "prices": {"all": 1}
        })),
    )
    .await;
    assert_eq!(
        service.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        service.body
    );

    let search = s.search("Hà\u{0}").await;
    assert_eq!(
        search.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        search.body
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn search_queries_are_bounded(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let long = s.search(&"hà".repeat(101)).await;
    assert_eq!(
        long.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        long.body
    );
    assert_eq!(field_codes(&long), ["q:too_long"]);
    let wildcards = s.search("%_%").await;
    assert_eq!(
        wildcards.body.as_array().map(Vec::len),
        Some(0),
        "LIKE wildcards are literal"
    );
    Ok(())
}

// ------------------------------------------------------------------------- business-day edges

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn check_in_belongs_to_the_spas_business_day_not_utc(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let today = Utc::now().with_timezone(&vn()).date_naive();
    let midnight = vn()
        .from_local_datetime(&today.and_hms_opt(0, 0, 0).unwrap_or_default())
        .single()
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or_default();
    let visit = |at: chrono::DateTime<Utc>| {
        json!({
            "customer_id": s.ha, "check_in_at": at.to_rfc3339(),
            "lines": [{"service_id": s.shampoo, "source": "one_off", "staff_id": s.lan}],
            "payments": [{"method": "cash", "amount": 49_000}],
        })
    };

    // 00:00:00 in Vietnam is 17:00 UTC of the previous calendar day: still today for the spa.
    let first = s.post(&s.manager, "/visits", visit(midnight)).await;
    assert_eq!(first.status, StatusCode::CREATED, "{}", first.body);
    assert_eq!(first.body["business_date"], json!(today));
    // 23:59:59 the evening before belongs to yesterday: a closed day.
    let before = s
        .post(
            &s.manager,
            "/visits",
            visit(midnight - Duration::seconds(1)),
        )
        .await;
    assert_eq!(
        code(&before),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("visit.check_in_not_today")
        )
    );
    // The day list uses the same calendar.
    let list = call(&s.h.app, "GET", "/visits", Some(&s.manager), None).await;
    assert_eq!(list.body.as_array().map(Vec::len), Some(1));
    let payments = s
        .count("SELECT count(*) FROM app.payment WHERE business_date = (now() AT TIME ZONE 'Asia/Ho_Chi_Minh')::date")
        .await?;
    assert_eq!(payments, 1, "the payment is booked on the spa's day");
    Ok(())
}

// --------------------------------------------------------------------------- money boundaries

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn catalog_amounts_above_the_limit_are_422(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let service = |price: i64| {
        json!({
            "category_id": s.category, "name": format!("Dịch vụ {price}"), "description": null,
            "duration_min": 60, "recommended": false, "active": true, "prices": {"all": price}
        })
    };
    let at_limit = call(
        &s.h.app,
        "POST",
        "/services",
        Some(&s.owner),
        Some(service(10_000_000_000)),
    )
    .await;
    assert_eq!(at_limit.status, StatusCode::CREATED, "{}", at_limit.body);
    let over = call(
        &s.h.app,
        "POST",
        "/services",
        Some(&s.owner),
        Some(service(10_000_000_001)),
    )
    .await;
    assert_eq!(
        over.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        over.body
    );
    assert_eq!(field_codes(&over), ["prices:out_of_range"]);
    let negative = call(
        &s.h.app,
        "POST",
        "/services",
        Some(&s.owner),
        Some(service(-1)),
    )
    .await;
    assert_eq!(
        negative.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        negative.body
    );

    let plan = call(
        &s.h.app,
        "POST",
        "/package-plans",
        Some(&s.owner),
        Some(json!({
            "name": "Gói quá đắt", "price": i64::MAX, "validity_months": null, "gift_note": null,
            "active": true, "items": [{"service_id": s.shampoo, "sessions": 1, "bonus_sessions": 0}],
        })),
    )
    .await;
    assert_eq!(
        plan.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        plan.body
    );
    assert_eq!(field_codes(&plan), ["price:out_of_range"]);

    let payment = s
        .post(
            &s.manager,
            "/visits",
            json!({
                "customer_id": s.ha,
                "lines": [{"service_id": s.shampoo, "source": "one_off", "staff_id": s.lan}],
                "payments": [{"method": "cash", "amount": i64::MAX}, {"method": "bank_transfer", "amount": i64::MAX}],
            }),
        )
        .await;
    assert_eq!(
        payment.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "overflow is a 422: {}",
        payment.body
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn per_session_value_rounds_down_and_free_packages_take_no_payment(
    pool: PgPool,
) -> TestResult {
    let s = spa(pool).await?;
    // 1.000.000đ over 3 bought sessions: 333.333đ each (D4, rounded down); the bonus one is 0.
    let thirds = s.plan(1_000_000, 3, 1).await;
    let sold = s
        .sell(
            &s.manager,
            &thirds,
            json!([{"method": "cash", "amount": 1_000_000}]),
            None,
        )
        .await;
    assert_eq!(sold.status, StatusCode::CREATED, "{}", sold.body);
    let pkg = &sold.body["customer_packages"][0];
    assert_eq!(pkg["items"][0]["per_session_value"], 333_333);
    assert_eq!(pkg["sessions_remaining"], 4);

    // A 0đ package (gift): no payment at all, and a payment would be a mismatch.
    let free = s.plan(0, 2, 0).await;
    let gift = s.sell(&s.manager, &free, json!([]), None).await;
    assert_eq!(gift.status, StatusCode::CREATED, "{}", gift.body);
    assert_eq!(gift.body["total"], 0);
    assert_eq!(
        gift.body["customer_packages"][0]["items"][0]["per_session_value"],
        0
    );
    let paid_for_free = s
        .sell(
            &s.manager,
            &free,
            json!([{"method": "cash", "amount": 1}]),
            None,
        )
        .await;
    assert_eq!(
        code(&paid_for_free),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("payment.amount_mismatch")
        )
    );

    // Owner: a 100% discount is allowed (unlimited) and leaves nothing to collect.
    let full = s
        .sell(
            &s.owner,
            &thirds,
            json!([]),
            Some(json!({"amount": 1_000_000, "reason": "Quà tri ân"})),
        )
        .await;
    assert_eq!(full.status, StatusCode::CREATED, "{}", full.body);
    assert_eq!(full.body["total"], 0);
    // More than the price is never a valid discount.
    let over = s
        .sell(
            &s.owner,
            &thirds,
            json!([]),
            Some(json!({"amount": 1_000_001, "reason": "x"})),
        )
        .await;
    assert_eq!(
        over.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        over.body
    );
    Ok(())
}

// ------------------------------------------------------------------- races between two routes

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn voiding_a_package_sale_while_a_visit_uses_it_leaves_one_consistent_outcome(
    pool: PgPool,
) -> TestResult {
    let s = spa(pool).await?;
    let plan = s.plan(1_500_000, 10, 2).await;
    for _ in 0..5 {
        let sold = s
            .sell(
                &s.manager,
                &plan,
                json!([{"method": "cash", "amount": 1_500_000}]),
                None,
            )
            .await;
        let sale = str_of(&sold.body["id"]);
        let item = str_of(&sold.body["customer_packages"][0]["items"][0]["id"]);
        let visit = s.post(
            &s.manager,
            "/visits",
            json!({"customer_id": s.ha, "lines": [{"service_id": s.shampoo, "source": "package",
                    "customer_package_item_id": item, "staff_id": s.lan}]}),
        );
        let void_path = format!("/sales/{sale}/void");
        let void = s.post(&s.manager, &void_path, json!({"reason": "nhầm"}));
        let (visit, void) = tokio::join!(visit, void);
        let remaining: i32 = sqlx::query_scalar(
            "SELECT sessions_remaining FROM app.customer_package_item WHERE id = $1::uuid",
        )
        .bind(&item)
        .fetch_one(&s.h.owner)
        .await?;
        match (visit.status, void.status) {
            (StatusCode::CREATED, StatusCode::CONFLICT) => assert_eq!(remaining, 11),
            (StatusCode::CONFLICT, StatusCode::OK) => assert_eq!(remaining, 0, "cancelled"),
            other => panic!(
                "exactly one may win, got {other:?}: {} / {}",
                visit.body, void.body
            ),
        }
    }

    // Same race against the owner cancelling the package (🔒; the sign-in is fresh).
    for _ in 0..5 {
        let sold = s
            .sell(
                &s.manager,
                &plan,
                json!([{"method": "cash", "amount": 1_500_000}]),
                None,
            )
            .await;
        let package = str_of(&sold.body["customer_packages"][0]["id"]);
        let item = str_of(&sold.body["customer_packages"][0]["items"][0]["id"]);
        let visit = s.post(
            &s.manager,
            "/visits",
            json!({"customer_id": s.ha, "lines": [{"service_id": s.shampoo, "source": "package",
                    "customer_package_item_id": item, "staff_id": s.lan}]}),
        );
        let cancel_path = format!("/customer-packages/{package}/cancel");
        let cancel = s.post(
            &s.owner,
            &cancel_path,
            json!({"reason": "Khách chuyển nhà", "refund_amount": 0}),
        );
        let (visit, cancel) = tokio::join!(visit, cancel);
        assert!(
            !visit.status.is_server_error() && !cancel.status.is_server_error(),
            "{} {} / {} {}",
            visit.status,
            visit.body,
            cancel.status,
            cancel.body
        );
        assert_eq!(cancel.status, StatusCode::OK, "{}", cancel.body);
        let remaining: i32 = sqlx::query_scalar(
            "SELECT sessions_remaining FROM app.customer_package_item WHERE id = $1::uuid",
        )
        .bind(&item)
        .fetch_one(&s.h.owner)
        .await?;
        assert_eq!(remaining, 0, "a cancelled package keeps no sessions");
        if visit.status != StatusCode::CREATED {
            assert_eq!(
                code(&visit),
                (StatusCode::CONFLICT, Some("package.cancelled"))
            );
        }
    }
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn two_corrections_of_the_same_visit_version_one_wins(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let v = s
        .post(
            &s.manager,
            "/visits",
            json!({"customer_id": s.ha,
                   "lines": [{"service_id": s.shampoo, "source": "one_off", "staff_id": s.lan}],
                   "payments": [{"method": "cash", "amount": 49_000}]}),
        )
        .await;
    let path = format!("/visits/{}", str_of(&v.body["id"]));
    let patch = |note: &'static str| {
        call_with_headers(
            &s.h.app,
            "PATCH",
            &path,
            Some(&s.manager),
            Some(json!({"note": note})),
            &[("If-Match", "\"1\"")],
        )
    };
    let (a, b) = tokio::join!(patch("A"), patch("B"));
    let mut statuses = [a.status, b.status];
    statuses.sort();
    assert_eq!(statuses, [StatusCode::OK, StatusCode::PRECONDITION_FAILED]);
    Ok(())
}

// ---------------------------------------------- double submit on routes without Idempotency-Key

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn double_submits_of_natural_key_writes_create_one_record(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let body = json!({"full_name": "Lê Thị Ánh", "phone": "0905 777 888"});
    let send = || {
        call(
            &s.h.app,
            "POST",
            "/customers",
            Some(&s.manager),
            Some(body.clone()),
        )
    };
    let (a, b) = tokio::join!(send(), send());
    let mut statuses = [a.status, b.status];
    statuses.sort();
    assert_eq!(statuses, [StatusCode::CREATED, StatusCode::CONFLICT]);
    let loser = if a.status == StatusCode::CONFLICT {
        &a
    } else {
        &b
    };
    assert_eq!(loser.body["code"], "customer.phone_taken");
    assert!(
        loser.body["existing_id"].is_string(),
        "links to the profile (AC2.4)"
    );

    let service = json!({
        "category_id": s.category, "name": "Dịch vụ gửi đôi", "description": null,
        "duration_min": 30, "recommended": false, "active": true, "prices": {"all": 50_000}
    });
    let send = || {
        call(
            &s.h.app,
            "POST",
            "/services",
            Some(&s.owner),
            Some(service.clone()),
        )
    };
    let (a, b) = tokio::join!(send(), send());
    assert_eq!(
        [a.status, b.status]
            .iter()
            .filter(|st| **st == StatusCode::CREATED)
            .count(),
        1,
        "{} / {}",
        a.body,
        b.body
    );
    assert_eq!(
        s.count("SELECT count(*) FROM app.service WHERE name = 'Dịch vụ gửi đôi'")
            .await?,
        1
    );
    Ok(())
}

// -------------------------------------------------------------------- 4xx response shape

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn malformed_bodies_get_a_problem_json_4xx_on_every_write_route(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let id = Uuid::now_v7();
    let routes = [
        ("POST", "/customers".to_owned()),
        ("PUT", format!("/customers/{id}")),
        ("PUT", format!("/customers/{id}/health-note")),
        ("POST", "/services".to_owned()),
        ("PUT", format!("/services/{id}")),
        ("POST", "/package-plans".to_owned()),
        ("POST", "/service-categories".to_owned()),
        ("POST", "/sales".to_owned()),
        ("POST", format!("/sales/{id}/void")),
        ("POST", "/visits".to_owned()),
        ("PATCH", format!("/visits/{id}")),
        ("POST", format!("/visits/{id}/void")),
        ("POST", "/opening-balances".to_owned()),
        ("POST", format!("/customer-packages/{id}/extend")),
        ("POST", format!("/customer-packages/{id}/cancel")),
    ];
    for (method, path) in &routes {
        for body in ["{", "[]", "{\"lines\": \"x\"}"] {
            let r = raw(&s.h.app, method, path, &s.owner, body).await;
            let ctype = r
                .headers
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default()
                .to_owned();
            // 404 is fine too: serde reads `[]` as an empty struct, so the id is then looked up.
            assert!(
                r.status.is_client_error(),
                "{method} {path} {body} → {}",
                r.status
            );
            assert_eq!(ctype, "application/problem+json", "{method} {path} {body}");
            assert!(
                r.body["code"].is_string(),
                "{method} {path} {body}: {}",
                r.body
            );
        }
    }
    Ok(())
}

// ------------------------------------------------------------------------------------ audit (G5)

/// G5 for the writes whose slice tests do not look at `audit_log`: each leaves a row naming the
/// caller, with the expected action.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn every_catalog_and_profile_write_is_audited(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let put = |path: String, body: Value, version: &'static str| {
        let app = s.h.app.clone();
        let owner = s.owner.clone();
        async move {
            call_with_headers(
                &app,
                "PUT",
                &path,
                Some(&owner),
                Some(body),
                &[("If-Match", version)],
            )
            .await
        }
    };
    let category = call(
        &s.h.app,
        "POST",
        "/service-categories",
        Some(&s.owner),
        Some(json!({"name": "Nhóm mới"})),
    )
    .await;
    assert_eq!(category.status, StatusCode::CREATED, "{}", category.body);
    let category_id = str_of(&category.body["id"]);
    let renamed = put(
        format!("/service-categories/{category_id}"),
        json!({"name": "Nhóm mới (sửa)", "sort_order": 3}),
        "\"1\"",
    )
    .await;
    assert_eq!(renamed.status, StatusCode::OK, "{}", renamed.body);
    let service = |name: &str| {
        json!({
            "category_id": category_id, "name": name, "description": null,
            "duration_min": 30, "recommended": false, "active": true, "prices": {"all": 50_000}
        })
    };
    let created = call(
        &s.h.app,
        "POST",
        "/services",
        Some(&s.owner),
        Some(service("DV kiểm toán")),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    let service_id = str_of(&created.body["id"]);
    let edited = put(
        format!("/services/{service_id}"),
        service("DV kiểm toán (sửa)"),
        "\"1\"",
    )
    .await;
    assert_eq!(edited.status, StatusCode::OK, "{}", edited.body);
    let plan = s.plan(500_000, 5, 0).await;
    let plan_edit = put(
        format!("/package-plans/{plan}"),
        json!({
            "name": "Gói kiểm toán (sửa)", "price": 600_000, "validity_months": 3, "gift_note": null,
            "active": true, "items": [{"service_id": s.shampoo, "sessions": 5, "bonus_sessions": 1}],
        }),
        "\"1\"",
    )
    .await;
    assert_eq!(plan_edit.status, StatusCode::OK, "{}", plan_edit.body);
    let profile = put(
        format!("/customers/{}", s.ha),
        json!({"full_name": "Nguyễn Thị Hà", "phone": "0905111222", "gender": "female",
               "birth_year": 1980, "address": null, "source": null, "note": null}),
        "\"1\"",
    )
    .await;
    assert_eq!(profile.status, StatusCode::OK, "{}", profile.body);
    let note = put(
        format!("/customers/{}/health-note", s.ha),
        json!({"health_note": "Huyết áp cao"}),
        "\"2\"",
    )
    .await;
    assert!(note.status.is_success(), "{} {}", note.status, note.body);
    // Managers import only on a day the owner opened (security review M2, AC8.6).
    let window = call(
        &s.h.app,
        "PUT",
        "/opening-balances/window",
        Some(&s.owner),
        Some(json!({"open": true})),
    )
    .await;
    assert_eq!(window.status, StatusCode::OK, "{}", window.body);
    let imported = s
        .post(
            &s.manager,
            "/opening-balances",
            json!({"customer_id": s.ha, "packages": [{
                "name": "Gói sổ cũ", "sold_price": 1_500_000, "starts_on": "2026-06-01",
                "items": [{"service_id": s.shampoo, "sessions_purchased": 10, "sessions_remaining": 4}]}]}),
        )
        .await;
    assert_eq!(imported.status, StatusCode::CREATED, "{}", imported.body);

    for action in [
        "catalog.category_create",
        "catalog.category_update",
        "catalog.service_create",
        "catalog.service_update",
        "catalog.plan_create",
        "catalog.plan_update",
        "customer.update",
        "customer.health_note_set",
        "opening_balance.import",
    ] {
        let rows = s
            .count(&format!(
                "SELECT count(*) FROM app.audit_log WHERE action = '{action}'
                   AND actor_kind = 'user' AND actor_user_id IS NOT NULL AND tenant_id IS NOT NULL"
            ))
            .await?;
        assert!(rows >= 1, "{action}: no audit row");
    }
    let leaked = s
        .count(
            "SELECT count(*) FROM app.audit_log
              WHERE (coalesce(before::text, '') || coalesce(after::text, '')) LIKE '%Huyết áp%'",
        )
        .await?;
    assert_eq!(leaked, 0, "health notes never reach the audit log in clear");
    Ok(())
}
