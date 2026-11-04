//! End-to-end auth flows through the real router, middleware and Postgres (as app_rw).

use app::AppState;
use app::middleware::HttpPolicy;
use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, Request, StatusCode, header};
use http_body_util::BodyExt;
use identity::SystemRole;
use identity::provisioning::{NewUser, add_user, create_tenant};
use kernel::testing::app_pool;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::net::SocketAddr;
use tower::ServiceExt;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const ORIGIN: &str = "http://localhost:5173";
const PHONE: &str = "0905123456";
const PASSWORD: &str = "mat-khau-cua-chu-spa";

struct Harness {
    app: Router,
    owner: PgPool,
}

async fn harness(owner: PgPool) -> Result<Harness, Box<dyn std::error::Error>> {
    create_tenant(&owner, "vpyd", "Viên Phúc Y Đường", "Đông Hà").await?;
    add_user(
        &owner,
        NewUser {
            tenant_slug: "vpyd",
            phone: PHONE,
            full_name: "Chủ Spa",
            password: PASSWORD,
            role: SystemRole::Owner,
        },
    )
    .await?;
    let pool = app_pool(&owner).await?;
    let app = app::router(AppState { pool }, &HttpPolicy::standard(ORIGIN.to_owned()));
    Ok(Harness { app, owner })
}

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    body: Value,
}

impl Reply {
    fn cookie(&self) -> Option<String> {
        self.headers
            .get(header::SET_COOKIE)?
            .to_str()
            .ok()
            .map(str::to_owned)
    }
    fn session(&self) -> Option<String> {
        let cookie = self.cookie()?;
        let value = cookie.split(';').next()?.strip_prefix("__Host-session=")?;
        (!value.is_empty()).then(|| value.to_owned())
    }
}

