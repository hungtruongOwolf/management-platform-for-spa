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
use identity::provisioning::{NewUser, add_user, create_tenant};
use kernel::testing::app_pool;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::net::SocketAddr;
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
}

/// Tenant `vpyd` with an owner account (OWNER_PHONE / PASSWORD).
pub async fn harness(owner: PgPool) -> Result<Harness, Box<dyn std::error::Error>> {
    create_tenant(&owner, "vpyd", "Viên Phúc Y Đường", "Đông Hà").await?;
    add_member(&owner, "vpyd", OWNER_PHONE, "Chủ Spa", SystemRole::Owner).await?;
    let pool = app_pool(&owner).await?;
    let cipher = crm::FieldCipher::from_base64(TEST_DATA_KEY)?;
    let app = app::router(
        AppState {
            pool,
            cipher: std::sync::Arc::new(cipher),
        },
        &HttpPolicy::standard(ORIGIN.to_owned()),
    );
    Ok(Harness { app, owner })
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

/// Logs in and returns the session token (empty string if login failed).
pub async fn session_for(app: &Router, phone: &str) -> String {
    login(app, phone, PASSWORD)
        .await
        .session()
        .unwrap_or_default()
}
