//! S3 tracer bullet: seeded menu → authenticated, tenant-scoped catalog API.

mod common;

use axum::http::StatusCode;
use catalog::service::seed_menu;
use common::{OWNER_PHONE, TestResult, add_member, call, harness, session_for};
use identity::SystemRole;
use serde_json::{Value, json};
use sqlx::PgPool;

fn find<'a>(services: &'a Value, name: &str) -> &'a Value {
    services
        .as_array()
        .and_then(|list| list.iter().find(|s| s["name"] == name))
        .unwrap_or(&Value::Null)
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn seeded_menu_is_served_with_categories_and_price_variants(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    let report = seed_menu(&h.owner, "vpyd").await?;
    assert_eq!(
        (
            report.categories,
            report.services,
            report.inactive_without_price
        ),
        (10, 35, 1)
    );
    let token = session_for(&h.app, OWNER_PHONE).await;

    let categories = call(&h.app, "GET", "/service-categories", Some(&token), None).await;
    assert_eq!(categories.status, StatusCode::OK);
    assert_eq!(categories.body.as_array().map(Vec::len), Some(10));
    assert_eq!(
        categories.body[0]["name"], "Gội đầu dưỡng sinh",
        "menu order is kept"
    );

    let services = call(&h.app, "GET", "/services", Some(&token), None).await;
    assert_eq!(
        services.body.as_array().map(Vec::len),
        Some(34),
        "inactive (unpriced) service hidden by default"
    );
    assert_eq!(
        find(&services.body, "Thư Giãn")["prices"],
        json!({"female": 79_000, "male": 99_000})
    );
    assert_eq!(
        find(&services.body, "Cơ Bản – Dầu thảo dược")["prices"],
        json!({"all": 49_000}),
        "equal menu prices become one flat price"
    );
    assert_eq!(
        find(&services.body, "Mặt nạ thuốc bắc")["duration_min"],
        Value::Null,
        "theo tư vấn"
    );
    assert_eq!(find(&services.body, "Đại Thông Lạc")["recommended"], true);

    let all = call(&h.app, "GET", "/services?active=false", Some(&token), None).await;
    let unpriced = find(&all.body, "Ôn ấm tử cung");
    assert_eq!(
        (unpriced["active"].clone(), unpriced["prices"].clone()),
        (json!(false), json!({}))
    );

    let plans = call(&h.app, "GET", "/package-plans", Some(&token), None).await;
    assert_eq!(
        (plans.status, plans.body.clone()),
        (StatusCode::OK, json!([]))
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn technicians_can_read_the_catalog(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    seed_menu(&h.owner, "vpyd").await?;
    add_member(
        &h.owner,
        "vpyd",
        "0912000111",
        "KTV Lan",
        SystemRole::Technician,
    )
    .await?;
    let token = session_for(&h.app, "0912000111").await;
    assert_eq!(
        call(&h.app, "GET", "/services", Some(&token), None)
            .await
            .status,
        StatusCode::OK
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn anonymous_and_permissionless_users_are_refused(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    seed_menu(&h.owner, "vpyd").await?;
    for path in ["/service-categories", "/services", "/package-plans"] {
        let reply = call(&h.app, "GET", path, None, None).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{path}");
    }

    // A member whose roles grant nothing (owner removed every permission from the role).
    add_member(
        &h.owner,
        "vpyd",
        "0912000222",
        "Không quyền",
        SystemRole::Technician,
    )
    .await?;
    sqlx::query("DELETE FROM app.role_permission WHERE role_id IN (SELECT id FROM app.role WHERE key = 'technician')")
        .execute(&h.owner)
        .await?;
    let token = session_for(&h.app, "0912000222").await;
    let reply = call(&h.app, "GET", "/services", Some(&token), None).await;
    assert_eq!(
        (reply.status, reply.body["code"].as_str()),
        (StatusCode::FORBIDDEN, Some("auth.forbidden"))
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn another_spa_never_sees_this_menu(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    seed_menu(&h.owner, "vpyd").await?;
    identity::provisioning::create_tenant(&h.owner, "spa-b", "Spa B", "Cơ sở 1").await?;
    add_member(
        &h.owner,
        "spa-b",
        "0987000333",
        "Chủ Spa B",
        SystemRole::Owner,
    )
    .await?;
    let token = session_for(&h.app, "0987000333").await;

    let services = call(&h.app, "GET", "/services?active=false", Some(&token), None).await;
    assert_eq!(
        (services.status, services.body.clone()),
        (StatusCode::OK, json!([]))
    );
    let categories = call(&h.app, "GET", "/service-categories", Some(&token), None).await;
    assert_eq!(categories.body, json!([]));
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn the_menu_cannot_be_seeded_twice(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    seed_menu(&h.owner, "vpyd").await?;
    let again = seed_menu(&h.owner, "vpyd").await;
    assert!(matches!(
        again,
        Err(kernel::AppError::Conflict {
            code: "catalog.already_seeded"
        })
    ));
    Ok(())
}
