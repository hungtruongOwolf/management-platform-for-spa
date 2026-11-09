//! Logins made from the staff roster, temporary passwords, password changes, owner resets, role
//! changes and forgotten-password requests (ADR-0016, spec staff-management §17).

mod common;

use axum::http::StatusCode;
use common::{
    Harness, OWNER_PHONE, PASSWORD, Reply, TestResult, add_member, call, call_with_headers,
    current_code, forget_used_step, harness, session_for,
};
use identity::SystemRole;
use identity::provisioning::create_tenant;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::sync::atomic::{AtomicU32, Ordering};
use uuid::Uuid;

const NEW_PHONE: &str = "0912300111";
const OTHER_OWNER_PHONE: &str = "0913300222";
const OWN_PASSWORD: &str = "mat-khau-rieng-cua-ha";

struct Spa {
    h: Harness,
    owner: String,
    manager: String,
    tech: String,
    /// A working profile without a login.
    ha: Uuid,
}

const MANAGER_PHONE: &str = "0912300333";
const TECH_PHONE: &str = "0912300444";

async fn spa(pool: PgPool) -> Result<Spa, Box<dyn std::error::Error>> {
    let h = harness(pool).await?;
    add_member(
        &h.owner,
        "vpyd",
        MANAGER_PHONE,
        "Quản lý Hoa",
        SystemRole::Manager,
    )
    .await?;
    add_member(
        &h.owner,
        "vpyd",
        TECH_PHONE,
        "KTV Lan",
        SystemRole::Technician,
    )
    .await?;
    let owner = session_for(&h, OWNER_PHONE).await;
    let manager = session_for(&h, MANAGER_PHONE).await;
    let tech = session_for(&h, TECH_PHONE).await;
    let hired = call(
        &h.app,
        "POST",
        "/staff",
        Some(&owner),
        Some(json!({"full_name": "Nguyễn Thị Thu Hà", "position": "Lễ tân"})),
    )
    .await;
    assert_eq!(hired.status, StatusCode::CREATED, "{}", hired.body);
    let ha = id_of(&hired);
    Ok(Spa {
        h,
        owner,
        manager,
        tech,
        ha,
    })
}

