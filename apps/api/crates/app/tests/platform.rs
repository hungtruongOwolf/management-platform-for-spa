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

    // 0049: one spa's row comes from the function's own filter, not from the whole overview.
    let other = uuid::Uuid::now_v7();
    sqlx::query("INSERT INTO app.tenant (id, name, slug) VALUES ($1, 'Spa khác', 'spa-khac')")
        .bind(other)
        .execute(&h.owner)
        .await?;
    let hash = identity::token::hash_secret(&op).to_vec();
    let app_pool = kernel::testing::app_pool(&h.owner).await?;
    let rows = |tenant: Option<uuid::Uuid>| {
        let (pool, hash) = (app_pool.clone(), hash.clone());
        async move {
            sqlx::query_scalar::<_, uuid::Uuid>(
                "SELECT id FROM app.platform_tenant_overview($1, $2)",
            )
            .bind(hash)
            .bind(tenant)
            .fetch_all(&pool)
            .await
        }
    };
    assert_eq!(rows(None).await?.len(), 2);
    assert_eq!(rows(Some(other)).await?, vec![other]);
    assert!(rows(Some(uuid::Uuid::now_v7())).await?.is_empty());
    let one = admin_call(
        &h.app,
        "GET",
        &format!("/admin/tenants/{other}"),
        Some(&op),
        None,
    )
    .await;
    assert_eq!(one.body["slug"], "spa-khac", "{}", one.body);
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

/// Spec US5: the operator switches a feature off for one spa; the spa's next request loses it —
/// `/me` no longer lists it or its permissions, its routes answer `403 feature.disabled` — and
/// switching it back on restores everything. Audited on both sides.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn an_operator_switches_a_feature_for_one_spa(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    add_operator(&h).await?;
    let op = operator_session(&h).await?;
    let spa_owner = session_for(&h, common::OWNER_PHONE).await;
    let id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM app.tenant WHERE slug = 'vpyd'")
        .fetch_one(&h.owner)
        .await?;
    let registry = admin_call(&h.app, "GET", "/admin/features", Some(&op), None).await;
    assert_eq!(
        registry.body.as_array().map(Vec::len),
        Some(kernel::Feature::ALL.len())
    );
    let before = admin_call(
        &h.app,
        "GET",
        &format!("/admin/tenants/{id}/features"),
        Some(&op),
        None,
    )
    .await;
    assert!(
        before
            .body
            .as_array()
            .is_some_and(|f| f.iter().all(|x| x["enabled"] == true)),
        "every feature is on by default: {}",
        before.body
    );

    let path = format!("/admin/tenants/{id}/features/appointments");
    let unknown = admin_call(
        &h.app,
        "PUT",
        &format!("/admin/tenants/{id}/features/packages"),
        Some(&op),
        Some(json!({"enabled": false, "reason": "Thử"})),
    )
    .await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);
    let off = admin_call(
        &h.app,
        "PUT",
        &path,
        Some(&op),
        Some(json!({"enabled": false, "reason": "Gói cơ bản"})),
    )
    .await;
    assert_eq!(off.status, StatusCode::OK, "{}", off.body);
    assert_eq!(off.body["enabled"], false);

    let me = call(&h.app, "GET", "/me", Some(&spa_owner), None).await;
    let has = |v: &Value, key: &str, value: &str| {
        v[key].as_array().is_some_and(|a| a.contains(&json!(value)))
    };
    assert!(!has(&me.body, "features", "appointments"), "{}", me.body);
    assert!(has(&me.body, "features", "payroll"));
    assert!(!has(&me.body, "permissions", "appointment.manage"));
    assert!(has(&me.body, "permissions", "customer.read"));
    let refused = call(
        &h.app,
        "PUT",
        "/appointment-settings",
        Some(&spa_owner),
        Some(json!({"max_per_slot": null})),
    )
    .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    assert_eq!(refused.body["code"], "feature.disabled");

    let on = admin_call(
        &h.app,
        "PUT",
        &path,
        Some(&op),
        Some(json!({"enabled": true, "reason": "Nâng gói"})),
    )
    .await;
    assert_eq!(on.status, StatusCode::OK);
    let me = call(&h.app, "GET", "/me", Some(&spa_owner), None).await;
    assert!(has(&me.body, "features", "appointments"));
    assert!(has(&me.body, "permissions", "appointment.manage"));

    let trail = count(
        &h.owner,
        &format!(
            "SELECT (SELECT count(*) FROM app.audit_log WHERE tenant_id = '{id}'
                       AND action = 'feature.changed' AND actor_kind = 'platform_admin')
                  + (SELECT count(*) FROM app.platform_audit_log WHERE action = 'platform.feature.set')"
        ),
    )
    .await?;
    assert_eq!(trail, 4);
    Ok(())
}

