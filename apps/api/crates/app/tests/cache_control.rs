//! Every API response says how it may be cached. Without a `Cache-Control` the CDN in front of
//! the API (Vercel) adds `public, max-age=0, must-revalidate`, which marks customer names and
//! phone numbers as shareable (perf research 2026-11-07, security side finding).

mod common;

use axum::http::StatusCode;
use catalog::service::seed_menu;
use common::{OWNER_PHONE, PASSWORD, Reply, TestResult, add_member, call, harness, login};
use identity::SystemRole;
use serde_json::json;
use sqlx::PgPool;

const MANAGER_PHONE: &str = "0912000333";
const DEFAULT: &str = "private, no-store";

fn cache_control(reply: &Reply) -> Option<&str> {
    reply
        .headers
        .get("cache-control")
        .and_then(|v| v.to_str().ok())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn responses_without_their_own_policy_are_private_no_store(pool: PgPool) -> TestResult {
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
    let manager = login(&h.app, MANAGER_PHONE, PASSWORD)
        .await
        .session()
        .unwrap_or_default();
    assert!(!manager.is_empty(), "manager logged in");
    let created = call(
        &h.app,
        "POST",
        "/customers",
        Some(&manager),
        Some(json!({"full_name": "Nguyễn Thị Hà", "phone": "0905111222"})),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);

    let cases: [(&str, Option<&str>, StatusCode); 8] = [
        ("/customers?q=nguyen", Some(&manager), StatusCode::OK),
        ("/customers", Some(&manager), StatusCode::OK),
        ("/services", Some(&manager), StatusCode::OK),
        ("/staff", Some(&manager), StatusCode::OK),
        ("/health", None, StatusCode::OK),
        // anonymous and forged sessions
        ("/customers?q=nguyen", None, StatusCode::UNAUTHORIZED),
        (
            "/me",
            Some("forged-session-token"),
            StatusCode::UNAUTHORIZED,
        ),
        // salary is owner-only
        (
            "/payroll?month=2026-11",
            Some(&manager),
            StatusCode::FORBIDDEN,
        ),
    ];
    for (path, session, status) in cases {
        let reply = call(&h.app, "GET", path, session, None).await;
        assert_eq!(reply.status, status, "GET {path}: {}", reply.body);
        assert_eq!(cache_control(&reply), Some(DEFAULT), "GET {path}");
    }

    let missing = call(&h.app, "GET", "/does-not-exist", None, None).await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
    assert_eq!(cache_control(&missing), Some(DEFAULT), "404 fallback");

    let invalid = call(
        &h.app,
        "POST",
        "/customers",
        Some(&manager),
        Some(json!({"full_name": "X", "phone": "12345"})),
    )
    .await;
    assert!(invalid.status.is_client_error(), "{}", invalid.status);
    assert_eq!(cache_control(&invalid), Some(DEFAULT), "validation error");
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn handlers_that_set_their_own_policy_keep_it(pool: PgPool) -> TestResult {
    let h = harness(pool).await?;
    let reply = login(&h.app, OWNER_PHONE, PASSWORD).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(
        cache_control(&reply),
        Some("no-store"),
        "login sets a session cookie and says so itself"
    );
    let values = reply.headers.get_all("cache-control").iter().count();
    assert_eq!(values, 1, "the default never adds a second value");
    Ok(())
}
