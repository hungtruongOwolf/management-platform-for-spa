//! S10: every route declares who may call it, and the server enforces it.
//!
//! `ROUTES` below is the single table of truth for this test and mirrors
//! memory/wiki/security/authz-matrix.md. Adding an endpoint without a row here fails
//! `every_mounted_route_has_a_matrix_row` (the CI gate "every route declares a permission").
//! How to add a row: memory/wiki/conventions/testing.md.

mod common;

use axum::Router;
use axum::http::StatusCode;
use common::{
    Harness, OWNER_PHONE, PASSWORD, Reply, TestResult, add_member, call_with_headers, code_now,
    current_code, forget_used_step, harness,
};
use identity::SystemRole::{self, Manager, Owner, Technician};
use identity::provisioning::create_tenant;
use kernel::Permission as P;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU32, Ordering};
use utoipa::OpenApi;

// ------------------------------------------------------------------------------------ the table

/// Who may call a route.
#[derive(Debug, Clone, Copy)]
enum Access {
    /// No session needed (login, health).
    Public,
    /// Any signed-in user; no tenant permission involved (`CurrentSession`).
    Session,
    /// Only a session that passed the password step and still owes its second factor
    /// (`PendingMfaSession`, S2b). A full session gets 409 `auth.mfa_not_pending`.
    PendingMfa,
    /// Signed in inside a spa and holding this permission (`AuthUser` + `actor.require`).
    /// `allow` lists the default roles that hold it (authz-matrix.md columns).
    Perm(P, &'static [SystemRole]),
}

/// What the `{id}` in the path refers to; the test substitutes a real record of that kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    None,
    Category,
    Service,
    Plan,
    Customer,
    Sale,
    CustomerPackage,
}

struct Route {
    method: &'static str,
    /// Exactly as in the OpenAPI document.
    path: &'static str,
    access: Access,
    id: Target,
    /// A valid request body for the caller's own tenant (`None` = no body). Must deserialize, or
    /// extraction fails with 4xx before the permission check runs.
    body: fn(&Fixture) -> Option<Value>,
    headers: &'static [(&'static str, &'static str)],
    /// The call revokes the caller's session (logout, switch-tenant): use a throwaway session.
    consumes_session: bool,
    /// The handler requires `Idempotency-Key`; every call sends a fresh one.
    idempotent: bool,
}

const ALL: &[SystemRole] = &[Owner, Manager, Technician];
const OWNER_MANAGER: &[SystemRole] = &[Owner, Manager];
const OWNER_ONLY: &[SystemRole] = &[Owner];
const IF_MATCH: &[(&str, &str)] = &[("If-Match", "\"1\"")];

const fn route(method: &'static str, path: &'static str, access: Access) -> Route {
    Route {
        method,
        path,
        access,
        id: Target::None,
        body: no_body,
        headers: &[],
        consumes_session: false,
        idempotent: false,
    }
}

impl Route {
    const fn id(mut self, id: Target) -> Self {
        self.id = id;
        self
    }
    const fn body(mut self, body: fn(&Fixture) -> Option<Value>) -> Self {
        self.body = body;
        self
    }
    const fn if_match(mut self) -> Self {
        self.headers = IF_MATCH;
        self
    }
    const fn consumes_session(mut self) -> Self {
        self.consumes_session = true;
        self
    }
    const fn idempotent(mut self) -> Self {
        self.idempotent = true;
        self
    }
}

