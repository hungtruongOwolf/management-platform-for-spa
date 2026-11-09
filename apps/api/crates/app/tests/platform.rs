//! Platform admin console, part A (ADR-0018, spec platform-admin US1–US4, US9): the operator
//! principal, its MFA and step-up, the spa list, creating and suspending spas, the platform audit
//! trail, and the gated DB functions behind them.

mod common;

use axum::http::StatusCode;
use common::{
    OPERATOR, OPERATOR_PASSWORD, PASSWORD, TestResult, add_operator, admin_call, call, code_now,
    expire_step_up, harness, login, operator_session, session_for, step_up,
};
use serde_json::{Value, json};
use sqlx::PgPool;

fn new_spa(slug: &str, phone: &str) -> Value {
    json!({
        "name": "Spa Hoa Sen",
        "slug": slug,
        "owner": {"phone": phone, "full_name": "Chủ Hoa Sen"},
    })
}

/// `(name, security definer, config, PUBLIC may execute, app_rw may execute)`.
type FunctionRow = (String, bool, Option<Vec<String>>, bool, bool);

async fn count(pool: &PgPool, sql: &str) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
        .fetch_one(pool)
        .await
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn an_operator_always_signs_in_with_a_second_factor(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    add_operator(&h).await?;
    let first = admin_call(
        &h.app,
        "POST",
        "/admin/auth/login",
        None,
        Some(json!({"username": "OPS", "password": OPERATOR_PASSWORD})),
    )
    .await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.body);
    assert_eq!(first.body, json!({"next": "mfa_enroll"}));
    let pending = first.op_session().unwrap_or_default();
    // A pending session reaches nothing but the MFA steps.
    for path in ["/admin/me", "/admin/tenants", "/admin/audit"] {
        let reply = admin_call(&h.app, "GET", path, Some(&pending), None).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{path}");
        assert_eq!(reply.body["code"], "auth.mfa_enrollment_required", "{path}");
    }
    let full = operator_session(&h).await?;
    let me = admin_call(&h.app, "GET", "/admin/me", Some(&full), None).await;
    assert_eq!(me.status, StatusCode::OK, "{}", me.body);
    assert_eq!(me.body["operator"]["username"], OPERATOR);
    assert!(me.body["step_up_until"].is_string());
    assert_eq!(
        me.headers
            .get("cache-control")
            .and_then(|v| v.to_str().ok()),
        Some("no-store")
    );

    // Second sign-in: verify, not enroll; enroll/start is refused once enrolled.
    let again = admin_call(
        &h.app,
        "POST",
        "/admin/auth/login",
        None,
        Some(json!({"username": OPERATOR, "password": OPERATOR_PASSWORD})),
    )
    .await;
    assert_eq!(again.body, json!({"next": "mfa_verify"}));
    let pending = again.op_session().unwrap_or_default();
    let restart = admin_call(
        &h.app,
        "POST",
        "/admin/auth/mfa/enroll/start",
        Some(&pending),
        None,
    )
    .await;
    assert_eq!(restart.status, StatusCode::CONFLICT);
    let wrong = admin_call(
        &h.app,
        "POST",
        "/admin/auth/mfa/verify",
        Some(&pending),
        Some(json!({"code": "000000"})),
    )
    .await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn wrong_passwords_lock_the_operator(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    add_operator(&h).await?;
    let attempt = |username: &'static str, password: &'static str| {
        admin_call(
            &h.app,
            "POST",
            "/admin/auth/login",
            None,
            Some(json!({"username": username, "password": password})),
        )
    };
    let unknown = attempt("nobody", OPERATOR_PASSWORD).await;
    let wrong = attempt(OPERATOR, "sai-mat-khau-roi").await;
    for reply in [&unknown, &wrong] {
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
        assert_eq!(reply.body["code"], "auth.invalid_credentials");
        assert!(reply.op_session().is_none());
    }
    for _ in 0..4 {
        attempt(OPERATOR, "sai-mat-khau-roi").await;
    }
    let locked = attempt(OPERATOR, OPERATOR_PASSWORD).await;
    assert_eq!(locked.status, StatusCode::UNAUTHORIZED);
    assert_eq!(locked.body["code"], "auth.invalid_credentials");
    Ok(())
}

