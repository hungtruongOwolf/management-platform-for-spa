//! `app.session_actor` (0034, db review A3, ADR-0017): one SECURITY DEFINER statement resolves a
//! request's caller. These tests pin what the definer may and may not do; the refusals it feeds
//! (expired / revoked session, disabled account or membership, suspended spa, temporary password,
//! former staff, two spas) are the flows of `auth.rs`, `staff_logins.rs` and `authz_matrix.rs`,
//! which run through it on every request.

mod common;

use axum::http::StatusCode;
use common::{PASSWORD, Reply, TestResult, add_member, call, harness, login};
use identity::SystemRole;
use identity::provisioning::{NewUser, add_user, create_tenant};
use sqlx::PgPool;
use uuid::Uuid;

const MANAGER: &str = "0912000555";

fn session_of(reply: &Reply) -> String {
    reply.session().unwrap_or_default()
}

fn statements(reply: &Reply) -> Option<u32> {
    let timing = reply.headers.get("server-timing")?.to_str().ok()?;
    let start = timing.find("desc=\"")? + 6;
    let rest = &timing[start..];
    rest[..rest.find(' ')?].parse().ok()
}

/// The token hash of a cookie secret, as the API stores it (SHA-256).
async fn token_hash(owner: &PgPool, secret: &str) -> Result<Vec<u8>, sqlx::Error> {
    sqlx::query_scalar("SELECT sha256(convert_to($1, 'UTF8'))")
        .bind(secret)
        .fetch_one(owner)
        .await
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_app_rw_may_call_it_and_its_search_path_is_pinned(owner: PgPool) -> TestResult {
    let (definer, config): (bool, Vec<String>) = sqlx::query_as(
        "SELECT prosecdef, coalesce(proconfig, '{}') FROM pg_proc
         WHERE oid = 'app.session_actor(bytea)'::regprocedure",
    )
    .fetch_one(&owner)
    .await?;
    assert!(definer, "SECURITY DEFINER");
    assert_eq!(config, ["search_path=app, pg_temp"]);
    let (public, app_rw): (bool, bool) = sqlx::query_as(
        "SELECT has_function_privilege('public', 'app.session_actor(bytea)', 'EXECUTE'),
                has_function_privilege('app_rw', 'app.session_actor(bytea)', 'EXECUTE')",
    )
    .fetch_one(&owner)
    .await?;
    assert!(!public, "no EXECUTE for PUBLIC");
    assert!(app_rw, "the runtime role calls it");
    Ok(())
}

/// The definer reads past RLS, so it must only ever answer about the session it is handed: an
/// unknown hash yields nothing, and the permissions are those of the session's own active spa —
/// never another spa the same person belongs to, never another person's.
#[sqlx::test(migrations = "../../migrations")]
async fn it_answers_only_for_the_session_and_its_active_spa(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    add_member(&h.owner, "vpyd", MANAGER, "Quản lý", SystemRole::Manager).await?;
    // The same person is only a technician in a second spa.
    create_tenant(&h.owner, "spa-b", "Spa B", "Cơ sở 1").await?;
    add_user(
        &h.owner,
        NewUser {
            tenant_slug: "spa-b",
            phone: MANAGER,
            full_name: "Quản lý",
            password: "ignored-existing-account",
            role: SystemRole::Technician,
        },
    )
    .await?;
    let app_rw = kernel::testing::app_pool(&h.owner).await?;

    let unknown: Option<Uuid> =
        sqlx::query_scalar("SELECT session_id FROM app.session_actor(sha256('nope'::bytea))")
            .fetch_optional(&app_rw)
            .await?;
    assert_eq!(unknown, None, "no session for an unknown hash");

    // Two spas: the login has to pick one first (auth.rs); pick vpyd.
    let signed_in = login(&h.app, MANAGER, PASSWORD).await;
    assert_eq!(signed_in.status, StatusCode::OK, "{}", signed_in.body);
    let session = session_of(&signed_in);
    let vpyd: Uuid = sqlx::query_scalar("SELECT id FROM app.tenant WHERE slug = 'vpyd'")
        .fetch_one(&h.owner)
        .await?;
    let switched = call(
        &h.app,
        "POST",
        "/auth/switch-tenant",
        Some(&session),
        Some(serde_json::json!({"tenant_id": vpyd})),
    )
    .await;
    let session = switched.session().unwrap_or(session);
    let hash = token_hash(&h.owner, &session).await?;
    let (tenant, member, permissions): (Option<Uuid>, bool, Vec<String>) =
        sqlx::query_as("SELECT active_tenant_id, member, permissions FROM app.session_actor($1)")
            .bind(&hash)
            .fetch_one(&app_rw)
            .await?;
    assert_eq!(tenant, Some(vpyd));
    assert!(member);
    let manager_role: Vec<String> = sqlx::query_scalar(
        "SELECT rp.permission FROM app.role_permission rp JOIN app.role r ON r.id = rp.role_id
         JOIN app.tenant t ON t.id = r.tenant_id
         WHERE t.slug = 'vpyd' AND r.key = 'manager' ORDER BY rp.permission",
    )
    .fetch_all(&h.owner)
    .await?;
    assert_eq!(
        permissions, manager_role,
        "exactly the manager role of the active spa, not the technician role of spa-b"
    );

    // Revoked: nothing comes back, whoever asks.
    let out = call(&h.app, "POST", "/auth/logout", Some(&session), None).await;
    assert!(out.status.is_success(), "{}", out.body);
    let after: Option<Uuid> = sqlx::query_scalar("SELECT session_id FROM app.session_actor($1)")
        .bind(&hash)
        .fetch_optional(&app_rw)
        .await?;
    assert_eq!(after, None, "a revoked session resolves to nothing");
    Ok(())
}

/// Db review A3: authentication of a business request is one statement (plus the occasional
/// touch of the session), no separate permission transaction.
#[sqlx::test(migrations = "../../migrations")]
async fn a_business_request_pays_one_statement_for_its_caller(owner: PgPool) -> TestResult {
    let h = harness(owner).await?;
    add_member(&h.owner, "vpyd", MANAGER, "Quản lý", SystemRole::Manager).await?;
    let session = session_of(&login(&h.app, MANAGER, PASSWORD).await);
    // Warm up once: the first request may touch the session (last_seen_at).
    let _ = call(&h.app, "GET", "/staff", Some(&session), None).await;
    let staff = call(&h.app, "GET", "/staff", Some(&session), None).await;
    assert_eq!(staff.status, StatusCode::OK, "{}", staff.body);
    // session_actor (1) + the use case's statements + COMMIT; it was 5 with the separate
    // permission transaction (session, permissions, COMMIT, list, COMMIT).
    let n = statements(&staff).unwrap_or(u32::MAX);
    assert!(n <= 3, "GET /staff ran {n} statements");
    let me = call(&h.app, "GET", "/me", Some(&session), None).await;
    assert_eq!(me.status, StatusCode::OK, "{}", me.body);
    let n = statements(&me).unwrap_or(u32::MAX);
    assert!(
        n <= 3,
        "GET /me ran {n} statements (session_actor, account, spas)"
    );
    assert!(
        me.body["permissions"]
            .as_array()
            .is_some_and(|p| p.iter().any(|x| x == "visit.record")),
        "{}",
        me.body
    );
    Ok(())
}