/// One row per (method, path) in the OpenAPI document. Keep in the same order as `openapi.rs`.
const ROUTES: &[Route] = &[
    // auth — identity
    route("POST", "/api/v1/auth/login", Access::Public),
    route("POST", "/api/v1/auth/logout", Access::Session).consumes_session(),
    route("POST", "/api/v1/auth/logout-all", Access::Session).consumes_session(),
    route("POST", "/api/v1/auth/reauth", Access::Session).body(reauth_body),
    route("POST", "/api/v1/auth/switch-tenant", Access::Session)
        .body(switch_body)
        .consumes_session(),
    route("GET", "/api/v1/me", Access::Session),
    route("POST", "/api/v1/auth/mfa/verify", Access::PendingMfa).body(mfa_code_body),
    route("POST", "/api/v1/auth/mfa/enroll/start", Access::PendingMfa),
    route(
        "POST",
        "/api/v1/auth/mfa/enroll/confirm",
        Access::PendingMfa,
    )
    .body(mfa_code_body),
    // catalog
    route(
        "GET",
        "/api/v1/service-categories",
        Access::Perm(P::CatalogRead, ALL),
    ),
    route("GET", "/api/v1/services", Access::Perm(P::CatalogRead, ALL)),
    route(
        "GET",
        "/api/v1/package-plans",
        Access::Perm(P::CatalogRead, ALL),
    ),
    route(
        "POST",
        "/api/v1/service-categories",
        Access::Perm(P::CatalogManage, OWNER_ONLY),
    )
    .body(category_body),
    route(
        "PUT",
        "/api/v1/service-categories/{id}",
        Access::Perm(P::CatalogManage, OWNER_ONLY),
    )
    .id(Target::Category)
    .body(category_update_body),
    route(
        "POST",
        "/api/v1/services",
        Access::Perm(P::CatalogManage, OWNER_ONLY),
    )
    .body(service_body),
    route(
        "PUT",
        "/api/v1/services/{id}",
        Access::Perm(P::CatalogManage, OWNER_ONLY),
    )
    .id(Target::Service)
    .body(service_body)
    .if_match(),
    route(
        "POST",
        "/api/v1/package-plans",
        Access::Perm(P::CatalogManage, OWNER_ONLY),
    )
    .body(plan_body),
    route(
        "PUT",
        "/api/v1/package-plans/{id}",
        Access::Perm(P::CatalogManage, OWNER_ONLY),
    )
    .id(Target::Plan)
    .body(plan_body)
    .if_match(),
    // customers — crm
    route(
        "GET",
        "/api/v1/customers",
        Access::Perm(P::CustomerRead, OWNER_MANAGER),
    ),
    route(
        "POST",
        "/api/v1/customers",
        Access::Perm(P::CustomerWrite, OWNER_MANAGER),
    )
    .body(customer_body),
    route(
        "GET",
        "/api/v1/customers/{id}",
        Access::Perm(P::CustomerRead, OWNER_MANAGER),
    )
    .id(Target::Customer),
    route(
        "PUT",
        "/api/v1/customers/{id}",
        Access::Perm(P::CustomerWrite, OWNER_MANAGER),
    )
    .id(Target::Customer)
    .body(customer_body)
    .if_match(),
    route(
        "PUT",
        "/api/v1/customers/{id}/health-note",
        Access::Perm(P::CustomerHealthNotesWrite, OWNER_MANAGER),
    )
    .id(Target::Customer)
    .body(health_note_body)
    .if_match(),
    // staff — org
    route("GET", "/api/v1/staff", Access::Perm(P::StaffReadBasic, ALL)),
    // packages
    route(
        "GET",
        "/api/v1/customer-packages/{id}",
        Access::Perm(P::CustomerRead, OWNER_MANAGER),
    )
    .id(Target::CustomerPackage),
    route(
        "GET",
        "/api/v1/customers/{id}/packages",
        Access::Perm(P::CustomerRead, OWNER_MANAGER),
    )
    .id(Target::Customer),
    // sales
    route(
        "POST",
        "/api/v1/sales",
        Access::Perm(P::PackageSell, OWNER_MANAGER),
    )
    .body(sell_body)
    .idempotent(),
    route(
        "GET",
        "/api/v1/sales/{id}",
        Access::Perm(P::SaleRead, OWNER_MANAGER),
    )
    .id(Target::Sale),
];

/// Mounted but deliberately left out of the OpenAPI contract. Must answer without a session.
const PUBLIC_UNDOCUMENTED: &[(&str, &str)] = &[("GET", "/api/v1/health")];

// ----------------------------------------------------------------------------------- bodies

fn no_body(_: &Fixture) -> Option<Value> {
    None
}
fn reauth_body(_: &Fixture) -> Option<Value> {
    Some(json!({"password": PASSWORD}))
}
fn mfa_code_body(_: &Fixture) -> Option<Value> {
    Some(json!({"code": "000000"}))
}

