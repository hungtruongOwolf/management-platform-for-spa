//! `GET /sales?date=`, `POST /sales`, `POST /sales/checkout`, `GET /sales/{id}`,
//! `PATCH /sales/{id}` (the note, F4), `POST /sales/{id}/void`.

use crate::dto::{
    DaySaleDto, SaleCheckoutCreate, SaleCheckoutDto, SaleCreate, SaleDto, SaleListQuery,
    SaleUpdate, VoidCreate,
};
use crate::{paper, service, voids};
use axum::body::Bytes;
use axum::extract::rejection::QueryRejection;
use axum::extract::{FromRef, Path, Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use crm::FieldCipher;
use identity::AuthUser;
use kernel::http::{IdempotencyKey, IfMatch, etag, invalid_query};
use kernel::idempotency::hash_route;
use kernel::{AppError, FieldError, Permission};
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
        .route("/sales", get(list_sales).post(create_sale))
        .route("/sales/checkout", post(create_checkout))
        .route("/sales/{id}", get(read_sale).patch(update_sale))
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
        (status = 422, description = "validation_failed (note too_long…), payment.amount_mismatch, discount.reason_required, staff.inactive, idempotency.key_reuse_mismatch")),
    description = "A `note` (\"Ghi chú gói\", health data) needs `customer.health_notes.write` (403 without it); it is encrypted at rest and never in this response or its replay.")]
pub async fn create_sale(
    State(pool): State<PgPool>,
    State(cipher): State<Arc<FieldCipher>>,
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
        &cipher,
        &user.actor,
        key,
        hash_route("POST", "/sales", None, &canonical),
        &input,
    )
    .await?;
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::CREATED);
    Ok(no_store(status, body))
}

#[utoipa::path(post, path = "/api/v1/sales/checkout", tag = "sales", request_body = SaleCheckoutCreate,
    params(("Idempotency-Key" = Uuid, Header, description = "One key per checkout attempt; a retry with the same key replays the first response")),
    responses(
        (status = 201, body = SaleCheckoutDto, description = "one sale per package, in request order; every one or none was created"),
        (status = 400, description = "idempotency.key_required / key_invalid"),
        (status = 401), (status = 403, description = "auth.forbidden / discount.limit_exceeded"),
        (status = 404, description = "customer, beneficiary or plan not in this spa"),
        (status = 422, description = "validation_failed (package_sales[k].…, package_sales count_out_of_range), payment.amount_mismatch, discount.reason_required, customer.name_required, staff.inactive, idempotency.key_reuse_mismatch")),
    description = "Each package may carry its own `note` (\"Ghi chú gói\"), which needs `customer.health_notes.write`; notes are encrypted at rest and never in this response or its replay.")]
pub async fn create_checkout(
    State(pool): State<PgPool>,
    State(cipher): State<Arc<FieldCipher>>,
    user: AuthUser,
    IdempotencyKey(key): IdempotencyKey,
    body: Bytes,
) -> Result<Response, AppError> {
    let value: Value = serde_json::from_slice(&body).map_err(|_| invalid_body())?;
    let canonical = serde_json::to_vec(&value).map_err(|e| AppError::Internal(e.to_string()))?;
    let input: SaleCheckoutCreate = serde_json::from_value(value).map_err(|_| invalid_body())?;
    let (status, body) = service::create_checkout(
        &pool,
        &cipher,
        &user.actor,
        key,
        hash_route("POST", "/sales/checkout", None, &canonical),
        &input,
    )
    .await?;
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::CREATED);
    Ok(no_store(status, body))
}

#[utoipa::path(get, path = "/api/v1/sales", tag = "sales", params(SaleListQuery),
    responses(
        (status = 200, body = [DaySaleDto], description = "Package sales of the day made without a visit (Quầy's day list), newest first; `sale.note` only for customer.health_notes.read"),
        (status = 400, description = "query.invalid"), (status = 401),
        (status = 403, description = "auth.forbidden: needs sale.read + customer.read")))]