/// A spa can never write its own switches, not even through a bug: app_rw only reads them.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn app_rw_cannot_write_flags(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    let operator = add_operator(&h).await?;
    let id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM app.tenant WHERE slug = 'vpyd'")
        .fetch_one(&h.owner)
        .await?;
    let app_pool = kernel::testing::app_pool(&h.owner).await?;
    let mut tx = kernel::TenantTx::begin(&app_pool, kernel::TenantId::from_uuid(id)).await?;
    let insert = sqlx::query(
        "INSERT INTO app.tenant_feature (tenant_id, feature, enabled, updated_by) VALUES ($1, 'payroll', false, $2)",
    )
    .bind(id)
    .bind(operator)
    .execute(tx.conn())
    .await;
    assert!(insert.is_err(), "app_rw must not insert feature switches");
    drop(tx);
    // Without an operator session the function refuses too.
    let direct = sqlx::query("SELECT * FROM app.platform_set_feature($1, $2, 'payroll', false)")
        .bind(vec![0_u8; 32])
        .bind(id)
        .fetch_all(&app_pool)
        .await;
    let code = direct.err().and_then(|e| {
        e.as_database_error()
            .and_then(|d| d.code().map(|c| c.into_owned()))
    });
    assert_eq!(code.as_deref(), Some("42501"));
    Ok(())
}

// ------------------------------------------------------------------ part C: roles, accounts, spa audit

/// The templates a fresh database starts with are the code defaults (ADR-0018 D5): the matrix
/// and the seeded roles stay in agreement.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn role_templates_start_as_the_code_defaults(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    for role in identity::SystemRole::ALL {
        let mut stored: Vec<String> = sqlx::query_scalar(
            "SELECT permission FROM app.role_template_permission WHERE role_key = $1",
        )
        .bind(role.key())
        .fetch_all(&h.owner)
        .await?;
        stored.sort();
        let mut code: Vec<String> = role
            .default_permissions()
            .iter()
            .map(|p| p.as_str().to_owned())
            .collect();
        code.sort();
        assert_eq!(stored, code, "{}", role.key());
    }
    let follows: bool =
        sqlx::query_scalar("SELECT bool_and(follows_template) FROM app.role WHERE is_system")
            .fetch_one(&h.owner)
            .await?;
    assert!(follows, "a freshly created spa follows every template");
    Ok(())
}

