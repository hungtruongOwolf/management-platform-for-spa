//! S4: owner manages the price list and package plans (spec R1-1 US1, AC1.1–AC1.8).

mod common;

use axum::http::{StatusCode, header};
use catalog::service::seed_menu;
use common::{OWNER_PHONE, Reply, TestResult, add_member, call, harness, session_for};
use identity::SystemRole;
use serde_json::{Value, json};
use sqlx::PgPool;

struct Owner {
    h: common::Harness,
    token: String,
}

async fn owner(pool: PgPool) -> Result<Owner, Box<dyn std::error::Error>> {
    let h = harness(pool).await?;
    seed_menu(&h.owner, "vpyd").await?;
    let token = session_for(&h, OWNER_PHONE).await;
    Ok(Owner { h, token })
}

impl Owner {
    async fn get(&self, path: &str) -> Reply {
        call(&self.h.app, "GET", path, Some(&self.token), None).await
    }
    async fn send(&self, method: &str, path: &str, body: Value) -> Reply {
        call(&self.h.app, method, path, Some(&self.token), Some(body)).await
    }
    async fn put_versioned(&self, path: &str, version: &str, body: Value) -> Reply {
        common::call_with_headers(
            &self.h.app,
            "PUT",
            path,
            Some(&self.token),
            Some(body),
            &[(header::IF_MATCH.as_str(), version)],
        )
        .await
    }
    async fn service(&self, name: &str) -> Value {
        let all = self.get("/services?active=false").await.body;
        all.as_array()
            .and_then(|l| l.iter().find(|s| s["name"] == name).cloned())
            .unwrap_or(Value::Null)
    }
    async fn category_id(&self, name: &str) -> String {
        let cats = self.get("/service-categories").await.body;
        cats.as_array()
            .and_then(|l| l.iter().find(|c| c["name"] == name))
            .and_then(|c| c["id"].as_str())
            .unwrap_or_default()
            .to_owned()
    }
}

