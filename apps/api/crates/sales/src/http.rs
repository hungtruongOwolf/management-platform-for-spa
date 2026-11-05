//! `POST /sales`, `GET /sales/{id}`, `POST /sales/{id}/void`.

use crate::dto::{SaleCreate, SaleDto, VoidCreate};
use crate::{paper, service, voids};
use axum::body::Bytes;
use axum::extract::{FromRef, Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use crm::FieldCipher;
use identity::AuthUser;
use kernel::http::IdempotencyKey;
use kernel::idempotency::hash_route;
use kernel::{AppError, FieldError};
use packages::opening_balance::{OpeningBalanceCreate, OpeningBalanceDto};
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
        .route("/sales", post(create_sale))
        .route("/sales/{id}", get(read_sale))
        .route("/sales/{id}/void", post(void_sale))
        // Lives here (not in `packages`) because it books the paper money as sales.
        .route("/opening-balances", post(import_opening_balance))
}

#[utoipa::path(post, path = "/api/v1/opening-balances", tag = "packages", request_body = OpeningBalanceCreate,
    params(("Idempotency-Key" = Uuid, Header, description = "One key per import form; a retry with the same key replays the first response")),
    responses(
        (status = 201, body = OpeningBalanceDto, description = "Paper-book packages; each `paid_amount` > 0 is booked as a sale + payment on its `paid_on` (revenue of that day). Owner-only; the team migrates with `app import-paper-book`"),
        (status = 400, description = "idempotency.key_required / key_invalid"),
        (status = 401), (status = 403),
        (status = 404, description = "customer or plan not in this spa"),
        (status = 409, description = "customer.phone_taken with existing_id"),
        (status = 422, description = "validation_failed (fields prefixed customer. / packages[j]., e.g. packages[0].paid_on required / in_future, packages[0].paid_amount negative / out_of_range), idempotency.key_reuse_mismatch")))]
pub async fn import_opening_balance(
    State(pool): State<PgPool>,
    State(cipher): State<Arc<FieldCipher>>,
    user: AuthUser,
    IdempotencyKey(key): IdempotencyKey,
    body: Bytes,
) -> Result<Response, AppError> {
    let value: Value = serde_json::from_slice(&body).map_err(|_| invalid_body())?;
    // Hash the parsed JSON re-serialized (sorted keys), as `POST /sales` does.
    let canonical = serde_json::to_vec(&value).map_err(|e| AppError::Internal(e.to_string()))?;
    let input: OpeningBalanceCreate = serde_json::from_value(value).map_err(|_| invalid_body())?;
    let (status, body) = paper::import_opening_balance(
        &pool,
        &cipher,
        &user.actor,
        key,
        hash_route("POST", "/opening-balances", None, &canonical),
        &input,
    )
    .await?;
    Ok(no_store(
        StatusCode::from_u16(status).unwrap_or(StatusCode::CREATED),
        body,
    ))
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
    let (status, body) = service::create_sale(
        &pool,
        &user.actor,
        key,
        hash_route("POST", "/sales", None, &canonical),
        &input,
    )
    .await?;
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

#[utoipa::path(post, path = "/api/v1/sales/{id}/void", tag = "sales", request_body = VoidCreate,
    params(("id" = Uuid, Path),
        ("Idempotency-Key" = Uuid, Header, description = "One key per void attempt; a retry replays the first response")),
    responses(
        (status = 200, body = SaleDto, description = "Voided: every payment has a reversal, its packages are cancelled"),
        (status = 400, description = "idempotency.key_required"),
        (status = 403, description = "auth.forbidden / day.locked (manager, a day before today)"),
        (status = 404),
        (status = 409, description = "sale.voided, sale.has_used_sessions (AC6.4), sale.void_via_visit, package.cancelled"),
        (status = 422, description = "validation_failed: reason required")))]
pub async fn void_sale(
    State(pool): State<PgPool>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    IdempotencyKey(key): IdempotencyKey,
    body: Bytes,
) -> Result<Response, AppError> {
    let value: Value = serde_json::from_slice(&body).map_err(|_| invalid_body())?;
    let canonical = serde_json::to_vec(&value).map_err(|e| AppError::Internal(e.to_string()))?;
    let input: VoidCreate = serde_json::from_value(value).map_err(|_| invalid_body())?;
    let (status, body) = voids::void_sale(
        &pool,
        &user.actor,
        key,
        hash_route("POST", "/sales/{id}/void", Some(id), &canonical),
        id,
        input.reason.as_deref(),
    )
    .await?;
    Ok(no_store(
        StatusCode::from_u16(status).unwrap_or(StatusCode::OK),
        body,
    ))
}
