//! `POST /visits`, `GET /visits?date=`, `GET /visits/{id}`, `PATCH /visits/{id}`,
//! `POST /visits/{id}/void`.

use crate::dto::{VisitCreate, VisitDto, VisitListQuery, VisitUpdate};
use crate::service;
use axum::body::Bytes;
use axum::extract::{FromRef, Path, Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use identity::AuthUser;
use kernel::crypto::FieldCipher;
use kernel::http::{IdempotencyKey, IfMatch, etag};
use kernel::idempotency::{hash_request, hash_request_on};
use kernel::{AppError, FieldError};
use sales::dto::VoidCreate;
use serde_json::Value;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

pub fn routes<S>() -> Router<S>
where
    PgPool: FromRef<S>,
    Arc<FieldCipher>: FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/visits", get(list_visits).post(create_visit))
        .route("/visits/{id}", get(read_visit).patch(update_visit))
        .route("/visits/{id}/void", post(void_visit))
}

fn invalid_body() -> AppError {
    AppError::Validation(vec![FieldError::new("body", "invalid")])
}

fn no_store(status: StatusCode, body: impl serde::Serialize, version: Option<i64>) -> Response {
    let mut response = (status, Json(body)).into_response();
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if let Some(v) = version {
        headers.insert(header::ETAG, etag(v));
    }
    response
}

#[utoipa::path(post, path = "/api/v1/visits", tag = "visits", request_body = VisitCreate,
    params(("Idempotency-Key" = Uuid, Header, description = "One key per visit being recorded; a retry with the same key replays the first response")),
    responses(
        (status = 201, body = VisitDto),
        (status = 400, description = "idempotency.key_required / key_invalid"),
        (status = 401), (status = 403),
        (status = 404, description = "customer or service not in this spa"),
        (status = 409, description = "package.no_sessions_left / package.expired / package.cancelled"),
        (status = 422, description = "validation_failed, payment.amount_mismatch, staff.inactive, package.item_not_found, price.gender_required, price.variant_not_available, visit.check_in_in_future, visit.check_in_not_today, idempotency.key_reuse_mismatch")))]
pub async fn create_visit(
    State(pool): State<PgPool>,
    State(cipher): State<Arc<FieldCipher>>,
    user: AuthUser,
    IdempotencyKey(key): IdempotencyKey,
    body: Bytes,
) -> Result<Response, AppError> {
    let value: Value = serde_json::from_slice(&body).map_err(|_| invalid_body())?;
    // Hash the canonical JSON so whitespace or key order never turns a retry into a "different body".
    let canonical = serde_json::to_vec(&value).map_err(|e| AppError::Internal(e.to_string()))?;
    let input: VisitCreate = serde_json::from_value(value).map_err(|_| invalid_body())?;
    let (status, body) = service::record(
        &pool,
        &cipher,
        &user.actor,
        key,
        hash_request(&canonical),
        &input,
    )
    .await?;
    let version = body["version"].as_i64();
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::CREATED);
    Ok(no_store(status, body, version))
}

#[utoipa::path(get, path = "/api/v1/visits", tag = "visits", params(VisitListQuery),
    responses((status = 200, body = [VisitDto]), (status = 401), (status = 403)))]
pub async fn list_visits(
    State(pool): State<PgPool>,
    State(cipher): State<Arc<FieldCipher>>,
    user: AuthUser,
    Query(query): Query<VisitListQuery>,
) -> Result<Response, AppError> {
    let visits = service::for_day(&pool, &cipher, &user.actor, query.date).await?;
    Ok(no_store(StatusCode::OK, visits, None))
}

#[utoipa::path(get, path = "/api/v1/visits/{id}", tag = "visits", params(("id" = Uuid, Path)),
    responses((status = 200, body = VisitDto), (status = 401), (status = 403), (status = 404)))]
pub async fn read_visit(
    State(pool): State<PgPool>,
    State(cipher): State<Arc<FieldCipher>>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let visit = service::get(&pool, &cipher, &user.actor, id).await?;
    let version = visit.version;
    Ok(no_store(StatusCode::OK, visit, Some(version)))
}

#[utoipa::path(patch, path = "/api/v1/visits/{id}", tag = "visits", request_body = VisitUpdate,
    params(("id" = Uuid, Path), ("If-Match" = String, Header)),
    responses((status = 200, body = VisitDto),
        (status = 401, description = "auth.reauth_required (owner correcting a closed day)"),
        (status = 403, description = "auth.forbidden / day.locked"), (status = 404),
        (status = 409, description = "visit.voided"), (status = 412), (status = 422)))]
pub async fn update_visit(
    State(pool): State<PgPool>,
    State(cipher): State<Arc<FieldCipher>>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    IfMatch(version): IfMatch,
    Json(input): Json<VisitUpdate>,
) -> Result<Response, AppError> {
    let visit = service::update(&pool, &cipher, &user.actor, id, version, &input).await?;
    let version = visit.version;
    Ok(no_store(StatusCode::OK, visit, Some(version)))
}

#[utoipa::path(post, path = "/api/v1/visits/{id}/void", tag = "visits", request_body = VoidCreate,
    params(("id" = Uuid, Path),
        ("Idempotency-Key" = Uuid, Header, description = "One key per void attempt; a retry replays the first response")),
    responses(
        (status = 200, body = VisitDto, description = "Voided: sessions given back, one-off payments reversed (AC6.1)"),
        (status = 400, description = "idempotency.key_required"),
        (status = 401, description = "auth.reauth_required (owner voiding a closed day)"),
        (status = 403, description = "auth.forbidden / day.locked (manager, a day before today, AC6.3)"),
        (status = 404),
        (status = 409, description = "visit.voided (second void), package.cancelled (its package was cancelled since)"),
        (status = 422, description = "validation_failed: reason required (AC6.2)")))]
pub async fn void_visit(
    State(pool): State<PgPool>,
    State(cipher): State<Arc<FieldCipher>>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    IdempotencyKey(key): IdempotencyKey,
    body: Bytes,
) -> Result<Response, AppError> {
    let value: Value = serde_json::from_slice(&body).map_err(|_| invalid_body())?;
    let canonical = serde_json::to_vec(&value).map_err(|e| AppError::Internal(e.to_string()))?;
    let input: VoidCreate = serde_json::from_value(value).map_err(|_| invalid_body())?;
    let (status, body) = service::void(
        &pool,
        &cipher,
        &user.actor,
        key,
        hash_request_on(id, &canonical),
        id,
        input.reason.as_deref(),
    )
    .await?;
    let version = body["version"].as_i64();
    Ok(no_store(
        StatusCode::from_u16(status).unwrap_or(StatusCode::OK),
        body,
        version,
    ))
}
