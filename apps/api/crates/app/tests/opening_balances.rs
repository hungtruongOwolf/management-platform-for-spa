//! S9: importing part-used packages from the paper books (spec R1-1 US8, AC8.1, AC8.3–AC8.5).

mod common;

use axum::http::StatusCode;
use catalog::service::seed_menu;
use common::{
    Harness, OWNER_PHONE, PASSWORD, Reply, TestResult, add_member, call, call_with_headers,
    current_code, forget_used_step, harness, session_for,
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
    // Import is open only on a day the owner opened (M2, AC8.6): open today.
    let window = call(
        &h.app,
        "PUT",
        "/opening-balances/window",
        Some(&owner),
        Some(json!({"open": true})),
    )
    .await;
    assert_eq!(window.status, StatusCode::OK, "{}", window.body);
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
    let imported = s.import(&s.manager, s.four_of_ten()).await;
    assert_eq!(imported.status, StatusCode::CREATED, "{}", imported.body);
    assert_eq!(imported.body["customer_id"], json!(s.customer));
    assert_eq!(imported.body["customer_created"], false);
    let package = &imported.body["customer_packages"][0];
    assert_eq!(
        (
            package["sessions_remaining"].clone(),
            package["sessions_total"].clone(),
            package["origin"].clone(),
            package["sale_id"].clone(),
            package["status"].clone(),
        ),
        (
            json!(4),
            json!(10),
            json!("opening_balance"),
            Value::Null,
            json!("active")
        ),
        "AC8.1: còn 4/10, đánh dấu opening_balance"
    );
    assert_eq!(package["items"][0]["per_session_value"], 150_000);
    assert_eq!(package["items"][0]["name"], "Cơ Bản – Dầu thảo dược");
    assert_eq!(package["starts_on"], "2026-06-01");

    // AC8.1/AC8.3: no revenue — no sale, no sale line, no payment exists at all, today or ever.
    for table in ["sale", "sale_line", "payment"] {
        assert_eq!(s.count(table).await?, 0, "{table} must stay empty");
    }

    // AC8.5: the profile list shows the origin, so the web can label it "Nhập từ sổ cũ".
    let listed = call(
        &s.h.app,
        "GET",
        &format!("/customers/{}/packages", s.customer),
        Some(&s.manager),
        None,
    )
    .await;
    assert_eq!(listed.body[0]["origin"], "opening_balance");
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
    let rejected = s.import(&s.manager, body).await;
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
    assert_eq!(s.import(&s.manager, full).await.status, StatusCode::CREATED);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn every_bad_field_is_reported_at_once(pool: PgPool) -> TestResult {
    let s = shop(pool).await?;
    let rejected = s
        .import(
            &s.manager,
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
        let reply = s.import(&s.manager, body).await;
        assert_eq!(
            field_codes(&reply),
            [("customer_id".to_owned(), "exactly_one".to_owned())]
        );
    }
    let mut none = s.four_of_ten();
    none["packages"] = json!([]);
    assert_eq!(
        field_codes(&s.import(&s.manager, none).await),
        [("packages".to_owned(), "count_out_of_range".to_owned())]
    );
    let mut too_many_items = s.four_of_ten();
    too_many_items["packages"][0]["items"] = json!(vec![
        json!({"service_id": s.shampoo, "sessions_purchased": 1, "sessions_remaining": 1});
        21
    ]);
    assert_eq!(
        field_codes(&s.import(&s.manager, too_many_items).await),
        [(
            "packages[0].items".to_owned(),
            "count_out_of_range".to_owned()
        )]
    );
    let mut bad_profile = s.four_of_ten();
    bad_profile["customer_id"] = Value::Null;
    bad_profile["customer"] = json!({"full_name": "", "phone": "123"});
    let mut profile_fields = field_codes(&s.import(&s.manager, bad_profile).await);
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
    let first = s.post(&s.manager, &key, s.four_of_ten()).await;
    let second = s.post(&s.manager, &key, s.four_of_ten()).await;
    assert_eq!(first.status, StatusCode::CREATED, "{}", first.body);
    assert_eq!(second.status, StatusCode::CREATED);
    assert_eq!(first.body, second.body, "replayed, not re-imported");
    assert_eq!(s.count("customer_package").await?, 1);

    let mut changed = s.four_of_ten();
    changed["packages"][0]["items"][0]["sessions_remaining"] = json!(3);
    let reused = s.post(&s.manager, &key, changed).await;
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
        Some(&s.manager),
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
    let taken = s.import(&s.manager, body).await;
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
    assert_eq!(
        s.import(&s.manager, body).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(s.count("customer_package").await?, 0);
    Ok(())
}

// ------------------------------------------- M2: same-day import window + refund cap (AC8.6–8.8)

impl Shop {
    async fn window(&self, session: &str, open: bool) -> Reply {
        call(
            &self.h.app,
            "PUT",
            "/opening-balances/window",
            Some(session),
            Some(json!({"open": open})),
        )
        .await
    }

    async fn window_status(&self, session: &str) -> Reply {
        call(
            &self.h.app,
            "GET",
            "/opening-balances/window",
            Some(session),
            None,
        )
        .await
    }

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

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn managers_import_only_on_a_day_the_owner_opened(pool: PgPool) -> TestResult {
    let s = shop(pool).await?;
    let status = s.window_status(&s.manager).await;
    assert_eq!(status.body["open_today"], true, "{}", status.body);

    assert_eq!(s.window(&s.owner, false).await.status, StatusCode::OK);
    let closed = s.import(&s.manager, s.four_of_ten()).await;
    assert_eq!(
        code(&closed),
        (StatusCode::FORBIDDEN, Some("opening_balance.closed")),
        "AC8.7: a day the owner did not open"
    );
    assert_eq!(
        code(&s.window(&s.manager, true).await),
        (StatusCode::FORBIDDEN, Some("auth.forbidden")),
        "only the owner opens the window"
    );

    assert_eq!(s.window(&s.owner, true).await.body["open_today"], true);
    let open = s.import(&s.manager, s.four_of_ten()).await;
    assert_eq!(open.status, StatusCode::CREATED, "{}", open.body);

    // Opened yesterday = closed today: the window lapses at midnight with no job to run.
    s.exec("UPDATE app.opening_balance_window SET open_on = open_on - 1")
        .await?;
    let next_day = s.import(&s.manager, s.four_of_ten()).await;
    assert_eq!(
        code(&next_day),
        (StatusCode::FORBIDDEN, Some("opening_balance.closed"))
    );
    assert_eq!(s.window_status(&s.manager).await.body["open_today"], false);
    assert_eq!(s.count("customer_package").await?, 1);

    let audit: Vec<String> = sqlx::query_scalar(
        "SELECT action FROM app.audit_log WHERE action LIKE 'opening_balance.window%' ORDER BY at, id",
    )
    .fetch_all(&s.h.owner)
    .await?;
    assert_eq!(
        audit,
        [
            "opening_balance.window_open",
            "opening_balance.window_close",
            "opening_balance.window_open"
        ]
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn the_owner_imports_on_a_closed_day_only_after_reauth(pool: PgPool) -> TestResult {
    let s = shop(pool).await?;
    assert_eq!(s.window(&s.owner, false).await.status, StatusCode::OK);
    s.exec("UPDATE app.session SET recent_auth_at = now() - interval '10 minutes'")
        .await?;
    let stale = s.import(&s.owner, s.four_of_ten()).await;
    assert_eq!(
        code(&stale),
        (StatusCode::UNAUTHORIZED, Some("auth.reauth_required")),
        "🔒 on a closed day"
    );
    assert_eq!(s.count("customer_package").await?, 0);

    forget_used_step(&s.h, OWNER_PHONE).await?;
    let totp = current_code(&s.h, OWNER_PHONE).await?;
    let reauth = call(
        &s.h.app,
        "POST",
        "/auth/reauth",
        Some(&s.owner),
        Some(json!({"password": PASSWORD, "code": totp})),
    )
    .await;
    assert_eq!(reauth.status, StatusCode::NO_CONTENT);
    let fresh = s.import(&s.owner, s.four_of_ten()).await;
    assert_eq!(fresh.status, StatusCode::CREATED, "{}", fresh.body);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_book_price_typed_by_a_manager_is_no_refund_cap_until_the_owner_confirms(
    pool: PgPool,
) -> TestResult {
    let s = shop(pool).await?;
    let by_manager = s.import(&s.manager, s.four_of_ten()).await;
    let by_owner = s.import(&s.owner, s.four_of_ten()).await;
    let manager_pkg = by_manager.body["customer_packages"][0].clone();
    let owner_pkg = by_owner.body["customer_packages"][0].clone();
    assert_eq!(manager_pkg["refund_cap"], 0, "AC8.8: {}", by_manager.body);
    assert_eq!(owner_pkg["refund_cap"], 1_500_000);
    let manager_id = manager_pkg["id"].as_str().unwrap_or_default().to_owned();
    let owner_id = owner_pkg["id"].as_str().unwrap_or_default().to_owned();
    let refund = |confirm: bool| {
        json!({"reason": "Khách chuyển nhà", "refund_amount": 300_000, "refund_method": "cash",
               "confirm_book_price": confirm})
    };

    // The database refuses a refund above the cap even if the service check were bypassed.
    let bypass = s
        .exec(&format!(
            "INSERT INTO app.refund (id, tenant_id, branch_id, customer_id, customer_package_id,
                                     business_date, method, amount, reason, created_by)
             SELECT gen_random_uuid(), p.tenant_id, b.id, p.customer_id, p.id, current_date, 'cash',
                    1, 'x', p.created_by
             FROM app.customer_package p JOIN app.branch b ON b.tenant_id = p.tenant_id
             WHERE p.id = '{manager_id}'"
        ))
        .await;
    assert!(
        bypass.is_err(),
        "trigger refund_within_paid uses refund_cap"
    );

    let unconfirmed = s.cancel(&manager_id, refund(false)).await;
    assert_eq!(
        code(&unconfirmed),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("refund.book_price_unconfirmed")
        )
    );
    let confirmed = s.cancel(&manager_id, refund(true)).await;
    assert_eq!(confirmed.status, StatusCode::OK, "{}", confirmed.body);
    assert_eq!(confirmed.body["refund"]["amount"], 300_000);
    assert_eq!(confirmed.body["refund_cap"], 1_500_000);
    let audited: Value = sqlx::query_scalar(
        "SELECT after FROM app.audit_log
         WHERE action = 'customer_package.cancel' AND entity_id = $1::uuid",
    )
    .bind(&manager_id)
    .fetch_one(&s.h.owner)
    .await?;
    assert_eq!(audited["book_price_confirmed"], true);

    // The owner typed this one: the book price already caps the refund.
    let owner_cancel = s.cancel(&owner_id, refund(false)).await;
    assert_eq!(owner_cancel.status, StatusCode::OK, "{}", owner_cancel.body);
    assert_eq!(s.count("refund").await?, 2);
    Ok(())
}