fn id_of(r: &Reply) -> Uuid {
    r.body["id"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or_default()
}

impl Spa {
    async fn version(&self, staff: Uuid) -> String {
        let p = call(
            &self.h.app,
            "GET",
            &format!("/staff/{staff}"),
            Some(&self.owner),
            None,
        )
        .await;
        format!("\"{}\"", p.body["version"].as_i64().unwrap_or_default())
    }

    async fn create_login(&self, session: &str, staff: Uuid, phone: &str, role: &str) -> Reply {
        let tag = self.version(staff).await;
        call_with_headers(
            &self.h.app,
            "POST",
            &format!("/staff/{staff}/login"),
            Some(session),
            Some(json!({"phone": phone, "role": role})),
            &[("If-Match", &tag)],
        )
        .await
    }

    async fn reset(&self, session: &str, staff: Uuid) -> Reply {
        call(
            &self.h.app,
            "POST",
            &format!("/staff/{staff}/login/reset-password"),
            Some(session),
            None,
        )
        .await
    }

    async fn audit_text(&self) -> Result<String, sqlx::Error> {
        let rows: Vec<Value> =
            sqlx::query_scalar("SELECT to_jsonb(a) FROM app.audit_log a ORDER BY at")
                .fetch_all(&self.h.owner)
                .await?;
        Ok(Value::Array(rows).to_string())
    }
}

/// A different client address per call: the `/auth/*` per-address limit (burst 10) is tested on
/// its own, not tripped by the many sign-ins of one scenario.
fn fresh_ip() -> String {
    static NEXT: AtomicU32 = AtomicU32::new(1);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    format!("10.{}.{}.{}", (n >> 16) & 255, (n >> 8) & 255, n & 255)
}

async fn auth_call(app: &axum::Router, path: &str, session: Option<&str>, body: Value) -> Reply {
    let ip = fresh_ip();
    call_with_headers(
        app,
        "POST",
        path,
        session,
        Some(body),
        &[("X-Forwarded-For", &ip)],
    )
    .await
}

async fn login(app: &axum::Router, phone: &str, password: &str) -> Reply {
    auth_call(
        app,
        "/auth/login",
        None,
        json!({"phone": phone, "password": password}),
    )
    .await
}

async fn change(app: &axum::Router, session: &str, current: &str, new: &str) -> Reply {
    auth_call(
        app,
        "/auth/password",
        Some(session),
        json!({"current_password": current, "new_password": new}),
    )
    .await
}

/// A brand-new person: the owner creates their login from the profile; the temporary password
/// comes back once; the first sign-in reaches nothing but the password change; afterwards the
/// login works, the temporary session is gone, and no audit row holds a password.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn the_owner_onboards_a_manager_who_must_set_their_own_password(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let created = s
        .create_login(&s.owner, s.ha, "0912 300 111", "manager")
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    assert_eq!(
        created
            .headers
            .get("cache-control")
            .and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    let temporary = created.body["temporary_password"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert_eq!(temporary.len(), 14, "{}", created.body);
    let profile = &created.body["profile"];
    assert_eq!(profile["login_linked"], true);
    assert_eq!(profile["position"], "Quản lý", "the role decides the label");
    assert_eq!(profile["details"]["login"]["role"], "manager");
    assert_eq!(profile["details"]["login"]["phone"], "0912 300 111");
    // Never again: not in the profile, not in the roster.
    for path in [format!("/staff/{}", s.ha), "/staff/profiles".to_owned()] {
        let body = call(&s.h.app, "GET", &path, Some(&s.owner), None)
            .await
            .body
            .to_string();
        assert!(!body.contains(&temporary), "{path}");
    }

    // First sign-in: only the password change.
    let first = login(&s.h.app, NEW_PHONE, &temporary).await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.body);
    assert_eq!(first.body["next"], "password_change");
    assert!(first.body["me"].is_null());
    let pending = first.session().unwrap_or_default();
    for path in ["/me", "/attendance", "/me/staff", "/staff"] {
        let r = call(&s.h.app, "GET", path, Some(&pending), None).await;
        assert_eq!(
            (r.status, r.body["code"].clone()),
            (
                StatusCode::FORBIDDEN,
                json!("auth.password_change_required")
            ),
            "{path}"
        );
    }
    // Policy and the current password are checked.
    let wrong = change(&s.h.app, &pending, "khong-phai-mat-khau", OWN_PASSWORD).await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED, "{}", wrong.body);
    for (new, code) in [
        ("ngan", "too_short"),
        (temporary.as_str(), "same_as_current"),
        ("0912300111", "is_phone"),
        // ADR-0019 D18: on the bundled common-password list (case ignored).
        ("Password1234", "too_common"),
    ] {
        let r = change(&s.h.app, &pending, &temporary, new).await;
        assert_eq!(r.body["errors"][0]["code"], code, "{new}: {}", r.body);
    }
    let changed = change(&s.h.app, &pending, &temporary, OWN_PASSWORD).await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.body);
    assert_eq!(changed.body["user"]["full_name"], "Nguyễn Thị Thu Hà");
    let fresh = changed.session().unwrap_or_default();
    assert_ne!(fresh, pending, "a new session for this browser");
    assert_eq!(
        call(&s.h.app, "GET", "/me", Some(&pending), None)
            .await
            .status,
        StatusCode::UNAUTHORIZED,
        "the temporary session ended"
    );
    // A working manager now, linked to their own profile.
    let sheet = call(&s.h.app, "GET", "/attendance", Some(&fresh), None).await;
    assert_eq!(sheet.status, StatusCode::OK, "{}", sheet.body);
    assert_eq!(sheet.body["link_required"], false);
    let again = login(&s.h.app, NEW_PHONE, OWN_PASSWORD).await;
    assert_eq!(again.body["next"], "done", "{}", again.body);
    assert_eq!(
        login(&s.h.app, NEW_PHONE, &temporary).await.status,
        StatusCode::UNAUTHORIZED,
        "the temporary password is gone"
    );
    // Audited, without any password.
    let audit = s.audit_text().await?;
    for action in ["staff.login_create", "account.password_change"] {
        assert!(audit.contains(action), "{action}");
    }
    for secret in [temporary.as_str(), OWN_PASSWORD] {
        assert!(!audit.contains(secret), "a password reached the audit");
    }
    Ok(())
}

