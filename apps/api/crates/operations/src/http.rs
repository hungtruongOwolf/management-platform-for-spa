//! `POST /visits`, `GET /visits?date=`, `GET /visits/{id}`, `PATCH /visits/{id}`,
//! `POST /visits/{id}/void`, `POST /visits/{id}/correct`.

use crate::dto::{
    VisitAdditions, VisitConversion, VisitCorrected, VisitCorrection, VisitCreate, VisitDto,
    VisitListQuery, VisitUpdate,
};
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
use kernel::idempotency::hash_route;
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
        .route("/visits/{id}/correct", post(correct_visit))
        .route("/visits/{id}/additions", post(add_to_visit))
        .route("/visits/{id}/convert-to-package", post(convert_visit))
}

/// Body bytes → canonical JSON (for the idempotency hash) and the typed input.
fn read_json<T: serde::de::DeserializeOwned>(body: &Bytes) -> Result<(Vec<u8>, T), AppError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| invalid_body())?;
    let canonical = serde_json::to_vec(&value).map_err(|e| AppError::Internal(e.to_string()))?;
    let input = serde_json::from_value(value).map_err(|_| invalid_body())?;
    Ok((canonical, input))
}

#[utoipa::path(post, path = "/api/v1/visits/{id}/additions", tag = "visits", request_body = VisitAdditions,
    params(("id" = Uuid, Path),
        ("Idempotency-Key" = Uuid, Header, description = "One key per addition; a retry replays the first response")),
    responses(
        (status = 201, body = VisitDto, description = "Lines and sales added (US12): each new sale is linked to the visit, nothing existing changes"),
        (status = 400, description = "idempotency.key_required"),
        (status = 403, description = "auth.forbidden / day.locked (manager, a day before today) / discount.limit_exceeded"),
        (status = 404, description = "visit, service, plan or beneficiary not in this spa"),
        (status = 409, description = "visit.voided / visit.corrected, payroll.period_closed, package.*"),
        (status = 422, description = "validation_failed, payment.amount_mismatch and every rule of POST /visits")))]
pub async fn add_to_visit(
    State(pool): State<PgPool>,
    State(cipher): State<Arc<FieldCipher>>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    IdempotencyKey(key): IdempotencyKey,
    body: Bytes,
) -> Result<Response, AppError> {
    let (canonical, input): (_, VisitAdditions) = read_json(&body)?;
    let visit = input.into_visit();
    let (status, body) = service::add(
        &pool,
        &cipher,
        &user.actor,
        key,
        hash_route("POST", "/visits/{id}/additions", Some(id), &canonical),
        id,
        &visit,
    )
    .await?;
    let version = body["version"].as_i64();
    Ok(no_store(
        StatusCode::from_u16(status).unwrap_or(StatusCode::CREATED),
        body,
        version,
    ))
}

#[utoipa::path(post, path = "/api/v1/visits/{id}/convert-to-package", tag = "visits", request_body = VisitConversion,
    params(("id" = Uuid, Path), ("If-Match" = String, Header, description = "Version of the visit being converted"),
        ("Idempotency-Key" = Uuid, Header, description = "One key per conversion; a retry replays the first response")),
    responses(
        (status = 201, body = VisitCorrected, description = "Old visit voided (one-off payments reversed), package sold, visit recorded again with today's session from the package (US11); `difference` = what the customer hands over now"),
        (status = 400, description = "idempotency.key_required / precondition.if_match_required"),
        (status = 403, description = "auth.forbidden / day.locked (manager, a day before today) / discount.limit_exceeded"),
        (status = 404, description = "visit or plan not in this spa"),
        (status = 409, description = "visit.voided / visit.corrected, payroll.period_closed, package.*"),
        (status = 412, description = "conflict.stale_version"),
        (status = 422, description = "visit.nothing_to_convert (no one-off line the package covers), validation_failed, payment.amount_mismatch")))]