async fn call(
    app: &Router,
    method: &str,
    path: &str,
    session: Option<&str>,
    body: Option<Value>,
) -> Reply {
    let mut builder = Request::builder()
        .method(method)
        .uri(format!("/api/v1{path}"))
        .header(header::ORIGIN, ORIGIN)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(s) = session {
        builder = builder.header(header::COOKIE, format!("__Host-session={s}"));
    }
    let body = body
        .map(|b| Body::from(b.to_string()))
        .unwrap_or_else(Body::empty);
    let mut request = builder.body(body).unwrap_or_default();
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

async fn login(app: &Router, phone: &str, password: &str) -> Reply {
    call(
        app,
        "POST",
        "/auth/login",
        None,
        Some(json!({"phone": phone, "password": password})),
    )
    .await
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn login_sets_a_hardened_cookie_and_returns_me(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    let reply = login(&h.app, "0905.123.456", PASSWORD).await;
    assert_eq!(reply.status, StatusCode::OK);
    let cookie = reply.cookie().unwrap_or_default();
    for flag in [
        "__Host-session=",
        "HttpOnly",
        "Secure",
        "SameSite=Strict",
        "Path=/",
    ] {
        assert!(cookie.contains(flag), "{flag} missing: {cookie}");
    }
    assert_eq!(reply.body["user"]["phone"], "0905 123 456");
    assert_eq!(reply.body["active_tenant"]["name"], "Viên Phúc Y Đường");
    let perms = reply.body["permissions"]
        .as_array()
        .map(Vec::len)
        .unwrap_or(0);
    assert!(perms > 10, "owner should have many permissions");
    assert!(
        !reply.body.to_string().contains("argon2"),
        "hash must never leave the server"
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn wrong_password_and_unknown_phone_are_indistinguishable(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    let wrong = login(&h.app, PHONE, "sai-mat-khau-roi").await;
    let unknown = login(&h.app, "0912888777", PASSWORD).await;
    let malformed = login(&h.app, "not-a-phone", PASSWORD).await;
    for reply in [&wrong, &unknown, &malformed] {
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
        assert_eq!(reply.body["code"], "auth.invalid_credentials");
        assert!(reply.session().is_none());
    }
    assert_eq!(wrong.body, unknown.body);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn five_failures_lock_the_account_even_for_the_right_password(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    for _ in 0..5 {
        assert_eq!(
            login(&h.app, PHONE, "sai-mat-khau-roi").await.status,
            StatusCode::UNAUTHORIZED
        );
    }
    let right = login(&h.app, PHONE, PASSWORD).await;
    assert_eq!(
        right.status,
        StatusCode::UNAUTHORIZED,
        "locked account must not log in"
    );

    sqlx::query("UPDATE app.user_account SET locked_until = now() - interval '1 second'")
        .execute(&h.owner)
        .await?;
    let after_lock = login(&h.app, PHONE, PASSWORD).await;
    assert_eq!(after_lock.status, StatusCode::OK);
    let failures: i32 = sqlx::query_scalar("SELECT failed_logins FROM app.user_account")
        .fetch_one(&h.owner)
        .await?;
    assert_eq!(failures, 0, "successful login resets the counter");
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn me_requires_a_live_session_and_logout_ends_it(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    let anonymous = call(&h.app, "GET", "/me", None, None).await;
    assert_eq!(
        (anonymous.status, anonymous.body["code"].as_str()),
        (StatusCode::UNAUTHORIZED, Some("auth.unauthenticated"))
    );

    let token = login(&h.app, PHONE, PASSWORD)
        .await
        .session()
        .unwrap_or_default();
    assert_eq!(
        call(&h.app, "GET", "/me", Some(&token), None).await.status,
        StatusCode::OK
    );
    assert_eq!(
        call(&h.app, "GET", "/me", Some("forged-token"), None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );

    let out = call(&h.app, "POST", "/auth/logout", Some(&token), None).await;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
    assert!(out.cookie().unwrap_or_default().contains("Max-Age=0"));
    assert_eq!(
        call(&h.app, "GET", "/me", Some(&token), None).await.status,
        StatusCode::UNAUTHORIZED
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn login_rotates_and_logout_all_revokes_every_device(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    let phone_a = login(&h.app, PHONE, PASSWORD)
        .await
        .session()
        .unwrap_or_default();
    let laptop = login(&h.app, PHONE, PASSWORD)
        .await
        .session()
        .unwrap_or_default();
    assert_ne!(phone_a, laptop);

    // Logging in again from a browser that already holds a session replaces that session.
    let rotated = call(
        &h.app,
        "POST",
        "/auth/login",
        Some(&laptop),
        Some(json!({"phone": PHONE, "password": PASSWORD})),
    )
    .await;
    let new_laptop = rotated.session().unwrap_or_default();
    assert_eq!(
        call(&h.app, "GET", "/me", Some(&laptop), None).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(&h.app, "GET", "/me", Some(&new_laptop), None)
            .await
            .status,
        StatusCode::OK
    );

    assert_eq!(
        call(&h.app, "POST", "/auth/logout-all", Some(&new_laptop), None)
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        call(&h.app, "GET", "/me", Some(&phone_a), None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn idle_and_absolute_expiry_end_the_session(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    let token = login(&h.app, PHONE, PASSWORD)
        .await
        .session()
        .unwrap_or_default();
    sqlx::query("UPDATE app.session SET idle_expires_at = now() - interval '1 second'")
        .execute(&h.owner)
        .await?;
    assert_eq!(
        call(&h.app, "GET", "/me", Some(&token), None).await.status,
        StatusCode::UNAUTHORIZED
    );

    let token = login(&h.app, PHONE, PASSWORD)
        .await
        .session()
        .unwrap_or_default();
    sqlx::query("UPDATE app.session SET absolute_expires_at = now() - interval '1 second', idle_expires_at = now() - interval '2 seconds' WHERE revoked_at IS NULL")
        .execute(&h.owner)
        .await?;
    assert_eq!(
        call(&h.app, "GET", "/me", Some(&token), None).await.status,
        StatusCode::UNAUTHORIZED
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn login_from_a_foreign_origin_is_rejected(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    let mut request = Request::post("/api/v1/auth/login")
        .header(header::ORIGIN, "https://evil.example")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({"phone": PHONE, "password": PASSWORD}).to_string(),
        ))
        .unwrap_or_default();
    request
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([10, 0, 0, 8], 50_000))));
    let response = h
        .app
        .clone()
        .oneshot(request)
        .await
        .unwrap_or_else(|never| match never {});
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(response.headers().get(header::SET_COOKIE).is_none());
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn member_of_two_spas_must_choose_and_cannot_switch_to_a_foreign_spa(
    owner: PgPool,
) -> TestResult {
    let h = harness(owner).await?;
    create_tenant(&h.owner, "spa-b", "Spa B", "Cơ sở 1").await?;
    create_tenant(&h.owner, "spa-c", "Spa C", "Cơ sở 1").await?;
    add_user(
        &h.owner,
        NewUser {
            tenant_slug: "spa-b",
            phone: PHONE,
            full_name: "Chủ Spa",
            password: "ignored-existing-account",
            role: SystemRole::Technician,
        },
    )
    .await?;

    let first = login(&h.app, PHONE, PASSWORD).await;
    assert!(
        first.body["active_tenant"].is_null(),
        "two spas → no automatic choice"
    );
    assert_eq!(first.body["tenants"].as_array().map(Vec::len), Some(2));
    assert_eq!(first.body["permissions"], json!([]));
    let token = first.session().unwrap_or_default();

    let spa_b: String = sqlx::query_scalar("SELECT id::text FROM app.tenant WHERE slug = 'spa-b'")
        .fetch_one(&h.owner)
        .await?;
    let switched = call(
        &h.app,
        "POST",
        "/auth/switch-tenant",
        Some(&token),
        Some(json!({"tenant_id": spa_b})),
    )
    .await;
    assert_eq!(switched.status, StatusCode::OK);
    assert_eq!(switched.body["active_tenant"]["name"], "Spa B");
    assert_eq!(
        switched.body["permissions"],
        json!(["catalog.read", "staff.read_basic"]),
        "technician role in spa B"
    );
    let new_token = switched.session().unwrap_or_default();
    assert_eq!(
        call(&h.app, "GET", "/me", Some(&token), None).await.status,
        StatusCode::UNAUTHORIZED,
        "switch rotates the token"
    );

    let spa_c: String = sqlx::query_scalar("SELECT id::text FROM app.tenant WHERE slug = 'spa-c'")
        .fetch_one(&h.owner)
        .await?;
    let foreign = call(
        &h.app,
        "POST",
        "/auth/switch-tenant",
        Some(&new_token),
        Some(json!({"tenant_id": spa_c})),
    )
    .await;
    assert_eq!(foreign.status, StatusCode::NOT_FOUND);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn reauth_checks_the_password(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    let token = login(&h.app, PHONE, PASSWORD)
        .await
        .session()
        .unwrap_or_default();
    let wrong = call(
        &h.app,
        "POST",
        "/auth/reauth",
        Some(&token),
        Some(json!({"password": "sai-mat-khau-roi"})),
    )
    .await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    let right = call(
        &h.app,
        "POST",
        "/auth/reauth",
        Some(&token),
        Some(json!({"password": PASSWORD})),
    )
    .await;
    assert_eq!(right.status, StatusCode::NO_CONTENT);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn disabled_accounts_and_memberships_lose_access(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    let token = login(&h.app, PHONE, PASSWORD)
        .await
        .session()
        .unwrap_or_default();

    sqlx::query("UPDATE app.membership SET status = 'disabled'")
        .execute(&h.owner)
        .await?;
    let me = call(&h.app, "GET", "/me", Some(&token), None).await;
    assert!(me.body["active_tenant"].is_null());
    assert_eq!(me.body["permissions"], json!([]));

    sqlx::query("UPDATE app.user_account SET status = 'disabled'")
        .execute(&h.owner)
        .await?;
    assert_eq!(
        call(&h.app, "GET", "/me", Some(&token), None).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        login(&h.app, PHONE, PASSWORD).await.status,
        StatusCode::UNAUTHORIZED
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn unknown_body_fields_are_rejected(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    let reply = call(
        &h.app,
        "POST",
        "/auth/login",
        None,
        Some(json!({"phone": PHONE, "password": PASSWORD, "role": "owner"})),
    )
    .await;
    assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(reply.session().is_none());
    Ok(())
}
