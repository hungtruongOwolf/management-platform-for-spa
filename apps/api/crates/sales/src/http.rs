//! `POST /sales`, `GET /sales/{id}`.

use crate::dto::{SaleCreate, SaleDto};
use crate::service;
use axum::body::Bytes;
use axum::extract::{FromRef, Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use identity::AuthUser;
use kernel::http::IdempotencyKey;
use kernel::idempotency::hash_request;
use kernel::{AppError, FieldError};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

pub fn routes<S>() -> Router<S>
where
    PgPool: FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/sales", post(create_sale))
        .route("/sales/{id}", get(read_sale))
}

fn invalid_body() -> AppError {
    AppError::Validation(vec![FieldError::new("body", "invalid")])
}

fn no_store(status: StatusCode, body: impl serde::Serialize) -> Response {
    let mut response = (status, Json(body)).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

#[utoipa::path(post, path = "/api/v1/sales", tag = "sales", request_body = SaleCreate,
    params(("Idempotency-Key" = Uuid, Header, description = "One key per sale attempt; a retry with the same key replays the first response")),
    responses(
        (status = 201, body = SaleDto),
        (status = 400, description = "idempotency.key_required / key_invalid"),
        (status = 401), (status = 403, description = "auth.forbidden / discount.limit_exceeded"),
        (status = 404, description = "customer or plan not in this spa"),
        (status = 422, description = "validation_failed, payment.amount_mismatch, discount.reason_required, staff.inactive, idempotency.key_reuse_mismatch")))]
pub async fn create_sale(
    State(pool): State<PgPool>,
    user: AuthUser,
    IdempotencyKey(key): IdempotencyKey,
    body: Bytes,
) -> Result<Response, AppError> {
    let value: Value = serde_json::from_slice(&body).map_err(|_| invalid_body())?;
    // Hash the parsed JSON re-serialized (sorted keys), so whitespace or key order never turns a
    // genuine retry into a "different body".
    let canonical = serde_json::to_vec(&value).map_err(|e| AppError::Internal(e.to_string()))?;
    let input: SaleCreate = serde_json::from_value(value).map_err(|_| invalid_body())?;
    let (status, body) =
        service::create_sale(&pool, &user.actor, key, hash_request(&canonical), &input).await?;
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::CREATED);
    Ok(no_store(status, body))
}

#[utoipa::path(get, path = "/api/v1/sales/{id}", tag = "sales", params(("id" = Uuid, Path)),
    responses((status = 200, body = SaleDto), (status = 401), (status = 403), (status = 404)))]
pub async fn read_sale(
    State(pool): State<PgPool>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let sale = service::get_sale(&pool, &user.actor, id).await?;
    Ok(no_store(StatusCode::OK, sale))
}
