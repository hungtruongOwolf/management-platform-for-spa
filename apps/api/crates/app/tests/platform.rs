//! Platform admin console, part A (ADR-0018, spec platform-admin US1–US4, US9): the operator
//! principal, its MFA and step-up, the spa list, creating and suspending spas, the platform audit
//! trail, and the gated DB functions behind them.

mod common;

use axum::http::StatusCode;
use common::{
    OPERATOR, OPERATOR_PASSWORD, PASSWORD, TestResult, add_operator, admin_call, call, code_now,
    expire_step_up, harness, login, operator_session, post_customer, session_for, step_up,
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
            "owner_mfa_required",
            "slug",
            "staff_count",
            "status",
            "visits_30d"
        ]
    );
    assert_eq!(spas[0]["owner_count"], 1);
    assert_eq!(spas[0]["status"], "active");
    assert_eq!(
        spas[0]["owner_mfa_required"], true,
        "ON by default (ADR-0020)"
    );
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
            .is_some_and(|f| f.iter().all(|x| x["enabled"] == x["default_enabled"])),
        "every feature has its default: {}",
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
    let before = post_customer(&h.app, Some(&manager), Some(new_customer())).await;
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
    let refused = post_customer(
        &h.app,
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
    let allowed = post_customer(
        &h.app,
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
    let created = post_customer(
        &h.app,
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
            "platform_set_owner_mfa",
            "TABLE(previous boolean, revoked bigint)",
        ),
        (
            "platform_set_record_history",
            "TABLE(role_id uuid, before text[], after text[])",
        ),
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
             last_activity_at timestamp with time zone, owner_mfa_required boolean)",
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

// ------------------------------------------------------------------ owner MFA policy (ADR-0020)

async fn tenant_id(h: &common::Harness, slug: &str) -> Result<uuid::Uuid, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM app.tenant WHERE slug = $1")
        .bind(slug)
        .fetch_one(&h.owner)
        .await
}

const OWNER_MFA_REASON: &str = "Chủ spa yêu cầu qua điện thoại";

/// `PUT /admin/tenants/{id}/owner-mfa` with a valid reason.
async fn set_owner_mfa(
    h: &common::Harness,
    op: &str,
    tenant: uuid::Uuid,
    required: bool,
) -> common::Reply {
    admin_call(
        &h.app,
        "PUT",
        &format!("/admin/tenants/{tenant}/owner-mfa"),
        Some(op),
        Some(json!({"required": required, "reason": OWNER_MFA_REASON})),
    )
    .await
}

async fn me_status(h: &common::Harness, session: &str) -> StatusCode {
    call(&h.app, "GET", "/me", Some(session), None).await.status
}