pub async fn convert_visit(
    State(pool): State<PgPool>,
    State(cipher): State<Arc<FieldCipher>>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    IfMatch(version): IfMatch,
    IdempotencyKey(key): IdempotencyKey,
    body: Bytes,
) -> Result<Response, AppError> {
    let (canonical, input): (_, VisitConversion) = read_json(&body)?;
    let (status, body) = service::convert(
        &pool,
        &cipher,
        &user.actor,
        key,
        hash_route(
            "POST",
            "/visits/{id}/convert-to-package",
            Some(id),
            &canonical,
        ),
        service::Conversion {
            id,
            version,
            input: &input,
        },
    )
    .await?;
    let version = body["visit"]["version"].as_i64();
    Ok(no_store(
        StatusCode::from_u16(status).unwrap_or(StatusCode::CREATED),
        body,
        version,
    ))
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
        hash_route("POST", "/visits", None, &canonical),
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
    let limit = match query.limit {
        None => None,
        Some(n) if (1..=crate::dto::VISIT_LIST_MAX_LIMIT).contains(&n) => Some(n as usize),
        Some(_) => {
            return Err(AppError::Validation(vec![FieldError::new(
                "limit",
                "out_of_range",
            )]));
        }
    };
    let mut visits = service::for_day(&pool, &cipher, &user.actor, query.date).await?;
    // Newest first (repo orders by check_in_at DESC), so truncating keeps the latest visits.
    if let Some(n) = limit {
        visits.truncate(n);
    }
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
        (status = 403, description = "auth.forbidden / day.locked"), (status = 404),
        (status = 409, description = "visit.voided / visit.corrected (open the newest version)"),
        (status = 412),
        (status = 422, description = "validation_failed (check_in_at not_visit_day), visit.check_in_in_future, staff.inactive")))]
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
        hash_route("POST", "/visits/{id}/void", Some(id), &canonical),
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

#[utoipa::path(post, path = "/api/v1/visits/{id}/correct", tag = "visits", request_body = VisitCorrection,
    params(("id" = Uuid, Path), ("If-Match" = String, Header, description = "Version of the visit being corrected"),
        ("Idempotency-Key" = Uuid, Header, description = "One key per correction; a retry replays the first response")),
    responses(
        (status = 201, body = VisitCorrected, description = "Old visit voided (sessions back, payments reversed) and the corrected visit recorded, in one transaction (US6b)"),
        (status = 400, description = "idempotency.key_required / precondition.if_match_required"),
        (status = 403, description = "auth.forbidden / day.locked (manager, a day before today)"),
        (status = 404, description = "visit, customer or service not in this spa"),
        (status = 409, description = "visit.voided / visit.corrected (already corrected: open the newest version), package.*"),
        (status = 412, description = "conflict.stale_version"),
        (status = 422, description = "validation_failed (reason required / too_long, check_in_at not_visit_day), payment.amount_mismatch and every rule of POST /visits")))]
pub async fn correct_visit(
    State(pool): State<PgPool>,
    State(cipher): State<Arc<FieldCipher>>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    IfMatch(version): IfMatch,
    IdempotencyKey(key): IdempotencyKey,
    body: Bytes,
) -> Result<Response, AppError> {
    let value: Value = serde_json::from_slice(&body).map_err(|_| invalid_body())?;
    let canonical = serde_json::to_vec(&value).map_err(|e| AppError::Internal(e.to_string()))?;
    let input: VisitCorrection = serde_json::from_value(value).map_err(|_| invalid_body())?;
    let (reason, note, visit) = input.into_parts();
    let (status, body) = service::correct(
        &pool,
        &cipher,
        &user.actor,
        key,
        hash_route("POST", "/visits/{id}/correct", Some(id), &canonical),
        service::Correction {
            id,
            version,
            reason: reason.as_deref(),
            note,
            visit: &visit,
        },
    )
    .await?;
    let version = body["visit"]["version"].as_i64();
    Ok(no_store(
        StatusCode::from_u16(status).unwrap_or(StatusCode::CREATED),
        body,
        version,
    ))
}
