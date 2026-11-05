//! S9: importing part-used packages from the paper books (spec R1-1 US8, AC8.1, AC8.3–AC8.5).

mod common;

use axum::http::StatusCode;
use catalog::service::seed_menu;
use common::{
    Harness, OWNER_PHONE, Reply, TestResult, add_member, call, call_with_headers, harness,
    session_for,
};
use identity::SystemRole;
use identity::provisioning::create_tenant;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const MANAGER_PHONE: &str = "0912000111";
const TECH_PHONE: &str = "0912000222";
const OTHER_OWNER_PHONE: &str = "0913000333";

struct Shop {
    h: Harness,
    owner: String,
    manager: String,
    customer: String,
    shampoo: String,
    massage: String,
    plan: String,
}

type Setup<T> = Result<T, Box<dyn std::error::Error>>;

fn id_of(reply: &Reply) -> String {
    reply.body["id"].as_str().unwrap_or_default().to_owned()
}

fn service_id(services: &Reply, name: &str) -> String {
    services
        .body
        .as_array()
        .and_then(|l| l.iter().find(|s| s["name"] == name))
        .and_then(|s| s["id"].as_str())
        .unwrap_or_default()
        .to_owned()
}

/// Spa with the printed menu, a "10 + 2 at 1.5M" plan, a customer and a manager.
async fn shop(pool: PgPool) -> Setup<Shop> {
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
    let services = call(&h.app, "GET", "/services", Some(&owner), None).await;
    let shampoo = service_id(&services, "Cơ Bản – Dầu thảo dược");
    let massage = services
        .body
        .as_array()
        .and_then(|l| l.iter().find(|s| s["id"] != json!(shampoo)))
        .and_then(|s| s["id"].as_str())
        .unwrap_or_default()
        .to_owned();
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
    let customer = call(
        &h.app,
        "POST",
        "/customers",
        Some(&owner),
        Some(json!({"full_name": "Nguyễn Thị Hà", "phone": "0905111222", "gender": "female"})),
    )
    .await;
    Ok(Shop {
        owner,
        manager,
        customer: id_of(&customer),
        plan: id_of(&plan),
        shampoo,
        massage,
        h,
    })
}

impl Shop {
    /// "10 buổi gội, đã dùng 6, giá 1.500.000đ" for the existing customer (AC8.1).
    fn four_of_ten(&self) -> Value {
        json!({
            "customer_id": self.customer,
            "packages": [{
                "name": "Gội đầu 10 buổi (sổ cũ)", "sold_price": 1_500_000,
                "starts_on": "2026-06-01", "expires_on": null,
                "items": [{"service_id": self.shampoo, "sessions_purchased": 10, "sessions_remaining": 4}],
            }],
        })
    }

    async fn post(&self, session: &str, key: &str, body: Value) -> Reply {
        call_with_headers(
            &self.h.app,
            "POST",
            "/opening-balances",
            Some(session),
            Some(body),
            &[("idempotency-key", key)],
        )
        .await
    }

    async fn import(&self, session: &str, body: Value) -> Reply {
        self.post(session, &Uuid::now_v7().to_string(), body).await
    }

    async fn count(&self, table: &str) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT count(*) FROM app.{table}"
        )))
        .fetch_one(&self.h.owner)
        .await
    }
}

fn code(reply: &Reply) -> (StatusCode, Option<&str>) {
    (reply.status, reply.body["code"].as_str())
}