/// OFF: the enrolled owner signs in with the password alone (no pending session) and re-auths
/// without a code; nothing is revoked and the authenticator is kept. ON again: the owner's sessions
/// end (not the manager's), and the next sign-in asks for a code of the SAME authenticator. Both
/// audit trails get one row per switch.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn owner_mfa_off_is_password_only_and_on_again_reuses_the_authenticator(
    owner: PgPool,
) -> TestResult {
    let h = harness(owner).await?;
    add_operator(&h).await?;
    let op = operator_session(&h).await?;
    let tenant = tenant_id(&h, "vpyd").await?;
    common::add_member(
        &h.owner,
        "vpyd",
        "0912000111",
        "Quản lý",
        identity::SystemRole::Manager,
    )
    .await?;
    let enrolled = session_for(&h, common::OWNER_PHONE).await;
    let secret = common::stored_secret(&h, common::OWNER_PHONE).await?;
    let manager = session_for(&h, "0912000111").await;

    let path = format!("/admin/tenants/{tenant}/owner-mfa");
    let no_reason = admin_call(
        &h.app,
        "PUT",
        &path,
        Some(&op),
        Some(json!({"required": false, "reason": " "})),
    )
    .await;
    assert_eq!(no_reason.status, StatusCode::UNPROCESSABLE_ENTITY);
    let unknown = set_owner_mfa(&h, &op, uuid::Uuid::now_v7(), false).await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);
    let unchanged = set_owner_mfa(&h, &op, tenant, true).await;
    assert_eq!(
        (unchanged.status, unchanged.body["code"].as_str()),
        (StatusCode::CONFLICT, Some("tenant.owner_mfa_unchanged"))
    );

    let off = set_owner_mfa(&h, &op, tenant, false).await;
    assert_eq!(off.status, StatusCode::OK, "{}", off.body);
    assert_eq!(off.body["tenant"]["owner_mfa_required"], false);
    assert_eq!(off.body["revoked_sessions"], 0, "OFF ends no session");
    assert_eq!(me_status(&h, &enrolled).await, StatusCode::OK);
    let list = admin_call(&h.app, "GET", "/admin/tenants", Some(&op), None).await;
    assert_eq!(list.body[0]["owner_mfa_required"], false);

    let plain = login(&h.app, common::OWNER_PHONE, PASSWORD).await;
    assert_eq!(plain.status, StatusCode::OK, "{}", plain.body);
    assert_eq!(plain.body["next"], "done", "no pending MFA session");
    assert_eq!(
        plain.body["me"]["active_tenant"]["name"],
        "Viên Phúc Y Đường"
    );
    let plain = plain.session().unwrap_or_default();
    assert_eq!(me_status(&h, &plain).await, StatusCode::OK);
    let reauth = call(
        &h.app,
        "POST",
        "/auth/reauth",
        Some(&plain),
        Some(json!({"password": PASSWORD})),
    )
    .await;
    assert_eq!(reauth.status, StatusCode::NO_CONTENT, "no code while OFF");
    assert_eq!(
        common::stored_secret(&h, common::OWNER_PHONE).await?,
        secret,
        "the authenticator is kept"
    );

    let on = set_owner_mfa(&h, &op, tenant, true).await;
    assert_eq!(on.status, StatusCode::OK, "{}", on.body);
    assert_eq!(on.body["tenant"]["owner_mfa_required"], true);
    assert_eq!(on.body["revoked_sessions"], 2, "both owner sessions");
    assert_eq!(me_status(&h, &enrolled).await, StatusCode::UNAUTHORIZED);
    assert_eq!(me_status(&h, &plain).await, StatusCode::UNAUTHORIZED);
    assert_eq!(
        me_status(&h, &manager).await,
        StatusCode::OK,
        "a manager never needed MFA"
    );

    let again = login(&h.app, common::OWNER_PHONE, PASSWORD).await;
    assert_eq!(again.body["next"], "mfa_verify", "no re-enrolment");
    common::forget_used_step(&h, common::OWNER_PHONE).await?;
    let verified =
        common::mfa_verify(&h, &again.session().unwrap_or_default(), &code_now(&secret)).await;
    assert_eq!(verified.status, StatusCode::OK, "{}", verified.body);

    let platform_rows: Vec<(Value, Option<String>)> = sqlx::query_as(
        "SELECT detail, reason FROM app.platform_audit_log
         WHERE action = 'platform.tenant.owner_mfa' AND tenant_id = $1 AND target_id = $1
         ORDER BY at, id",
    )
    .bind(tenant)
    .fetch_all(&h.owner)
    .await?;
    let reason = Some(OWNER_MFA_REASON.to_owned());
    assert_eq!(
        platform_rows,
        [
            (
                json!({"before": true, "after": false, "revoked_sessions": 0}),
                reason.clone()
            ),
            (
                json!({"before": false, "after": true, "revoked_sessions": 2}),
                reason.clone()
            ),
        ]
    );
    let spa_rows: Vec<(Option<Value>, Option<Value>, Option<String>)> = sqlx::query_as(
        "SELECT before, after, reason FROM app.audit_log
         WHERE tenant_id = $1 AND action = 'tenant.owner_mfa_changed'
           AND actor_kind = 'platform_admin' AND entity_type = 'tenant' AND entity_id = $1
         ORDER BY at, id",
    )
    .bind(tenant)
    .fetch_all(&h.owner)
    .await?;
    assert_eq!(
        spa_rows,
        [
            (
                Some(json!({"owner_mfa_required": true})),
                Some(json!({"owner_mfa_required": false})),
                reason.clone()
            ),
            (
                Some(json!({"owner_mfa_required": false})),
                Some(json!({"owner_mfa_required": true})),
                reason
            ),
        ]
    );
    Ok(())
}

