//! S6: selling packages (spec R1-1 US3, AC3.1–AC3.7; AC1.2 snapshot).

mod common;

use axum::http::{StatusCode, header};
use catalog::service::seed_menu;
use common::{
    Harness, OWNER_PHONE, Reply, TestResult, add_member, call, call_with_headers, harness,
    session_for,
};
use identity::SystemRole;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const MANAGER_PHONE: &str = "0912000111";
const TECH_PHONE: &str = "0912000222";

struct Shop {
    h: Harness,
    owner: String,
    manager: String,
    customer: String,
    plan: String,
    lan: String,
}

type Setup<T> = Result<T, Box<dyn std::error::Error>>;

/// Spa with the printed menu, a "10 + 2 bonus at 1.5M, 6 months" plan, a customer, a closer
/// (KTV Lan) and a manager.
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
    let shampoo = services
        .body
        .as_array()
        .and_then(|l| l.iter().find(|s| s["name"] == "Cơ Bản – Dầu thảo dược"))
        .map(|s| s["id"].clone())
        .unwrap_or(Value::Null);
    let plan = call(
        &h.app,
        "POST",
        "/package-plans",
        Some(&owner),
        Some(json!({
            "name": "Gội đầu dưỡng sinh 10 buổi + tặng 2", "price": 1_500_000, "validity_months": 6,
            "gift_note": "Tặng 1 chai dầu gội", "active": true,
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
    let lan = org::provisioning::add_staff(&h.owner, "vpyd", "KTV Lan", "Kỹ thuật viên").await?;
    Ok(Shop {
        owner,
        manager,
        customer: id_of(&customer),
        plan: id_of(&plan),
        lan: lan.to_string(),
        h,
    })
}

fn id_of(reply: &Reply) -> String {
    reply.body["id"].as_str().unwrap_or_default().to_owned()
}

impl Shop {
    fn sale(&self, payments: Value) -> Value {
        json!({
            "customer_id": self.customer, "closer_staff_id": self.lan,
            "lines": [{"package_plan_id": self.plan}],
            "payments": payments,
        })
    }

    fn cash(&self, amount: i64) -> Value {
        self.sale(json!([{"method": "cash", "amount": amount}]))
    }

    async fn post(&self, session: &str, key: &str, body: Value) -> Reply {
        call_with_headers(
            &self.h.app,
            "POST",
            "/sales",
            Some(session),
            Some(body),
            &[("idempotency-key", key)],
        )
        .await
    }

    async fn sell(&self, session: &str, body: Value) -> Reply {
        self.post(session, &Uuid::now_v7().to_string(), body).await
    }

    async fn get(&self, session: &str, path: &str) -> Reply {
        call(&self.h.app, "GET", path, Some(session), None).await
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

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn ten_plus_two_at_1_5m_gives_12_sessions_worth_150k_each(pool: PgPool) -> TestResult {
    let s = shop(pool).await?;
    let sold = s.sell(&s.manager, s.cash(1_500_000)).await;
    assert_eq!(sold.status, StatusCode::CREATED, "{}", sold.body);
    assert_eq!(
        (sold.body["total"].clone(), sold.body["amount_paid"].clone()),
        (json!(1_500_000), json!(1_500_000))
    );
    assert_eq!(sold.body["closer_name"], "KTV Lan");
    let package = &sold.body["customer_packages"][0];
    assert_eq!(
        (
            package["sessions_total"].clone(),
            package["sessions_remaining"].clone(),
            package["status"].clone()
        ),
        (json!(12), json!(12), json!("active")),
        "AC3.1: còn 12/12"
    );
    assert_eq!(package["items"][0]["per_session_value"], 150_000, "D4");
    assert_eq!(package["items"][0]["name"], "Cơ Bản – Dầu thảo dược");
    assert_eq!(package["gift_note"], "Tặng 1 chai dầu gội");
    let starts = package["starts_on"].as_str().unwrap_or_default();
    let expires = package["expires_on"].as_str().unwrap_or_default();
    let start = chrono::NaiveDate::parse_from_str(starts, "%Y-%m-%d")?;
    assert_eq!(
        Some(expires.to_owned()),
        start
            .checked_add_months(chrono::Months::new(6))
            .map(|d| d.to_string()),
        "expiry = sale day + 6 months"
    );
    assert_eq!(sold.body["lines"][0]["customer_package_id"], package["id"]);

    let sale_id = id_of(&sold);
    let read = s.get(&s.manager, &format!("/sales/{sale_id}")).await;
    assert_eq!(read.status, StatusCode::OK);
    assert_eq!(read.body["payments"][0]["method"], "cash");

    let listed = s
        .get(&s.manager, &format!("/customers/{}/packages", s.customer))
        .await;
    assert_eq!(listed.status, StatusCode::OK);
    assert_eq!(listed.body.as_array().map(Vec::len), Some(1));
    let one = s
        .get(
            &s.manager,
            &format!(
                "/customer-packages/{}",
                package["id"].as_str().unwrap_or("")
            ),
        )
        .await;
    assert_eq!(one.body["sessions_remaining"], 12);

    let audit: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app.audit_log WHERE action IN ('sale.create', 'customer_package.create')",
    )
    .fetch_one(&s.h.owner)
    .await?;
    assert_eq!(audit, 2);
    Ok(())
}

/// Security review INFO: taking money on `POST /sales` needs `payment.collect`, as on `/visits`.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn taking_money_needs_payment_collect(pool: PgPool) -> TestResult {
    let s = shop(pool).await?;
    sqlx::query(
        "DELETE FROM app.role_permission rp USING app.role r
         WHERE rp.role_id = r.id AND r.key = 'manager' AND rp.permission = 'payment.collect'",
    )
    .execute(&s.h.owner)
    .await?;
    let refused = s.sell(&s.manager, s.cash(1_500_000)).await;
    assert_eq!(
        code(&refused),
        (StatusCode::FORBIDDEN, Some("auth.forbidden"))
    );
    assert_eq!(s.count("sale").await?, 0);
    assert_eq!(s.count("payment").await?, 0);

    let owner = s.sell(&s.owner, s.cash(1_500_000)).await;
    assert_eq!(owner.status, StatusCode::CREATED, "{}", owner.body);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn mixed_cash_and_transfer_is_two_payments(pool: PgPool) -> TestResult {
    let s = shop(pool).await?;
    let sold = s
        .sell(
            &s.manager,
            s.sale(json!([
                {"method": "cash", "amount": 500_000},
                {"method": "bank_transfer", "amount": 1_000_000, "reference": "VCB 123"},
            ])),
        )
        .await;
    assert_eq!(sold.status, StatusCode::CREATED, "{}", sold.body);
    assert_eq!(sold.body["payments"].as_array().map(Vec::len), Some(2));
    let paid: i64 =
        sqlx::query_scalar("SELECT sum(amount)::bigint FROM app.payment WHERE kind = 'payment'")
            .fetch_one(&s.h.owner)
            .await?;
    assert_eq!(paid, 1_500_000, "AC3.2");

    let same_method_twice = s
        .sell(
            &s.manager,
            s.sale(json!([
                {"method": "cash", "amount": 500_000},
                {"method": "cash", "amount": 1_000_000},
            ])),
        )
        .await;
    assert_eq!(same_method_twice.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        same_method_twice.body["errors"][0]["field"],
        "payments[1].method"
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn payments_must_add_up_to_the_total_exactly(pool: PgPool) -> TestResult {
    let s = shop(pool).await?;
    for (case, body) in [
        ("short", s.cash(1_400_000)),
        ("over", s.cash(2_000_000)),
        ("nothing", s.sale(json!([]))),
    ] {
        let reply = s.sell(&s.manager, body).await;
        assert_eq!(
            code(&reply),
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Some("payment.amount_mismatch")
            ),
            "AC3.3 {case}"
        );
    }
    for table in ["sale", "payment", "customer_package"] {
        assert_eq!(s.count(table).await?, 0, "nothing written to {table}");
    }
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn discounts_need_a_reason_and_managers_stop_at_ten_percent(pool: PgPool) -> TestResult {
    let s = shop(pool).await?;
    let with_discount = |amount: i64, reason: Option<&str>| {
        let mut body = s.cash(1_500_000 - amount);
        body["discount"] = json!({"amount": amount, "reason": reason});
        body
    };

    let no_reason = s.sell(&s.manager, with_discount(100_000, None)).await;
    assert_eq!(
        code(&no_reason),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("discount.reason_required")
        )
    );
    let blank_reason = s.sell(&s.manager, with_discount(100_000, Some("  "))).await;
    assert_eq!(blank_reason.status, StatusCode::UNPROCESSABLE_ENTITY);

    let over = s
        .sell(&s.manager, with_discount(150_001, Some("Khách quen")))
        .await;
    assert_eq!(
        code(&over),
        (StatusCode::FORBIDDEN, Some("discount.limit_exceeded")),
        "AC3.5 / authz test 6"
    );

    let at_cap = s
        .sell(&s.manager, with_discount(150_000, Some("Khách quen")))
        .await;
    assert_eq!(at_cap.status, StatusCode::CREATED, "{}", at_cap.body);
    let package = &at_cap.body["customer_packages"][0];
    assert_eq!(package["sold_price"], 1_350_000);
    assert_eq!(
        package["items"][0]["per_session_value"], 135_000,
        "value follows the price after discount"
    );
    assert_eq!(at_cap.body["discount_reason"], "Khách quen");

    let owner_big = s
        .sell(&s.owner, with_discount(600_000, Some("Chủ spa tặng")))
        .await;
    assert_eq!(
        owner_big.status,
        StatusCode::CREATED,
        "the owner has no cap: {}",
        owner_big.body
    );

    let beyond_subtotal = s.sell(&s.owner, with_discount(1_600_000, Some("x"))).await;
    assert_eq!(beyond_subtotal.status, StatusCode::UNPROCESSABLE_ENTITY);
    let reason: Option<String> = sqlx::query_scalar(
        "SELECT reason FROM app.audit_log WHERE action = 'sale.create' ORDER BY at LIMIT 1",
    )
    .fetch_one(&s.h.owner)
    .await?;
    assert_eq!(
        reason.as_deref(),
        Some("Khách quen"),
        "discount reason audited"
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn the_closer_must_be_working_here(pool: PgPool) -> TestResult {
    let s = shop(pool).await?;
    sqlx::query("UPDATE app.staff SET status = 'left', left_on = current_date")
        .execute(&s.h.owner)
        .await?;
    let left = s.sell(&s.manager, s.cash(1_500_000)).await;
    assert_eq!(
        code(&left),
        (StatusCode::UNPROCESSABLE_ENTITY, Some("staff.inactive")),
        "AC3.6"
    );

    identity::provisioning::create_tenant(&s.h.owner, "spa-b", "Spa B", "Cơ sở 1").await?;
    let elsewhere = org::provisioning::add_staff(&s.h.owner, "spa-b", "KTV B", "KTV").await?;
    let mut body = s.cash(1_500_000);
    body["closer_staff_id"] = json!(elsewhere);
    let foreign = s.sell(&s.manager, body).await;
    assert_eq!(
        code(&foreign),
        (StatusCode::UNPROCESSABLE_ENTITY, Some("staff.inactive"))
    );
    assert_eq!(s.count("sale").await?, 0);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn saving_twice_with_one_key_creates_one_sale(pool: PgPool) -> TestResult {
    let s = shop(pool).await?;
    let key = Uuid::now_v7().to_string();
    let first = s.post(&s.manager, &key, s.cash(1_500_000)).await;
    let again = s.post(&s.manager, &key, s.cash(1_500_000)).await;
    assert_eq!(first.status, StatusCode::CREATED);
    assert_eq!(again.status, StatusCode::CREATED);
    assert_eq!(first.body, again.body, "replayed, not re-run");

    // Two simultaneous submits (double click): the second waits for the first, then replays.
    let key2 = Uuid::now_v7().to_string();
    let (a, b) = tokio::join!(
        s.post(&s.manager, &key2, s.cash(1_500_000)),
        s.post(&s.manager, &key2, s.cash(1_500_000)),
    );
    assert_eq!(
        (a.status, b.status),
        (StatusCode::CREATED, StatusCode::CREATED)
    );
    assert_eq!(a.body["id"], b.body["id"]);

    assert_eq!(s.count("sale").await?, 2, "AC3.7: one sale per key");
    assert_eq!(s.count("customer_package").await?, 2);
    assert_eq!(s.count("payment").await?, 2);

    let reused = s.post(&s.manager, &key, s.cash(1_400_000)).await;
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
        "/sales",
        Some(&s.manager),
        Some(s.cash(1_500_000)),
    )
    .await;
    assert_eq!(
        code(&keyless),
        (StatusCode::BAD_REQUEST, Some("idempotency.key_required"))
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn prices_sent_by_the_client_are_ignored(pool: PgPool) -> TestResult {
    let s = shop(pool).await?;
    let mut body = s.cash(1_500_000);
    body["total"] = json!(1);
    body["subtotal"] = json!(1);
    body["lines"][0]["unit_price"] = json!(1);
    body["lines"][0]["price"] = json!(1);
    body["lines"][0]["per_session_value"] = json!(1);
    let sold = s.sell(&s.manager, body).await;
    assert_eq!(sold.status, StatusCode::CREATED, "{}", sold.body);
    assert_eq!(sold.body["lines"][0]["unit_price"], 1_500_000);
    assert_eq!(sold.body["total"], 1_500_000);
    assert_eq!(
        sold.body["customer_packages"][0]["items"][0]["per_session_value"],
        150_000
    );

    // Paying the client's fake total is a mismatch against the real one.
    let mut cheap = s.cash(1);
    cheap["total"] = json!(1);
    let rejected = s.sell(&s.manager, cheap).await;
    assert_eq!(
        code(&rejected),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("payment.amount_mismatch")
        )
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn payments_cannot_be_updated_or_deleted(pool: PgPool) -> TestResult {
    let s = shop(pool).await?;
    let sold = s.sell(&s.manager, s.cash(1_500_000)).await;
    assert_eq!(sold.status, StatusCode::CREATED);
    let tenant: Uuid = sqlx::query_scalar("SELECT id FROM app.tenant WHERE slug = 'vpyd'")
        .fetch_one(&s.h.owner)
        .await?;
    let app = kernel::testing::app_pool(&s.h.owner).await?;
    for statement in [
        "UPDATE app.payment SET amount = 1",
        "DELETE FROM app.payment",
        "UPDATE app.sale_line SET unit_price = 1",
        "DELETE FROM app.sale_line",
    ] {
        let mut tx = kernel::TenantTx::begin(&app, kernel::TenantId::from_uuid(tenant)).await?;
        let result = sqlx::query(statement).execute(tx.conn()).await;
        assert!(result.is_err(), "app_rw: {statement} must fail");
    }
    // Even the schema owner is stopped by the trigger.
    let owner_update = sqlx::query("UPDATE app.payment SET amount = 1")
        .execute(&s.h.owner)
        .await;
    assert!(owner_update.is_err());
    let amount: i64 = sqlx::query_scalar("SELECT amount FROM app.payment")
        .fetch_one(&s.h.owner)
        .await?;
    assert_eq!(amount, 1_500_000);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn technicians_and_other_spas_are_kept_out(pool: PgPool) -> TestResult {
    let s = shop(pool).await?;
    let sold = s.sell(&s.manager, s.cash(1_500_000)).await;
    let sale_id = id_of(&sold);
    let package_id = sold.body["customer_packages"][0]["id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();

    add_member(
        &s.h.owner,
        "vpyd",
        TECH_PHONE,
        "KTV Mai",
        SystemRole::Technician,
    )
    .await?;
    let tech = session_for(&s.h, TECH_PHONE).await;
    assert_eq!(
        s.sell(&tech, s.cash(1_500_000)).await.status,
        StatusCode::FORBIDDEN
    );
    for path in [
        format!("/sales/{sale_id}"),
        format!("/customer-packages/{package_id}"),
        format!("/customers/{}/packages", s.customer),
    ] {
        assert_eq!(
            s.get(&tech, &path).await.status,
            StatusCode::FORBIDDEN,
            "{path}"
        );
    }
    let staff = s.get(&tech, "/staff").await;
    assert_eq!(
        staff.status,
        StatusCode::OK,
        "picker is open to technicians"
    );
    assert_eq!(staff.body[0]["full_name"], "KTV Lan");
    assert!(staff.body[0].get("phone").is_none());

    identity::provisioning::create_tenant(&s.h.owner, "spa-b", "Spa B", "Cơ sở 1").await?;
    add_member(
        &s.h.owner,
        "spa-b",
        "0987000999",
        "Chủ Spa B",
        SystemRole::Owner,
    )
    .await?;
    let other = session_for(&s.h, "0987000999").await;
    for path in [
        format!("/sales/{sale_id}"),
        format!("/customer-packages/{package_id}"),
        format!("/customers/{}/packages", s.customer),
    ] {
        assert_eq!(
            s.get(&other, &path).await.status,
            StatusCode::NOT_FOUND,
            "{path}"
        );
    }
    assert_eq!(s.get(&other, "/staff").await.body, json!([]));
    // Spa B cannot sell spa A's plan to spa A's customer.
    let foreign = s.sell(&other, s.cash(1_500_000)).await;
    assert_eq!(foreign.status, StatusCode::NOT_FOUND);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn later_plan_edits_do_not_change_sold_packages(pool: PgPool) -> TestResult {
    let s = shop(pool).await?;
    let sold = s.sell(&s.manager, s.cash(1_500_000)).await;
    let package_id = sold.body["customer_packages"][0]["id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let plan = s.get(&s.owner, "/package-plans").await.body[0].clone();
    let edited = call_with_headers(
        &s.h.app,
        "PUT",
        &format!("/package-plans/{}", s.plan),
        Some(&s.owner),
        Some(json!({
            "name": "Gội đầu 8 buổi", "price": 2_000_000, "validity_months": 3, "gift_note": null,
            "active": true,
            "items": [{"service_id": plan["items"][0]["service_id"], "sessions": 8, "bonus_sessions": 0}],
        })),
        &[(header::IF_MATCH.as_str(), "\"1\"")],
    )
    .await;
    assert_eq!(edited.status, StatusCode::OK, "{}", edited.body);
    let after = s
        .get(&s.manager, &format!("/customer-packages/{package_id}"))
        .await;
    assert_eq!(
        (
            after.body["name"].clone(),
            after.body["sold_price"].clone(),
            after.body["sessions_total"].clone()
        ),
        (
            json!("Gội đầu dưỡng sinh 10 buổi + tặng 2"),
            json!(1_500_000),
            json!(12)
        ),
        "AC1.2 snapshot"
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn inactive_plans_and_unknown_customers_cannot_be_sold(pool: PgPool) -> TestResult {
    let s = shop(pool).await?;
    sqlx::query("UPDATE app.package_plan SET active = false")
        .execute(&s.h.owner)
        .await?;
    let inactive = s.sell(&s.manager, s.cash(1_500_000)).await;
    assert_eq!(inactive.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        inactive.body["errors"][0]["field"],
        "lines[0].package_plan_id"
    );

    let mut nobody = s.cash(1_500_000);
    nobody["customer_id"] = json!(Uuid::now_v7());
    assert_eq!(
        s.sell(&s.manager, nobody).await.status,
        StatusCode::NOT_FOUND
    );
    let mut empty = s.cash(0);
    empty["lines"] = json!([]);
    assert_eq!(
        s.sell(&s.manager, empty).await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    Ok(())
}
