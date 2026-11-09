//! `/api/v1/admin/*` (ADR-0018). Every route but login and the MFA steps takes an operator
//! extractor: `Operator` (full session; writes need a step-up), `OperatorAuth` (full session,
//! no step-up: me, re-auth), `PendingOperatorSession` (the MFA steps). Spa cookies are never read.

use crate::audit::{self, AuditQuery, PlatformAuditPage};
use crate::auth::{
    self, Operator, OperatorAuth, OperatorEnrolled, OperatorEnrollment, OperatorLogin, OperatorMe,
    OperatorSummary, PendingOperatorSession,
};
use crate::tenants::{
    self, CreatedTenant, NewTenant, Reason, StatusChanged, TenantRow, TenantStatus,
};
use axum::extract::{FromRef, Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use identity::http::Client;
use kernel::crypto::FieldCipher;
use kernel::{AppError, FieldError};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use utoipa::ToSchema;
use uuid::Uuid;

/// `(auth, console)`: mount `auth` behind the stricter admin-auth rate limit.
pub fn routes<S>() -> (Router<S>, Router<S>)
where
    PgPool: FromRef<S>,
    Arc<FieldCipher>: FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    let auth = Router::new()
        .route("/admin/auth/login", post(admin_login))
        .route("/admin/auth/logout", post(admin_logout))
        .route("/admin/auth/reauth", post(admin_reauth))
        .route("/admin/auth/mfa/verify", post(admin_mfa_verify))
        .route("/admin/auth/mfa/enroll/start", post(admin_mfa_enroll_start))
        .route(
            "/admin/auth/mfa/enroll/confirm",
            post(admin_mfa_enroll_confirm),
        );
    let console = Router::new()
        .route("/admin/me", get(admin_me))
        .route("/admin/tenants", get(list_tenants).post(create_tenant))
        .route("/admin/tenants/{id}", get(get_tenant))
        .route("/admin/tenants/{id}/suspend", post(suspend_tenant))
        .route("/admin/tenants/{id}/unsuspend", post(unsuspend_tenant))
        .route("/admin/audit", get(list_audit));
    (auth, console)
}

fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn with_cookie<T: Serialize>(cookie: &str, body: Option<T>) -> Result<Response, AppError> {
    let value = HeaderValue::from_str(cookie).map_err(|e| AppError::Internal(e.to_string()))?;
    let mut response = match body {
        Some(body) => (StatusCode::OK, Json(body)).into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    };
    response.headers_mut().insert(header::SET_COOKIE, value);
    Ok(no_store(response))
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OperatorLoginBody {
    username: String,
    password: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OperatorCodeBody {
    /// 6-digit TOTP code, or (verify / re-auth) a recovery code.
    code: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OperatorReauthBody {
    password: String,
    code: String,
}

#[utoipa::path(post, path = "/api/v1/admin/auth/login", tag = "admin", request_body = OperatorLoginBody,
    responses((status = 200, body = OperatorLogin, description = "Sets a pending __Host-op-session cookie: always a second factor next"),
        (status = 401, description = "auth.invalid_credentials"), (status = 422), (status = 429)))]
pub async fn admin_login(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Client(meta): Client,
    Json(body): Json<OperatorLoginBody>,
) -> Result<Response, AppError> {
    if body.username.trim().is_empty() || body.password.is_empty() {
        return Err(AppError::Validation(vec![FieldError::new(
            "credentials",
            "required",
        )]));
    }
    let previous = auth::operator_secret(&headers);
    let (token, next) = auth::login(
        &pool,
        &body.username,
        &body.password,
        previous.as_deref(),
        &meta,
    )
    .await?;
    with_cookie(&auth::operator_cookie(&token.secret), Some(next))
}

#[utoipa::path(post, path = "/api/v1/admin/auth/mfa/verify", tag = "admin", request_body = OperatorCodeBody,
    responses((status = 200, body = OperatorSummary, description = "Signed in: new full-session cookie"),
        (status = 401, description = "auth.invalid_mfa_code | auth.mfa_enrollment_required | auth.unauthenticated"),
        (status = 409, description = "auth.mfa_not_pending"), (status = 429)))]
pub async fn admin_mfa_verify(
    State(pool): State<PgPool>,
    State(cipher): State<Arc<FieldCipher>>,
    PendingOperatorSession(pending): PendingOperatorSession,
    Json(body): Json<OperatorCodeBody>,
) -> Result<Response, AppError> {
    let (token, operator) = auth::verify_mfa(&pool, &cipher, &pending, &body.code).await?;
    with_cookie(&auth::operator_cookie(&token.secret), Some(operator))
}

#[utoipa::path(post, path = "/api/v1/admin/auth/mfa/enroll/start", tag = "admin",
    responses((status = 200, body = OperatorEnrollment), (status = 401),
        (status = 409, description = "auth.mfa_already_enrolled | auth.mfa_not_pending"), (status = 429)))]
pub async fn admin_mfa_enroll_start(
    State(pool): State<PgPool>,
    State(cipher): State<Arc<FieldCipher>>,
    PendingOperatorSession(pending): PendingOperatorSession,
) -> Result<Response, AppError> {
    let enrollment = auth::start_enrollment(&pool, &cipher, &pending).await?;
    Ok(no_store(Json(enrollment).into_response()))
}

#[utoipa::path(post, path = "/api/v1/admin/auth/mfa/enroll/confirm", tag = "admin", request_body = OperatorCodeBody,
    responses((status = 200, body = OperatorEnrolled, description = "MFA on; recovery codes shown once; new full-session cookie"),
        (status = 401, description = "auth.invalid_mfa_code"),
        (status = 409, description = "auth.mfa_already_enrolled | auth.mfa_enrollment_not_started | auth.mfa_enrollment_changed | auth.mfa_not_pending"),
        (status = 429)))]