/// An owner who never enrolled works password-only while OFF; ON ends that session and the next
/// sign-in enrolls an authenticator.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn an_owner_who_never_enrolled_enrolls_once_the_policy_is_on(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    add_operator(&h).await?;
    let op = operator_session(&h).await?;
    let tenant = tenant_id(&h, "vpyd").await?;
    assert_eq!(
        set_owner_mfa(&h, &op, tenant, false).await.status,
        StatusCode::OK
    );
    let first = login(&h.app, common::OWNER_PHONE, PASSWORD).await;
    assert_eq!(first.body["next"], "done");
    let first = first.session().unwrap_or_default();
    assert_eq!(me_status(&h, &first).await, StatusCode::OK);

    let on = set_owner_mfa(&h, &op, tenant, true).await;
    assert_eq!(on.body["revoked_sessions"], 1);
    assert_eq!(me_status(&h, &first).await, StatusCode::UNAUTHORIZED);
    let again = login(&h.app, common::OWNER_PHONE, PASSWORD).await;
    assert_eq!(again.body["next"], "mfa_enroll");
    let enrolled = common::enroll(&h, &again.session().unwrap_or_default()).await;
    assert_eq!(enrolled.status, StatusCode::OK, "{}", enrolled.body);
    assert_eq!(
        enrolled.body["recovery_codes"].as_array().map(Vec::len),
        Some(10)
    );
    Ok(())
}

/// Strictest wins: an owner of two spas needs MFA while either spa's policy is ON, and switching
/// a spa ON ends no session of someone who already needed MFA. A suspended spa's policy still
/// counts (ADR-0004 M3).
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn the_strictest_spa_policy_wins_and_a_suspended_spa_still_counts(
    owner: PgPool,
) -> TestResult {
    let h = harness(owner).await?;
    add_operator(&h).await?;
    let op = operator_session(&h).await?;
    identity::provisioning::create_tenant(&h.owner, "hoa-sen", "Spa Hoa Sen", "Cơ sở chính")
        .await?;
    common::add_member(
        &h.owner,
        "hoa-sen",
        common::OWNER_PHONE,
        "Chủ Spa",
        identity::SystemRole::Owner,
    )
    .await?;
    let (vpyd, hoa_sen) = (
        tenant_id(&h, "vpyd").await?,
        tenant_id(&h, "hoa-sen").await?,
    );
    let session = session_for(&h, common::OWNER_PHONE).await;

    assert_eq!(
        set_owner_mfa(&h, &op, vpyd, false).await.status,
        StatusCode::OK
    );
    let still = login(&h.app, common::OWNER_PHONE, PASSWORD).await;
    assert_eq!(still.body["next"], "mfa_verify", "Hoa Sen is still ON");
    let on = set_owner_mfa(&h, &op, vpyd, true).await;
    assert_eq!(on.status, StatusCode::OK, "{}", on.body);
    assert_eq!(
        on.body["revoked_sessions"], 0,
        "the owner already passed MFA for Hoa Sen"
    );
    assert_eq!(me_status(&h, &session).await, StatusCode::OK);

    // Hoa Sen suspended (policy ON), Viên Phúc active (policy OFF): MFA still applies.
    assert_eq!(
        set_owner_mfa(&h, &op, vpyd, false).await.status,
        StatusCode::OK
    );
    let suspended = admin_call(
        &h.app,
        "POST",
        &format!("/admin/tenants/{hoa_sen}/suspend"),
        Some(&op),
        Some(json!({"reason": "Chưa gia hạn hợp đồng"})),
    )
    .await;
    assert_eq!(suspended.status, StatusCode::OK, "{}", suspended.body);
    let suspended_counts = login(&h.app, common::OWNER_PHONE, PASSWORD).await;
    assert_eq!(suspended_counts.status, StatusCode::OK);
    assert_eq!(suspended_counts.body["next"], "mfa_verify");

    // Both spas OFF: password only.
    assert_eq!(
        set_owner_mfa(&h, &op, hoa_sen, false).await.status,
        StatusCode::OK,
        "a suspended spa's policy can still be switched"
    );
    let plain = login(&h.app, common::OWNER_PHONE, PASSWORD).await;
    assert_eq!(plain.body["next"], "done");
    Ok(())
}

