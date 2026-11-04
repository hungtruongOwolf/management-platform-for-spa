//! MFA (TOTP) end to end (S2b, ADR-0004): two-step login, forced enrollment for owners, replay
//! and recovery-code rules, lockout, and that a pending-MFA session reaches nothing else.
//! Each test builds its own router, so the per-IP /auth limit (burst 10) bounds calls per test.

mod common;

use axum::http::StatusCode;
use common::{
    Harness, OWNER_PHONE, PASSWORD, Reply, TestResult, add_member, call, code_now, current_code,
    enroll, forget_used_step, harness, login, mfa_verify, session_for, stored_secret,
};
use identity::SystemRole;
use serde_json::json;
use sqlx::PgPool;

/// A 6-digit code that is not the current one (and, barring a 1-in-a-million collision, not a
/// neighbouring step's either).
fn wrong(code: &str) -> String {
    let n: u32 = code.parse().unwrap_or(0);
    format!("{:06}", (n + 500_000) % 1_000_000)
}

/// Owner logs in for the first time and enrolls; returns the confirm reply (full session + codes).
async fn enrolled_owner(h: &Harness) -> Reply {
    let pending = login(&h.app, OWNER_PHONE, PASSWORD)
        .await
        .session()
        .unwrap_or_default();
    enroll(h, &pending).await
}