fn switch_body(f: &Fixture) -> Option<Value> {
    Some(json!({"tenant_id": f.tenant_id}))
}
fn category_body(_: &Fixture) -> Option<Value> {
    Some(json!({"name": "Nhóm kiểm thử"}))
}
fn category_update_body(_: &Fixture) -> Option<Value> {
    Some(json!({"name": "Nhóm kiểm thử (sửa)", "sort_order": 1}))
}
fn service_body(f: &Fixture) -> Option<Value> {
    Some(json!({
        "category_id": f.category, "name": "Gội đầu dưỡng sinh", "description": null,
        "duration_min": 60, "recommended": false, "active": true, "prices": {"all": 150_000}
    }))
}
fn plan_body(f: &Fixture) -> Option<Value> {
    Some(json!({
        "name": "Gói 10 buổi", "price": 1_200_000, "validity_months": 6, "gift_note": null,
        "active": true, "items": [{"service_id": f.service, "sessions": 10, "bonus_sessions": 0}]
    }))
}
fn customer_body(f: &Fixture) -> Option<Value> {
    Some(
        json!({"full_name": "Trần Thị Lan", "phone": f.customer_phone, "gender": "female",
                "birth_year": null, "address": null, "source": null, "note": null}),
    )
}
fn health_note_body(_: &Fixture) -> Option<Value> {
    Some(json!({"health_note": "Dị ứng tinh dầu sả"}))
}
fn sell_body(f: &Fixture) -> Option<Value> {
    Some(json!({
        "customer_id": f.customer, "closer_staff_id": f.staff,
        "lines": [{"package_plan_id": f.plan}],
        "payments": [{"method": "cash", "amount": 1_200_000}]
    }))
}

// --------------------------------------------------------------------------------- fixtures

/// One spa with one record of every kind a `{id}` can point at.
struct Fixture {
    tenant_id: String,
    category: String,
    service: String,
    plan: String,
    customer: String,
    customer_phone: &'static str,
    staff: String,
    sale: String,
    customer_package: String,
}

impl Fixture {
    fn id_of(&self, target: Target) -> &str {
        match target {
            Target::None => "",
            Target::Category => &self.category,
            Target::Service => &self.service,
            Target::Plan => &self.plan,
            Target::Customer => &self.customer,
            Target::Sale => &self.sale,
            Target::CustomerPackage => &self.customer_package,
        }
    }
}

const MANAGER_PHONE: &str = "0905000002";
const TECHNICIAN_PHONE: &str = "0905000003";
const OTHER_OWNER_PHONE: &str = "0906000001";

fn phone_of(role: SystemRole) -> &'static str {
    match role {
        Owner => OWNER_PHONE,
        Manager => MANAGER_PHONE,
        Technician => TECHNICIAN_PHONE,
    }
}

/// Every request comes from its own client IP so per-IP rate limits never mask a result.
fn next_ip() -> String {
    static N: AtomicU32 = AtomicU32::new(1);
    let n = N.fetch_add(1, Ordering::Relaxed);
    format!("10.{}.{}.{}", (n >> 16) & 0xff, (n >> 8) & 0xff, n & 0xff)
}

async fn send(
    app: &Router,
    method: &str,
    path: &str,
    session: Option<&str>,
    body: Option<Value>,
    headers: &[(&str, &str)],
) -> Reply {
    let ip = next_ip();
    let mut all: Vec<(&str, &str)> = vec![("X-Forwarded-For", &ip)];
    all.extend_from_slice(headers);
    let path = path.strip_prefix("/api/v1").unwrap_or(path);
    call_with_headers(app, method, path, session, body, &all).await
}

/// Sends `route` as written in the table: its headers, plus a fresh Idempotency-Key if it needs one.
async fn send_route(
    app: &Router,
    route: &Route,
    path: &str,
    session: Option<&str>,
    body: Option<Value>,
) -> Reply {
    static KEY: AtomicU32 = AtomicU32::new(1);
    let key = format!(
        "00000000-0000-4000-8000-{:012x}",
        KEY.fetch_add(1, Ordering::Relaxed)
    );
    let mut headers = route.headers.to_vec();
    if route.idempotent {
        headers.push(("Idempotency-Key", &key));
    }
    send(app, route.method, path, session, body, &headers).await
}

