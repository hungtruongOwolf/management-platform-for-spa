//! Shared harness for HTTP integration tests: real router + middleware, Postgres as app_rw.
#![allow(dead_code)] // each test binary uses a different subset

use app::AppState;
use app::middleware::HttpPolicy;
use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, Request, StatusCode, header};
use http_body_util::BodyExt;
use identity::SystemRole;
use identity::mfa::TotpSecret;
use identity::provisioning::{NewUser, add_user, create_tenant};
use kernel::crypto::FieldCipher;
use kernel::testing::app_pool;
use kernel::{PhoneE164, UserId};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::net::SocketAddr;
use std::sync::Arc;
use tower::ServiceExt;

pub type TestResult = Result<(), Box<dyn std::error::Error>>;

pub const ORIGIN: &str = "http://localhost:5173";
pub const OWNER_PHONE: &str = "0905123456";
pub const PASSWORD: &str = "mat-khau-cua-chu-spa";
/// Test-only AES key (32 ASCII bytes, base64).
pub const TEST_DATA_KEY: &str = "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=";

pub struct Harness {
    pub app: Router,
    pub owner: PgPool,
    /// Same key as the app's, so tests can read back an enrolled TOTP secret.
    pub cipher: Arc<FieldCipher>,
}

/// Tenant `vpyd` with an owner account (OWNER_PHONE / PASSWORD).
pub async fn harness(owner: PgPool) -> Result<Harness, Box<dyn std::error::Error>> {
    create_tenant(&owner, "vpyd", "Viên Phúc Y Đường", "Đông Hà").await?;
    add_member(&owner, "vpyd", OWNER_PHONE, "Chủ Spa", SystemRole::Owner).await?;
    let pool = app_pool(&owner).await?;
    let cipher = Arc::new(FieldCipher::from_base64(TEST_DATA_KEY)?);
    let app = app::router(
        AppState {
            pool,
            cipher: cipher.clone(),
        },
        &HttpPolicy::standard(ORIGIN.to_owned()),
    );
    Ok(Harness { app, owner, cipher })
}

pub async fn add_member(
    owner: &PgPool,
    tenant: &str,
    phone: &str,
    name: &str,
    role: SystemRole,
) -> Result<(), Box<dyn std::error::Error>> {
    add_user(
        owner,
        NewUser {
            tenant_slug: tenant,
            phone,
            full_name: name,
            password: PASSWORD,
            role,
        },
    )
    .await?;
    Ok(())
}

pub struct Reply {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Value,
}

impl Reply {
    pub fn cookie(&self) -> Option<String> {
        self.headers
            .get(header::SET_COOKIE)?
            .to_str()
            .ok()
            .map(str::to_owned)
    }
    pub fn session(&self) -> Option<String> {
        let cookie = self.cookie()?;
        let value = cookie.split(';').next()?.strip_prefix("__Host-session=")?;
        (!value.is_empty()).then(|| value.to_owned())
    }
}

pub async fn call(
    app: &Router,
    method: &str,
    path: &str,
    session: Option<&str>,
    body: Option<Value>,
) -> Reply {
    call_with_headers(app, method, path, session, body, &[]).await
}

/// `POST /customers` with a fresh `Idempotency-Key`, as the web sends one per create attempt
/// (ADR-0019 D11: the route requires it).
pub async fn post_customer(app: &Router, session: Option<&str>, body: Option<Value>) -> Reply {
    let key = uuid::Uuid::now_v7().to_string();
    call_with_headers(
        app,
        "POST",
        "/customers",
        session,
        body,
        &[("idempotency-key", &key)],
    )
    .await
}