async fn pending_login(h: &Harness) -> (String, Reply) {
    let reply = login(&h.app, OWNER_PHONE, PASSWORD).await;
    (reply.session().unwrap_or_default(), reply)
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn owner_without_mfa_must_enroll_before_anything_else(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    let (pending, first) = pending_login(&h).await;
    assert_eq!(first.status, StatusCode::OK);
    assert_eq!(first.body["next"], "mfa_enroll");
    assert!(
        first.body["me"].is_null(),
        "no spa or permission data before MFA"
    );
    assert!(!pending.is_empty());

    let early = mfa_verify(&h, &pending, "123456").await;
    assert_eq!(
        (early.status, early.body["code"].as_str()),
        (
            StatusCode::UNAUTHORIZED,
            Some("auth.mfa_enrollment_required")
        )
    );

    let start = call(
        &h.app,
        "POST",
        "/auth/mfa/enroll/start",
        Some(&pending),
        None,
    )
    .await;
    assert_eq!(start.status, StatusCode::OK);
    let key = start.body["manual_key"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let uri = start.body["otpauth_uri"].as_str().unwrap_or_default();
    assert!(uri.starts_with("otpauth://totp/"), "{uri}");
    assert!(uri.contains(&format!("secret={key}")));
    assert_eq!(key.len(), 32, "160-bit secret");

    let ct: Vec<u8> = sqlx::query_scalar(
        "SELECT mfa_secret_ct FROM app.user_account WHERE mfa_secret_ct IS NOT NULL",
    )
    .fetch_one(&h.owner)
    .await?;
    assert!(
        !String::from_utf8_lossy(&ct).contains(&key),
        "secret is stored encrypted"
    );

    let bad = call(
        &h.app,
        "POST",
        "/auth/mfa/enroll/confirm",
        Some(&pending),
        Some(json!({"code": wrong(&code_now(&key))})),
    )
    .await;
    assert_eq!(
        (bad.status, bad.body["code"].as_str()),
        (StatusCode::UNAUTHORIZED, Some("auth.invalid_mfa_code"))
    );

    let done = call(
        &h.app,
        "POST",
        "/auth/mfa/enroll/confirm",
        Some(&pending),
        Some(json!({"code": code_now(&key)})),
    )
    .await;
    assert_eq!(done.status, StatusCode::OK);
    let codes = done.body["recovery_codes"]
        .as_array()
        .map(Vec::len)
        .unwrap_or(0);
    assert_eq!(codes, 10);
    assert_eq!(
        done.body["me"]["active_tenant"]["name"],
        "Viên Phúc Y Đường"
    );

    // Completing MFA rotates the token: the pending one is dead, the new one is a full session.
    let full = done.session().unwrap_or_default();
    assert_ne!(full, pending);
    assert_eq!(
        call(&h.app, "GET", "/me", Some(&pending), None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(&h.app, "GET", "/me", Some(&full), None).await.status,
        StatusCode::OK
    );

    // Enrolled now: the next login asks for a code, and enrollment cannot be restarted.
    let (again, second) = pending_login(&h).await;
    assert_eq!(second.body["next"], "mfa_verify");
    let restart = call(&h.app, "POST", "/auth/mfa/enroll/start", Some(&again), None).await;
    assert_eq!(
        (restart.status, restart.body["code"].as_str()),
        (StatusCode::CONFLICT, Some("auth.mfa_already_enrolled"))
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn pending_session_reaches_only_the_mfa_endpoints(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    let (pending, _) = pending_login(&h).await;

    let me = call(&h.app, "GET", "/me", Some(&pending), None).await;
    assert_eq!(
        (me.status, me.body["code"].as_str()),
        (
            StatusCode::UNAUTHORIZED,
            Some("auth.mfa_enrollment_required")
        )
    );
    for path in ["/services", "/service-categories", "/customers?q=a"] {
        let reply = call(&h.app, "GET", path, Some(&pending), None).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{path}");
    }
    let reauth = call(
        &h.app,
        "POST",
        "/auth/reauth",
        Some(&pending),
        Some(json!({"password": PASSWORD})),
    )
    .await;
    assert_eq!(reauth.status, StatusCode::UNAUTHORIZED);
    let logout_all = call(&h.app, "POST", "/auth/logout-all", Some(&pending), None).await;
    assert_eq!(
        logout_all.status,
        StatusCode::UNAUTHORIZED,
        "a password alone must not end the owner's other devices"
    );

    // The pending session is short-lived and can be abandoned.
    let minutes: f64 = sqlx::query_scalar(
        "SELECT extract(epoch FROM absolute_expires_at - created_at)::float8 / 60 FROM app.session WHERE mfa_pending",
    )
    .fetch_one(&h.owner)
    .await?;
    assert!(minutes <= 10.5, "pending session lives {minutes} min");
    let out = call(&h.app, "POST", "/auth/logout", Some(&pending), None).await;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
    let after = call(
        &h.app,
        "POST",
        "/auth/mfa/enroll/start",
        Some(&pending),
        None,
    )
    .await;
    assert_eq!(after.status, StatusCode::UNAUTHORIZED);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_full_session_cannot_use_the_mfa_step_endpoints(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    let full = enrolled_owner(&h).await.session().unwrap_or_default();
    let reply = mfa_verify(&h, &full, "123456").await;
    assert_eq!(
        (reply.status, reply.body["code"].as_str()),
        (StatusCode::CONFLICT, Some("auth.mfa_not_pending"))
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn wrong_and_replayed_codes_are_refused(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    enrolled_owner(&h).await;
    let (pending, _) = pending_login(&h).await;
    let code = current_code(&h, OWNER_PHONE).await?;

    let bad = mfa_verify(&h, &pending, &wrong(&code)).await;
    assert_eq!(
        (bad.status, bad.body["code"].as_str()),
        (StatusCode::UNAUTHORIZED, Some("auth.invalid_mfa_code"))
    );
    assert!(bad.session().is_none());

    // Start from an unused step so the test does not depend on where the 30 s boundary falls
    // relative to enrollment; replay is asserted below against a code this test just used.
    forget_used_step(&h, OWNER_PHONE).await?;
    let ok = mfa_verify(&h, &pending, &code).await;
    assert_eq!(ok.status, StatusCode::OK);
    assert_eq!(ok.body["active_tenant"]["name"], "Viên Phúc Y Đường");
    let full = ok.session().unwrap_or_default();
    assert_eq!(
        call(&h.app, "GET", "/services", Some(&full), None)
            .await
            .status,
        StatusCode::OK
    );

    // A second login presenting the code just used (an observed / phished code) is a replay.
    let (pending, _) = pending_login(&h).await;
    let replay = mfa_verify(&h, &pending, &code).await;
    assert_eq!(
        (replay.status, replay.body["code"].as_str()),
        (StatusCode::UNAUTHORIZED, Some("auth.invalid_mfa_code"))
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn recovery_codes_work_once_and_are_stored_hashed(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    let enrolled = enrolled_owner(&h).await;
    let codes: Vec<String> = enrolled.body["recovery_codes"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|c| c.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let (first, second) = (
        codes.first().cloned().unwrap_or_default(),
        codes.get(1).cloned().unwrap_or_default(),
    );

    let stored: Vec<Vec<u8>> = sqlx::query_scalar("SELECT code_hash FROM app.mfa_recovery_code")
        .fetch_all(&h.owner)
        .await?;
    assert_eq!(stored.len(), 10);
    assert!(
        stored
            .iter()
            .all(|hash| !String::from_utf8_lossy(hash).contains(&first[..4]))
    );

    let (pending, _) = pending_login(&h).await;
    let used = mfa_verify(&h, &pending, &first.to_uppercase()).await;
    assert_eq!(used.status, StatusCode::OK, "case and dashes do not matter");

    let (pending, _) = pending_login(&h).await;
    let again = mfa_verify(&h, &pending, &first).await;
    assert_eq!(again.status, StatusCode::UNAUTHORIZED, "single use");
    let other = mfa_verify(&h, &pending, &second).await;
    assert_eq!(other.status, StatusCode::OK);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn repeated_bad_codes_lock_the_account(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    enrolled_owner(&h).await;
    let (pending, _) = pending_login(&h).await;
    forget_used_step(&h, OWNER_PHONE).await?;
    let code = current_code(&h, OWNER_PHONE).await?;
    for _ in 0..5 {
        assert_eq!(
            mfa_verify(&h, &pending, &wrong(&code)).await.status,
            StatusCode::UNAUTHORIZED
        );
    }
    let right = mfa_verify(&h, &pending, &code).await;
    assert_eq!(
        right.status,
        StatusCode::UNAUTHORIZED,
        "locked: even the right code is refused"
    );
    let locked: bool = sqlx::query_scalar(
        "SELECT locked_until > now() FROM app.user_account WHERE phone_e164 = '+84905123456'",
    )
    .fetch_one(&h.owner)
    .await?;
    assert!(locked);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn re_entering_the_password_does_not_reset_the_mfa_failure_count(
    owner: PgPool,
) -> TestResult {
    let h = harness(owner).await?;
    enrolled_owner(&h).await;
    let code = current_code(&h, OWNER_PHONE).await?;
    let (pending, _) = pending_login(&h).await;
    for _ in 0..3 {
        mfa_verify(&h, &pending, &wrong(&code)).await;
    }
    let (pending, _) = pending_login(&h).await;
    for _ in 0..2 {
        mfa_verify(&h, &pending, &wrong(&code)).await;
    }
    let (failures, locked): (i32, bool) = sqlx::query_as(
        "SELECT failed_logins, coalesce(locked_until > now(), false) FROM app.user_account WHERE phone_e164 = '+84905123456'",
    )
    .fetch_one(&h.owner)
    .await?;
    assert_eq!((failures, locked), (5, true));
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn manager_logs_in_with_password_only(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    add_member(
        &h.owner,
        "vpyd",
        "0912000111",
        "Quản lý",
        SystemRole::Manager,
    )
    .await?;
    let reply = login(&h.app, "0912000111", PASSWORD).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.body["next"], "done");
    assert_eq!(
        reply.body["me"]["active_tenant"]["name"],
        "Viên Phúc Y Đường"
    );
    let token = reply.session().unwrap_or_default();
    assert_eq!(
        call(&h.app, "GET", "/me", Some(&token), None).await.status,
        StatusCode::OK
    );
    let reauth = call(
        &h.app,
        "POST",
        "/auth/reauth",
        Some(&token),
        Some(json!({"password": PASSWORD})),
    )
    .await;
    assert_eq!(reauth.status, StatusCode::NO_CONTENT, "no code needed");
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn mfa_requirement_is_role_data(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    add_member(
        &h.owner,
        "vpyd",
        "0912000222",
        "KTV",
        SystemRole::Technician,
    )
    .await?;
    sqlx::query("UPDATE app.role SET requires_mfa = true WHERE key = 'technician'")
        .execute(&h.owner)
        .await?;
    let reply = login(&h.app, "0912000222", PASSWORD).await;
    assert_eq!(reply.body["next"], "mfa_enroll");

    let unset = sqlx::query("UPDATE app.role SET requires_mfa = false WHERE key = 'owner'")
        .execute(&h.owner)
        .await;
    assert!(
        unset.is_err(),
        "the system owner role can never drop MFA (CHECK constraint)"
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn reauth_for_an_mfa_account_needs_a_fresh_code(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    let full = enrolled_owner(&h).await.session().unwrap_or_default();
    let reauth = |body: serde_json::Value| {
        let (app, full) = (h.app.clone(), full.clone());
        async move { call(&app, "POST", "/auth/reauth", Some(&full), Some(body)).await }
    };

    let missing = reauth(json!({"password": PASSWORD})).await;
    assert_eq!(missing.status, StatusCode::UNPROCESSABLE_ENTITY);
    // Deterministic regardless of the 30 s boundary: use a fresh step once, then replay it.
    forget_used_step(&h, OWNER_PHONE).await?;
    let code = current_code(&h, OWNER_PHONE).await?;
    let ok = reauth(json!({"password": PASSWORD, "code": code})).await;
    assert_eq!(ok.status, StatusCode::NO_CONTENT);
    let replayed = reauth(json!({"password": PASSWORD, "code": code})).await;
    assert_eq!(
        (replayed.status, replayed.body["code"].as_str()),
        (StatusCode::UNAUTHORIZED, Some("auth.invalid_mfa_code"))
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn operator_reset_sends_the_owner_back_to_enrollment(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    let full = session_for(&h, OWNER_PHONE).await;
    assert!(stored_secret(&h, OWNER_PHONE).await.is_ok());

    assert!(identity::provisioning::reset_mfa(&h.owner, OWNER_PHONE).await?);
    assert_eq!(
        call(&h.app, "GET", "/me", Some(&full), None).await.status,
        StatusCode::UNAUTHORIZED,
        "reset ends existing sessions"
    );
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM app.mfa_recovery_code")
        .fetch_one(&h.owner)
        .await?;
    assert_eq!(left, 0);
    let (_, reply) = pending_login(&h).await;
    assert_eq!(reply.body["next"], "mfa_enroll");
    Ok(())
}