fn field_codes(reply: &Reply) -> Vec<(String, String)> {
    reply.body["errors"]
        .as_array()
        .map(|l| {
            l.iter()
                .map(|e| {
                    (
                        e["field"].as_str().unwrap_or_default().to_owned(),
                        e["code"].as_str().unwrap_or_default().to_owned(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn four_of_ten_left_is_imported_without_any_sale_or_payment(pool: PgPool) -> TestResult {
    let s = shop(pool).await?;
    let imported = s.import(&s.owner, s.four_of_ten()).await;
    assert_eq!(imported.status, StatusCode::CREATED, "{}", imported.body);
    assert_eq!(imported.body["customer_id"], json!(s.customer));
    assert_eq!(imported.body["customer_created"], false);
    let package = &imported.body["customer_packages"][0];
    assert_eq!(
        (
            package["sessions_remaining"].clone(),
            package["sessions_total"].clone(),
            package.get("origin").cloned(),
            package["sale_id"].clone(),
            package["status"].clone(),
        ),
        (json!(4), json!(10), None, Value::Null, json!("active")),
        "AC8.1: còn 4/10; no paper-book flag in the DTO (spec management-views §7)"
    );
    assert_eq!(package["items"][0]["per_session_value"], 150_000);
    assert_eq!(package["items"][0]["name"], "Cơ Bản – Dầu thảo dược");
    assert_eq!(package["starts_on"], "2026-06-01");

    // AC8.1/AC8.3: no revenue — no sale, no sale line, no payment exists at all, today or ever.
    for table in ["sale", "sale_line", "payment"] {
        assert_eq!(s.count(table).await?, 0, "{table} must stay empty");
    }

    // EM 2026-11-05 (spec management-views §7): a migrated package looks like any other one;
    // the only trace is that it has no sale to void.
    let listed = call(
        &s.h.app,
        "GET",
        &format!("/customers/{}/packages", s.customer),
        Some(&s.owner),
        None,
    )
    .await;
    assert!(listed.body[0].get("origin").is_none(), "{}", listed.body);
    assert_eq!(listed.body[0]["sale_id"], Value::Null);
    assert_eq!(listed.body[0]["sessions_remaining"], 4);

    let audit: Vec<String> = sqlx::query_scalar(
        "SELECT action FROM app.audit_log WHERE action IN ('opening_balance.import', 'customer_package.create') ORDER BY action",
    )
    .fetch_all(&s.h.owner)
    .await?;
    assert_eq!(audit, ["customer_package.create", "opening_balance.import"]);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn new_customer_and_two_packages_in_one_form(pool: PgPool) -> TestResult {
    let s = shop(pool).await?;
    let imported = s
        .import(
            &s.owner,
            json!({
                "customer": {"full_name": "Trần Văn Bình", "phone": "0914 555 666", "gender": "male"},
                "packages": [
                    {   // Matches the current plan: name defaults to the plan's.
                        "package_plan_id": s.plan, "sold_price": 1_400_000, "gift_note": "Đã tặng dầu gội",
                        "starts_on": "2026-05-10", "expires_on": "2026-11-10",
                        "items": [{"service_id": s.shampoo, "sessions_purchased": 10, "sessions_bonus": 2, "sessions_remaining": 7}],
                    },
                    {   // Free-form "Gói khác" with two services.
                        "name": "Combo cũ", "sold_price": 0, "starts_on": "2025-12-01",
                        "items": [
                            {"service_id": s.shampoo, "sessions_purchased": 3, "sessions_remaining": 0},
                            {"service_id": s.massage, "sessions_purchased": 2, "sessions_remaining": 2},
                        ],
                    },
                ],
            }),
        )
        .await;
    assert_eq!(imported.status, StatusCode::CREATED, "{}", imported.body);
    assert_eq!(imported.body["customer_created"], true);
    let from_plan = &imported.body["customer_packages"][0];
    assert_eq!(from_plan["name"], "Gội đầu dưỡng sinh 10 buổi + tặng 2");
    assert_eq!(from_plan["plan_id"], json!(s.plan));
    assert_eq!(from_plan["gift_note"], "Đã tặng dầu gội");
    assert_eq!(
        (
            from_plan["sessions_remaining"].clone(),
            from_plan["sessions_total"].clone()
        ),
        (json!(7), json!(12))
    );
    assert_eq!(
        from_plan["items"][0]["per_session_value"], 140_000,
        "bonus sessions are free"
    );
    let free_form = &imported.body["customer_packages"][1];
    assert_eq!(free_form["plan_id"], Value::Null);
    assert_eq!(free_form["sessions_remaining"], 2);
    assert_eq!(free_form["sessions_total"], 5);

    let customer_id = imported.body["customer_id"].as_str().unwrap_or_default();
    let customer = call(
        &s.h.app,
        "GET",
        &format!("/customers/{customer_id}"),
        Some(&s.owner),
        None,
    )
    .await;
    assert_eq!(customer.body["full_name"], "Trần Văn Bình");
    assert_eq!(customer.body["phone_e164"], "+84914555666");
    assert_eq!(s.count("payment").await?, 0);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn more_left_than_total_is_422_and_nothing_is_written(pool: PgPool) -> TestResult {
    let s = shop(pool).await?;
    let customers_before = s.count("customer").await?;
    let mut body = s.four_of_ten();
    body["customer_id"] = Value::Null;
    body["customer"] = json!({"full_name": "Lê Thị Cúc", "phone": "0935000111"});
    body["packages"][0]["items"][0]["sessions_remaining"] = json!(11);
    let rejected = s.import(&s.owner, body).await;
    assert_eq!(
        code(&rejected),
        (StatusCode::UNPROCESSABLE_ENTITY, Some("validation_failed"))
    );
    assert_eq!(
        field_codes(&rejected),
        [(
            "packages[0].items[0].sessions_remaining".to_owned(),
            "exceeds_total".to_owned()
        )],
        "AC8.4"
    );
    assert_eq!(
        s.count("customer").await?,
        customers_before,
        "customer rolled back too"
    );
    assert_eq!(s.count("customer_package").await?, 0);

    // Exactly the total is fine (an untouched package carried over).
    let mut full = s.four_of_ten();
    full["packages"][0]["items"][0]["sessions_remaining"] = json!(10);
    assert_eq!(s.import(&s.owner, full).await.status, StatusCode::CREATED);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn every_bad_field_is_reported_at_once(pool: PgPool) -> TestResult {
    let s = shop(pool).await?;
    let rejected = s
        .import(
            &s.owner,
            json!({
                "customer_id": s.customer,
                "packages": [
                    {"sold_price": -1, "starts_on": "2999-01-01",
                     "items": [{"service_id": Uuid::now_v7(), "sessions_purchased": 1, "sessions_remaining": 1}]},
                    {"name": "B", "sold_price": 1, "starts_on": "2026-01-02", "expires_on": "2026-01-01",
                     "items": [
                        {"service_id": s.shampoo, "sessions_purchased": 1, "sessions_remaining": 1},
                        {"service_id": s.shampoo, "sessions_purchased": 1, "sessions_remaining": 1}]},
                ],
            }),
        )
        .await;
    assert_eq!(
        rejected.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        rejected.body
    );
    let mut fields = field_codes(&rejected);
    fields.sort();
    let expected: Vec<(String, String)> = [
        ("packages[0].items[0].service_id", "not_found"),
        ("packages[0].name", "required"),
        ("packages[0].sold_price", "negative"),
        ("packages[0].starts_on", "in_future"),
        ("packages[1].expires_on", "before_start"),
        ("packages[1].items[1].service_id", "duplicate"),
    ]
    .iter()
    .map(|(f, c)| ((*f).to_owned(), (*c).to_owned()))
    .collect();
    assert_eq!(fields, expected);

    for body in [
        json!({"packages": s.four_of_ten()["packages"].clone()}),
        json!({"customer_id": s.customer, "customer": {"full_name": "X", "phone": "0935000111"},
               "packages": s.four_of_ten()["packages"].clone()}),
    ] {
        let reply = s.import(&s.owner, body).await;
        assert_eq!(
            field_codes(&reply),
            [("customer_id".to_owned(), "exactly_one".to_owned())]
        );
    }
    let mut none = s.four_of_ten();
    none["packages"] = json!([]);
    assert_eq!(
        field_codes(&s.import(&s.owner, none).await),
        [("packages".to_owned(), "count_out_of_range".to_owned())]
    );
    let mut too_many_items = s.four_of_ten();
    too_many_items["packages"][0]["items"] = json!(vec![
        json!({"service_id": s.shampoo, "sessions_purchased": 1, "sessions_remaining": 1});
        21
    ]);
    assert_eq!(
        field_codes(&s.import(&s.owner, too_many_items).await),
        [(
            "packages[0].items".to_owned(),
            "count_out_of_range".to_owned()
        )]
    );
    let mut bad_profile = s.four_of_ten();
    bad_profile["customer_id"] = Value::Null;
    bad_profile["customer"] = json!({"full_name": "", "phone": "123"});
    let mut profile_fields = field_codes(&s.import(&s.owner, bad_profile).await);
    profile_fields.sort();
    assert_eq!(
        profile_fields,
        [
            ("customer.full_name".to_owned(), "required".to_owned()),
            ("customer.phone".to_owned(), "invalid_format".to_owned())
        ]
    );
    assert_eq!(s.count("customer_package").await?, 0);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn technician_cannot_import(pool: PgPool) -> TestResult {
    let s = shop(pool).await?;
    add_member(
        &s.h.owner,
        "vpyd",
        TECH_PHONE,
        "KTV Lan",
        SystemRole::Technician,
    )
    .await?;
    let tech = session_for(&s.h, TECH_PHONE).await;
    let denied = s.import(&tech, s.four_of_ten()).await;
    assert_eq!(
        code(&denied),
        (StatusCode::FORBIDDEN, Some("auth.forbidden"))
    );
    assert_eq!(s.count("customer_package").await?, 0);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn double_submit_with_one_key_imports_once(pool: PgPool) -> TestResult {
    let s = shop(pool).await?;
    let key = Uuid::now_v7().to_string();
    let first = s.post(&s.owner, &key, s.four_of_ten()).await;
    let second = s.post(&s.owner, &key, s.four_of_ten()).await;
    assert_eq!(first.status, StatusCode::CREATED, "{}", first.body);
    assert_eq!(second.status, StatusCode::CREATED);
    assert_eq!(first.body, second.body, "replayed, not re-imported");
    assert_eq!(s.count("customer_package").await?, 1);

    let mut changed = s.four_of_ten();
    changed["packages"][0]["items"][0]["sessions_remaining"] = json!(3);
    let reused = s.post(&s.owner, &key, changed).await;
    assert_eq!(
        code(&reused),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("idempotency.key_reuse_mismatch")
        )
    );
    let keyless = call(
        &s.h.app,
        "POST",
        "/opening-balances",
        Some(&s.owner),
        Some(s.four_of_ten()),
    )
    .await;
    assert_eq!(
        code(&keyless),
        (StatusCode::BAD_REQUEST, Some("idempotency.key_required"))
    );
    assert_eq!(s.count("customer_package").await?, 1);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn duplicate_phone_points_at_the_existing_profile(pool: PgPool) -> TestResult {
    let s = shop(pool).await?;
    let mut body = s.four_of_ten();
    body["customer_id"] = Value::Null;
    body["customer"] = json!({"full_name": "Hà trùng số", "phone": "0905 111 222"});
    let taken = s.import(&s.owner, body).await;
    assert_eq!(
        code(&taken),
        (StatusCode::CONFLICT, Some("customer.phone_taken"))
    );
    assert_eq!(taken.body["existing_id"], json!(s.customer));
    assert_eq!(s.count("customer_package").await?, 0);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn another_spas_customer_or_plan_is_not_found(pool: PgPool) -> TestResult {
    let s = shop(pool).await?;
    create_tenant(&s.h.owner, "other", "Spa khác", "Huế").await?;
    add_member(
        &s.h.owner,
        "other",
        OTHER_OWNER_PHONE,
        "Chủ spa khác",
        SystemRole::Owner,
    )
    .await?;
    let intruder = session_for(&s.h, OTHER_OWNER_PHONE).await;

    // The intruder names our customer: indistinguishable from a missing one.
    let foreign = s.import(&intruder, s.four_of_ten()).await;
    assert_eq!(code(&foreign), (StatusCode::NOT_FOUND, Some("not_found")));

    // Our manager names an unknown plan.
    let mut body = s.four_of_ten();
    body["packages"][0]["package_plan_id"] = json!(Uuid::now_v7());
    assert_eq!(s.import(&s.owner, body).await.status, StatusCode::NOT_FOUND);
    assert_eq!(s.count("customer_package").await?, 0);
    Ok(())
}

// ------------------------------------------- EM decision 2026-11-05: no window, no refund cap (AC8.6–8.8)

impl Shop {
    async fn exec(&self, sql: &str) -> Result<(), sqlx::Error> {
        sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()))
            .execute(&self.h.owner)
            .await
            .map(|_| ())
    }

    async fn cancel(&self, package: &str, body: Value) -> Reply {
        call_with_headers(
            &self.h.app,
            "POST",
            &format!("/customer-packages/{package}/cancel"),
            Some(&self.owner),
            Some(body),
            &[("idempotency-key", &Uuid::now_v7().to_string())],
        )
        .await
    }
}

/// AC8.6–8.7: no "open import today" switch; the import lands on today's business day and the
/// owner is not asked for the password again. Since 0013 the manager may not import at all: the
/// paper books are an operator migration (spec management-views §7).
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn only_the_owner_imports_today_without_a_window_or_reauth(pool: PgPool) -> TestResult {
    let s = shop(pool).await?;
    let by_manager = s.import(&s.manager, s.four_of_ten()).await;
    assert_eq!(
        code(&by_manager),
        (StatusCode::FORBIDDEN, Some("auth.forbidden")),
        "{}",
        by_manager.body
    );
    let first = s.import(&s.owner, s.four_of_ten()).await;
    assert_eq!(first.status, StatusCode::CREATED, "{}", first.body);

    s.exec("UPDATE app.session SET recent_auth_at = now() - interval '2 days'")
        .await?;
    let by_owner = s.import(&s.owner, s.four_of_ten()).await;
    assert_eq!(by_owner.status, StatusCode::CREATED, "{}", by_owner.body);
    assert_eq!(s.count("customer_package").await?, 2);

    for method in ["GET", "PUT"] {
        let gone = call(
            &s.h.app,
            method,
            "/opening-balances/window",
            Some(&s.owner),
            (method == "PUT").then(|| json!({"open": true})),
        )
        .await;
        assert!(
            matches!(
                gone.status,
                StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED
            ),
            "{method} /opening-balances/window is removed: {}",
            gone.status
        );
    }
    let left: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app.role_permission WHERE permission = 'opening_balance.manage_window'",
    )
    .fetch_one(&s.h.owner)
    .await?;
    assert_eq!(left, 0, "migration 0012 removes the permission");
    Ok(())
}

/// AC8.8: a refund on a paper-book package is capped by its book price (`sold_price`), whoever
/// typed it; no confirmation step. The DB trigger enforces the same cap.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn an_opening_balance_refund_is_capped_by_its_book_price(pool: PgPool) -> TestResult {
    let s = shop(pool).await?;
    let imported = s.import(&s.owner, s.four_of_ten()).await;
    let package = imported.body["customer_packages"][0].clone();
    assert!(package.get("refund_cap").is_none(), "{package}");
    let id = package["id"].as_str().unwrap_or_default().to_owned();

    let insert_refund = |amount: i64| {
        format!(
            "INSERT INTO app.refund (id, tenant_id, branch_id, customer_id, customer_package_id,
                                     business_date, method, amount, reason, created_by)
             SELECT gen_random_uuid(), p.tenant_id, b.id, p.customer_id, p.id, current_date, 'cash',
                    {amount}, 'x', p.created_by
             FROM app.customer_package p JOIN app.branch b ON b.tenant_id = p.tenant_id
             WHERE p.id = '{id}'"
        )
    };
    assert!(
        s.exec(&insert_refund(1_500_001)).await.is_err(),
        "trigger refund_within_paid checks sold_price"
    );

    let refund = |amount: i64| json!({"reason": "Khách chuyển nhà", "refund_amount": amount, "refund_method": "cash"});
    let too_much = s.cancel(&id, refund(1_500_001)).await;
    assert_eq!(
        code(&too_much),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("refund.exceeds_paid")
        )
    );
    let mut with_confirm = refund(1_500_000);
    with_confirm["confirm_book_price"] = json!(true);
    let removed = s.cancel(&id, with_confirm).await;
    assert_eq!(
        removed.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "confirm_book_price is no longer part of the contract: {}",
        removed.body
    );

    let full = s.cancel(&id, refund(1_500_000)).await;
    assert_eq!(full.status, StatusCode::OK, "{}", full.body);
    assert_eq!(full.body["refund"]["amount"], 1_500_000);
    assert_eq!(s.count("refund").await?, 1);
    Ok(())
}