pub async fn list_sales(
    State(pool): State<PgPool>,
    State(cipher): State<Arc<FieldCipher>>,
    user: AuthUser,
    query: Result<Query<SaleListQuery>, QueryRejection>,
) -> Result<Response, AppError> {
    // The rights first, then the query (F17).
    user.actor.require(Permission::SaleRead)?;
    user.actor.require(Permission::CustomerRead)?;
    let Query(query) = query.map_err(invalid_query)?;
    let sales = service::for_day(&pool, &cipher, &user.actor, query.date).await?;
    Ok(no_store(StatusCode::OK, sales))
}

#[utoipa::path(get, path = "/api/v1/sales/{id}", tag = "sales", params(("id" = Uuid, Path)),
    responses((status = 200, body = SaleDto, description = "`note` only for customer.health_notes.read; `ETag` = `version`"),
        (status = 401), (status = 403), (status = 404)))]
pub async fn read_sale(
    State(pool): State<PgPool>,
    State(cipher): State<Arc<FieldCipher>>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let sale = service::get_sale(&pool, &cipher, &user.actor, id).await?;
    Ok(with_etag(sale))
}

/// `200` with the sale and its `ETag` (the `If-Match` of `PATCH /sales/{id}`).
fn with_etag(sale: SaleDto) -> Response {
    let version = sale.version;
    let mut response = no_store(StatusCode::OK, sale);
    response.headers_mut().insert(header::ETAG, etag(version));
    response
}

#[utoipa::path(patch, path = "/api/v1/sales/{id}", tag = "sales", request_body = SaleUpdate,
    params(("id" = Uuid, Path),
        ("If-Match" = String, Header, description = "The sale's `version` as read (`\"3\"`); required")),
    responses(
        (status = 200, body = SaleDto, description = "\"Sửa ghi chú gói\": the note replaced (null / blank = removed, absent = unchanged); audited without its text; `ETag` = the new `version`"),
        (status = 400, description = "precondition.if_match_required / if_match_invalid"),
        (status = 401),
        (status = 403, description = "auth.forbidden (needs package.sell + customer.health_notes.write) / day.locked (a day before today without record.void_closed_day)"),
        (status = 404),
        (status = 409, description = "sale.voided, sale.not_package (a visit's one-off sale: its note is the visit's)"),
        (status = 412, description = "conflict.stale_version: the sale changed since it was read"),
        (status = 422, description = "validation_failed: note too_long, body invalid")))]
pub async fn update_sale(
    State(pool): State<PgPool>,
    State(cipher): State<Arc<FieldCipher>>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    if_match: Result<IfMatch, AppError>,
    body: Bytes,
) -> Result<Response, AppError> {
    // The rights first, then the headers and the body (F17): the service answers 404 /
    // day.locked before it looks at what is wrong with either.
    let input: Option<SaleUpdate> = serde_json::from_slice(&body).ok();
    let version = if_match.map(|IfMatch(v)| v);
    let sale =
        service::update_note(&pool, &cipher, &user.actor, id, version, input.as_ref()).await?;
    Ok(with_etag(sale))
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
    // Authorize before saying what is wrong with the body (F17).
    user.actor.require(Permission::RecordVoidSameDay)?;
    let parsed = serde_json::from_slice::<Value>(&body)
        .map_err(|_| invalid_body())
        .and_then(|value| {
            let canonical =
                serde_json::to_vec(&value).map_err(|e| AppError::Internal(e.to_string()))?;
            let input: VoidCreate = serde_json::from_value(value).map_err(|_| invalid_body())?;
            Ok((canonical, input))
        });
    let (canonical, input) = match parsed {
        Ok(p) => p,
        Err(invalid) => return Err(voids::authorized_then(&pool, &user.actor, id, invalid).await),
    };
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
