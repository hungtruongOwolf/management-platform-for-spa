//! Package endpoints: reads for the customer profile, and the opening-balance import (S9).

use crate::domain::{PackageStatus, RefundMethod};
use crate::lifecycle::{self, PackageCancel, PackageExtend};
use crate::opening_balance::{self, OpeningBalanceCreate, OpeningBalanceDto};
use crate::service::{self, CustomerPackage, PackageItem, Refund};
use axum::body::Bytes;
use axum::extract::{FromRef, Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, NaiveDate, Utc};
use crm::FieldCipher;
use identity::AuthUser;
use kernel::http::{IdempotencyKey, IfMatch, etag};
use kernel::idempotency::{hash_request, hash_request_on};
use kernel::{AppError, FieldError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use std::sync::Arc;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PackageStatusDto {
    Active,
    Expired,
    Exhausted,
    Cancelled,
}

impl From<PackageStatus> for PackageStatusDto {
    fn from(s: PackageStatus) -> Self {
        match s {
            PackageStatus::Active => Self::Active,
            PackageStatus::Expired => Self::Expired,
            PackageStatus::Exhausted => Self::Exhausted,
            PackageStatus::Cancelled => Self::Cancelled,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PackageOriginDto {
    Sale,
    OpeningBalance,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CustomerPackageItemDto {
    pub id: Uuid,
    pub service_id: Uuid,
    /// Service name when sold (snapshot).
    pub name: String,
    pub sessions_purchased: i32,
    pub sessions_bonus: i32,
    pub sessions_total: i32,
    pub sessions_remaining: i32,
    /// Value of one purchased session in đồng (bonus sessions are worth 0).
    pub per_session_value: i64,
}

impl From<PackageItem> for CustomerPackageItemDto {
    fn from(i: PackageItem) -> Self {
        Self {
            id: i.id,
            service_id: i.service_id,
            name: i.name,
            sessions_purchased: i.sessions_purchased,
            sessions_bonus: i.sessions_bonus,
            sessions_total: i.sessions_total,
            sessions_remaining: i.sessions_remaining,
            per_session_value: i.per_session_value.as_i64(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RefundMethodDto {
    Cash,
    BankTransfer,
}

impl From<RefundMethodDto> for RefundMethod {
    fn from(m: RefundMethodDto) -> Self {
        match m {
            RefundMethodDto::Cash => Self::Cash,
            RefundMethodDto::BankTransfer => Self::BankTransfer,
        }
    }
}

/// Money paid back when the package was cancelled (US7).
#[derive(Debug, Serialize, ToSchema)]
pub struct RefundDto {
    pub id: Uuid,
    /// đồng.
    pub amount: i64,
    pub method: RefundMethodDto,
    /// Cash-book day of the refund.
    pub business_date: NaiveDate,
    pub created_at: DateTime<Utc>,
}

impl From<Refund> for RefundDto {
    fn from(r: Refund) -> Self {
        Self {
            id: r.id,
            amount: r.amount.as_i64(),
            method: match r.method {
                RefundMethod::Cash => RefundMethodDto::Cash,
                RefundMethod::BankTransfer => RefundMethodDto::BankTransfer,
            },
            business_date: r.business_date,
            created_at: r.created_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CustomerPackageDto {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub plan_id: Option<Uuid>,
    /// Plan name when sold (snapshot).
    pub name: String,
    pub gift_note: Option<String>,
    /// Price paid after discount, in đồng.
    pub sold_price: i64,
    pub origin: PackageOriginDto,
    pub sale_id: Option<Uuid>,
    pub closer_staff_id: Option<Uuid>,
    pub closer_name: Option<String>,
    pub starts_on: NaiveDate,
    pub expires_on: Option<NaiveDate>,
    /// Set when the package was cancelled (a voided sale or a cancellation with refund).
    pub cancelled_at: Option<DateTime<Utc>>,
    pub cancel_reason: Option<String>,
    pub refund: Option<RefundDto>,
    /// Derived on read: cancelled → exhausted → expired → active.
    pub status: PackageStatusDto,
    pub sessions_total: i64,
    pub sessions_remaining: i64,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub items: Vec<CustomerPackageItemDto>,
}

impl From<CustomerPackage> for CustomerPackageDto {
    fn from(p: CustomerPackage) -> Self {
        Self {
            id: p.id,
            customer_id: p.customer_id,
            plan_id: p.plan_id,
            name: p.name,
            gift_note: p.gift_note,
            sold_price: p.sold_price.as_i64(),
            origin: if p.origin == "opening_balance" {
                PackageOriginDto::OpeningBalance
            } else {
                PackageOriginDto::Sale
            },
            sale_id: p.sale_id,
            closer_staff_id: p.closer_staff_id,
            closer_name: p.closer_name,
            starts_on: p.starts_on,
            expires_on: p.expires_on,
            cancelled_at: p.cancelled_at,
            cancel_reason: p.cancel_reason,
            refund: p.refund.map(Into::into),
            status: p.status.into(),
            sessions_total: p.sessions_total,
            sessions_remaining: p.sessions_remaining,
            version: p.version,
            created_at: p.created_at,
            items: p.items.into_iter().map(Into::into).collect(),
        }
    }
}

pub fn routes<S>() -> Router<S>
where
    PgPool: FromRef<S>,
    Arc<FieldCipher>: FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/customer-packages/{id}", get(read_package))
        .route("/customer-packages/{id}/extend", post(extend_package))
        .route("/customer-packages/{id}/cancel", post(cancel_package))
        .route("/customers/{id}/packages", get(list_customer_packages))
        .route("/opening-balances", post(import_opening_balance))
}

fn no_store(body: impl Serialize) -> Response {
    no_store_status(StatusCode::OK, body)
}

fn no_store_status(status: StatusCode, body: impl Serialize) -> Response {
    let mut response = (status, Json(body)).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

#[utoipa::path(get, path = "/api/v1/customer-packages/{id}", tag = "packages", params(("id" = Uuid, Path)),
    responses((status = 200, body = CustomerPackageDto), (status = 401), (status = 403), (status = 404)))]
pub async fn read_package(
    State(pool): State<PgPool>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let package = service::get(&pool, &user.actor, id).await?;
    Ok(no_store(CustomerPackageDto::from(package)))
}

#[utoipa::path(get, path = "/api/v1/customers/{id}/packages", tag = "packages", params(("id" = Uuid, Path)),
    responses((status = 200, body = [CustomerPackageDto]), (status = 401), (status = 403), (status = 404)))]
pub async fn list_customer_packages(
    State(pool): State<PgPool>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let packages = service::for_customer(&pool, &user.actor, id).await?;
    Ok(no_store(
        packages
            .into_iter()
            .map(CustomerPackageDto::from)
            .collect::<Vec<_>>(),
    ))
}

#[utoipa::path(post, path = "/api/v1/opening-balances", tag = "packages", request_body = OpeningBalanceCreate,
    params(("Idempotency-Key" = Uuid, Header, description = "One key per import form; a retry with the same key replays the first response")),
    responses(
        (status = 201, body = OpeningBalanceDto, description = "Packages created with origin opening_balance; no sale, no payment"),
        (status = 400, description = "idempotency.key_required / key_invalid"),
        (status = 401), (status = 403),
        (status = 404, description = "customer or plan not in this spa"),
        (status = 409, description = "customer.phone_taken with existing_id"),
        (status = 422, description = "validation_failed (fields prefixed customer. / packages[j].), idempotency.key_reuse_mismatch")))]
pub async fn import_opening_balance(
    State(pool): State<PgPool>,
    State(cipher): State<Arc<FieldCipher>>,
    user: AuthUser,
    IdempotencyKey(key): IdempotencyKey,
    body: Bytes,
) -> Result<Response, AppError> {
    let invalid = || AppError::Validation(vec![FieldError::new("body", "invalid")]);
    let value: Value = serde_json::from_slice(&body).map_err(|_| invalid())?;
    // Hash the parsed JSON re-serialized (sorted keys), as `POST /sales` does.
    let canonical = serde_json::to_vec(&value).map_err(|e| AppError::Internal(e.to_string()))?;
    let input: OpeningBalanceCreate = serde_json::from_value(value).map_err(|_| invalid())?;
    let (status, body) = opening_balance::import(
        &pool,
        &cipher,
        &user.actor,
        key,
        hash_request(&canonical),
        &input,
    )
    .await?;
    Ok(no_store_status(
        StatusCode::from_u16(status).unwrap_or(StatusCode::CREATED),
        body,
    ))
}

/// Parses a JSON body and hashes its canonical form (sorted keys) together with the package id,
/// so a genuine retry replays and the same key on another package is a mismatch.
fn canonical<T: serde::de::DeserializeOwned>(
    id: Uuid,
    body: &[u8],
) -> Result<(T, [u8; 32]), AppError> {
    let invalid = || AppError::Validation(vec![FieldError::new("body", "invalid")]);
    let value: Value = serde_json::from_slice(body).map_err(|_| invalid())?;
    let bytes = serde_json::to_vec(&value).map_err(|e| AppError::Internal(e.to_string()))?;
    let input = serde_json::from_value(value).map_err(|_| invalid())?;
    Ok((input, hash_request_on(id, &bytes)))
}

fn with_etag(body: Value) -> Response {
    let version = body["version"].as_i64();
    let mut response = no_store(body);
    if let Some(v) = version {
        response.headers_mut().insert(header::ETAG, etag(v));
    }
    response
}

#[utoipa::path(post, path = "/api/v1/customer-packages/{id}/extend", tag = "packages", request_body = PackageExtend,
    params(("id" = Uuid, Path), ("If-Match" = String, Header),
        ("Idempotency-Key" = Uuid, Header, description = "One key per extension; a retry replays the first response")),
    responses(
        (status = 200, body = CustomerPackageDto, description = "New expiry, audited with the reason (AC7.1)"),
        (status = 400, description = "idempotency.key_required, precondition.if_match_required"),
        (status = 401), (status = 403, description = "auth.forbidden (managers, AC7.3)"), (status = 404),
        (status = 409, description = "package.cancelled"), (status = 412, description = "conflict.stale_version"),
        (status = 422, description = "validation_failed: reason required, expires_on not_later / in_past / package_has_no_expiry")))]
pub async fn extend_package(
    State(pool): State<PgPool>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    IfMatch(version): IfMatch,
    IdempotencyKey(key): IdempotencyKey,
    body: Bytes,
) -> Result<Response, AppError> {
    let (input, hash): (PackageExtend, _) = canonical(id, &body)?;
    let (_, body) = lifecycle::extend(&pool, &user.actor, key, hash, id, version, &input).await?;
    Ok(with_etag(body))
}

#[utoipa::path(post, path = "/api/v1/customer-packages/{id}/cancel", tag = "packages", request_body = PackageCancel,
    params(("id" = Uuid, Path),
        ("Idempotency-Key" = Uuid, Header, description = "One key per cancellation; a retry replays the first response")),
    responses(
        (status = 200, body = CustomerPackageDto, description = "Cancelled, sessions → 0, refund recorded (AC7.2)"),
        (status = 400, description = "idempotency.key_required"),
        (status = 401, description = "auth.reauth_required: confirm the password (and code) via /auth/reauth first"),
        (status = 403, description = "auth.forbidden (managers, AC7.3)"), (status = 404),
        (status = 409, description = "package.cancelled"),
        (status = 422, description = "refund.exceeds_paid (AC7.4), validation_failed: reason / refund_method")))]
pub async fn cancel_package(
    State(pool): State<PgPool>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    IdempotencyKey(key): IdempotencyKey,
    body: Bytes,
) -> Result<Response, AppError> {
    let (input, hash): (PackageCancel, _) = canonical(id, &body)?;
    let (_, body) = lifecycle::cancel(&pool, &user.actor, key, hash, id, &input).await?;
    Ok(with_etag(body))
}
