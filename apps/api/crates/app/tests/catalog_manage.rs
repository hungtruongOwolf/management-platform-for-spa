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
    /// A new group of services with nothing in it; its id.
    async fn new_group(&self, name: &str) -> String {
        let created = self
            .send("POST", "/service-categories", json!({ "name": name }))
            .await;
        assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
        created.body["id"].as_str().unwrap_or_default().to_owned()
    }
    async fn delete_group(&self, id: &str) -> Reply {
        call(
            &self.h.app,
            "DELETE",
            &format!("/service-categories/{id}"),
            Some(&self.token),
            None,
        )
        .await
    }
    async fn count(&self, sql: &'static str) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar(sql).fetch_one(&self.h.owner).await
    }
}

/// A service body in `category_id`, on sale or not.
fn service_in(category_id: &str, name: &str, active: bool) -> Value {
    json!({
        "category_id": category_id, "name": name, "description": null, "duration_min": 30,
        "recommended": false, "active": active, "prices": {"all": 50_000},
    })
}

/// Waits until some session of the test database waits on a row lock (the owner pool sees every
/// session's wait event).
async fn until_a_request_waits_on_a_lock(owner: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    for _ in 0..2_000 {
        let waiting: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_stat_activity
             WHERE datname = current_database() AND wait_event_type = 'Lock'",
        )
        .fetch_one(owner)
        .await?;
        if waiting > 0 {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    Err("the request never waited on the other transaction's lock".into())
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
    // Not even an empty group (K68): only the owner deletes one.
    let empty = o.new_group("Nhóm trống").await;
    let delete = call(
        &o.h.app,
        "DELETE",
        &format!("/service-categories/{empty}"),
        Some(&manager),
        None,
    )
    .await;
    assert_eq!(
        (delete.status, delete.body["code"].as_str()),
        (StatusCode::FORBIDDEN, Some("auth.forbidden"))
    );
    assert!(!o.category_id("Nhóm trống").await.is_empty(), "still there");
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
    // An empty group of this spa (K68): another spa's owner cannot delete it either.
    let empty = o.new_group("Nhóm trống").await;
    let delete = call(
        &o.h.app,
        "DELETE",
        &format!("/service-categories/{empty}"),
        Some(&other),
        None,
    )
    .await;
    assert_eq!(delete.status, StatusCode::NOT_FOUND, "{}", delete.body);
    assert_eq!(o.category_id("Nhóm trống").await, empty, "still there");
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

// ---------------------------------------------------------------- deleting a group (K68)

/// A group added by mistake goes: `204`, off the list, once (`404` after), and the trail keeps
/// its last name and who deleted it. Its own history is gone with it (`404`).
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn an_empty_group_is_deleted_once_and_the_trail_keeps_its_name(pool: PgPool) -> TestResult {
    let o = owner(pool).await?;
    let id = o.new_group("Nhóm nhầm").await;
    let renamed = o
        .send(
            "PUT",
            &format!("/service-categories/{id}"),
            json!({"name": "Nhóm nhầm (sửa)", "sort_order": 10}),
        )
        .await;
    assert_eq!(renamed.status, StatusCode::OK, "{}", renamed.body);

    let deleted = o.delete_group(&id).await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT, "{}", deleted.body);
    assert!(
        o.category_id("Nhóm nhầm (sửa)").await.is_empty(),
        "off the list"
    );
    assert_eq!(
        o.delete_group(&id).await.status,
        StatusCode::NOT_FOUND,
        "a group goes once"
    );
    let history = o
        .get(&format!("/record-history/service-category/{id}"))
        .await;
    assert_eq!(history.status, StatusCode::NOT_FOUND, "{}", history.body);

    let (actor, before, after): (Option<String>, Value, Option<Value>) = sqlx::query_as(
        "SELECT u.full_name, a.before, a.after FROM app.audit_log a
           LEFT JOIN app.user_account u ON u.id = a.actor_user_id
          WHERE a.action = 'catalog.category_delete' AND a.entity_type = 'service_category'
            AND a.entity_id = $1::uuid AND a.actor_kind = 'user'",
    )
    .bind(&id)
    .fetch_one(&o.h.owner)
    .await?;
    assert_eq!(actor.as_deref(), Some("Chủ Spa"));
    assert_eq!(before, json!({"name": "Nhóm nhầm (sửa)", "sort_order": 10}));
    assert_eq!(after, None);
    Ok(())
}

/// Any service keeps its group: one on sale, and one off sale (the price list's "Ngừng bán", which
/// stays in the database for ever, AC1.6). `409 catalog.category_in_use`, nothing deleted or audited.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_group_with_a_service_on_sale_or_not_is_never_deleted(pool: PgPool) -> TestResult {
    let o = owner(pool).await?;
    let seeded = o.category_id("Gội đầu dưỡng sinh").await;
    let off_sale = o.new_group("Nhóm ngừng bán").await;
    let made = o
        .send(
            "POST",
            "/services",
            service_in(&off_sale, "Dịch vụ đã ngừng", false),
        )
        .await;
    assert_eq!(made.status, StatusCode::CREATED, "{}", made.body);
    assert_eq!(made.body["active"], false);

    for (case, id) in [("on sale", &seeded), ("off sale", &off_sale)] {
        let refused = o.delete_group(id).await;
        assert_eq!(
            (refused.status, refused.body["code"].as_str()),
            (StatusCode::CONFLICT, Some("catalog.category_in_use")),
            "{case}: {}",
            refused.body
        );
    }
    assert_eq!(o.category_id("Gội đầu dưỡng sinh").await, seeded);
    assert_eq!(o.category_id("Nhóm ngừng bán").await, off_sale);
    assert_eq!(
        o.count("SELECT count(*) FROM app.audit_log WHERE action = 'catalog.category_delete'")
            .await?,
        0
    );
    Ok(())
}