/// Only the owner creates, resets and re-roles logins and handles forgotten-password requests;
/// another spa cannot reach our profiles.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn only_the_owner_looks_after_logins(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    for who in [&s.manager, &s.tech] {
        let r = s.create_login(who, s.ha, NEW_PHONE, "technician").await;
        assert_eq!(r.status, StatusCode::FORBIDDEN, "{}", r.body);
        assert_eq!(s.reset(who, s.ha).await.status, StatusCode::FORBIDDEN);
        let role = call(
            &s.h.app,
            "PUT",
            &format!("/staff/{}/login/role", s.ha),
            Some(who),
            Some(json!({"role": "manager"})),
        )
        .await;
        assert_eq!(role.status, StatusCode::FORBIDDEN);
        // F17: the right comes before the If-Match header and the body (each was 400/422 first).
        for (method, path) in [
            ("POST", format!("/staff/{}/login", s.ha)),
            ("PUT", format!("/staff/{}/login/role", s.ha)),
        ] {
            let r = call(&s.h.app, method, &path, Some(who), Some(json!({"role": 7}))).await;
            assert_eq!(
                r.status,
                StatusCode::FORBIDDEN,
                "{method} {path}: {}",
                r.body
            );
        }
        let list = call(
            &s.h.app,
            "GET",
            "/staff/password-reset-requests",
            Some(who),
            None,
        )
        .await;
        assert_eq!(list.status, StatusCode::FORBIDDEN);
        let dismiss = call(
            &s.h.app,
            "POST",
            &format!("/staff/password-reset-requests/{}/dismiss", Uuid::now_v7()),
            Some(who),
            None,
        )
        .await;
        assert_eq!(dismiss.status, StatusCode::FORBIDDEN);
    }
    create_tenant(&s.h.owner, "khac", "Spa khác", "Huế").await?;
    add_member(
        &s.h.owner,
        "khac",
        OTHER_OWNER_PHONE,
        "Chủ spa khác",
        SystemRole::Owner,
    )
    .await?;
    let other = session_for(&s.h, OTHER_OWNER_PHONE).await;
    let r = call_with_headers(
        &s.h.app,
        "POST",
        &format!("/staff/{}/login", s.ha),
        Some(&other),
        Some(json!({"phone": NEW_PHONE, "role": "technician"})),
        &[("If-Match", "\"1\"")],
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND, "{}", r.body);
    assert_eq!(s.reset(&other, s.ha).await.status, StatusCode::NOT_FOUND);
    let anonymous = call(
        &s.h.app,
        "GET",
        "/staff/password-reset-requests",
        None,
        None,
    )
    .await;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
    Ok(())
}

/// Creating refuses a phone any account already uses (never take over someone else's login),
/// a bad phone, a profile that has a login or has left, an older version.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_login_is_created_only_for_a_new_phone_and_a_working_profile(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let taken = s
        .create_login(&s.owner, s.ha, MANAGER_PHONE, "technician")
        .await;
    assert_eq!(taken.body["code"], "account.phone_taken", "{}", taken.body);
    let bad = s.create_login(&s.owner, s.ha, "12345", "technician").await;
    assert_eq!(bad.body["errors"][0]["field"], "phone", "{}", bad.body);
    let unknown_role = s.create_login(&s.owner, s.ha, NEW_PHONE, "owner").await;
    assert_eq!(unknown_role.status, StatusCode::UNPROCESSABLE_ENTITY);
    let stale = call_with_headers(
        &s.h.app,
        "POST",
        &format!("/staff/{}/login", s.ha),
        Some(&s.owner),
        Some(json!({"phone": NEW_PHONE, "role": "technician"})),
        &[("If-Match", "\"99\"")],
    )
    .await;
    assert_eq!(stale.status, StatusCode::PRECONDITION_FAILED);
    let ok = s
        .create_login(&s.owner, s.ha, NEW_PHONE, "technician")
        .await;
    assert_eq!(ok.status, StatusCode::CREATED, "{}", ok.body);
    assert_eq!(ok.body["profile"]["position"], "Kỹ thuật viên");
    let twice = s
        .create_login(&s.owner, s.ha, "0912300999", "technician")
        .await;
    assert_eq!(twice.body["code"], "staff.account_exists");
    // Nothing half-made on a refusal: one account for the phone, linked once.
    let accounts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app.user_account WHERE phone_e164 IN ('+84912300111', '+84912300999')",
    )
    .fetch_one(&s.h.owner)
    .await?;
    assert_eq!(accounts, 1);
    Ok(())
}

