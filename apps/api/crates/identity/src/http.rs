//! Auth routes and the extractors every other context uses: `CurrentSession` (logged in) and
//! `AuthUser` (logged in + active spa + permissions, i.e. a `kernel::Actor`).

use crate::service::{
    self, ClientMeta, LoginResponse, Me, MfaEnrolled, MfaEnrollment, PendingMfa, Session,
};
use axum::extract::{ConnectInfo, FromRef, FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use kernel::crypto::FieldCipher;
use kernel::{Actor, AppError, FieldError, TenantId};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use utoipa::ToSchema;

/// `__Host-` prefix: Secure, Path=/, no Domain — cannot be set or overwritten by subdomains.
pub const SESSION_COOKIE: &str = "__Host-session";

pub fn session_cookie(secret: &str) -> String {
    let max_age = service::ABSOLUTE_TIMEOUT_DAYS * 24 * 60 * 60;
    format!(
        "{SESSION_COOKIE}={secret}; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age={max_age}"
    )
}

pub fn clear_session_cookie() -> String {
    format!("{SESSION_COOKIE}=; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=0")
}

/// Reads our session cookie from the `Cookie` header(s).
pub fn session_secret(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, value)| *name == SESSION_COOKIE && !value.is_empty())
        .map(|(_, value)| value.to_owned())
}

/// Client IP: first `X-Forwarded-For` hop set by Caddy (the API is only reachable through it), else the socket peer.
pub fn client_meta(parts: &Parts) -> ClientMeta {
    let forwarded = parts
        .headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .and_then(|ip| ip.trim().parse::<IpAddr>().ok());
    let peer = parts
        .extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip());
    let user_agent = parts
        .headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|ua| ua.chars().take(300).collect());
    ClientMeta {
        ip: forwarded.or(peer),
        user_agent,
    }
}

/// Any logged-in user (no spa required). Rejects with 401.
pub struct CurrentSession(pub Session);

impl<S> FromRequestParts<S> for CurrentSession
where
    PgPool: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let secret = session_secret(&parts.headers).ok_or(AppError::Unauthenticated)?;
        let pool = PgPool::from_ref(state);
        service::resolve(&pool, &secret).await.map(CurrentSession)
    }
}

/// A session that passed the password step and still owes its second factor. Only the MFA
/// endpoints take this extractor; everything else uses `CurrentSession`, which refuses it.
pub struct PendingMfaSession(pub PendingMfa);

impl<S> FromRequestParts<S> for PendingMfaSession
where
    PgPool: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let secret = session_secret(&parts.headers).ok_or(AppError::Unauthenticated)?;
        let pool = PgPool::from_ref(state);
        service::resolve_pending(&pool, &secret)
            .await
            .map(PendingMfaSession)
    }
}

/// A logged-in user acting inside their active spa, with that spa's permissions loaded.
pub struct AuthUser {
    pub actor: Actor,
    pub session: Session,
}

impl<S> FromRequestParts<S> for AuthUser
where
    PgPool: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let CurrentSession(session) = CurrentSession::from_request_parts(parts, state).await?;
        let pool = PgPool::from_ref(state);
        let actor = service::actor_for(&pool, &session, client_meta(parts).ip).await?;
        Ok(Self { actor, session })
    }
}

impl AuthUser {
    /// Guard for 🔒 actions: password confirmed within the re-auth window.
    pub fn require_recent_auth(&self) -> Result<(), AppError> {
        if self.session.recently_authenticated(chrono::Utc::now()) {
            Ok(())
        } else {
            Err(AppError::ReauthRequired)
        }
    }
}

/// Request metadata extractor (never fails).
pub struct Client(pub ClientMeta);