/// Password step only. For owners this is a pending-MFA session.
async fn password_session(app: &Router, phone: &str) -> Result<(String, Reply), String> {
    let ip = next_ip();
    let reply = call_with_headers(
        app,
        "POST",
        "/auth/login",
        None,
        Some(json!({"phone": phone, "password": PASSWORD})),
        &[("X-Forwarded-For", &ip)],
    )
    .await;
    let token = reply
        .session()
        .ok_or_else(|| format!("login {phone} → {} {}", reply.status, reply.body))?;
    Ok((token, reply))
}

/// A full session: owners enroll on their first login and enter a current code afterwards.
async fn session(h: &Harness, phone: &str) -> Result<String, String> {
    let (pending, reply) = password_session(&h.app, phone).await?;
    let done = match reply.body["next"].as_str() {
        Some("mfa_enroll") => {
            let start = send(
                &h.app,
                "POST",
                "/auth/mfa/enroll/start",
                Some(&pending),
                None,
                &[],
            )
            .await;
            let key = start.body["manual_key"].as_str().unwrap_or_default();
            let body = json!({"code": code_now(key)});
            send(
                &h.app,
                "POST",
                "/auth/mfa/enroll/confirm",
                Some(&pending),
                Some(body),
                &[],
            )
            .await
        }
        Some("mfa_verify") => {
            // Many owner logins per test fall inside one 30 s step; replay is tested in mfa.rs.
            forget_used_step(h, phone)
                .await
                .map_err(|e| e.to_string())?;
            let code = current_code(h, phone).await.map_err(|e| e.to_string())?;
            let body = json!({"code": code});
            send(
                &h.app,
                "POST",
                "/auth/mfa/verify",
                Some(&pending),
                Some(body),
                &[],
            )
            .await
        }
        _ => return Ok(pending),
    };
    done.session()
        .ok_or_else(|| format!("MFA for {phone} → {} {}", done.status, done.body))
}

async fn created_id(app: &Router, owner: &str, path: &str, body: Value) -> Result<String, String> {
    let reply = send(app, "POST", path, Some(owner), Some(body), &[]).await;
    reply.body["id"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("fixture POST {path} → {} {}", reply.status, reply.body))
}

async fn fixture(
    h: &Harness,
    slug: &str,
    owner_phone: &str,
    customer_phone: &'static str,
) -> Result<Fixture, Box<dyn std::error::Error>> {
    let (app, db) = (&h.app, &h.owner);
    let owner = session(h, owner_phone).await?;
    let tenant_id: String = sqlx::query_scalar("SELECT id::text FROM app.tenant WHERE slug = $1")
        .bind(slug)
        .fetch_one(db)
        .await?;
    let category = created_id(
        app,
        &owner,
        "/service-categories",
        json!({"name": "Dưỡng sinh"}),
    )
    .await?;
    let mut f = Fixture {
        tenant_id,
        category,
        service: String::new(),
        plan: String::new(),
        customer: String::new(),
        customer_phone,
        staff: org::provisioning::add_staff(db, slug, "KTV Lan", "Kỹ thuật viên")
            .await?
            .to_string(),
        sale: String::new(),
        customer_package: String::new(),
    };
    f.service = created_id(
        app,
        &owner,
        "/services",
        service_body(&f).unwrap_or_default(),
    )
    .await?;
    f.plan = created_id(
        app,
        &owner,
        "/package-plans",
        plan_body(&f).unwrap_or_default(),
    )
    .await?;
    f.customer = created_id(
        app,
        &owner,
        "/customers",
        customer_body(&f).unwrap_or_default(),
    )
    .await?;
    let sold = send(
        app,
        "POST",
        "/sales",
        Some(&owner),
        sell_body(&f),
        &[("Idempotency-Key", &uuid::Uuid::now_v7().to_string())],
    )
    .await;
    let (Some(sale), Some(package)) = (
        sold.body["id"].as_str(),
        sold.body["customer_packages"][0]["id"].as_str(),
    ) else {
        return Err(format!("fixture POST /sales → {} {}", sold.status, sold.body).into());
    };
    f.sale = sale.to_owned();
    f.customer_package = package.to_owned();
    Ok(f)
}

struct World {
    app: Router,
    h: Harness,
    home: Fixture,
    other: Fixture,
}