/// Spec US7: a template edit reaches every spa that follows it on the next request; a spa with
/// its own grants keeps them until "Về mẫu"; the guardrails hold.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn role_edits_take_effect_on_the_next_request(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    add_operator(&h).await?;
    common::add_member(
        &h.owner,
        "vpyd",
        "0905000002",
        "Quản lý",
        identity::SystemRole::Manager,
    )
    .await?;
    let op = operator_session(&h).await?;
    let manager = session_for(&h, "0905000002").await;
    let new_customer =
        || json!({"full_name": "Khách Thử", "phone": "0912345678", "gender": "female"});
    let before = call(
        &h.app,
        "POST",
        "/customers",
        Some(&manager),
        Some(new_customer()),
    )
    .await;
    assert_eq!(before.status, StatusCode::CREATED, "{}", before.body);

    let mut grants: Vec<String> = identity::SystemRole::Manager
        .default_permissions()
        .iter()
        .map(|p| p.as_str().to_owned())
        .filter(|p| p != "customer.write")
        .collect();
    let edited = admin_call(
        &h.app,
        "PUT",
        "/admin/role-templates/manager",
        Some(&op),
        Some(json!({"permissions": grants, "reason": "Quản lý không sửa hồ sơ khách"})),
    )
    .await;
    assert_eq!(edited.status, StatusCode::OK, "{}", edited.body);
    assert_eq!(edited.body["updated_spas"], 1);
    let refused = call(
        &h.app,
        "POST",
        "/customers",
        Some(&manager),
        Some(json!({"full_name": "Khách Hai", "phone": "0912345679", "gender": "female"})),
    )
    .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.body);

    // This spa's own grants: customer.write back for its manager only.
    let id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM app.tenant WHERE slug = 'vpyd'")
        .fetch_one(&h.owner)
        .await?;
    grants.push("customer.write".to_owned());
    let own = admin_call(
        &h.app,
        "PUT",
        &format!("/admin/tenants/{id}/roles/manager"),
        Some(&op),
        Some(json!({"permissions": grants, "reason": "Spa này cho phép"})),
    )
    .await;
    assert_eq!(own.status, StatusCode::OK, "{}", own.body);
    assert_eq!(own.body["follows_template"], false);
    let allowed = call(
        &h.app,
        "POST",
        "/customers",
        Some(&manager),
        Some(json!({"full_name": "Khách Ba", "phone": "0912345670", "gender": "female"})),
    )
    .await;
    assert_eq!(allowed.status, StatusCode::CREATED, "{}", allowed.body);
    // A later template edit leaves the spa's own grants alone.
    let again = admin_call(
        &h.app,
        "PUT",
        "/admin/role-templates/manager",
        Some(&op),
        Some(json!({"permissions": grants[..grants.len() - 1].to_vec(), "reason": "Lần nữa"})),
    )
    .await;
    assert_eq!(again.body["updated_spas"], 0, "{}", again.body);
    let reset = admin_call(
        &h.app,
        "POST",
        &format!("/admin/tenants/{id}/roles/manager/reset"),
        Some(&op),
        Some(json!({"reason": "Về mẫu"})),
    )
    .await;
    assert_eq!(reset.body["follows_template"], true, "{}", reset.body);
    assert!(
        reset.body["permissions"]
            .as_array()
            .is_some_and(|p| !p.contains(&json!("customer.write")))
    );

    let roles = admin_call(
        &h.app,
        "GET",
        &format!("/admin/tenants/{id}/roles"),
        Some(&op),
        None,
    )
    .await;
    assert_eq!(roles.body.as_array().map(Vec::len), Some(3));
    let trail = count(
        &h.owner,
        &format!(
            "SELECT count(*) FROM app.audit_log WHERE tenant_id = '{id}' AND actor_kind = 'platform_admin'
               AND action IN ('role.permissions_changed', 'role.permissions_reset')"
        ),
    )
    .await?;
    assert_eq!(trail, 3, "template, own grants, reset");
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn the_owner_floor_is_kept(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    add_operator(&h).await?;
    let op = operator_session(&h).await?;
    let owner_without = |drop: &str| -> Vec<String> {
        identity::SystemRole::Owner
            .default_permissions()
            .iter()
            .map(|p| p.as_str().to_owned())
            .filter(|p| p != drop)
            .collect()
    };
    let floor = admin_call(
        &h.app,
        "PUT",
        "/admin/role-templates/owner",
        Some(&op),
        Some(json!({"permissions": owner_without("staff.manage"), "reason": "Thử"})),
    )
    .await;
    assert_eq!(floor.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(floor.body["code"], "role.owner_floor");
    assert_eq!(floor.body["details"]["missing"], json!(["staff.manage"]));
    let manager_admin = admin_call(
        &h.app,
        "PUT",
        "/admin/role-templates/manager",
        Some(&op),
        Some(json!({"permissions": ["catalog.read", "staff.manage"], "reason": "Thử"})),
    )
    .await;
    assert_eq!(manager_admin.body["code"], "role.owner_only_permission");
    let derived = admin_call(
        &h.app,
        "PUT",
        "/admin/role-templates/technician",
        Some(&op),
        Some(json!({"permissions": ["staff.read_own_past"], "reason": "Thử"})),
    )
    .await;
    assert_eq!(derived.body["code"], "role.not_grantable");
    let custom = admin_call(
        &h.app,
        "PUT",
        "/admin/role-templates/cashier",
        Some(&op),
        Some(json!({"permissions": ["catalog.read"], "reason": "Thử"})),
    )
    .await;
    assert_eq!(custom.status, StatusCode::NOT_FOUND);
    Ok(())
}

/// Spec US8: find a login, lock it (never the last owner of an active spa), unlock it, reset
/// its password or MFA, end its sessions — audited in every spa of the account.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn accounts_across_spas(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    add_operator(&h).await?;
    let op = operator_session(&h).await?;
    let spa_owner = session_for(&h, common::OWNER_PHONE).await;

    let by_phone = admin_call(&h.app, "GET", "/admin/accounts?q=0905123", Some(&op), None).await;
    assert_eq!(by_phone.status, StatusCode::OK, "{}", by_phone.body);
    let found = by_phone.body[0].clone();
    let mut keys: Vec<String> = found
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();
    keys.sort();
    assert_eq!(
        keys,
        [
            "created_at",
            "full_name",
            "id",
            "locked_until",
            "memberships",
            "mfa_enabled",
            "must_change_password",
            "phone",
            "status"
        ]
    );
    assert_eq!(found["memberships"][0]["is_owner"], true);
    assert_eq!(found["mfa_enabled"], true);
    let by_name = admin_call(
        &h.app,
        "GET",
        "/admin/accounts?q=chu%20spa",
        Some(&op),
        None,
    )
    .await;
    assert_eq!(
        by_name.body.as_array().map(Vec::len),
        Some(1),
        "{}",
        by_name.body
    );
    let short = admin_call(&h.app, "GET", "/admin/accounts?q=ab", Some(&op), None).await;
    assert_eq!(short.status, StatusCode::UNPROCESSABLE_ENTITY);
    // ADR-0019 D17: LIKE wildcards in a name match themselves, not every account.
    for wildcard in ["%25%25%25", "___", "a%5C%25b"] {
        let none = admin_call(
            &h.app,
            "GET",
            &format!("/admin/accounts?q={wildcard}"),
            Some(&op),
            None,
        )
        .await;
        assert_eq!(none.status, StatusCode::OK, "{}", none.body);
        assert_eq!(none.body, json!([]), "q={wildcard}");
    }
    // Every search across spas is in the platform trail, the query masked.
    let searches: Vec<Value> = sqlx::query_scalar(
        "SELECT detail FROM app.platform_audit_log
         WHERE action = 'platform.account.search' ORDER BY at, id",
    )
    .fetch_all(&h.owner)
    .await?;
    assert_eq!(searches.len(), 5, "{searches:?}");
    assert_eq!(
        searches[0],
        json!({"q": "090****", "by": "phone", "results": 1})
    );
    assert_eq!(
        searches[1],
        json!({"q": "c… s…", "by": "name", "results": 1})
    );
    assert!(
        !searches
            .iter()
            .any(|s| s.to_string().contains("905123") || s.to_string().contains("chu spa")),
        "{searches:?}"
    );
    let id = found["id"].as_str().unwrap_or_default().to_owned();
    let reason = || Some(json!({"reason": "Chủ spa gọi hỗ trợ"}));

    let last = admin_call(
        &h.app,
        "POST",
        &format!("/admin/accounts/{id}/lock"),
        Some(&op),
        reason(),
    )
    .await;
    assert_eq!(last.status, StatusCode::CONFLICT);
    assert_eq!(last.body["code"], "platform.last_owner");

    // The owner forgot their password: a temporary one, every session ended.
    let reset = admin_call(
        &h.app,
        "POST",
        &format!("/admin/accounts/{id}/reset-password"),
        Some(&op),
        reason(),
    )
    .await;
    assert_eq!(reset.status, StatusCode::OK, "{}", reset.body);
    assert_eq!(
        reset
            .headers
            .get("cache-control")
            .and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    let temporary = reset.body["temporary_password"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert_eq!(reset.body["account"]["must_change_password"], true);
    assert_eq!(
        call(&h.app, "GET", "/me", Some(&spa_owner), None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    let first = login(&h.app, common::OWNER_PHONE, &temporary).await;
    assert_eq!(first.body["next"], "mfa_verify", "{}", first.body);

    // MFA lost too: the next sign-in enrolls again.
    let mfa = admin_call(
        &h.app,
        "POST",
        &format!("/admin/accounts/{id}/reset-mfa"),
        Some(&op),
        reason(),
    )
    .await;
    assert_eq!(mfa.status, StatusCode::OK, "{}", mfa.body);
    let again = admin_call(
        &h.app,
        "POST",
        &format!("/admin/accounts/{id}/reset-mfa"),
        Some(&op),
        reason(),
    )
    .await;
    assert_eq!(again.body["code"], "account.mfa_not_enabled");
    assert_eq!(
        login(&h.app, common::OWNER_PHONE, &temporary).await.body["next"],
        "mfa_enroll"
    );

    // With a second owner, the first can be locked; locked = no sign-in, no reset.
    common::add_member(
        &h.owner,
        "vpyd",
        "0906000009",
        "Chủ thứ hai",
        identity::SystemRole::Owner,
    )
    .await?;
    let locked = admin_call(
        &h.app,
        "POST",
        &format!("/admin/accounts/{id}/lock"),
        Some(&op),
        reason(),
    )
    .await;
    assert_eq!(locked.status, StatusCode::OK, "{}", locked.body);
    assert_eq!(locked.body["account"]["status"], "disabled");
    assert_eq!(
        login(&h.app, common::OWNER_PHONE, &temporary).await.status,
        StatusCode::UNAUTHORIZED
    );
    let blocked = admin_call(
        &h.app,
        "POST",
        &format!("/admin/accounts/{id}/reset-password"),
        Some(&op),
        reason(),
    )
    .await;
    assert_eq!(blocked.body["code"], "account.locked");
    let unlocked = admin_call(
        &h.app,
        "POST",
        &format!("/admin/accounts/{id}/unlock"),
        Some(&op),
        reason(),
    )
    .await;
    assert_eq!(unlocked.body["account"]["status"], "active");
    let revoked = admin_call(
        &h.app,
        "POST",
        &format!("/admin/accounts/{id}/revoke-sessions"),
        Some(&op),
        reason(),
    )
    .await;
    assert_eq!(revoked.status, StatusCode::OK);
    let missing = admin_call(
        &h.app,
        "POST",
        &format!("/admin/accounts/{}/revoke-sessions", uuid::Uuid::now_v7()),
        Some(&op),
        reason(),
    )
    .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);

    // Both trails; never the temporary password.
    let spa_rows = count(
        &h.owner,
        &format!(
            "SELECT count(*) FROM app.audit_log WHERE actor_kind = 'platform_admin' AND entity_id = '{id}'"
        ),
    )
    .await?;
    assert_eq!(
        spa_rows, 5,
        "reset-password, reset-mfa, lock, unlock, revoke; refusals leave none"
    );
    let leaks = count(
        &h.owner,
        &format!(
            "SELECT (SELECT count(*) FROM app.audit_log WHERE concat(before, after, reason) LIKE '%{temporary}%')
                  + (SELECT count(*) FROM app.platform_audit_log WHERE concat(detail, reason) LIKE '%{temporary}%')"
        ),
    )
    .await?;
    assert_eq!(leaks, 0);
    Ok(())
}

/// Spec US9 AC9.2: the spa's trail for support is metadata only, and the read is audited.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn tenant_audit_is_metadata_only(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    add_operator(&h).await?;
    let op = operator_session(&h).await?;
    let spa_owner = session_for(&h, common::OWNER_PHONE).await;
    let created = call(
        &h.app,
        "POST",
        "/customers",
        Some(&spa_owner),
        Some(json!({"full_name": "Khách Bí Mật", "phone": "0912345678", "gender": "female"})),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    let id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM app.tenant WHERE slug = 'vpyd'")
        .fetch_one(&h.owner)
        .await?;
    let page = admin_call(
        &h.app,
        "GET",
        &format!("/admin/tenants/{id}/audit"),
        Some(&op),
        None,
    )
    .await;
    assert_eq!(page.status, StatusCode::OK, "{}", page.body);
    let items = page.body["items"].as_array().cloned().unwrap_or_default();
    assert!(!items.is_empty());
    for item in &items {
        let mut keys: Vec<String> = item
            .as_object()
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default();
        keys.sort();
        assert_eq!(
            keys,
            [
                "action",
                "actor_kind",
                "actor_name",
                "at",
                "entity_type",
                "id"
            ]
        );
    }
    assert!(
        !page.body.to_string().contains("Khách Bí Mật"),
        "no customer data"
    );
    assert!(!page.body.to_string().contains("0912345678"));
    let reads = count(
        &h.owner,
        "SELECT count(*) FROM app.platform_audit_log WHERE action = 'platform.tenant_audit.read'",
    )
    .await?;
    assert_eq!(reads, 1);
    Ok(())
}

/// The cross-spa functions return exactly these columns (ADR-0003, ADR-0018 D3: counts and
/// metadata only). A new column is a reviewed change of this list, never a silent widening.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn platform_functions_return_only_their_columns(owner: PgPool) -> TestResult {
    let found: Vec<(String, String)> = sqlx::query_as(
        "SELECT p.proname::text, pg_get_function_result(p.oid)
         FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace
         WHERE n.nspname = 'app' AND p.proname LIKE 'platform\\_%'
         ORDER BY p.proname",
    )
    .fetch_all(&owner)
    .await?;
    let expected = [
        ("platform_account_action", "bigint"),
        (
            "platform_account_memberships",
            "TABLE(user_id uuid, tenant_id uuid, tenant_name text, tenant_status text, \
             membership_status text, role_keys text[], is_owner boolean, other_active_owners bigint)",
        ),
        ("platform_apply_role_template", "integer"),
        ("platform_gate", "uuid"),
        ("platform_insert_tenant", "void"),
        ("platform_set_feature", "TABLE(previous boolean)"),
        (
            "platform_set_tenant_role",
            "TABLE(role_id uuid, before text[], after text[])",
        ),
        (
            "platform_set_tenant_status",
            "TABLE(previous text, revoked integer)",
        ),
        (
            "platform_tenant_overview",
            "TABLE(id uuid, name text, slug text, status text, created_at timestamp with time zone, \
             staff_count bigint, customer_count bigint, visits_30d bigint, owner_count bigint, \
             last_activity_at timestamp with time zone)",
        ),
    ];
    let expected: Vec<(String, String)> = expected
        .iter()
        .map(|(name, result)| ((*name).to_owned(), (*result).to_owned()))
        .collect();
    assert_eq!(found, expected);
    Ok(())
}

/// `(SQLSTATE, constraint)` of a refused statement; `None` = it ran.
fn refusal(result: Result<sqlx::postgres::PgQueryResult, sqlx::Error>) -> Option<(String, String)> {
    let error = result.err()?;
    let db = error.as_database_error()?;
    Some((
        db.code().map(|c| c.into_owned()).unwrap_or_default(),
        db.constraint().unwrap_or_default().to_owned(),
    ))
}

/// 0041 (lead review of PR C): account actions and a spa's own role grants are written by
/// functions that check the step-up themselves, so a code path that skipped the `Operator`
/// extractor is still refused; the owner floor of a lock is enforced by the database too.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn account_and_role_writes_check_the_step_up_in_the_database(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    add_operator(&h).await?;
    let op = operator_session(&h).await?;
    let hash = identity::token::hash_secret(&op).to_vec();
    let app_pool = kernel::testing::app_pool(&h.owner).await?;
    let spa_owner: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM app.user_account WHERE phone_e164 = '+84905123456'")
            .fetch_one(&h.owner)
            .await?;
    let tenant: uuid::Uuid = sqlx::query_scalar("SELECT id FROM app.tenant WHERE slug = 'vpyd'")
        .fetch_one(&h.owner)
        .await?;
    let account_action =
        |hash: Vec<u8>, action: &'static str, password_hash: Option<&'static str>| {
            let pool = app_pool.clone();
            async move {
                sqlx::query("SELECT app.platform_account_action($1, $2, $3, $4)")
                    .bind(hash)
                    .bind(spa_owner)
                    .bind(action)
                    .bind(password_hash)
                    .execute(&pool)
                    .await
            }
        };
    let tenant_role = |hash: Vec<u8>| {
        let pool = app_pool.clone();
        async move {
            sqlx::query("SELECT * FROM app.platform_set_tenant_role($1, $2, 'technician', NULL)")
                .bind(hash)
                .bind(tenant)
                .execute(&pool)
                .await
        }
    };
    let insufficient = Some(("42501".to_owned(), String::new()));

    // Stepped up: the database itself keeps the last owner of an active spa.
    assert_eq!(
        refusal(account_action(hash.clone(), "lock", None).await),
        Some(("23514".to_owned(), "platform_owner_floor".to_owned()))
    );
    assert_eq!(
        refusal(account_action(hash.clone(), "reset_password", Some("plain-text")).await)
            .map(|(code, _)| code)
            .as_deref(),
        Some("22023"),
        "a temporary password is stored only as an Argon2id hash"
    );
    assert_eq!(
        refusal(account_action(hash.clone(), "delete", None).await)
            .map(|(code, _)| code)
            .as_deref(),
        Some("22023")
    );
    assert_eq!(refusal(tenant_role(hash.clone()).await), None);

    // Step-up expired: every write is refused by the database, whatever Rust checked.
    expire_step_up(&h).await?;
    for action in ["revoke_sessions", "unlock", "reset_mfa"] {
        assert_eq!(
            refusal(account_action(hash.clone(), action, None).await),
            insufficient,
            "{action}"
        );
    }
    assert_eq!(refusal(tenant_role(hash.clone()).await), insufficient);
    // A spa session's hash is no operator session at all.
    let spa_hash =
        identity::token::hash_secret(&session_for(&h, common::OWNER_PHONE).await).to_vec();
    assert_eq!(
        refusal(account_action(spa_hash, "revoke_sessions", None).await),
        insufficient
    );
    // Over HTTP the extractor answers first.
    let id = spa_owner.to_string();
    let refused = admin_call(
        &h.app,
        "POST",
        &format!("/admin/accounts/{id}/revoke-sessions"),
        Some(&op),
        Some(json!({"reason": "Chủ spa gọi hỗ trợ"})),
    )
    .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    assert_eq!(refused.body["code"], "platform.step_up_required");
    let trail = count(
        &h.owner,
        &format!(
            "SELECT count(*) FROM app.audit_log WHERE actor_kind = 'platform_admin' AND entity_id = '{id}'"
        ),
    )
    .await?;
    assert_eq!(trail, 0, "refused writes leave no trail");
    Ok(())
}