impl<S: Send + Sync> FromRequestParts<S> for Client {
    type Rejection = std::convert::Infallible;
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(Self(client_meta(parts)))
    }
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LoginBody {
    phone: String,
    password: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ReauthBody {
    password: String,
    /// Current TOTP (or a recovery code); required when the account has MFA enabled.
    #[serde(default)]
    code: Option<String>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MfaCodeBody {
    /// 6-digit TOTP code, or (for `verify`) one of the recovery codes.
    code: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SwitchTenantBody {
    #[schema(value_type = String, format = Uuid)]
    tenant_id: TenantId,
}

fn with_cookie<T: Serialize>(
    status: StatusCode,
    cookie: &str,
    body: Option<T>,
) -> Result<Response, AppError> {
    let value = HeaderValue::from_str(cookie).map_err(|e| AppError::Internal(e.to_string()))?;
    let mut response = match body {
        Some(body) => (status, Json(body)).into_response(),
        None => status.into_response(),
    };
    response.headers_mut().insert(header::SET_COOKIE, value);
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

/// Login/logout/re-auth/switch/MFA under `/auth`, plus `GET /me`. Mount under `/api/v1`.
pub fn routes<S>() -> (Router<S>, Router<S>)
where
    PgPool: FromRef<S>,
    Arc<FieldCipher>: FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    let auth = Router::new()
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/auth/logout-all", post(logout_all))
        .route("/auth/reauth", post(reauth))
        .route("/auth/switch-tenant", post(switch_tenant))
        .route("/auth/mfa/verify", post(mfa_verify))
        .route("/auth/mfa/enroll/start", post(mfa_enroll_start))
        .route("/auth/mfa/enroll/confirm", post(mfa_enroll_confirm));
    let me_routes = Router::new().route("/me", get(me));
    (auth, me_routes)
}

#[utoipa::path(post, path = "/api/v1/auth/login", tag = "auth", request_body = LoginBody,
    responses((status = 200, body = LoginResponse, description = "Sets the __Host-session cookie. `next` = done | mfa_verify | mfa_enroll; until done the session only works on /auth/mfa/* and logout"), (status = 401, description = "auth.invalid_credentials"), (status = 422), (status = 429)))]
pub async fn login(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Client(meta): Client,
    Json(body): Json<LoginBody>,
) -> Result<Response, AppError> {
    if body.password.is_empty() || body.phone.trim().is_empty() {
        return Err(AppError::Validation(vec![FieldError::new(
            "credentials",
            "required",
        )]));
    }
    let previous = session_secret(&headers);
    let (token, outcome) = service::login(
        &pool,
        &body.phone,
        &body.password,
        previous.as_deref(),
        &meta,
    )
    .await?;
    with_cookie(
        StatusCode::OK,
        &session_cookie(&token.secret),
        Some(outcome),
    )
}

#[utoipa::path(post, path = "/api/v1/auth/mfa/verify", tag = "auth", request_body = MfaCodeBody,
    responses((status = 200, body = Me, description = "Second factor accepted; the pending session is replaced by a full one (new cookie)"),
        (status = 401, description = "auth.invalid_mfa_code (wrong, expired, replayed, or account locked) | auth.mfa_enrollment_required | auth.unauthenticated"),
        (status = 409, description = "auth.mfa_not_pending"), (status = 429)))]
pub async fn mfa_verify(
    State(pool): State<PgPool>,
    State(cipher): State<Arc<FieldCipher>>,
    PendingMfaSession(pending): PendingMfaSession,
    Client(meta): Client,
    Json(body): Json<MfaCodeBody>,
) -> Result<Response, AppError> {
    let (token, me) = service::verify_mfa(&pool, &cipher, &pending, &body.code, &meta).await?;
    with_cookie(StatusCode::OK, &session_cookie(&token.secret), Some(me))
}

#[utoipa::path(post, path = "/api/v1/auth/mfa/enroll/start", tag = "auth",
    responses((status = 200, body = MfaEnrollment), (status = 401),
        (status = 409, description = "auth.mfa_already_enrolled | auth.mfa_not_pending"), (status = 429)))]
pub async fn mfa_enroll_start(
    State(pool): State<PgPool>,
    State(cipher): State<Arc<FieldCipher>>,
    PendingMfaSession(pending): PendingMfaSession,
) -> Result<Response, AppError> {
    let enrollment = service::start_enrollment(&pool, &cipher, &pending).await?;
    let mut response = Json(enrollment).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

#[utoipa::path(post, path = "/api/v1/auth/mfa/enroll/confirm", tag = "auth", request_body = MfaCodeBody,
    responses((status = 200, body = MfaEnrolled, description = "MFA enabled; recovery codes are shown this once. New full-session cookie"),
        (status = 401, description = "auth.invalid_mfa_code"),
        (status = 409, description = "auth.mfa_already_enrolled | auth.mfa_enrollment_not_started | auth.mfa_enrollment_changed | auth.mfa_not_pending"),
        (status = 429)))]