/// Spa `vpyd` (owner, manager, technician) and spa `other` (its own owner), each with fixtures.
async fn world(pool: PgPool) -> Result<World, Box<dyn std::error::Error>> {
    let h = harness(pool).await?;
    add_member(&h.owner, "vpyd", MANAGER_PHONE, "Quản lý", Manager).await?;
    add_member(&h.owner, "vpyd", TECHNICIAN_PHONE, "KTV", Technician).await?;
    create_tenant(&h.owner, "other", "Spa khác", "Huế").await?;
    add_member(&h.owner, "other", OTHER_OWNER_PHONE, "Chủ spa khác", Owner).await?;
    let home = fixture(&h, "vpyd", OWNER_PHONE, "0905 777 001").await?;
    let other = fixture(&h, "other", OTHER_OWNER_PHONE, "0905 777 002").await?;
    Ok(World {
        app: h.app.clone(),
        h,
        home,
        other,
    })
}

fn concrete(route: &Route, f: &Fixture) -> String {
    route.path.replace("{id}", f.id_of(route.id))
}

fn label(route: &Route) -> String {
    format!("{} {}", route.method, route.path)
}

// -------------------------------------------------------------------------------- the tests

/// Routes mounted in the router source (`.route("/x", get(a).post(b))`), read from every context
/// crate. Catches a route that was mounted but never added to the OpenAPI document.
fn mounted_routes() -> Result<BTreeSet<String>, Box<dyn std::error::Error>> {
    let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut found = BTreeSet::new();
    let mut stack = vec![crates];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if path.is_dir() {
                if name != "tests" && name != "target" {
                    stack.push(path);
                }
            } else if name.ends_with(".rs") {
                let text = std::fs::read_to_string(&path)?;
                // Unit-test modules mount throwaway routes (e.g. /echo); production code ends before them.
                let production = text.split("#[cfg(test)]").next().unwrap_or("");
                found.extend(routes_in(production));
            }
        }
    }
    Ok(found)
}

/// Parses `.route("<path>", <method>(..).<method>(..))` calls into "METHOD /api/v1<path>".
fn routes_in(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = source;
    while let Some(start) = rest.find(".route(") {
        rest = &rest[start + ".route(".len()..];
        // The argument list, up to the matching close paren.
        let mut depth = 1;
        let end = rest
            .char_indices()
            .find(|&(_, c)| {
                depth += match c {
                    '(' => 1,
                    ')' => -1,
                    _ => 0,
                };
                depth == 0
            })
            .map_or(rest.len(), |(i, _)| i);
        let args = &rest[..end];
        let Some(path) = args.split('"').nth(1) else {
            continue;
        };
        let handlers = args.split_once(',').map_or("", |(_, h)| h);
        for method in ["get", "post", "put", "patch", "delete"] {
            let calls = handlers
                .match_indices(&format!("{method}("))
                .filter(|(i, _)| {
                    *i == 0 || !handlers[..*i].ends_with(|c: char| c.is_alphanumeric() || c == '_')
                })
                .count();
            if calls > 0 {
                out.push(format!("{} /api/v1{path}", method.to_uppercase()));
            }
        }
    }
    out
}

#[test]
fn every_mounted_route_has_a_matrix_row() -> TestResult {
    let doc = app::openapi::ApiDoc::openapi();
    let mut documented = BTreeSet::new();
    for (path, item) in &doc.paths.paths {
        for (method, op) in [
            ("GET", &item.get),
            ("POST", &item.post),
            ("PUT", &item.put),
            ("PATCH", &item.patch),
            ("DELETE", &item.delete),
        ] {
            if op.is_some() {
                documented.insert(format!("{method} {path}"));
            }
        }
    }
    let rows: BTreeSet<String> = ROUTES.iter().map(label).collect();
    assert_eq!(rows.len(), ROUTES.len(), "duplicate row in ROUTES");
    let undocumented: BTreeSet<String> = PUBLIC_UNDOCUMENTED
        .iter()
        .map(|(m, p)| format!("{m} {p}"))
        .collect();

    let missing: Vec<&String> = documented.difference(&rows).collect();
    assert!(
        missing.is_empty(),
        "route(s) without an authorization row: {missing:?}\n\
         → add a row to ROUTES in apps/api/crates/app/tests/authz_matrix.rs that mirrors \
         memory/wiki/security/authz-matrix.md (see memory/wiki/conventions/testing.md)"
    );
    let stale: Vec<&String> = rows.difference(&documented).collect();
    assert!(
        stale.is_empty(),
        "ROUTES has rows for routes that are not in the OpenAPI document: {stale:?}"
    );

    let mounted = mounted_routes()?;
    let known: BTreeSet<String> = documented.union(&undocumented).cloned().collect();
    let hidden: Vec<&String> = mounted.difference(&known).collect();
    assert!(
        hidden.is_empty(),
        "route(s) mounted in a router but missing from the OpenAPI document (openapi.rs): {hidden:?}"
    );
    let ghost: Vec<&String> = known.difference(&mounted).collect();
    assert!(
        ghost.is_empty(),
        "documented route(s) not found in any router source: {ghost:?} (route parser out of date?)"
    );
    Ok(())
}