/// The owner's reset: a new temporary password once, every session of the login ends, the next
/// sign-in must change it. Never for the owner, a locked leaver or a login of another spa too.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn an_owner_reset_ends_every_session_and_forces_a_change(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let created = s
        .create_login(&s.owner, s.ha, NEW_PHONE, "technician")
        .await;
    let temporary = created.body["temporary_password"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let first = login(&s.h.app, NEW_PHONE, &temporary)
        .await
        .session()
        .unwrap_or_default();
    let session = change(&s.h.app, &first, &temporary, OWN_PASSWORD)
        .await
        .session()
        .unwrap_or_default();
    assert_eq!(
        call(&s.h.app, "GET", "/me", Some(&session), None)
            .await
            .status,
        StatusCode::OK
    );

    let reset = s.reset(&s.owner, s.ha).await;
    assert_eq!(reset.status, StatusCode::OK, "{}", reset.body);
    assert_eq!(
        reset
            .headers
            .get("cache-control")
            .and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    let second = reset.body["temporary_password"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert_ne!(second, temporary);
    assert_eq!(
        call(&s.h.app, "GET", "/me", Some(&session), None)
            .await
            .status,
        StatusCode::UNAUTHORIZED,
        "sessions revoked"
    );
    assert_eq!(
        login(&s.h.app, NEW_PHONE, OWN_PASSWORD).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        login(&s.h.app, NEW_PHONE, &second).await.body["next"],
        "password_change"
    );
    assert!(!s.audit_text().await?.contains(&second));

    // The owner's own profile and login: protected.
    let mine = call(
        &s.h.app,
        "POST",
        "/staff",
        Some(&s.owner),
        Some(json!({"full_name": "Chủ Spa", "position": "Chủ spa"})),
    )
    .await;
    let mine = id_of(&mine);
    sqlx::query("UPDATE app.staff SET user_id = (SELECT id FROM app.user_account WHERE phone_e164 = '+84905123456') WHERE id = $1")
        .bind(mine)
        .execute(&s.h.owner)
        .await?;
    assert_eq!(
        s.reset(&s.owner, mine).await.body["code"],
        "account.owner_protected"
    );
    // A login that also belongs to another spa: that spa's access is not ours to reset.
    create_tenant(&s.h.owner, "khac", "Spa khác", "Huế").await?;
    add_member(&s.h.owner, "khac", NEW_PHONE, "Hà", SystemRole::Technician).await?;
    assert_eq!(
        s.reset(&s.owner, s.ha).await.body["code"],
        "account.other_spa"
    );
    sqlx::query("DELETE FROM app.membership_role WHERE tenant_id = (SELECT id FROM app.tenant WHERE slug = 'khac')")
        .execute(&s.h.owner)
        .await?;
    sqlx::query("UPDATE app.membership SET status = 'disabled' WHERE tenant_id = (SELECT id FROM app.tenant WHERE slug = 'khac')")
        .execute(&s.h.owner)
        .await?;
    // Locked in the other spa still counts: unlocked there later, our password would open it
    // (0042). Red before: `tenants_of_user` saw only active memberships and allowed the reset.
    assert_eq!(
        s.reset(&s.owner, s.ha).await.body["code"],
        "account.other_spa"
    );
    // A leaver whose login was locked: a reset must not reopen access.
    sqlx::query("UPDATE app.membership SET status = 'disabled' WHERE user_id = (SELECT id FROM app.user_account WHERE phone_e164 = '+84912300111')")
        .execute(&s.h.owner)
        .await?;
    assert_eq!(s.reset(&s.owner, s.ha).await.body["code"], "account.locked");
    Ok(())
}

/// Quản lý ↔ Kỹ thuật viên: permissions follow on the very next request of the same session.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_role_change_applies_on_the_next_request(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let created = s
        .create_login(&s.owner, s.ha, NEW_PHONE, "technician")
        .await;
    let temporary = created.body["temporary_password"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let first = login(&s.h.app, NEW_PHONE, &temporary)
        .await
        .session()
        .unwrap_or_default();
    let session = change(&s.h.app, &first, &temporary, OWN_PASSWORD)
        .await
        .session()
        .unwrap_or_default();
    assert_eq!(
        call(&s.h.app, "GET", "/customers?q=a", Some(&session), None)
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    let role = call(
        &s.h.app,
        "PUT",
        &format!("/staff/{}/login/role", s.ha),
        Some(&s.owner),
        Some(json!({"role": "manager"})),
    )
    .await;
    assert_eq!(role.status, StatusCode::OK, "{}", role.body);
    assert_eq!(role.body["position"], "Quản lý");
    assert_ne!(
        call(&s.h.app, "GET", "/customers?q=a", Some(&session), None)
            .await
            .status,
        StatusCode::FORBIDDEN,
        "manager permissions on the next request"
    );
    assert!(s.audit_text().await?.contains("staff.login_role"));
    Ok(())
}

/// Creating, resetting and re-roling a login ask the owner for their password again when the
/// last check is older than the re-auth window.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn owner_login_actions_need_a_recent_reauth(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    sqlx::query("UPDATE app.session SET recent_auth_at = now() - interval '1 hour'")
        .execute(&s.h.owner)
        .await?;
    let r = s
        .create_login(&s.owner, s.ha, NEW_PHONE, "technician")
        .await;
    assert_eq!(r.body["code"], "auth.reauth_required", "{}", r.body);
    assert_eq!(
        s.reset(&s.owner, s.ha).await.body["code"],
        "auth.reauth_required"
    );
    let _ = forget_used_step(&s.h, OWNER_PHONE).await;
    let code = current_code(&s.h, OWNER_PHONE).await?;
    let reauth = auth_call(
        &s.h.app,
        "/auth/reauth",
        Some(&s.owner),
        json!({"password": PASSWORD, "code": code}),
    )
    .await;
    assert_eq!(reauth.status, StatusCode::NO_CONTENT, "{}", reauth.body);
    let r = s
        .create_login(&s.owner, s.ha, NEW_PHONE, "technician")
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);
    Ok(())
}

async fn ask(app: &axum::Router, phone: &str) -> Reply {
    auth_call(
        app,
        "/auth/password-reset-requests",
        None,
        json!({"phone": phone}),
    )
    .await
}

/// "Quên mật khẩu?" (option A): the answer never tells whether a phone has a login; the owner
/// sees a real request and handles it; nobody else sees it; another spa never does; the owner's
/// own phone records nothing; repeats do not pile up; a reset or "Bỏ qua" closes it.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn forgotten_password_requests_reach_only_the_owner(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let known = ask(&s.h.app, TECH_PHONE).await;
    let unknown = ask(&s.h.app, "0912399999").await;
    let garbage = ask(&s.h.app, "xin chào").await;
    for r in [&known, &unknown, &garbage] {
        assert_eq!(r.status, StatusCode::ACCEPTED, "{}", r.body);
        assert_eq!(r.body, json!({"status": "received"}));
    }
    assert_eq!(
        ask(&s.h.app, OWNER_PHONE).await.status,
        StatusCode::ACCEPTED
    );
    assert_eq!(
        ask(&s.h.app, TECH_PHONE).await.status,
        StatusCode::ACCEPTED,
        "a repeat"
    );

    let list = call(
        &s.h.app,
        "GET",
        "/staff/password-reset-requests",
        Some(&s.owner),
        None,
    )
    .await;
    assert_eq!(list.status, StatusCode::OK, "{}", list.body);
    let items = list.body.as_array().cloned().unwrap_or_default();
    assert_eq!(
        items.len(),
        1,
        "one request, not the owner's: {}",
        list.body
    );
    assert_eq!(items[0]["full_name"], "KTV Lan");
    for who in [&s.manager, &s.tech] {
        let r = call(
            &s.h.app,
            "GET",
            "/staff/password-reset-requests",
            Some(who),
            None,
        )
        .await;
        assert_eq!(r.status, StatusCode::FORBIDDEN);
    }
    // Another spa's owner sees none of ours, and cannot dismiss it.
    create_tenant(&s.h.owner, "khac", "Spa khác", "Huế").await?;
    add_member(
        &s.h.owner,
        "khac",
        OTHER_OWNER_PHONE,
        "Chủ spa khác",
        SystemRole::Owner,
    )
    .await?;
    let other = session_for(&s.h, OTHER_OWNER_PHONE).await;
    let theirs = call(
        &s.h.app,
        "GET",
        "/staff/password-reset-requests",
        Some(&other),
        None,
    )
    .await;
    assert_eq!(theirs.body, json!([]));
    let request = items[0]["id"].as_str().unwrap_or_default().to_owned();
    let dismiss = format!("/staff/password-reset-requests/{request}/dismiss");
    assert_eq!(
        call(&s.h.app, "POST", &dismiss, Some(&other), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    // "Bỏ qua".
    assert_eq!(
        call(&s.h.app, "POST", &dismiss, Some(&s.owner), None)
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        call(&s.h.app, "POST", &dismiss, Some(&s.owner), None)
            .await
            .status,
        StatusCode::NOT_FOUND,
        "closed"
    );
    let after = call(
        &s.h.app,
        "GET",
        "/staff/password-reset-requests",
        Some(&s.owner),
        None,
    )
    .await;
    assert_eq!(after.body, json!([]));
    // Within the hour a new request does not reach the owner again.
    ask(&s.h.app, TECH_PHONE).await;
    let still = call(
        &s.h.app,
        "GET",
        "/staff/password-reset-requests",
        Some(&s.owner),
        None,
    )
    .await;
    assert_eq!(still.body, json!([]));
    let audit = s.audit_text().await?;
    assert!(audit.contains("account.password_reset_requested"));
    assert!(audit.contains("account.password_reset_dismissed"));
    assert!(!audit.contains("912300444"), "no phone in the audit");

    // A reset answers an open request.
    sqlx::query("UPDATE app.password_reset_request SET requested_at = now() - interval '2 hours', expires_at = now() + interval '1 hour'")
        .execute(&s.h.owner)
        .await?;
    ask(&s.h.app, MANAGER_PHONE).await;
    let open = call(
        &s.h.app,
        "GET",
        "/staff/password-reset-requests",
        Some(&s.owner),
        None,
    )
    .await;
    assert_eq!(open.body.as_array().map(Vec::len), Some(1), "{}", open.body);
    let manager_profile = call(
        &s.h.app,
        "POST",
        "/staff",
        Some(&s.owner),
        Some(json!({"full_name": "Quản lý Hoa", "position": "x"})),
    )
    .await;
    let hoa = id_of(&manager_profile);
    sqlx::query("UPDATE app.staff SET user_id = (SELECT id FROM app.user_account WHERE phone_e164 = '+84912300333') WHERE id = $1")
        .bind(hoa)
        .execute(&s.h.owner)
        .await?;
    assert_eq!(s.reset(&s.owner, hoa).await.status, StatusCode::OK);
    let answered = call(
        &s.h.app,
        "GET",
        "/staff/password-reset-requests",
        Some(&s.owner),
        None,
    )
    .await;
    assert_eq!(answered.body, json!([]));
    Ok(())
}

/// The forgotten-password route sits under `/auth`: the per-address limit of the sign-in routes
/// applies (no flooding the owner, no phone enumeration at speed).
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn forgotten_password_requests_are_rate_limited(pool: PgPool) -> TestResult {
    let h = harness(pool).await?;
    let mut limited = false;
    for i in 0..30 {
        // One client address: the limit is per address.
        let r = call(
            &h.app,
            "POST",
            "/auth/password-reset-requests",
            None,
            Some(json!({"phone": format!("09123{i:05}")})),
        )
        .await;
        if r.status == StatusCode::TOO_MANY_REQUESTS {
            limited = true;
            break;
        }
        assert_eq!(r.status, StatusCode::ACCEPTED, "{}", r.body);
    }
    assert!(limited, "30 requests in a burst are throttled");
    Ok(())
}

/// Collects everything the app logs during a test.
#[derive(Clone, Default)]
struct Capture(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for Capture {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Ok(mut v) = self.0.lock() {
            v.extend_from_slice(buf);
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Temporary and chosen passwords never reach the logs, at any level, on any thread.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn passwords_never_reach_the_logs(pool: PgPool) -> TestResult {
    let capture = Capture::default();
    let writer = capture.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_writer(move || writer.clone())
        .finish();
    // Only this test installs a subscriber (one per process under nextest, too).
    let installed = tracing::subscriber::set_global_default(subscriber).is_ok();
    let s = spa(pool).await?;
    let created = s
        .create_login(&s.owner, s.ha, NEW_PHONE, "technician")
        .await;
    let temporary = created.body["temporary_password"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let first = login(&s.h.app, NEW_PHONE, &temporary)
        .await
        .session()
        .unwrap_or_default();
    change(&s.h.app, &first, &temporary, OWN_PASSWORD).await;
    let reset = s.reset(&s.owner, s.ha).await;
    let second = reset.body["temporary_password"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert!(!temporary.is_empty() && !second.is_empty());
    let logged = capture
        .0
        .lock()
        .map(|v| String::from_utf8_lossy(&v).into_owned())
        .unwrap_or_default();
    if installed {
        assert!(!logged.is_empty(), "the capture works");
    }
    for secret in [temporary.as_str(), second.as_str(), OWN_PASSWORD] {
        assert!(!logged.contains(secret), "a password was logged");
    }
    Ok(())
}