/// The switch is a console write like any other: the extractor wants a recent step-up, and the
/// DB function refuses a stale step-up (or a spa session) on its own; app_rw cannot write the
/// column directly; refused writes leave no trail.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn the_owner_mfa_switch_checks_the_step_up_in_the_database(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    add_operator(&h).await?;
    let op = operator_session(&h).await?;
    let hash = identity::token::hash_secret(&op).to_vec();
    let tenant = tenant_id(&h, "vpyd").await?;
    let app_pool = kernel::testing::app_pool(&h.owner).await?;
    let direct = |hash: Vec<u8>| {
        let pool = app_pool.clone();
        async move {
            sqlx::query("SELECT * FROM app.platform_set_owner_mfa($1, $2, false)")
                .bind(hash)
                .bind(tenant)
                .execute(&pool)
                .await
        }
    };
    let insufficient = Some(("42501".to_owned(), String::new()));
    let spa_hash =
        identity::token::hash_secret(&session_for(&h, common::OWNER_PHONE).await).to_vec();
    assert_eq!(refusal(direct(spa_hash).await), insufficient);

    expire_step_up(&h).await?;
    assert_eq!(refusal(direct(hash).await), insufficient);
    let refused = set_owner_mfa(&h, &op, tenant, false).await;
    assert_eq!(
        (refused.status, refused.body["code"].as_str()),
        (StatusCode::FORBIDDEN, Some("platform.step_up_required"))
    );
    let column = sqlx::query("UPDATE app.tenant SET owner_mfa_required = false")
        .execute(&app_pool)
        .await;
    assert!(column.is_err(), "app_rw may not write the policy");

    let still_on: bool =
        sqlx::query_scalar("SELECT owner_mfa_required FROM app.tenant WHERE id = $1")
            .bind(tenant)
            .fetch_one(&h.owner)
            .await?;
    assert!(still_on);
    let trail = count(
        &h.owner,
        "SELECT (SELECT count(*) FROM app.platform_audit_log WHERE action = 'platform.tenant.owner_mfa')
              + (SELECT count(*) FROM app.audit_log WHERE action = 'tenant.owner_mfa_changed')",
    )
    .await?;
    assert_eq!(trail, 0, "refused writes leave no trail");
    Ok(())
}

// ------------------------------------------------------------------ record history (ADR-0022)

const MANAGER_PHONE: &str = "0905000002";
const SWITCH_REASON: &str = "Chủ spa nhờ hiện cho quản lý";

/// `PUT /admin/tenants/{id}/record-history/{role}` with a valid reason.
async fn set_history(
    h: &common::Harness,
    op: &str,
    tenant: uuid::Uuid,
    role: &str,
    visible: bool,
) -> common::Reply {
    admin_call(
        &h.app,
        "PUT",
        &format!("/admin/tenants/{tenant}/record-history/{role}"),
        Some(op),
        Some(json!({"visible": visible, "reason": SWITCH_REASON})),
    )
    .await
}

/// The spa's role, as the console reads it.
async fn spa_role(h: &common::Harness, op: &str, tenant: uuid::Uuid, key: &str) -> Value {
    let roles = admin_call(
        &h.app,
        "GET",
        &format!("/admin/tenants/{tenant}/roles"),
        Some(op),
        None,
    )
    .await;
    roles
        .body
        .as_array()
        .and_then(|r| r.iter().find(|x| x["key"] == key).cloned())
        .unwrap_or_default()
}

fn grants(role: &Value, permission: &str) -> bool {
    role["permissions"]
        .as_array()
        .is_some_and(|p| p.contains(&json!(permission)))
}

fn me_has(me: &Value, permission: &str) -> bool {
    me["permissions"]
        .as_array()
        .is_some_and(|p| p.contains(&json!(permission)))
}