fn ten_shampoo_plan(service_id: &Value) -> Value {
    json!({
        "name": "Gội đầu dưỡng sinh 10 buổi + tặng 2",
        "price": 1_500_000,
        "validity_months": 6,
        "gift_note": null,
        "active": true,
        "items": [{"service_id": service_id, "sessions": 10, "bonus_sessions": 2}],
    })
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn owner_creates_a_package_plan_visible_at_the_counter(pool: PgPool) -> TestResult {
    let o = owner(pool).await?;
    let shampoo = o.service("Cơ Bản – Dầu thảo dược").await;
    let created = o
        .send("POST", "/package-plans", ten_shampoo_plan(&shampoo["id"]))
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    assert_eq!(
        created
            .headers
            .get(header::ETAG)
            .and_then(|v| v.to_str().ok()),
        Some("\"1\"")
    );
    assert_eq!(created.body["items"][0]["bonus_sessions"], 2);

    let plans = o.get("/package-plans").await;
    assert_eq!(
        plans.body.as_array().map(Vec::len),
        Some(1),
        "AC1.1: listed for sale"
    );

    let audit: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app.audit_log WHERE action = 'catalog.plan_create'",
    )
    .fetch_one(&o.h.owner)
    .await?;
    assert_eq!(audit, 1);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn invalid_plans_are_rejected_with_field_errors(pool: PgPool) -> TestResult {
    let o = owner(pool).await?;
    let shampoo = o.service("Cơ Bản – Dầu thảo dược").await;
    let unpriced = o.service("Ôn ấm tử cung").await;

    let mut empty = ten_shampoo_plan(&shampoo["id"]);
    empty["items"] = json!([]);
    let mut zero = ten_shampoo_plan(&shampoo["id"]);
    zero["items"][0]["sessions"] = json!(0);
    let mut duplicate = ten_shampoo_plan(&shampoo["id"]);
    duplicate["items"] = json!([
        {"service_id": shampoo["id"], "sessions": 5, "bonus_sessions": 0},
        {"service_id": shampoo["id"], "sessions": 5, "bonus_sessions": 0},
    ]);
    let mut inactive_service = ten_shampoo_plan(&unpriced["id"]);
    inactive_service["name"] = json!("Gói tử cung");
    let mut negative = ten_shampoo_plan(&shampoo["id"]);
    negative["price"] = json!(-1);

    for (case, body, field, code) in [
        ("no items", empty, "items", "required"),
        ("zero sessions", zero, "items[0].sessions", "out_of_range"),
        (
            "duplicate service",
            duplicate,
            "items[1].service_id",
            "duplicate",
        ),
        (
            "inactive service",
            inactive_service,
            "items[0].service_id",
            "inactive",
        ),
        ("negative price", negative, "price", "negative"),
    ] {
        let reply = o.send("POST", "/package-plans", body).await;
        assert_eq!(
            reply.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{case}: {}",
            reply.body
        );
        let errors = reply.body["errors"].as_array().cloned().unwrap_or_default();
        assert!(
            errors
                .iter()
                .any(|e| e["field"] == field && e["code"] == code),
            "{case}: {}",
            reply.body
        );
    }

    assert_eq!(
        o.send("POST", "/package-plans", ten_shampoo_plan(&shampoo["id"]))
            .await
            .status,
        StatusCode::CREATED
    );
    let same_name = o
        .send("POST", "/package-plans", ten_shampoo_plan(&shampoo["id"]))
        .await;
    assert_eq!(
        same_name.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "AC1.5: duplicate name of an active plan"
    );
    assert_eq!(same_name.body["errors"][0]["field"], "name");
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn owner_edits_a_service_with_optimistic_locking(pool: PgPool) -> TestResult {
    let o = owner(pool).await?;
    let thu_gian = o.service("Thư Giãn").await;
    let path = format!("/services/{}", thu_gian["id"].as_str().unwrap_or_default());
    let mut body = json!({
        "category_id": thu_gian["category_id"], "name": "Thư Giãn", "description": "+ Cạo gió đầu, đá nóng",
        "duration_min": 45, "recommended": true, "active": true, "prices": {"female": 89_000, "male": 109_000},
    });

    let updated = o.put_versioned(&path, "\"1\"", body.clone()).await;
    assert_eq!(updated.status, StatusCode::OK, "{}", updated.body);
    assert_eq!(
        (
            updated.body["version"].clone(),
            updated.body["prices"].clone()
        ),
        (json!(2), json!({"female": 89_000, "male": 109_000}))
    );

    body["prices"] = json!({"female": 1, "male": 1});
    let stale = o.put_versioned(&path, "\"1\"", body.clone()).await;
    assert_eq!(
        (stale.status, stale.body["code"].as_str()),
        (
            StatusCode::PRECONDITION_FAILED,
            Some("conflict.stale_version")
        )
    );
    assert_eq!(
        o.service("Thư Giãn").await["prices"]["female"],
        89_000,
        "stale write changed nothing"
    );

    let missing =
        common::call_with_headers(&o.h.app, "PUT", &path, Some(&o.token), Some(body), &[]).await;
    assert_eq!(
        (missing.status, missing.body["code"].as_str()),
        (
            StatusCode::BAD_REQUEST,
            Some("precondition.if_match_required")
        )
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn price_variant_rules_are_enforced(pool: PgPool) -> TestResult {
    let o = owner(pool).await?;
    let category = o.category_id("Dịch vụ bổ sung").await;
    let service = |prices: Value, active: bool| {
        json!({
            "category_id": category, "name": "Ngâm chân muối hồng", "description": null,
            "duration_min": null, "recommended": false, "active": active, "prices": prices,
        })
    };

    let mixed = o
        .send(
            "POST",
            "/services",
            service(json!({"all": 100_000, "male": 120_000}), true),
        )
        .await;
    assert_eq!(
        mixed.body["errors"][0],
        json!({"field": "prices", "code": "mixed_variants"})
    );
    let none = o.send("POST", "/services", service(json!({}), true)).await;
    assert_eq!(
        none.body["errors"][0],
        json!({"field": "prices", "code": "required"}),
        "AC1.8: sellable service needs a price"
    );

    let draft = o.send("POST", "/services", service(json!({}), false)).await;
    assert_eq!(
        draft.status,
        StatusCode::CREATED,
        "inactive draft may wait for its price: {}",
        draft.body
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn activating_the_unpriced_service_requires_its_price(pool: PgPool) -> TestResult {
    let o = owner(pool).await?;
    let on_am = o.service("Ôn ấm tử cung").await;
    let path = format!("/services/{}", on_am["id"].as_str().unwrap_or_default());
    let mut body = json!({
        "category_id": on_am["category_id"], "name": "Ôn ấm tử cung", "description": null,
        "duration_min": 120, "recommended": true, "active": true, "prices": {},
    });
    assert_eq!(
        o.put_versioned(&path, "\"1\"", body.clone()).await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );

    body["prices"] = json!({"female": 399_000});
    let priced = o.put_versioned(&path, "\"1\"", body).await;
    assert_eq!(priced.status, StatusCode::OK, "{}", priced.body);
    let active = o.get("/services").await.body;
    assert!(
        active
            .as_array()
            .is_some_and(|l| l.iter().any(|s| s["name"] == "Ôn ấm tử cung")),
        "now on sale, female only"
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn deactivated_services_leave_the_menu_but_are_never_deleted(pool: PgPool) -> TestResult {
    let o = owner(pool).await?;
    let mask = o.service("Mặt nạ thuốc bắc").await;
    let path = format!("/services/{}", mask["id"].as_str().unwrap_or_default());
    let body = json!({
        "category_id": mask["category_id"], "name": "Mặt nạ thuốc bắc", "description": null,
        "duration_min": null, "recommended": false, "active": false, "prices": {"all": 50_000},
    });
    assert_eq!(
        o.put_versioned(&path, "\"1\"", body).await.status,
        StatusCode::OK
    );
    let on_sale = o.get("/services").await.body;
    assert!(
        !on_sale
            .as_array()
            .is_some_and(|l| l.iter().any(|s| s["name"] == "Mặt nạ thuốc bắc")),
        "AC1.3"
    );

    let delete = call(&o.h.app, "DELETE", &path, Some(&o.token), None).await;
    assert_eq!(
        delete.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "AC1.6: no delete endpoint"
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn managers_cannot_change_the_price_list(pool: PgPool) -> TestResult {
    let o = owner(pool).await?;
    add_member(
        &o.h.owner,
        "vpyd",
        "0912000444",
        "Quản lý Ngọc",
        SystemRole::Manager,
    )
    .await?;
    let manager = session_for(&o.h, "0912000444").await;
    let thu_gian = o.service("Thư Giãn").await;
    let path = format!("/services/{}", thu_gian["id"].as_str().unwrap_or_default());
    let body = json!({
        "category_id": thu_gian["category_id"], "name": "Thư Giãn", "description": null, "duration_min": 45,
        "recommended": false, "active": true, "prices": {"all": 1},
    });
    let reply = common::call_with_headers(
        &o.h.app,
        "PUT",
        &path,
        Some(&manager),
        Some(body),
        &[("if-match", "\"1\"")],
    )
    .await;
    assert_eq!(
        (reply.status, reply.body["code"].as_str()),
        (StatusCode::FORBIDDEN, Some("auth.forbidden")),
        "AC1.4"
    );
    let create = call(
        &o.h.app,
        "POST",
        "/service-categories",
        Some(&manager),
        Some(json!({"name": "Mới"})),
    )
    .await;
    assert_eq!(create.status, StatusCode::FORBIDDEN);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn another_spa_cannot_edit_this_catalog(pool: PgPool) -> TestResult {
    let o = owner(pool).await?;
    identity::provisioning::create_tenant(&o.h.owner, "spa-b", "Spa B", "Cơ sở 1").await?;
    add_member(
        &o.h.owner,
        "spa-b",
        "0987000555",
        "Chủ Spa B",
        SystemRole::Owner,
    )
    .await?;
    let other = session_for(&o.h, "0987000555").await;
    let thu_gian = o.service("Thư Giãn").await;
    let path = format!("/services/{}", thu_gian["id"].as_str().unwrap_or_default());
    let body = json!({
        "category_id": thu_gian["category_id"], "name": "Hacked", "description": null, "duration_min": 45,
        "recommended": false, "active": true, "prices": {"all": 1},
    });
    let reply = common::call_with_headers(
        &o.h.app,
        "PUT",
        &path,
        Some(&other),
        Some(body),
        &[("if-match", "\"1\"")],
    )
    .await;
    assert_eq!(
        reply.status,
        StatusCode::NOT_FOUND,
        "foreign id behaves as missing"
    );
    assert_eq!(o.service("Thư Giãn").await["prices"]["male"], 99_000);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn categories_can_be_added_and_renamed(pool: PgPool) -> TestResult {
    let o = owner(pool).await?;
    let created = o
        .send(
            "POST",
            "/service-categories",
            json!({"name": "  Sản phẩm  "}),
        )
        .await;
    assert_eq!(
        (
            created.status,
            created.body["name"].clone(),
            created.body["sort_order"].clone()
        ),
        (StatusCode::CREATED, json!("Sản phẩm"), json!(10))
    );
    let id = created.body["id"].as_str().unwrap_or_default().to_owned();
    let renamed = o
        .send(
            "PUT",
            &format!("/service-categories/{id}"),
            json!({"name": "Liệu pháp mới", "sort_order": 3}),
        )
        .await;
    assert_eq!(renamed.body["name"], "Liệu pháp mới");
    let duplicate = o
        .send(
            "POST",
            "/service-categories",
            json!({"name": "Chăm sóc chân"}),
        )
        .await;
    assert_eq!(duplicate.status, StatusCode::UNPROCESSABLE_ENTITY);
    Ok(())
}
