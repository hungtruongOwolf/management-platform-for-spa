//! Catalog read endpoints (US1). Contract documented with utoipa → openapi.json → TS client.

use crate::commands::{self, PlanInput, PlanItemInput, ServiceInput};
use crate::domain::ServicePrices;
use crate::repo::{CategoryRow, PlanRow, ServiceRow};
use crate::service;
use axum::extract::{FromRef, Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, put};
use axum::{Json, Router};
use identity::AuthUser;
use kernel::http::{IfMatch, etag};
use kernel::{AppError, FieldError, Money};
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
    /// Send back as `If-Match` when updating.
    pub version: i64,
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
    /// Send back as `If-Match` when updating.
    pub version: i64,
    pub items: Vec<PackagePlanItemDto>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
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
            version: r.version,
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
            version: p.version,
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
        .route(
            "/service-categories",
            get(list_categories).post(create_category),
        )
        .route(
            "/service-categories/{id}",
            put(update_category).delete(delete_category),
        )
        .route("/services", get(list_services).post(create_service))
        .route("/services/{id}", put(update_service))
        .route("/package-plans", get(list_package_plans).post(create_plan))
        .route("/package-plans/{id}", put(update_plan))
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

// ---------------------------------------------------------------- writes (owner: catalog.manage)

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CategoryCreate {
    pub name: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CategoryUpdate {
    pub name: String,
    pub sort_order: i32,
}

/// Prices in đồng: either `all`, or `female` and/or `male` (a missing gender = not offered).
#[derive(Debug, Default, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PricesInput {
    pub all: Option<i64>,
    pub female: Option<i64>,
    pub male: Option<i64>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ServiceWrite {
    pub category_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    /// Minutes; null = "theo tư vấn".
    pub duration_min: Option<i32>,
    pub recommended: bool,
    pub active: bool,
    pub prices: PricesInput,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanItemWrite {
    pub service_id: Uuid,
    pub sessions: i32,
    pub bonus_sessions: i32,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanWrite {
    pub name: String,
    /// đồng
    pub price: i64,
    pub validity_months: Option<i32>,
    pub gift_note: Option<String>,
    pub active: bool,
    pub items: Vec<PlanItemWrite>,
}

fn money(field: &'static str, amount: i64, errors: &mut Vec<FieldError>) -> Option<Money> {
    if let Ok(m) = Money::vnd(amount) {
        Some(m)
    } else {
        errors.push(FieldError::new(field, "negative"));
        None
    }
}

impl ServiceWrite {
    fn into_input(self) -> Result<ServiceInput, AppError> {
        let mut errors = Vec::new();
        let prices = ServicePrices {
            all: self
                .prices
                .all
                .and_then(|a| money("prices.all", a, &mut errors)),
            female: self
                .prices
                .female
                .and_then(|a| money("prices.female", a, &mut errors)),
            male: self
                .prices
                .male
                .and_then(|a| money("prices.male", a, &mut errors)),
        };
        if !errors.is_empty() {
            return Err(AppError::Validation(errors));
        }
        Ok(ServiceInput {
            category_id: self.category_id,
            name: self.name,
            description: self.description,
            duration_min: self.duration_min,
            recommended: self.recommended,
            active: self.active,
            prices,
        })
    }
}

impl PlanWrite {
    fn into_input(self) -> Result<PlanInput, AppError> {
        let mut errors = Vec::new();
        let Some(price) = money("price", self.price, &mut errors) else {
            return Err(AppError::Validation(errors));
        };
        Ok(PlanInput {
            name: self.name,
            price,
            validity_months: self.validity_months,
            gift_note: self.gift_note,
            active: self.active,
            items: self
                .items
                .into_iter()
                .map(|i| PlanItemInput {
                    service_id: i.service_id,
                    sessions: i.sessions,
                    bonus_sessions: i.bonus_sessions,
                })
                .collect(),
        })
    }
}

fn versioned<T: Serialize>(status: StatusCode, version: i64, body: T) -> Response {
    let mut response = (status, Json(body)).into_response();
    response.headers_mut().insert(header::ETAG, etag(version));
    response
}

#[utoipa::path(post, path = "/api/v1/service-categories", tag = "catalog", request_body = CategoryCreate,
    responses((status = 201, body = ServiceCategoryDto), (status = 403), (status = 422)))]
pub async fn create_category(
    State(pool): State<PgPool>,
    user: AuthUser,
    Json(body): Json<CategoryCreate>,
) -> Result<Response, AppError> {
    let row = commands::create_category(&pool, &user.actor, &body.name).await?;
    Ok((StatusCode::CREATED, Json(ServiceCategoryDto::from(row))).into_response())
}

#[utoipa::path(put, path = "/api/v1/service-categories/{id}", tag = "catalog", request_body = CategoryUpdate,
    params(("id" = Uuid, Path)), responses((status = 200, body = ServiceCategoryDto), (status = 403), (status = 404), (status = 422)))]