/// The spa cookie (even the owner's) is not an operator; the operator cookie is not a spa member.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn spa_and_operator_sessions_never_cross(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    add_operator(&h).await?;
    let spa_owner = session_for(&h, common::OWNER_PHONE).await;
    assert!(!spa_owner.is_empty());
    let as_spa = call(&h.app, "GET", "/admin/tenants", Some(&spa_owner), None).await;
    assert_eq!(as_spa.status, StatusCode::UNAUTHORIZED);
    // The same secret presented as an operator cookie is unknown to the operator tables.
    let forged = admin_call(&h.app, "GET", "/admin/tenants", Some(&spa_owner), None).await;
    assert_eq!(forged.status, StatusCode::UNAUTHORIZED);

    let op = operator_session(&h).await?;
    let as_operator = admin_call(&h.app, "GET", "/customers", Some(&op), None).await;
    assert_eq!(as_operator.status, StatusCode::UNAUTHORIZED);
    let me = call(&h.app, "GET", "/me", Some(&op), None).await;
    assert_eq!(me.status, StatusCode::UNAUTHORIZED);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn writes_need_a_recent_step_up(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    add_operator(&h).await?;
    let op = operator_session(&h).await?;
    expire_step_up(&h).await?;

    // Reads stay open; writes ask for a step-up.
    let list = admin_call(&h.app, "GET", "/admin/tenants", Some(&op), None).await;
    assert_eq!(list.status, StatusCode::OK);
    let refused = admin_call(
        &h.app,
        "POST",
        "/admin/tenants",
        Some(&op),
        Some(new_spa("hoa-sen", "0907000111")),
    )
    .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    assert_eq!(refused.body["code"], "platform.step_up_required");
    let me = admin_call(&h.app, "GET", "/admin/me", Some(&op), None).await;
    assert!(me.body["step_up_until"].is_null());

    // The database refuses too, whatever Rust thinks (a forgotten check is still a refusal).
    let hash = identity::token::hash_secret(&op);
    let app_pool = kernel::testing::app_pool(&h.owner).await?;
    let direct = sqlx::query("SELECT app.platform_insert_tenant($1, gen_random_uuid(), 'x', 'x')")
        .bind(&hash[..])
        .execute(&app_pool)
        .await;
    let code = direct.err().and_then(|e| {
        e.as_database_error()
            .and_then(|d| d.code().map(|c| c.into_owned()))
    });
    assert_eq!(code.as_deref(), Some("42501"));

    // Re-auth needs password and a code; a wrong code counts as a failure.
    let no_code = admin_call(
        &h.app,
        "POST",
        "/admin/auth/reauth",
        Some(&op),
        Some(json!({"password": OPERATOR_PASSWORD, "code": ""})),
    )
    .await;
    assert_eq!(no_code.status, StatusCode::UNPROCESSABLE_ENTITY);
    let reauth = step_up(&h, &op).await?;
    assert_eq!(reauth.status, StatusCode::NO_CONTENT, "{}", reauth.body);
    let created = admin_call(
        &h.app,
        "POST",
        "/admin/tenants",
        Some(&op),
        Some(new_spa("hoa-sen", "0907000111")),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    Ok(())
}

/// Spec US3: the spa, its roles and its owner in one go; the owner signs in with the temporary
/// password, enrolls MFA, must change the password, then works.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn creating_a_spa_shows_the_password_once(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    add_operator(&h).await?;
    let op = operator_session(&h).await?;
    let created = admin_call(
        &h.app,
        "POST",
        "/admin/tenants",
        Some(&op),
        Some(new_spa("hoa-sen", "0907000111")),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    assert_eq!(
        created
            .headers
            .get("cache-control")
            .and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    let temporary = created.body["owner"]["temporary_password"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert_eq!(temporary.len(), 14);
    assert_eq!(created.body["tenant"]["slug"], "hoa-sen");
    let tenant_id = created.body["tenant"]["id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();

    // Three system roles with the default grants, one owner membership.
    let roles = count(
        &h.owner,
        &format!("SELECT count(*) FROM app.role WHERE tenant_id = '{tenant_id}' AND is_system"),
    )
    .await?;
    assert_eq!(roles, 3);

    // The temporary password is nowhere else: not in either audit trail.
    let leaks = count(
        &h.owner,
        &format!(
            "SELECT (SELECT count(*) FROM app.audit_log WHERE concat(before, after, reason) LIKE '%{temporary}%')
                  + (SELECT count(*) FROM app.platform_audit_log WHERE concat(detail, reason) LIKE '%{temporary}%')"
        ),
    )
    .await?;
    assert_eq!(leaks, 0);
    let spa_audit = count(
        &h.owner,
        &format!(
            "SELECT count(*) FROM app.audit_log WHERE tenant_id = '{tenant_id}'
               AND action = 'tenant.created' AND actor_kind = 'platform_admin'"
        ),
    )
    .await?;
    assert_eq!(spa_audit, 1);
    let platform_audit: Value = sqlx::query_scalar(
        "SELECT detail FROM app.platform_audit_log WHERE action = 'platform.tenant.create'",
    )
    .fetch_one(&h.owner)
    .await?;
    assert_eq!(platform_audit["owner_phone"], "…111");

    // The new owner: temporary password → MFA enrollment (owner) → must change the password.
    let first = login(&h.app, "0907000111", &temporary).await;
    assert_eq!(first.body["next"], "mfa_enroll", "{}", first.body);
    let enrolled = common::enroll(&h, &first.session().unwrap_or_default()).await;
    let session = enrolled.session().unwrap_or_default();
    let gated = call(&h.app, "GET", "/me", Some(&session), None).await;
    assert_eq!(gated.status, StatusCode::FORBIDDEN);
    assert_eq!(gated.body["code"], "auth.password_change_required");
    let changed = call(
        &h.app,
        "POST",
        "/auth/password",
        Some(&session),
        Some(json!({"current_password": temporary, "new_password": "mat-khau-moi-cua-chu"})),
    )
    .await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.body);
    let me = call(
        &h.app,
        "GET",
        "/me",
        Some(&changed.session().unwrap_or_default()),
        None,
    )
    .await;
    assert_eq!(me.body["active_tenant"]["name"], "Spa Hoa Sen");
    assert!(
        me.body["permissions"]
            .as_array()
            .is_some_and(|p| p.contains(&json!("staff.manage")))
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_failed_creation_leaves_nothing_behind(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    add_operator(&h).await?;
    let op = operator_session(&h).await?;
    let tenants = || count(&h.owner, "SELECT count(*) FROM app.tenant");
    let before = tenants().await?;
    let taken_slug = admin_call(
        &h.app,
        "POST",
        "/admin/tenants",
        Some(&op),
        Some(new_spa("vpyd", "0907000111")),
    )
    .await;
    assert_eq!(taken_slug.status, StatusCode::CONFLICT);
    assert_eq!(taken_slug.body["code"], "tenant.slug_taken");
    // One person, one spa (ADR-0016): the harness owner's phone already has a login.
    let taken_phone = admin_call(
        &h.app,
        "POST",
        "/admin/tenants",
        Some(&op),
        Some(new_spa("hoa-sen", common::OWNER_PHONE)),
    )
    .await;
    assert_eq!(taken_phone.status, StatusCode::CONFLICT);
    assert_eq!(taken_phone.body["code"], "account.phone_taken");
    let invalid = admin_call(
        &h.app,
        "POST",
        "/admin/tenants",
        Some(&op),
        Some(new_spa("Bad Slug", "12")),
    )
    .await;
    assert_eq!(invalid.status, StatusCode::UNPROCESSABLE_ENTITY);
    let extra = admin_call(
        &h.app,
        "POST",
        "/admin/tenants",
        Some(&op),
        Some(json!({"name": "x", "slug": "x", "owner": {"phone": "0907000111", "full_name": "x"}, "status": "active"})),
    )
    .await;
    assert_eq!(extra.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(tenants().await?, before);
    assert_eq!(
        count(
            &h.owner,
            "SELECT count(*) FROM app.user_account WHERE phone_e164 = '+84907000111'"
        )
        .await?,
        0
    );
    Ok(())
}

/// Spec US2: counts and metadata only — the list carries no business data.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn the_spa_list_is_counts_only(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    add_operator(&h).await?;
    let op = operator_session(&h).await?;
    let list = admin_call(&h.app, "GET", "/admin/tenants", Some(&op), None).await;
    assert_eq!(list.status, StatusCode::OK, "{}", list.body);
    let spas = list.body.as_array().cloned().unwrap_or_default();
    assert_eq!(spas.len(), 1);
    let mut keys: Vec<String> = spas[0]
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();
    keys.sort();
    assert_eq!(
        keys,
        [
            "created_at",
            "customer_count",
            "id",
            "last_activity_at",
            "name",
            "owner_count",
            "slug",
            "staff_count",
            "status",
            "visits_30d"
        ]
    );
    assert_eq!(spas[0]["owner_count"], 1);
    assert_eq!(spas[0]["status"], "active");
    let id = spas[0]["id"].as_str().unwrap_or_default();
    let one = admin_call(
        &h.app,
        "GET",
        &format!("/admin/tenants/{id}"),
        Some(&op),
        None,
    )
    .await;
    assert_eq!(one.body["slug"], "vpyd");
    let missing = admin_call(
        &h.app,
        "GET",
        &format!("/admin/tenants/{}", uuid::Uuid::now_v7()),
        Some(&op),
        None,
    )
    .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
    Ok(())
}

/// Spec US4: members of a suspended spa cannot sign in and lose their open sessions; data stays.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_suspended_spa_cannot_sign_in(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    add_operator(&h).await?;
    let op = operator_session(&h).await?;
    let spa_owner = session_for(&h, common::OWNER_PHONE).await;
    let id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM app.tenant WHERE slug = 'vpyd'")
        .fetch_one(&h.owner)
        .await?;
    let path = format!("/admin/tenants/{id}/suspend");
    let no_reason = admin_call(
        &h.app,
        "POST",
        &path,
        Some(&op),
        Some(json!({"reason": " "})),
    )
    .await;
    assert_eq!(no_reason.status, StatusCode::UNPROCESSABLE_ENTITY);
    let suspended = admin_call(
        &h.app,
        "POST",
        &path,
        Some(&op),
        Some(json!({"reason": "Chưa gia hạn hợp đồng"})),
    )
    .await;
    assert_eq!(suspended.status, StatusCode::OK, "{}", suspended.body);
    assert_eq!(suspended.body["tenant"]["status"], "suspended");
    assert!(
        suspended.body["revoked_sessions"]
            .as_i64()
            .unwrap_or_default()
            >= 1
    );

    let old = call(&h.app, "GET", "/customers", Some(&spa_owner), None).await;
    assert_eq!(old.status, StatusCode::UNAUTHORIZED);
    let refused = login(&h.app, common::OWNER_PHONE, PASSWORD).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    assert_eq!(refused.body["code"], "auth.spa_suspended");
    let again = admin_call(
        &h.app,
        "POST",
        &path,
        Some(&op),
        Some(json!({"reason": "Lần nữa"})),
    )
    .await;
    assert_eq!(again.status, StatusCode::CONFLICT);
    assert_eq!(again.body["code"], "tenant.status_unchanged");

    let reopened = admin_call(
        &h.app,
        "POST",
        &format!("/admin/tenants/{id}/unsuspend"),
        Some(&op),
        Some(json!({"reason": "Đã gia hạn"})),
    )
    .await;
    assert_eq!(reopened.status, StatusCode::OK, "{}", reopened.body);
    let back = login(&h.app, common::OWNER_PHONE, PASSWORD).await;
    assert_eq!(back.status, StatusCode::OK);
    assert_eq!(back.body["next"], "mfa_verify");

    let trail = count(
        &h.owner,
        &format!(
            "SELECT count(*) FROM app.audit_log WHERE tenant_id = '{id}' AND actor_kind = 'platform_admin'
               AND action IN ('tenant.suspended', 'tenant.unsuspended')"
        ),
    )
    .await?;
    assert_eq!(trail, 2);
    Ok(())
}

/// Every operator write leaves one platform audit row, and the trail is append-only.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn every_operator_write_is_audited(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    add_operator(&h).await?;
    let op = operator_session(&h).await?;
    let created = admin_call(
        &h.app,
        "POST",
        "/admin/tenants",
        Some(&op),
        Some(new_spa("hoa-sen", "0907000111")),
    )
    .await;
    let id = created.body["tenant"]["id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    admin_call(
        &h.app,
        "POST",
        &format!("/admin/tenants/{id}/suspend"),
        Some(&op),
        Some(json!({"reason": "Thử nghiệm"})),
    )
    .await;
    let page = admin_call(&h.app, "GET", "/admin/audit?limit=2", Some(&op), None).await;
    assert_eq!(page.status, StatusCode::OK, "{}", page.body);
    let actions: Vec<&str> = page.body["items"]
        .as_array()
        .map(|items| items.iter().filter_map(|i| i["action"].as_str()).collect())
        .unwrap_or_default();
    assert_eq!(
        actions,
        ["platform.tenant.suspend", "platform.tenant.create"]
    );
    assert_eq!(page.body["items"][0]["reason"], "Thử nghiệm");
    assert_eq!(page.body["items"][0]["operator_username"], OPERATOR);
    let before = page.body["next_before"].as_str().unwrap_or_default();
    let older = admin_call(
        &h.app,
        "GET",
        &format!(
            "/admin/audit?limit=100&before={}",
            before.replace('+', "%2B")
        ),
        Some(&op),
        None,
    )
    .await;
    assert!(
        older.body["items"]
            .as_array()
            .is_some_and(|items| items.iter().any(|i| i["action"] == "platform.auth.login"))
    );

    let app_pool = kernel::testing::app_pool(&h.owner).await?;
    for sql in [
        "UPDATE app.platform_audit_log SET reason = 'x'",
        "DELETE FROM app.platform_audit_log",
    ] {
        let denied = sqlx::query(sql).execute(&app_pool).await;
        assert!(denied.is_err(), "{sql} must be refused to app_rw");
    }
    Ok(())
}

/// The cross-tenant functions: definer, pinned search_path, not executable by PUBLIC, and refused
/// without a live, MFA-complete operator session.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn platform_functions_check_the_operator_session_themselves(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    add_operator(&h).await?;
    let functions: Vec<FunctionRow> = sqlx::query_as(
        "SELECT p.proname, p.prosecdef, p.proconfig,
                has_function_privilege('public', p.oid, 'EXECUTE'),
                has_function_privilege('app_rw', p.oid, 'EXECUTE')
         FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace
         WHERE n.nspname = 'app' AND (p.proname LIKE 'platform\\_%' OR p.proname = 'user_spa_suspended')",
    )
    .fetch_all(&h.owner)
    .await?;
    assert!(functions.len() >= 5, "{functions:?}");
    for (name, definer, config, public, app_rw) in &functions {
        assert!(definer, "{name} must be SECURITY DEFINER");
        assert_eq!(
            config.as_deref(),
            Some(&["search_path=app, pg_temp".to_owned()][..]),
            "{name}"
        );
        assert!(!public, "{name} must not be executable by PUBLIC");
        assert_eq!(
            *app_rw,
            name != "platform_gate",
            "{name}: EXECUTE for app_rw"
        );
    }

    let app_pool = kernel::testing::app_pool(&h.owner).await?;
    let refused = |hash: Vec<u8>| {
        let pool = app_pool.clone();
        async move {
            sqlx::query("SELECT * FROM app.platform_tenant_overview($1)")
                .bind(hash)
                .fetch_all(&pool)
                .await
                .err()
                .and_then(|e| {
                    e.as_database_error()
                        .and_then(|d| d.code().map(|c| c.into_owned()))
                })
        }
    };
    assert_eq!(refused(vec![0; 32]).await.as_deref(), Some("42501"));
    // A spa owner's session hash is no operator session.
    let spa_owner = session_for(&h, common::OWNER_PHONE).await;
    let spa_hash = identity::token::hash_secret(&spa_owner).to_vec();
    assert_eq!(refused(spa_hash).await.as_deref(), Some("42501"));
    // Pending MFA is not enough.
    let pending = admin_call(
        &h.app,
        "POST",
        "/admin/auth/login",
        None,
        Some(json!({"username": OPERATOR, "password": OPERATOR_PASSWORD})),
    )
    .await
    .op_session()
    .unwrap_or_default();
    let pending_hash = identity::token::hash_secret(&pending).to_vec();
    assert_eq!(refused(pending_hash).await.as_deref(), Some("42501"));
    // A full session reads; after logout it no longer does.
    let op = operator_session(&h).await?;
    let op_hash = identity::token::hash_secret(&op).to_vec();
    assert_eq!(refused(op_hash.clone()).await, None);
    let out = admin_call(&h.app, "POST", "/admin/auth/logout", Some(&op), None).await;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
    assert_eq!(refused(op_hash).await.as_deref(), Some("42501"));
    assert_eq!(
        admin_call(&h.app, "GET", "/admin/me", Some(&op), None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );

    // app_rw cannot insert a tenant on its own: only through the gated function.
    let direct =
        sqlx::query("INSERT INTO app.tenant (id, name, slug) VALUES (gen_random_uuid(), 'x', 'x')")
            .execute(&app_pool)
            .await;
    assert!(direct.is_err());
    Ok(())
}

/// A recovery code completes a sign-in once; the same TOTP step is never accepted twice.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn operator_codes_are_single_use(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    add_operator(&h).await?;
    let first = admin_call(
        &h.app,
        "POST",
        "/admin/auth/login",
        None,
        Some(json!({"username": OPERATOR, "password": OPERATOR_PASSWORD})),
    )
    .await;
    let pending = first.op_session().unwrap_or_default();
    let start = admin_call(
        &h.app,
        "POST",
        "/admin/auth/mfa/enroll/start",
        Some(&pending),
        None,
    )
    .await;
    let key = start.body["manual_key"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let code = code_now(&key);
    let confirmed = admin_call(
        &h.app,
        "POST",
        "/admin/auth/mfa/enroll/confirm",
        Some(&pending),
        Some(json!({"code": code})),
    )
    .await;
    assert_eq!(confirmed.status, StatusCode::OK, "{}", confirmed.body);
    let recovery = confirmed.body["recovery_codes"][0]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert_eq!(
        confirmed.body["recovery_codes"].as_array().map(Vec::len),
        Some(10)
    );

    let sign_in = || {
        admin_call(
            &h.app,
            "POST",
            "/admin/auth/login",
            None,
            Some(json!({"username": OPERATOR, "password": OPERATOR_PASSWORD})),
        )
    };
    let replay_pending = sign_in().await.op_session().unwrap_or_default();
    let replay = admin_call(
        &h.app,
        "POST",
        "/admin/auth/mfa/verify",
        Some(&replay_pending),
        Some(json!({"code": code})),
    )
    .await;
    assert_eq!(
        replay.status,
        StatusCode::UNAUTHORIZED,
        "a used step is a replay"
    );
    let with_recovery = admin_call(
        &h.app,
        "POST",
        "/admin/auth/mfa/verify",
        Some(&replay_pending),
        Some(json!({"code": recovery})),
    )
    .await;
    assert_eq!(
        with_recovery.status,
        StatusCode::OK,
        "{}",
        with_recovery.body
    );
    let reused_pending = sign_in().await.op_session().unwrap_or_default();
    let reused = admin_call(
        &h.app,
        "POST",
        "/admin/auth/mfa/verify",
        Some(&reused_pending),
        Some(json!({"code": recovery})),
    )
    .await;
    assert_eq!(reused.status, StatusCode::UNAUTHORIZED);
    Ok(())
}

/// ADR-0018 D3: only the platform crate's SQL names the operator tables and functions, so no spa
/// route can reach them by accident.
#[test]
fn platform_sql_stays_in_platform() -> TestResult {
    let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut offenders = Vec::new();
    let mut stack = vec![crates];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if path.is_dir() {
                if !matches!(name, "tests" | "target" | "platform") {
                    stack.push(path);
                }
            } else if name.ends_with(".rs")
                && std::fs::read_to_string(&path)?.contains("app.platform_")
            {
                offenders.push(path.display().to_string());
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "app.platform_* outside crates/platform: {offenders:?}"
    );
    Ok(())
}