/// AC-H5: the operator shows / hides the history per role of one spa. Shown to the owner by
/// default, hidden from the manager; never a technician's; effective on the next request; audited
/// on both sides; the role keeps following its template.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn the_record_history_switch_shows_and_hides_per_role(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    add_operator(&h).await?;
    common::add_member(
        &h.owner,
        "vpyd",
        MANAGER_PHONE,
        "Quản lý",
        identity::SystemRole::Manager,
    )
    .await?;
    let op = operator_session(&h).await?;
    let tenant = tenant_id(&h, "vpyd").await?;
    let spa_owner = session_for(&h, common::OWNER_PHONE).await;
    let manager = session_for(&h, MANAGER_PHONE).await;

    // Defaults: the owner sees it, the manager does not.
    assert!(grants(
        &spa_role(&h, &op, tenant, "owner").await,
        "record.history.read"
    ));
    assert!(!grants(
        &spa_role(&h, &op, tenant, "manager").await,
        "record.history.read"
    ));
    let me = call(&h.app, "GET", "/me", Some(&manager), None).await;
    assert!(!me_has(&me.body, "record.history.read"));

    let shown = set_history(&h, &op, tenant, "manager", true).await;
    assert_eq!(shown.status, StatusCode::OK, "{}", shown.body);
    assert_eq!(shown.body["key"], "manager");
    assert_eq!(shown.body["follows_template"], true, "{}", shown.body);
    assert!(grants(&shown.body, "record.history.read"));
    // The manager's next request has it; nothing else of the role changed.
    let me = call(&h.app, "GET", "/me", Some(&manager), None).await;
    assert!(me_has(&me.body, "record.history.read"), "{}", me.body);
    assert!(!me_has(&me.body, "audit.read"));
    let after = spa_role(&h, &op, tenant, "manager").await;
    let mut expected: Vec<String> = identity::SystemRole::Manager
        .default_permissions()
        .iter()
        .map(|p| p.as_str().to_owned())
        .chain(["record.history.read".to_owned()])
        .collect();
    expected.sort();
    let mut got: Vec<String> = after["permissions"]
        .as_array()
        .map(|p| {
            p.iter()
                .filter_map(|x| x.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    got.sort();
    assert_eq!(got, expected);

    // Already shown: 409, nothing written.
    let again = set_history(&h, &op, tenant, "manager", true).await;
    assert_eq!(
        (again.status, again.body["code"].as_str()),
        (StatusCode::CONFLICT, Some("role.record_history_unchanged"))
    );
    // Never a technician; an unknown role or spa is not found.
    let tech = set_history(&h, &op, tenant, "technician", true).await;
    assert_eq!(
        (tech.status, tech.body["code"].as_str()),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("role.not_for_technician")
        )
    );
    assert_eq!(
        set_history(&h, &op, tenant, "cashier", true).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        set_history(&h, &op, uuid::Uuid::now_v7(), "manager", true)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    // A reason is required.
    let unreasoned = admin_call(
        &h.app,
        "PUT",
        &format!("/admin/tenants/{tenant}/record-history/owner"),
        Some(&op),
        Some(json!({"visible": false, "reason": ""})),
    )
    .await;
    assert_eq!(unreasoned.status, StatusCode::UNPROCESSABLE_ENTITY);
    // A spa session never reaches the console.
    let spa_call = call(
        &h.app,
        "PUT",
        &format!("/admin/tenants/{tenant}/record-history/owner"),
        Some(&spa_owner),
        Some(json!({"visible": false, "reason": SWITCH_REASON})),
    )
    .await;
    assert_eq!(spa_call.status, StatusCode::UNAUTHORIZED);

    // Hidden from the owner (not part of the owner floor): their next request is refused.
    let hidden = set_history(&h, &op, tenant, "owner", false).await;
    assert_eq!(hidden.status, StatusCode::OK, "{}", hidden.body);
    let me = call(&h.app, "GET", "/me", Some(&spa_owner), None).await;
    assert!(!me_has(&me.body, "record.history.read"));
    assert!(me_has(&me.body, "audit.read"));
    let customer = post_customer(
        &h.app,
        Some(&spa_owner),
        Some(json!({"full_name": "Khách Thử", "phone": "0912345678", "gender": "female"})),
    )
    .await;
    let customer = customer.body["id"].as_str().unwrap_or_default().to_owned();
    let refused = call(
        &h.app,
        "GET",
        &format!("/record-history/customer/{customer}"),
        Some(&spa_owner),
        None,
    )
    .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.body);
    let shown_to_manager = call(
        &h.app,
        "GET",
        &format!("/record-history/customer/{customer}"),
        Some(&manager),
        None,
    )
    .await;
    assert_eq!(
        shown_to_manager.status,
        StatusCode::OK,
        "{}",
        shown_to_manager.body
    );

    // Both trails: one spa row (with the role's permissions before / after) and one platform row
    // per switch that changed something.
    let spa_rows: Vec<(Value, Value)> = sqlx::query_as(
        "SELECT before, after FROM app.audit_log
         WHERE tenant_id = $1 AND action = 'role.permissions_changed' AND actor_kind = 'platform_admin'
         ORDER BY at, id",
    )
    .bind(tenant)
    .fetch_all(&h.owner)
    .await?;
    assert_eq!(spa_rows.len(), 2);
    let lists = |v: &Value| v["permissions"].as_array().cloned().unwrap_or_default();
    assert!(!lists(&spa_rows[0].0).contains(&json!("record.history.read")));
    assert!(lists(&spa_rows[0].1).contains(&json!("record.history.read")));
    let platform_rows: Vec<Value> = sqlx::query_scalar(
        "SELECT detail FROM app.platform_audit_log WHERE action = 'platform.record_history.set'
         ORDER BY at, id",
    )
    .fetch_all(&h.owner)
    .await?;
    assert_eq!(
        platform_rows,
        [
            json!({"role": "manager", "visible": true}),
            json!({"role": "owner", "visible": false})
        ]
    );
    Ok(())
}

/// D1 (spa-local): a template edit, a spa's own grants and "Về mẫu" all keep each spa's switch;
/// the template's value is only a new spa's initial one; `follows_template` is untouched by the
/// switch. The audit row of a template apply records what the spa got.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn the_record_history_switch_is_the_spas_own(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    add_operator(&h).await?;
    let op = operator_session(&h).await?;
    let tenant = tenant_id(&h, "vpyd").await?;
    let shown = set_history(&h, &op, tenant, "manager", true).await;
    assert_eq!(shown.status, StatusCode::OK, "{}", shown.body);

    // A template edit (customer.write out) reaches the spa that follows it, which keeps its switch.
    let template: Vec<String> = identity::SystemRole::Manager
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
        Some(json!({"permissions": template, "reason": "Quản lý không sửa hồ sơ khách"})),
    )
    .await;
    assert_eq!(edited.status, StatusCode::OK, "{}", edited.body);
    assert_eq!(edited.body["updated_spas"], 1);
    let manager = spa_role(&h, &op, tenant, "manager").await;
    assert!(!grants(&manager, "customer.write"));
    assert!(grants(&manager, "record.history.read"), "{manager}");
    assert_eq!(manager["follows_template"], true);
    let applied: Value = sqlx::query_scalar(
        "SELECT after FROM app.audit_log
         WHERE tenant_id = $1 AND action = 'role.permissions_changed' AND before ->> 'source' = 'template'",
    )
    .bind(tenant)
    .fetch_one(&h.owner)
    .await?;
    let mut stored: Vec<String> = serde_json::from_value(applied["permissions"].clone())?;
    stored.sort();
    let mut now: Vec<String> = serde_json::from_value(manager["permissions"].clone())?;
    now.sort();
    assert_eq!(stored, now, "the apply's audit row is the spa's new set");
    // A template that lists it for the manager does not show it to spas that hid it, and the
    // reverse: the template is a new spa's initial value only.
    let hidden = set_history(&h, &op, tenant, "manager", false).await;
    assert_eq!(hidden.status, StatusCode::OK, "{}", hidden.body);
    let with_it: Vec<String> = template
        .iter()
        .cloned()
        .chain(["record.history.read".to_owned()])
        .collect();
    let edited = admin_call(
        &h.app,
        "PUT",
        "/admin/role-templates/manager",
        Some(&op),
        Some(json!({"permissions": with_it, "reason": "Spa mới thấy lịch sử"})),
    )
    .await;
    assert_eq!(edited.status, StatusCode::OK, "{}", edited.body);
    assert_eq!(
        edited.body["updated_spas"], 0,
        "nothing else differs: no write"
    );
    assert!(!grants(
        &spa_role(&h, &op, tenant, "manager").await,
        "record.history.read"
    ));
    // A spa created now starts from the template.
    let created = admin_call(
        &h.app,
        "POST",
        "/admin/tenants",
        Some(&op),
        Some(new_spa("hoa-sen", "0912000999")),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    let new_tenant = tenant_id(&h, "hoa-sen").await?;
    assert!(grants(
        &spa_role(&h, &op, new_tenant, "manager").await,
        "record.history.read"
    ));

    // The spa's own grants ignore it either way: sent with it, it stays hidden here...
    let own = admin_call(
        &h.app,
        "PUT",
        &format!("/admin/tenants/{tenant}/roles/manager"),
        Some(&op),
        Some(json!({"permissions": with_it, "reason": "Spa này riêng"})),
    )
    .await;
    assert_eq!(own.status, StatusCode::OK, "{}", own.body);
    assert_eq!(own.body["follows_template"], false);
    assert!(!grants(&own.body, "record.history.read"), "{}", own.body);
    // ...and once shown, a list without it keeps it; "Về mẫu" keeps it too.
    assert_eq!(
        set_history(&h, &op, tenant, "manager", true).await.status,
        StatusCode::OK
    );
    let own = admin_call(
        &h.app,
        "PUT",
        &format!("/admin/tenants/{tenant}/roles/manager"),
        Some(&op),
        Some(json!({"permissions": template, "reason": "Spa này riêng"})),
    )
    .await;
    assert!(grants(&own.body, "record.history.read"), "{}", own.body);
    let reset = admin_call(
        &h.app,
        "POST",
        &format!("/admin/tenants/{tenant}/roles/manager/reset"),
        Some(&op),
        Some(json!({"reason": "Về mẫu"})),
    )
    .await;
    assert_eq!(reset.body["follows_template"], true, "{}", reset.body);
    assert!(grants(&reset.body, "record.history.read"), "{}", reset.body);
    // The switch never touches follows_template (the role follows the template here).
    let hidden = set_history(&h, &op, tenant, "manager", false).await;
    assert_eq!(hidden.body["follows_template"], true, "{}", hidden.body);

    // Never a technician's, in a template or a spa.
    let mut tech: Vec<String> = identity::SystemRole::Technician
        .default_permissions()
        .iter()
        .map(|p| p.as_str().to_owned())
        .collect();
    tech.push("record.history.read".to_owned());
    for path in [
        "/admin/role-templates/technician".to_owned(),
        format!("/admin/tenants/{tenant}/roles/technician"),
    ] {
        let refused = admin_call(
            &h.app,
            "PUT",
            &path,
            Some(&op),
            Some(json!({"permissions": tech, "reason": "Thử"})),
        )
        .await;
        assert_eq!(
            (refused.status, refused.body["code"].as_str()),
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Some("role.not_for_technician")
            ),
            "{path}"
        );
    }
    // The catalogue marks it for the role editor.
    let catalogue = admin_call(&h.app, "GET", "/admin/permissions", Some(&op), None).await;
    let info = catalogue
        .body
        .as_array()
        .and_then(|c| {
            c.iter()
                .find(|p| p["key"] == "record.history.read")
                .cloned()
        })
        .unwrap_or_default();
    assert_eq!(
        (&info["spa_local"], &info["not_for_technician"]),
        (&json!(true), &json!(true))
    );
    Ok(())
}