pub async fn update_category(
    State(pool): State<PgPool>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<CategoryUpdate>,
) -> Result<Json<ServiceCategoryDto>, AppError> {
    let row =
        commands::update_category(&pool, &user.actor, id, &body.name, body.sort_order).await?;
    Ok(Json(row.into()))
}

/// Deletes a group with no service in it (on sale or not); its name stays in the trail.
#[utoipa::path(delete, path = "/api/v1/service-categories/{id}", tag = "catalog",
    params(("id" = Uuid, Path)),
    responses((status = 204), (status = 401), (status = 403), (status = 404),
              (status = 409, description = "catalog.category_in_use: the group still has services (on sale or not)")))]
pub async fn delete_category(
    State(pool): State<PgPool>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    commands::delete_category(&pool, &user.actor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(post, path = "/api/v1/services", tag = "catalog", request_body = ServiceWrite,
    responses((status = 201, body = ServiceDto), (status = 403), (status = 422)))]
pub async fn create_service(
    State(pool): State<PgPool>,
    user: AuthUser,
    Json(body): Json<ServiceWrite>,
) -> Result<Response, AppError> {
    let row = commands::create_service(&pool, &user.actor, body.into_input()?).await?;
    Ok(versioned(
        StatusCode::CREATED,
        row.version,
        ServiceDto::from(row),
    ))
}

#[utoipa::path(put, path = "/api/v1/services/{id}", tag = "catalog", request_body = ServiceWrite,
    params(("id" = Uuid, Path), ("If-Match" = String, Header, description = "version from the last read")),
    responses((status = 200, body = ServiceDto), (status = 403), (status = 404), (status = 412, description = "conflict.stale_version"), (status = 422)))]
pub async fn update_service(
    State(pool): State<PgPool>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    IfMatch(version): IfMatch,
    Json(body): Json<ServiceWrite>,
) -> Result<Response, AppError> {
    let row = commands::update_service(&pool, &user.actor, id, version, body.into_input()?).await?;
    Ok(versioned(
        StatusCode::OK,
        row.version,
        ServiceDto::from(row),
    ))
}

#[utoipa::path(post, path = "/api/v1/package-plans", tag = "catalog", request_body = PlanWrite,
    responses((status = 201, body = PackagePlanDto), (status = 403), (status = 422)))]
pub async fn create_plan(
    State(pool): State<PgPool>,
    user: AuthUser,
    Json(body): Json<PlanWrite>,
) -> Result<Response, AppError> {
    let row = commands::create_plan(&pool, &user.actor, body.into_input()?).await?;
    Ok(versioned(
        StatusCode::CREATED,
        row.version,
        PackagePlanDto::from(row),
    ))
}

#[utoipa::path(put, path = "/api/v1/package-plans/{id}", tag = "catalog", request_body = PlanWrite,
    params(("id" = Uuid, Path), ("If-Match" = String, Header, description = "version from the last read")),
    responses((status = 200, body = PackagePlanDto), (status = 403), (status = 404), (status = 412), (status = 422)))]
pub async fn update_plan(
    State(pool): State<PgPool>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    IfMatch(version): IfMatch,
    Json(body): Json<PlanWrite>,
) -> Result<Response, AppError> {
    let row = commands::update_plan(&pool, &user.actor, id, version, body.into_input()?).await?;
    Ok(versioned(
        StatusCode::OK,
        row.version,
        PackagePlanDto::from(row),
    ))
}