pub async fn mfa_enroll_confirm(
    State(pool): State<PgPool>,
    State(cipher): State<Arc<FieldCipher>>,
    PendingMfaSession(pending): PendingMfaSession,
    Client(meta): Client,
    Json(body): Json<MfaCodeBody>,
) -> Result<Response, AppError> {
    let (token, enrolled) =
        service::confirm_enrollment(&pool, &cipher, &pending, &body.code, &meta).await?;
    with_cookie(
        StatusCode::OK,
        &session_cookie(&token.secret),
        Some(enrolled),
    )
}

/// Works for full and pending-MFA sessions alike (a user may abandon the MFA step).
#[utoipa::path(post, path = "/api/v1/auth/logout", tag = "auth", responses((status = 204), (status = 401)))]
pub async fn logout(State(pool): State<PgPool>, headers: HeaderMap) -> Result<Response, AppError> {
    let secret = session_secret(&headers).ok_or(AppError::Unauthenticated)?;
    service::logout(&pool, &secret).await?;
    with_cookie::<()>(StatusCode::NO_CONTENT, &clear_session_cookie(), None)
}

#[utoipa::path(post, path = "/api/v1/auth/logout-all", tag = "auth", responses((status = 204), (status = 401)))]
pub async fn logout_all(
    State(pool): State<PgPool>,
    CurrentSession(session): CurrentSession,
) -> Result<Response, AppError> {
    service::logout_all(&pool, &session).await?;
    with_cookie::<()>(StatusCode::NO_CONTENT, &clear_session_cookie(), None)
}

#[utoipa::path(post, path = "/api/v1/auth/reauth", tag = "auth", request_body = ReauthBody,
    responses((status = 204), (status = 401, description = "auth.invalid_credentials | auth.invalid_mfa_code"),
        (status = 422, description = "`code` required: the account has MFA enabled")))]
pub async fn reauth(
    State(pool): State<PgPool>,
    State(cipher): State<Arc<FieldCipher>>,
    CurrentSession(session): CurrentSession,
    Json(body): Json<ReauthBody>,
) -> Result<StatusCode, AppError> {
    service::reauth(
        &pool,
        &cipher,
        &session,
        &body.password,
        body.code.as_deref(),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(post, path = "/api/v1/auth/switch-tenant", tag = "auth", request_body = SwitchTenantBody,
    responses((status = 200, body = Me), (status = 401), (status = 404)))]
pub async fn switch_tenant(
    State(pool): State<PgPool>,
    CurrentSession(session): CurrentSession,
    Client(meta): Client,
    Json(body): Json<SwitchTenantBody>,
) -> Result<Response, AppError> {
    let (token, me) = service::switch_tenant(&pool, &session, body.tenant_id, &meta).await?;
    with_cookie(StatusCode::OK, &session_cookie(&token.secret), Some(me))
}

#[utoipa::path(get, path = "/api/v1/me", tag = "auth", responses((status = 200, body = Me), (status = 401)))]
pub async fn me(
    State(pool): State<PgPool>,
    CurrentSession(session): CurrentSession,
) -> Result<Response, AppError> {
    let me = service::me(&pool, &session).await?;
    let mut response = Json(me).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_cookie_has_all_hardening_flags() {
        let cookie = session_cookie("abc");
        for flag in [
            "__Host-session=abc",
            "Path=/",
            "HttpOnly",
            "Secure",
            "SameSite=Strict",
            "Max-Age=2592000",
        ] {
            assert!(cookie.contains(flag), "{flag} missing in {cookie}");
        }
        assert!(!cookie.contains("Domain"));
        assert!(clear_session_cookie().contains("Max-Age=0"));
    }

    #[test]
    fn finds_our_cookie_among_others() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("theme=dark; __Host-session=tok123; x=1"),
        );
        assert_eq!(session_secret(&headers).as_deref(), Some("tok123"));
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("session=fake; __Host-session="),
        );
        assert_eq!(session_secret(&headers), None);
    }
}