/// The race: a service is being added to an empty group when its delete arrives. The delete waits
/// for that write, then finds the service and refuses; the service keeps its group.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_delete_waits_for_a_service_being_added_and_then_refuses(pool: PgPool) -> TestResult {
    let o = owner(pool).await?;
    let id = o.new_group("Nhóm đang thêm").await;
    let tenant: uuid::Uuid = sqlx::query_scalar("SELECT id FROM app.tenant WHERE slug = 'vpyd'")
        .fetch_one(&o.h.owner)
        .await?;
    // The other request, mid-transaction: its service row is written, not yet committed.
    let mut adding = o.h.owner.begin().await?;
    sqlx::query(
        "INSERT INTO app.service (id, tenant_id, category_id, name) VALUES ($1, $2, $3::uuid, 'Vừa thêm')",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(tenant)
    .bind(&id)
    .execute(&mut *adding)
    .await?;

    let delete = {
        let (app, token, path) = (
            o.h.app.clone(),
            o.token.clone(),
            format!("/service-categories/{id}"),
        );
        tokio::spawn(async move { call(&app, "DELETE", &path, Some(&token), None).await })
    };
    until_a_request_waits_on_a_lock(&o.h.owner).await?;
    adding.commit().await?;
    let refused = delete.await?;
    assert_eq!(
        (refused.status, refused.body["code"].as_str()),
        (StatusCode::CONFLICT, Some("catalog.category_in_use")),
        "{}",
        refused.body
    );
    assert_eq!(o.service("Vừa thêm").await["category_id"], json!(id));
    Ok(())
}

/// The other order: the delete holds the group when a service is added to it. The write waits,
/// then finds no group: `422 category_id not_found` (not a foreign-key 500), and no service
/// points at a group that is gone.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_service_added_while_its_group_is_deleted_is_refused(pool: PgPool) -> TestResult {
    let o = owner(pool).await?;
    let id = o.new_group("Nhóm đang xóa").await;
    // The delete, mid-transaction: the group's row is deleted, not yet committed.
    let mut deleting = o.h.owner.begin().await?;
    sqlx::query("DELETE FROM app.service_category WHERE id = $1::uuid")
        .bind(&id)
        .execute(&mut *deleting)
        .await?;

    let add = {
        let (app, token) = (o.h.app.clone(), o.token.clone());
        let body = service_in(&id, "Thêm muộn", true);
        tokio::spawn(async move { call(&app, "POST", "/services", Some(&token), Some(body)).await })
    };
    until_a_request_waits_on_a_lock(&o.h.owner).await?;
    deleting.commit().await?;
    let refused = add.await?;
    assert_eq!(
        (refused.status, refused.body["errors"].clone()),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            json!([{"field": "category_id", "code": "not_found"}])
        ),
        "{}",
        refused.body
    );
    assert_eq!(
        o.count(
            "SELECT count(*) FROM app.service s WHERE NOT EXISTS (
               SELECT 1 FROM app.service_category c WHERE c.tenant_id = s.tenant_id AND c.id = s.category_id)"
        )
        .await?,
        0
    );
    assert_eq!(o.service("Thêm muộn").await, Value::Null);
    Ok(())
}
