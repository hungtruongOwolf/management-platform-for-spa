//! End-to-end auth flows through the real router, middleware and Postgres (as app_rw).

mod common;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode, header};
use common::{Harness, PASSWORD, TestResult, add_member, call, harness, login};
use identity::SystemRole;
use identity::provisioning::{NewUser, add_user, create_tenant};
use serde_json::json;
use sqlx::PgPool;
use std::net::SocketAddr;
use tower::ServiceExt;

/// Session mechanics are exercised with a manager: no second factor, so each login is one call.
/// MFA flows (owners) live in tests/mfa.rs.
const PHONE: &str = "0912345678";

async fn staff_harness(owner: PgPool) -> Result<Harness, Box<dyn std::error::Error>> {
    let h = harness(owner).await?;
    add_member(&h.owner, "vpyd", PHONE, "Quản lý", SystemRole::Manager).await?;
    Ok(h)
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn login_sets_a_hardened_cookie_and_returns_me(owner: PgPool) -> TestResult {
    let h = staff_harness(owner).await?;
    let reply = login(&h.app, "0912.345.678", PASSWORD).await;
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
    assert_eq!(reply.body["me"]["user"]["phone"], "0912 345 678");
    assert_eq!(
        reply.body["me"]["active_tenant"]["name"],
        "Viên Phúc Y Đường"
    );
    let perms = reply.body["me"]["permissions"]
        .as_array()
        .map(Vec::len)
        .unwrap_or(0);
    assert!(perms > 10, "manager should have many permissions");
    assert!(
        !reply.body.to_string().contains("argon2"),
        "hash must never leave the server"
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn wrong_password_and_unknown_phone_are_indistinguishable(owner: PgPool) -> TestResult {
    let h = staff_harness(owner).await?;
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
    let h = staff_harness(owner).await?;
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

/// Security review L2: whoever knows the phone can lock the account; the operator can lift it.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn operator_unlock_lifts_a_lockout(owner: PgPool) -> TestResult {
    let h = staff_harness(owner).await?;
    for _ in 0..5 {
        login(&h.app, PHONE, "sai-mat-khau-roi").await;
    }
    assert_eq!(
        login(&h.app, PHONE, PASSWORD).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert!(identity::provisioning::unlock_account(&h.owner, PHONE).await?);
    assert!(!identity::provisioning::unlock_account(&h.owner, "0987000000").await?);
    assert_eq!(login(&h.app, PHONE, PASSWORD).await.status, StatusCode::OK);
    Ok(())
}

/// Security review L4: granting a role that requires MFA ends the sessions opened without it.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn promotion_to_an_mfa_role_ends_existing_sessions(owner: PgPool) -> TestResult {
    let h = staff_harness(owner).await?;
    let token = login(&h.app, PHONE, PASSWORD)
        .await
        .session()
        .unwrap_or_default();
    add_member(&h.owner, "vpyd", PHONE, "Quản lý", SystemRole::Technician).await?;
    assert_eq!(
        call(&h.app, "GET", "/me", Some(&token), None).await.status,
        StatusCode::OK,
        "a role without MFA keeps the session"
    );

    add_member(&h.owner, "vpyd", PHONE, "Quản lý", SystemRole::Owner).await?;
    assert_eq!(
        call(&h.app, "GET", "/me", Some(&token), None).await.status,
        StatusCode::UNAUTHORIZED,
        "the password-only session cannot use owner rights"
    );
    assert_eq!(
        login(&h.app, PHONE, PASSWORD).await.body["next"],
        "mfa_enroll"
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn me_requires_a_live_session_and_logout_ends_it(owner: PgPool) -> TestResult {
    let h = staff_harness(owner).await?;
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
    let h = staff_harness(owner).await?;
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
    let h = staff_harness(owner).await?;
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
    let h = staff_harness(owner).await?;
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
    let h = staff_harness(owner).await?;
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
        first.body["me"]["active_tenant"].is_null(),
        "two spas → no automatic choice"
    );
    assert_eq!(
        first.body["me"]["tenants"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(first.body["me"]["permissions"], json!([]));
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
        json!(["catalog.read", "staff.read_basic", "staff.read_own"]),
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

/// Security review H1: the rotated session inherits the old one's absolute expiry and re-auth
/// time, so switching spas can neither extend a stolen session nor stand in for a password.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn switch_tenant_keeps_absolute_expiry_and_reauth_time(owner: PgPool) -> TestResult {
    let h = staff_harness(owner).await?;
    let token = login(&h.app, PHONE, PASSWORD)
        .await
        .session()
        .unwrap_or_default();
    sqlx::query(
        "UPDATE app.session SET absolute_expires_at = now() + interval '3 days',
                                recent_auth_at = now() - interval '2 days'
         WHERE revoked_at IS NULL",
    )
    .execute(&h.owner)
    .await?;
    let (old_absolute, old_reauth): (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) =
        sqlx::query_as(
            "SELECT absolute_expires_at, recent_auth_at FROM app.session WHERE revoked_at IS NULL",
        )
        .fetch_one(&h.owner)
        .await?;
    let tenant: String = sqlx::query_scalar("SELECT id::text FROM app.tenant WHERE slug = 'vpyd'")
        .fetch_one(&h.owner)
        .await?;

    let switched = call(
        &h.app,
        "POST",
        "/auth/switch-tenant",
        Some(&token),
        Some(json!({"tenant_id": tenant})),
    )
    .await;
    assert_eq!(switched.status, StatusCode::OK, "{}", switched.body);

    let (absolute, idle, reauth): (
        chrono::DateTime<chrono::Utc>,
        chrono::DateTime<chrono::Utc>,
        chrono::DateTime<chrono::Utc>,
    ) = sqlx::query_as(
        "SELECT absolute_expires_at, idle_expires_at, recent_auth_at
         FROM app.session WHERE revoked_at IS NULL",
    )
    .fetch_one(&h.owner)
    .await?;
    assert_eq!(absolute, old_absolute, "absolute expiry is carried over");
    assert_eq!(reauth, old_reauth, "switching is not a re-authentication");
    assert!(idle <= absolute);
    Ok(())
}

/// Security review H1, as the re-auth guard sees it: after `switch-tenant` the rotated session is
/// not "recently authenticated", so it cannot pass `AuthUser::require_recent_auth` (the 🔒 guard
/// for account-security actions) without a real `/auth/reauth`. Business actions no longer ask for
/// re-auth (EM decision 2026-11-05), so this is checked on the resolved session itself.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_rotated_session_needs_a_real_reauth(owner: PgPool) -> TestResult {
    let h = staff_harness(owner).await?;
    let pool = kernel::testing::app_pool(&h.owner).await?;
    let token = login(&h.app, PHONE, PASSWORD)
        .await
        .session()
        .unwrap_or_default();
    sqlx::query("UPDATE app.session SET recent_auth_at = now() - interval '10 minutes'")
        .execute(&h.owner)
        .await?;
    let tenant: String = sqlx::query_scalar("SELECT id::text FROM app.tenant WHERE slug = 'vpyd'")
        .fetch_one(&h.owner)
        .await?;
    let switched = call(
        &h.app,
        "POST",
        "/auth/switch-tenant",
        Some(&token),
        Some(json!({"tenant_id": tenant})),
    )
    .await;
    assert_eq!(switched.status, StatusCode::OK, "{}", switched.body);
    let rotated = switched.session().unwrap_or_default();

    let now = chrono::Utc::now();
    let session = identity::service::resolve(&pool, &rotated).await?;
    assert!(
        !session.recently_authenticated(now),
        "switch-tenant is not a re-authentication"
    );

    let reauth = call(
        &h.app,
        "POST",
        "/auth/reauth",
        Some(&rotated),
        Some(json!({"password": PASSWORD})),
    )
    .await;
    assert_eq!(reauth.status, StatusCode::NO_CONTENT);
    let session = identity::service::resolve(&pool, &rotated).await?;
    assert!(session.recently_authenticated(chrono::Utc::now()));
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn login_marks_the_session_as_just_authenticated(owner: PgPool) -> TestResult {
    let h = staff_harness(owner).await?;
    let before = chrono::Utc::now() - chrono::Duration::seconds(5);
    assert_eq!(login(&h.app, PHONE, PASSWORD).await.status, StatusCode::OK);
    let (reauth, absolute): (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) =
        sqlx::query_as(
            "SELECT recent_auth_at, absolute_expires_at FROM app.session WHERE revoked_at IS NULL",
        )
        .fetch_one(&h.owner)
        .await?;
    assert!(reauth >= before, "login is a fresh authentication");
    assert!(absolute > chrono::Utc::now() + chrono::Duration::days(29));
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn reauth_checks_the_password(owner: PgPool) -> TestResult {
    let h = staff_harness(owner).await?;
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
    let h = staff_harness(owner).await?;
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

/// Security review M3: suspending a spa takes effect on sessions that are already open.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn suspending_a_spa_cuts_off_its_open_sessions(owner: PgPool) -> TestResult {
    let h = staff_harness(owner).await?;
    let token = login(&h.app, PHONE, PASSWORD)
        .await
        .session()
        .unwrap_or_default();
    assert_eq!(
        call(&h.app, "GET", "/customers", Some(&token), None)
            .await
            .status,
        StatusCode::OK
    );

    sqlx::query("UPDATE app.tenant SET status = 'suspended' WHERE slug = 'vpyd'")
        .execute(&h.owner)
        .await?;
    let refused = call(&h.app, "GET", "/customers", Some(&token), None).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    assert_eq!(refused.body["code"], "auth.not_a_member");
    let me = call(&h.app, "GET", "/me", Some(&token), None).await;
    assert!(me.body["active_tenant"].is_null());
    assert_eq!(me.body["permissions"], json!([]));

    // ADR-0018 (spec platform-admin US4): nobody signs in to a suspended spa, the owner included,
    // so no password-only owner session can exist when it reopens. Only a correct password
    // learns why.
    let owner_login = login(&h.app, common::OWNER_PHONE, PASSWORD).await;
    assert_eq!(owner_login.status, StatusCode::FORBIDDEN);
    assert_eq!(owner_login.body["code"], "auth.spa_suspended");
    assert!(owner_login.session().is_none());
    let wrong = login(&h.app, common::OWNER_PHONE, "not-the-password").await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn unknown_body_fields_are_rejected(owner: PgPool) -> TestResult {
    let h = staff_harness(owner).await?;
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
