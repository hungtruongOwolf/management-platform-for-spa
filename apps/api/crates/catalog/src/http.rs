//! Catalog read endpoints (US1). Contract documented with utoipa → openapi.json → TS client.

use crate::domain::ServicePrices;
use crate::repo::{CategoryRow, PlanRow, ServiceRow};
use crate::service;
use axum::extract::{FromRef, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use identity::AuthUser;
use kernel::AppError;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

#[derive(Debug, Serialize, ToSchema)]
pub struct ServiceCategoryDto {
    pub id: Uuid,
    pub name: String,
    pub sort_order: i32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ServiceDto {
    pub id: Uuid,
    pub category_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    /// Minutes; null means "theo tư vấn".
    pub duration_min: Option<i32>,
    pub recommended: bool,
    pub active: bool,
    pub prices: ServicePrices,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PackagePlanItemDto {
    pub service_id: Uuid,
    pub sessions: i32,
    pub bonus_sessions: i32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PackagePlanDto {
    pub id: Uuid,
    pub name: String,
    /// đồng
    pub price: i64,
    pub validity_months: Option<i32>,
    pub gift_note: Option<String>,
    pub active: bool,
    pub items: Vec<PackagePlanItemDto>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
pub struct ActiveFilter {
    /// Only items currently on sale (default true).
    pub active: Option<bool>,
}

impl From<CategoryRow> for ServiceCategoryDto {
    fn from(r: CategoryRow) -> Self {
        Self {
            id: r.id,
            name: r.name,
            sort_order: r.sort_order,
        }
    }
}

impl From<ServiceRow> for ServiceDto {
    fn from(r: ServiceRow) -> Self {
        Self {
            id: r.id,
            category_id: r.category_id,
            name: r.name,
            description: r.description,
            duration_min: r.duration_min,
            recommended: r.recommended,
            active: r.active,
            prices: r.prices,
        }
    }
}

impl From<PlanRow> for PackagePlanDto {
    fn from(p: PlanRow) -> Self {
        Self {
            id: p.id,
            name: p.name,
            price: p.price.as_i64(),
            validity_months: p.validity_months,
            gift_note: p.gift_note,
            active: p.active,
            items: p
                .items
                .into_iter()
                .map(|i| PackagePlanItemDto {
                    service_id: i.service_id,
                    sessions: i.sessions,
                    bonus_sessions: i.bonus_sessions,
                })
                .collect(),
        }
    }
}

pub fn routes<S>() -> Router<S>
where
    PgPool: FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/service-categories", get(list_categories))
        .route("/services", get(list_services))
        .route("/package-plans", get(list_package_plans))
}

#[utoipa::path(get, path = "/api/v1/service-categories", tag = "catalog",
    responses((status = 200, body = [ServiceCategoryDto]), (status = 401), (status = 403)))]
pub async fn list_categories(
    State(pool): State<PgPool>,
    user: AuthUser,
) -> Result<Json<Vec<ServiceCategoryDto>>, AppError> {
    let rows = service::categories(&pool, &user.actor).await?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

#[utoipa::path(get, path = "/api/v1/services", tag = "catalog", params(ActiveFilter),
    responses((status = 200, body = [ServiceDto]), (status = 401), (status = 403)))]
pub async fn list_services(
    State(pool): State<PgPool>,
    user: AuthUser,
    Query(filter): Query<ActiveFilter>,
) -> Result<Json<Vec<ServiceDto>>, AppError> {
    let rows = service::services(&pool, &user.actor, filter.active.unwrap_or(true)).await?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

#[utoipa::path(get, path = "/api/v1/package-plans", tag = "catalog", params(ActiveFilter),
    responses((status = 200, body = [PackagePlanDto]), (status = 401), (status = 403)))]
pub async fn list_package_plans(
    State(pool): State<PgPool>,
    user: AuthUser,
    Query(filter): Query<ActiveFilter>,
) -> Result<Json<Vec<PackagePlanDto>>, AppError> {
    let rows = service::package_plans(&pool, &user.actor, filter.active.unwrap_or(true)).await?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}