pub async fn admin_mfa_enroll_confirm(
    State(pool): State<PgPool>,
    State(cipher): State<Arc<FieldCipher>>,
    PendingOperatorSession(pending): PendingOperatorSession,
    Json(body): Json<OperatorCodeBody>,
) -> Result<Response, AppError> {
    let (token, enrolled) = auth::confirm_enrollment(&pool, &cipher, &pending, &body.code).await?;
    with_cookie(&auth::operator_cookie(&token.secret), Some(enrolled))
}

#[utoipa::path(post, path = "/api/v1/admin/auth/reauth", tag = "admin", request_body = OperatorReauthBody,
    responses((status = 204, description = "Step-up open for 5 minutes"),
        (status = 401, description = "auth.invalid_credentials | auth.invalid_mfa_code | auth.unauthenticated"),
        (status = 422, description = "code required"), (status = 429)))]
pub async fn admin_reauth(
    State(pool): State<PgPool>,
    State(cipher): State<Arc<FieldCipher>>,
    OperatorAuth(session): OperatorAuth,
    Json(body): Json<OperatorReauthBody>,
) -> Result<Response, AppError> {
    auth::reauth(&pool, &cipher, &session, &body.password, &body.code).await?;
    Ok(no_store(StatusCode::NO_CONTENT.into_response()))
}

/// Works for full and pending sessions alike.
#[utoipa::path(post, path = "/api/v1/admin/auth/logout", tag = "admin",
    responses((status = 204), (status = 401)))]
pub async fn admin_logout(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Client(meta): Client,
) -> Result<Response, AppError> {
    let secret = auth::operator_secret(&headers).ok_or(AppError::Unauthenticated)?;
    auth::logout(&pool, &secret, &meta).await?;
    with_cookie::<()>(&auth::clear_operator_cookie(), None)
}

#[utoipa::path(get, path = "/api/v1/admin/me", tag = "admin",
    responses((status = 200, body = OperatorMe), (status = 401)))]
pub async fn admin_me(
    State(pool): State<PgPool>,
    OperatorAuth(session): OperatorAuth,
) -> Result<Response, AppError> {
    Ok(no_store(
        Json(auth::me(&pool, &session).await?).into_response(),
    ))
}

#[utoipa::path(get, path = "/api/v1/admin/tenants", tag = "admin",
    responses((status = 200, body = Vec<TenantRow>), (status = 401)))]
pub async fn list_tenants(
    State(pool): State<PgPool>,
    Operator(op): Operator,
) -> Result<Json<Vec<TenantRow>>, AppError> {
    tenants::list(&pool, &op).await.map(Json)
}

#[utoipa::path(post, path = "/api/v1/admin/tenants", tag = "admin", request_body = NewTenant,
    responses((status = 201, body = CreatedTenant, description = "The owner's temporary password is in this response only (no-store)"),
        (status = 401), (status = 403, description = "platform.step_up_required"),
        (status = 409, description = "tenant.slug_taken | account.phone_taken"), (status = 422)))]
pub async fn create_tenant(
    State(pool): State<PgPool>,
    Operator(op): Operator,
    Json(body): Json<NewTenant>,
) -> Result<Response, AppError> {
    let created = tenants::create(&pool, &op, &body).await?;
    Ok(no_store(
        (StatusCode::CREATED, Json(created)).into_response(),
    ))
}

#[utoipa::path(get, path = "/api/v1/admin/tenants/{id}", tag = "admin",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = TenantRow), (status = 401), (status = 404)))]
pub async fn get_tenant(
    State(pool): State<PgPool>,
    Operator(op): Operator,
    Path(id): Path<Uuid>,
) -> Result<Json<TenantRow>, AppError> {
    tenants::get(&pool, &op, id).await.map(Json)
}

#[utoipa::path(post, path = "/api/v1/admin/tenants/{id}/suspend", tag = "admin",
    params(("id" = Uuid, Path)), request_body = Reason,
    responses((status = 200, body = StatusChanged), (status = 401),
        (status = 403, description = "platform.step_up_required"), (status = 404),
        (status = 409, description = "tenant.status_unchanged"), (status = 422)))]
pub async fn suspend_tenant(
    State(pool): State<PgPool>,
    Operator(op): Operator,
    Path(id): Path<Uuid>,
    Json(body): Json<Reason>,
) -> Result<Json<StatusChanged>, AppError> {
    tenants::set_status(&pool, &op, id, TenantStatus::Suspended, &body)
        .await
        .map(Json)
}

#[utoipa::path(post, path = "/api/v1/admin/tenants/{id}/unsuspend", tag = "admin",
    params(("id" = Uuid, Path)), request_body = Reason,
    responses((status = 200, body = StatusChanged), (status = 401),
        (status = 403, description = "platform.step_up_required"), (status = 404),
        (status = 409, description = "tenant.status_unchanged"), (status = 422)))]
pub async fn unsuspend_tenant(
    State(pool): State<PgPool>,
    Operator(op): Operator,
    Path(id): Path<Uuid>,
    Json(body): Json<Reason>,
) -> Result<Json<StatusChanged>, AppError> {
    tenants::set_status(&pool, &op, id, TenantStatus::Active, &body)
        .await
        .map(Json)
}

#[utoipa::path(get, path = "/api/v1/admin/audit", tag = "admin", params(AuditQuery),
    responses((status = 200, body = PlatformAuditPage), (status = 401), (status = 422)))]
pub async fn list_audit(
    State(pool): State<PgPool>,
    Operator(_op): Operator,
    Query(query): Query<AuditQuery>,
) -> Result<Json<PlatformAuditPage>, AppError> {
    audit::list(&pool, &query).await.map(Json)
}