/// Like `call`, with extra request headers (e.g. `If-Match`).
pub async fn call_with_headers(
    app: &Router,
    method: &str,
    path: &str,
    session: Option<&str>,
    body: Option<Value>,
    extra: &[(&str, &str)],
) -> Reply {
    let mut builder = Request::builder()
        .method(method)
        .uri(format!("/api/v1{path}"))
        .header(header::ORIGIN, ORIGIN)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(s) = session {
        builder = builder.header(header::COOKIE, format!("__Host-session={s}"));
    }
    for (name, value) in extra {
        builder = builder.header(*name, *value);
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

pub async fn login(app: &Router, phone: &str, password: &str) -> Reply {
    call(
        app,
        "POST",
        "/auth/login",
        None,
        Some(json!({"phone": phone, "password": password})),
    )
    .await
}

/// Logs in and, when the account needs MFA (owners), completes it: first login enrolls an
/// authenticator, later logins verify a current code. Returns the full session token ("" on failure).
pub async fn session_for(h: &Harness, phone: &str) -> String {
    let reply = login(&h.app, phone, PASSWORD).await;
    let token = reply.session().unwrap_or_default();
    match reply.body["next"].as_str() {
        Some("mfa_enroll") => enroll(h, &token).await.session().unwrap_or_default(),
        Some("mfa_verify") => {
            // Several logins of one owner inside a 30 s step would otherwise be (correctly)
            // refused as code replays; tests that are not about replay start from a clean slate.
            let _ = forget_used_step(h, phone).await;
            let code = current_code(h, phone).await.unwrap_or_default();
            mfa_verify(h, &token, &code)
                .await
                .session()
                .unwrap_or_default()
        }
        _ => token,
    }
}

/// Code an authenticator app would show now for `manual_key`.
pub fn code_now(manual_key: &str) -> String {
    let now = u64::try_from(unix_now()).unwrap_or_default();
    TotpSecret::from_base32(manual_key.to_owned())
        .code_at(now)
        .unwrap_or_default()
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or_default())
        .unwrap_or_default()
}

/// enroll/start + enroll/confirm with a valid code; returns the confirm reply.
pub async fn enroll(h: &Harness, pending: &str) -> Reply {
    let start = call(
        &h.app,
        "POST",
        "/auth/mfa/enroll/start",
        Some(pending),
        None,
    )
    .await;
    let key = start.body["manual_key"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    call(
        &h.app,
        "POST",
        "/auth/mfa/enroll/confirm",
        Some(pending),
        Some(json!({"code": code_now(&key)})),
    )
    .await
}

pub async fn mfa_verify(h: &Harness, pending: &str, code: &str) -> Reply {
    call(
        &h.app,
        "POST",
        "/auth/mfa/verify",
        Some(pending),
        Some(json!({"code": code})),
    )
    .await
}

async fn user_id(h: &Harness, phone: &str) -> Result<UserId, Box<dyn std::error::Error>> {
    let phone = PhoneE164::parse(phone).map_err(|_| "bad phone")?;
    let id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM app.user_account WHERE phone_e164 = $1")
            .bind(phone.as_str())
            .fetch_one(&h.owner)
            .await?;
    Ok(UserId::from_uuid(id))
}

/// The enrolled (or pending) TOTP secret, decrypted the way the server does.
pub async fn stored_secret(h: &Harness, phone: &str) -> Result<String, Box<dyn std::error::Error>> {
    let user = user_id(h, phone).await?;
    let ct: Vec<u8> =
        sqlx::query_scalar("SELECT mfa_secret_ct FROM app.user_account WHERE id = $1")
            .bind(user.as_uuid())
            .fetch_one(&h.owner)
            .await?;
    Ok(h.cipher.decrypt(&ct, &identity::mfa::secret_aad(user))?)
}

pub async fn current_code(h: &Harness, phone: &str) -> Result<String, Box<dyn std::error::Error>> {
    Ok(code_now(&stored_secret(h, phone).await?))
}

/// Moves bookings made for tomorrow onto the same time today (F12, 2026-11-08): a slot of today
/// already over can no longer be booked, yet arriving and completing happen today. Book early
/// tomorrow, then call this: it works whatever time the test runs.
pub async fn bookings_to_today(
    h: &Harness,
    ids: &[&str],
) -> Result<(), Box<dyn std::error::Error>> {
    let ids: Vec<uuid::Uuid> = ids
        .iter()
        .filter_map(|i| uuid::Uuid::parse_str(i).ok())
        .collect();
    sqlx::query(
        "UPDATE app.appointment SET starts_at = starts_at - interval '1 day',
                ends_at = ends_at - interval '1 day', business_date = business_date - 1
         WHERE id = ANY($1)",
    )
    .bind(&ids)
    .execute(&h.owner)
    .await?;
    Ok(())
}

pub async fn forget_used_step(h: &Harness, phone: &str) -> Result<(), Box<dyn std::error::Error>> {
    let user = user_id(h, phone).await?;
    sqlx::query("UPDATE app.user_account SET mfa_last_step = NULL WHERE id = $1")
        .bind(user.as_uuid())
        .execute(&h.owner)
        .await?;
    Ok(())
}

// ------------------------------------------------------------------ platform operator (ADR-0018)

pub const OPERATOR: &str = "ops";
pub const OPERATOR_PASSWORD: &str = "mat-khau-cua-operator";

/// The CLI path (`app add-operator`): the only way an operator exists.
pub async fn add_operator(h: &Harness) -> Result<uuid::Uuid, Box<dyn std::error::Error>> {
    Ok(
        platform::operator::add_operator(&h.owner, OPERATOR, "Người vận hành", OPERATOR_PASSWORD)
            .await?,
    )
}

impl Reply {
    /// The `__Host-op-session` value this reply set, if any.
    pub fn op_session(&self) -> Option<String> {
        self.headers
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .find_map(|c| {
                c.split(';')
                    .next()?
                    .strip_prefix("__Host-op-session=")
                    .map(str::to_owned)
            })
            .filter(|v| !v.is_empty())
    }
}

/// Like `call`, with the operator cookie (`op`) instead of the spa one. Each call comes from its
/// own client address, so the strict `/admin/auth/*` rate limit (burst 5) never masks a result.
pub async fn admin_call(
    app: &Router,
    method: &str,
    path: &str,
    op: Option<&str>,
    body: Option<Value>,
) -> Reply {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let ip = format!("10.200.{}.{}", (n >> 8) & 0xff, n & 0xff);
    let cookie = op.map(|s| format!("__Host-op-session={s}"));
    let mut extra: Vec<(&str, &str)> = vec![("X-Forwarded-For", ip.as_str())];
    extra.extend(cookie.iter().map(|c| ("Cookie", c.as_str())));
    call_with_headers(app, method, path, None, body, &extra).await
}

/// The operator's TOTP secret, decrypted the way the server does.
pub async fn operator_secret(h: &Harness) -> Result<String, Box<dyn std::error::Error>> {
    let (id, ct): (uuid::Uuid, Vec<u8>) =
        sqlx::query_as("SELECT id, mfa_secret_ct FROM app.platform_operator WHERE username = $1")
            .bind(OPERATOR)
            .fetch_one(&h.owner)
            .await?;
    let aad = kernel::crypto::record_aad("platform_operator.mfa", &[id]);
    Ok(h.cipher.decrypt(&ct, &aad)?)
}

/// A current code for the operator, forgetting the last used step first (several sign-ins and
/// step-ups inside one 30 s step are otherwise, correctly, replays).
pub async fn operator_code(h: &Harness) -> Result<String, Box<dyn std::error::Error>> {
    sqlx::query("UPDATE app.platform_operator SET mfa_last_step = NULL WHERE username = $1")
        .bind(OPERATOR)
        .execute(&h.owner)
        .await?;
    Ok(code_now(&operator_secret(h).await?))
}

/// Signs the operator in (enrolling an authenticator the first time); the full session.
pub async fn operator_session(h: &Harness) -> Result<String, Box<dyn std::error::Error>> {
    let login = admin_call(
        &h.app,
        "POST",
        "/admin/auth/login",
        None,
        Some(json!({"username": OPERATOR, "password": OPERATOR_PASSWORD})),
    )
    .await;
    let pending = login
        .op_session()
        .ok_or(format!("login → {} {}", login.status, login.body))?;
    let done = if login.body["next"] == "mfa_enroll" {
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
        admin_call(
            &h.app,
            "POST",
            "/admin/auth/mfa/enroll/confirm",
            Some(&pending),
            Some(json!({"code": code_now(&key)})),
        )
        .await
    } else {
        let code = operator_code(h).await?;
        admin_call(
            &h.app,
            "POST",
            "/admin/auth/mfa/verify",
            Some(&pending),
            Some(json!({"code": code})),
        )
        .await
    };
    done.op_session()
        .ok_or_else(|| format!("mfa → {} {}", done.status, done.body).into())
}

/// Ages the step-up of every operator session past the 5-minute window.
pub async fn expire_step_up(h: &Harness) -> Result<(), Box<dyn std::error::Error>> {
    sqlx::query("UPDATE app.platform_session SET recent_auth_at = now() - interval '6 minutes'")
        .execute(&h.owner)
        .await?;
    Ok(())
}

/// `POST /admin/auth/reauth` with the password and a current code.
pub async fn step_up(h: &Harness, op: &str) -> Result<Reply, Box<dyn std::error::Error>> {
    let code = operator_code(h).await?;
    Ok(admin_call(
        &h.app,
        "POST",
        "/admin/auth/reauth",
        Some(op),
        Some(json!({"password": OPERATOR_PASSWORD, "code": code})),
    )
    .await)
}
