//! Package read endpoints for the customer profile.

use crate::domain::PackageStatus;
use crate::service::{self, CustomerPackage, PackageItem};
use axum::extract::{FromRef, Path, State};
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, NaiveDate, Utc};
use identity::AuthUser;
use kernel::AppError;
use serde::Serialize;
use sqlx::PgPool;
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
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/customer-packages/{id}", get(read_package))
        .route("/customers/{id}/packages", get(list_customer_packages))
}

fn no_store(body: impl Serialize) -> Response {
    let mut response = Json(body).into_response();
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