/// The table must agree with the seeded roles: a row's `allow` list is exactly the roles whose
/// default permissions contain that permission. Catches drift between roles.rs and the matrix.
#[test]
fn matrix_rows_agree_with_seeded_roles() {
    for route in ROUTES {
        if let Access::Perm(permission, allow) = route.access {
            let holders: Vec<SystemRole> = SystemRole::ALL
                .into_iter()
                .filter(|r| r.default_permissions().contains(&permission))
                .collect();
            assert_eq!(
                holders,
                allow.to_vec(),
                "{}: authz-matrix says {allow:?} hold {}, roles.rs seeds {holders:?}",
                label(route),
                permission.as_str()
            );
        }
    }
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn every_non_public_route_rejects_anonymous_and_forged_sessions(pool: PgPool) -> TestResult {
    let w = world(pool).await?;
    let mut failures = Vec::new();
    for route in ROUTES {
        let path = concrete(route, &w.home);
        let body = (route.body)(&w.home);
        let anonymous = send_route(&w.app, route, &path, None, body.clone()).await;
        if matches!(route.access, Access::Public) {
            if anonymous.status == StatusCode::UNAUTHORIZED
                && anonymous.body["code"] == "auth.unauthenticated"
            {
                failures.push(format!("{}: public route demanded a session", label(route)));
            }
            continue;
        }
        let forged = send_route(&w.app, route, &path, Some("forged-session-token"), body).await;
        for (who, reply) in [("anonymous", &anonymous), ("forged cookie", &forged)] {
            if reply.status != StatusCode::UNAUTHORIZED {
                failures.push(format!(
                    "{} as {who}: expected 401, got {} {}",
                    label(route),
                    reply.status,
                    reply.body
                ));
            }
        }
    }
    for (method, path) in PUBLIC_UNDOCUMENTED {
        let reply = send(&w.app, method, path, None, None, &[]).await;
        if reply.status != StatusCode::OK {
            failures.push(format!(
                "{method} {path}: expected 200, got {}",
                reply.status
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn each_role_gets_exactly_what_the_matrix_grants(pool: PgPool) -> TestResult {
    let w = world(pool).await?;
    let mut failures = Vec::new();
    let mut cells = 0;
    for role in SystemRole::ALL {
        let mut shared = session(&w.h, phone_of(role)).await?;
        for route in ROUTES {
            let expected_allowed = match route.access {
                // Pending-MFA routes: `pending_mfa_sessions_reach_only_the_mfa_routes`.
                Access::Public | Access::PendingMfa => continue,
                Access::Session => true,
                Access::Perm(_, allow) => allow.contains(&role),
            };
            let token = if route.consumes_session {
                session(&w.h, phone_of(role)).await?
            } else {
                shared.clone()
            };
            let reply = send_route(
                &w.app,
                route,
                &concrete(route, &w.home),
                Some(&token),
                (route.body)(&w.home),
            )
            .await;
            cells += 1;
            if route.consumes_session {
                // logout-all revokes every session of the user, the shared one included.
                shared = session(&w.h, phone_of(role)).await?;
            }
            let ok = if expected_allowed {
                !matches!(
                    reply.status,
                    StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
                ) && !reply.status.is_server_error()
            } else {
                reply.status == StatusCode::FORBIDDEN && reply.body["code"] == "auth.forbidden"
            };
            if !ok {
                failures.push(format!(
                    "{} as {}: expected {}, got {} {}",
                    label(route),
                    role.key(),
                    if expected_allowed {
                        "allowed"
                    } else {
                        "403 auth.forbidden"
                    },
                    reply.status,
                    reply.body
                ));
            }
        }
    }
    assert!(cells > 0);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}

/// authz-matrix test #7 (S2b): a session that only passed the password step reaches nothing but
/// the MFA routes and logout; a full session cannot use the MFA-step routes.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn pending_mfa_sessions_reach_only_the_mfa_routes(pool: PgPool) -> TestResult {
    let w = world(pool).await?;
    let (pending, login) = password_session(&w.app, OWNER_PHONE).await?;
    assert_eq!(
        login.body["next"], "mfa_verify",
        "owner enrolled by the fixture"
    );
    let full = session(&w.h, OWNER_PHONE).await?;
    let mut failures = Vec::new();
    for route in ROUTES {
        let path = concrete(route, &w.home);
        let body = (route.body)(&w.home);
        match route.access {
            Access::Public => {}
            Access::PendingMfa => {
                let reply = send_route(&w.app, route, &path, Some(&full), body).await;
                if (reply.status, reply.body["code"].as_str())
                    != (StatusCode::CONFLICT, Some("auth.mfa_not_pending"))
                {
                    failures.push(format!(
                        "{} with a full session: expected 409 auth.mfa_not_pending, got {} {}",
                        label(route),
                        reply.status,
                        reply.body
                    ));
                }
            }
            // Logout accepts a pending session by design; checked last (it ends the session).
            Access::Session if route.path == "/api/v1/auth/logout" => {}
            Access::Session | Access::Perm(..) => {
                let reply = send_route(&w.app, route, &path, Some(&pending), body).await;
                let code = reply.body["code"].as_str().unwrap_or_default();
                if reply.status != StatusCode::UNAUTHORIZED || !code.starts_with("auth.mfa_") {
                    failures.push(format!(
                        "{} with a pending-MFA session: expected 401 auth.mfa_*, got {} {}",
                        label(route),
                        reply.status,
                        reply.body
                    ));
                }
            }
        }
    }
    let out = send(&w.app, "POST", "/auth/logout", Some(&pending), None, &[]).await;
    if out.status != StatusCode::NO_CONTENT {
        failures.push(format!("logout of a pending session: got {}", out.status));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}

/// authz-matrix test #2: an id from another spa is indistinguishable from a missing one.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn ids_from_another_spa_are_not_found(pool: PgPool) -> TestResult {
    let w = world(pool).await?;
    let intruder = session(&w.h, OTHER_OWNER_PHONE).await?;
    let mut failures = Vec::new();
    let mut swept = 0;
    for route in ROUTES.iter().filter(|r| r.id != Target::None) {
        // The intruder's body references only their own records; the path id is the only foreign thing.
        let reply = send_route(
            &w.app,
            route,
            &concrete(route, &w.home),
            Some(&intruder),
            (route.body)(&w.other),
        )
        .await;
        swept += 1;
        if reply.status != StatusCode::NOT_FOUND {
            failures.push(format!(
                "{} with another spa's id: expected 404, got {} {}",
                label(route),
                reply.status,
                reply.body
            ));
        }
        // Control: the same call on the intruder's own record is not a 404, so the 404 above is
        // tenant isolation and not a broken request.
        let own = send_route(
            &w.app,
            route,
            &concrete(route, &w.other),
            Some(&intruder),
            (route.body)(&w.other),
        )
        .await;
        if own.status == StatusCode::NOT_FOUND {
            failures.push(format!(
                "{}: control call on own record was 404",
                label(route)
            ));
        }
    }
    assert!(swept > 0, "no {{id}} routes swept");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}

#[test]
fn route_parser_reads_chained_and_multiline_routes() {
    let source = r#"
        Router::new()
            .route(
                "/a",
                get(list_a).post(create_a),
            )
            .route("/a/{id}", put(update_a))
            .route("/b", get(forget(x)))
    "#;
    assert_eq!(
        routes_in(source),
        [
            "GET /api/v1/a",
            "POST /api/v1/a",
            "PUT /api/v1/a/{id}",
            "GET /api/v1/b"
        ]
    );
}