/// The spa-local list lives in Rust and in SQL (0052): the two stay equal.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn spa_local_permissions_agree_between_rust_and_sql(owner: PgPool) -> TestResult {
    let sql: Vec<String> = sqlx::query_scalar("SELECT unnest(app.spa_local_permissions())")
        .fetch_all(&owner)
        .await?;
    let rust: Vec<String> = platform::roles::SPA_LOCAL
        .iter()
        .map(|p| p.as_str().to_owned())
        .collect();
    assert_eq!(sql, rust);
    Ok(())
}

/// The switch's function checks the step-up and its roles itself (0052): a code path that skipped
/// the Rust checks is still refused.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn the_record_history_switch_checks_in_the_database(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    add_operator(&h).await?;
    let op = operator_session(&h).await?;
    let hash = identity::token::hash_secret(&op).to_vec();
    let tenant = tenant_id(&h, "vpyd").await?;
    let app_pool = kernel::testing::app_pool(&h.owner).await?;
    let direct = |hash: Vec<u8>, role: &'static str| {
        let pool = app_pool.clone();
        async move {
            sqlx::query("SELECT * FROM app.platform_set_record_history($1, $2, $3, true)")
                .bind(hash)
                .bind(tenant)
                .bind(role)
                .execute(&pool)
                .await
        }
    };
    assert_eq!(
        refusal(direct(hash.clone(), "technician").await).map(|(code, _)| code),
        Some("22023".to_owned())
    );
    assert_eq!(
        refusal(direct(hash.clone(), "cashier").await).map(|(code, _)| code),
        Some("22023".to_owned())
    );
    expire_step_up(&h).await?;
    let insufficient = Some(("42501".to_owned(), String::new()));
    assert_eq!(refusal(direct(hash, "manager").await), insufficient);
    let spa_hash =
        identity::token::hash_secret(&session_for(&h, common::OWNER_PHONE).await).to_vec();
    assert_eq!(refusal(direct(spa_hash, "manager").await), insufficient);
    let refused = set_history(&h, &op, tenant, "manager", true).await;
    assert_eq!(
        (refused.status, refused.body["code"].as_str()),
        (StatusCode::FORBIDDEN, Some("platform.step_up_required"))
    );
    let granted: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app.role_permission rp JOIN app.role r ON r.tenant_id = rp.tenant_id AND r.id = rp.role_id
         WHERE r.tenant_id = $1 AND r.key = 'manager' AND rp.permission = 'record.history.read'",
    )
    .bind(tenant)
    .fetch_one(&h.owner)
    .await?;
    assert_eq!(granted, 0);
    let trail = count(
        &h.owner,
        "SELECT count(*) FROM app.platform_audit_log WHERE action = 'platform.record_history.set'",
    )
    .await?;
    assert_eq!(trail, 0, "refused writes leave no trail");
    Ok(())
}

/// AC-H6: "Nhật ký thao tác" is off for every spa without a row; the operator switches it on for
/// one spa (its owner's next request sees it, the manager still never), and the console shows the
/// default.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn the_activity_screen_is_off_until_an_operator_switches_it_on(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    add_operator(&h).await?;
    common::add_member(
        &h.owner,
        "vpyd",
        MANAGER_PHONE,
        "Quản lý",
        identity::SystemRole::Manager,
    )
    .await?;
    let op = operator_session(&h).await?;
    let tenant = tenant_id(&h, "vpyd").await?;
    let spa_owner = session_for(&h, common::OWNER_PHONE).await;
    let manager = session_for(&h, MANAGER_PHONE).await;

    let refused = call(&h.app, "GET", "/audit-log", Some(&spa_owner), None).await;
    assert_eq!(
        (refused.status, refused.body["code"].as_str()),
        (StatusCode::FORBIDDEN, Some("feature.disabled"))
    );
    let me = call(&h.app, "GET", "/me", Some(&spa_owner), None).await;
    assert!(
        me.body["features"]
            .as_array()
            .is_some_and(|f| !f.contains(&json!("activity_log")) && f.contains(&json!("payroll"))),
        "{}",
        me.body
    );
    // `audit.read` itself stays the owner's (the switch owns the route only).
    assert!(me_has(&me.body, "audit.read"));

    let registry = admin_call(&h.app, "GET", "/admin/features", Some(&op), None).await;
    let defaults: Vec<(String, bool)> = registry
        .body
        .as_array()
        .map(|f| {
            f.iter()
                .map(|x| {
                    (
                        x["key"].as_str().unwrap_or_default().to_owned(),
                        x["default_enabled"] == true,
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    assert!(
        defaults.contains(&("activity_log".to_owned(), false)),
        "{defaults:?}"
    );
    assert!(defaults.contains(&("payroll".to_owned(), true)));
    let features = admin_call(
        &h.app,
        "GET",
        &format!("/admin/tenants/{tenant}/features"),
        Some(&op),
        None,
    )
    .await;
    for f in features.body.as_array().cloned().unwrap_or_default() {
        assert_eq!(f["enabled"], f["default_enabled"], "{f}");
        assert!(f["updated_at"].is_null());
    }

    let on = admin_call(
        &h.app,
        "PUT",
        &format!("/admin/tenants/{tenant}/features/activity_log"),
        Some(&op),
        Some(json!({"enabled": true, "reason": "Chủ spa cần xem nhật ký"})),
    )
    .await;
    assert_eq!(on.status, StatusCode::OK, "{}", on.body);
    assert_eq!(
        (&on.body["enabled"], &on.body["default_enabled"]),
        (&json!(true), &json!(false))
    );
    let page = call(&h.app, "GET", "/audit-log", Some(&spa_owner), None).await;
    assert_eq!(page.status, StatusCode::OK, "{}", page.body);
    let me = call(&h.app, "GET", "/me", Some(&spa_owner), None).await;
    assert!(
        me.body["features"]
            .as_array()
            .is_some_and(|f| f.contains(&json!("activity_log")))
    );
    let manager_page = call(&h.app, "GET", "/audit-log", Some(&manager), None).await;
    assert_eq!(
        (manager_page.status, manager_page.body["code"].as_str()),
        (StatusCode::FORBIDDEN, Some("auth.forbidden"))
    );
    // The audit's "before" is the default that was in force.
    let before: Value = sqlx::query_scalar(
        "SELECT before FROM app.audit_log WHERE tenant_id = $1 AND action = 'feature.changed'",
    )
    .bind(tenant)
    .fetch_one(&h.owner)
    .await?;
    assert_eq!(before, json!({"feature": "activity_log", "enabled": false}));
    // Off again: refused again.
    let off = admin_call(
        &h.app,
        "PUT",
        &format!("/admin/tenants/{tenant}/features/activity_log"),
        Some(&op),
        Some(json!({"enabled": false, "reason": "Tạm ẩn lại"})),
    )
    .await;
    assert_eq!(off.status, StatusCode::OK, "{}", off.body);
    let refused = call(&h.app, "GET", "/audit-log", Some(&spa_owner), None).await;
    assert_eq!(refused.body["code"], "feature.disabled");
    Ok(())
}
