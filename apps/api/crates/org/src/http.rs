//! `GET /staff` — picker for closer / technician; staff profiles and pay terms (`staff.manage`).

use crate::profile::{self, Account, PayInput, PayTerms, ProfileInput, ProfileView};
use crate::service::{self, Staff, StaffStatus};
use axum::extract::{FromRef, Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, put};
use axum::{Json, Router};
use chrono::{NaiveDate, NaiveTime};
use identity::AuthUser;
use kernel::http::{IfMatch, etag};
use kernel::{AppError, FieldError};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StaffStatusDto {
    Active,
    Left,
}

impl From<StaffStatus> for StaffStatusDto {
    fn from(s: StaffStatus) -> Self {
        match s {
            StaffStatus::Active => Self::Active,
            StaffStatus::Left => Self::Left,
        }
    }
}

impl From<StaffStatusDto> for StaffStatus {
    fn from(s: StaffStatusDto) -> Self {
        match s {
            StaffStatusDto::Active => Self::Active,
            StaffStatusDto::Left => Self::Left,
        }
    }
}

/// Basic staff card: no phone or personal data (`staff.read_basic`).
#[derive(Debug, Serialize, ToSchema)]
pub struct StaffDto {
    pub id: Uuid,
    pub full_name: String,
    pub position: String,
    pub status: StaffStatusDto,
}

impl From<Staff> for StaffDto {
    fn from(s: Staff) -> Self {
        Self {
            id: s.id,
            full_name: s.full_name,
            position: s.position,
            status: s.status.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StaffFilter {
    Active,
    Left,
    All,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct StaffQuery {
    /// Default `active` (pickers only offer people still working here).
    #[param(inline)]
    pub status: Option<StaffFilter>,
}

/// Pay terms (salary data). Only ever sent to holders of `payroll.read`.
#[derive(Debug, Serialize, ToSchema)]
pub struct StaffPayDto {
    pub base_salary: i64,
    /// Basis points: 500 = 5 %.
    pub commission_rate_bp: i32,
    pub loyalty_bonus_monthly: i64,
    pub meal_allowance: i64,
}

impl From<PayTerms> for StaffPayDto {
    fn from(p: PayTerms) -> Self {
        Self {
            base_salary: p.base_salary,
            commission_rate_bp: p.commission_rate_bp,
            loyalty_bonus_monthly: p.loyalty_bonus_monthly,
            meal_allowance: p.meal_allowance,
        }
    }
}

/// A staff profile (`staff.manage`). `pay` is present only for `payroll.read` holders who have
/// set terms; `pay_visible` says whether the caller may see pay at all.
#[derive(Debug, Serialize, ToSchema)]
pub struct StaffProfileDto {
    pub id: Uuid,
    pub full_name: String,
    pub position: String,
    /// National form, e.g. `0905 123 456`.
    pub phone: Option<String>,
    pub status: StaffStatusDto,
    pub joined_on: Option<NaiveDate>,
    pub left_on: Option<NaiveDate>,
    pub user_id: Option<Uuid>,
    pub user_name: Option<String>,
    /// `HH:MM`; `null` = the spa default (payroll rules).
    pub shift_start: Option<String>,
    pub version: i64,
    pub pay_visible: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pay: Option<StaffPayDto>,
}

impl From<ProfileView> for StaffProfileDto {
    fn from(v: ProfileView) -> Self {
        let p = v.profile;
        Self {
            id: p.id,
            full_name: p.full_name,
            position: p.position,
            phone: p.phone.map(|ph| ph.display_national()),
            status: p.status.into(),
            joined_on: p.joined_on,
            left_on: p.left_on,
            user_id: p.user_id,
            user_name: p.user_name,
            shift_start: p.shift_start.map(|t| t.format("%H:%M").to_string()),
            version: p.version,
            pay_visible: v.pay_visible,
            pay: v.pay.map(Into::into),
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StaffProfileWrite {
    pub full_name: String,
    pub position: String,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default = "active")]
    pub status: StaffStatusDto,
    #[serde(default)]
    pub joined_on: Option<NaiveDate>,
    #[serde(default)]
    pub left_on: Option<NaiveDate>,
    /// Login to link (an active member of this spa, not linked to anyone else).
    #[serde(default)]
    pub user_id: Option<Uuid>,
    /// `HH:MM`.
    #[serde(default)]
    pub shift_start: Option<String>,
}

const fn active() -> StaffStatusDto {
    StaffStatusDto::Active
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StaffPayWrite {
    pub base_salary: i64,
    pub commission_rate_bp: i32,
    #[serde(default)]
    pub loyalty_bonus_monthly: i64,
    #[serde(default)]
    pub meal_allowance: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StaffAccountDto {
    pub user_id: Uuid,
    pub full_name: String,
    pub roles: Vec<String>,
    /// Staff profile already linked to this login.
    pub staff_id: Option<Uuid>,
}

impl From<Account> for StaffAccountDto {
    fn from(a: Account) -> Self {
        Self {
            user_id: a.user_id,
            full_name: a.full_name,
            roles: a.roles,
            staff_id: a.staff_id,
        }
    }
}

/// `HH:MM` (or `HH:MM:SS`) at the boundary.
pub fn parse_hhmm(field: &str, raw: Option<&str>) -> Result<Option<NaiveTime>, AppError> {
    match raw.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(s) => NaiveTime::parse_from_str(s, "%H:%M")
            .or_else(|_| NaiveTime::parse_from_str(s, "%H:%M:%S"))
            .map(Some)
            .map_err(|_| AppError::Validation(vec![FieldError::new(field, "invalid_time")])),
    }
}

fn to_input(b: StaffProfileWrite) -> Result<ProfileInput, AppError> {
    Ok(ProfileInput {
        shift_start: parse_hhmm("shift_start", b.shift_start.as_deref())?,
        full_name: b.full_name,
        position: b.position,
        phone: b.phone,
        status: b.status.into(),
        joined_on: b.joined_on,
        left_on: b.left_on,
        user_id: b.user_id,
    })
}

pub fn routes<S>() -> Router<S>
where
    PgPool: FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/staff", get(list_staff).post(create_staff))
        .route("/staff/profiles", get(list_profiles))
        .route("/staff/accounts", get(list_accounts))
        .route("/staff/{id}", get(get_staff).put(update_staff))
        .route("/staff/{id}/pay", put(set_staff_pay))
}

fn no_store(status: StatusCode, headers: HeaderMap, body: impl Serialize) -> Response {
    let mut response = (status, headers, Json(body)).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response
}

fn with_etag(version: i64) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(header::ETAG, etag(version));
    h
}

#[utoipa::path(get, path = "/api/v1/staff", tag = "staff", params(StaffQuery),
    responses((status = 200, body = [StaffDto]), (status = 401), (status = 403)))]
pub async fn list_staff(
    State(pool): State<PgPool>,
    user: AuthUser,
    Query(query): Query<StaffQuery>,
) -> Result<Json<Vec<StaffDto>>, AppError> {
    let status = match query.status.unwrap_or(StaffFilter::Active) {
        StaffFilter::Active => Some(StaffStatus::Active),
        StaffFilter::Left => Some(StaffStatus::Left),
        StaffFilter::All => None,
    };
    let rows = service::list(&pool, &user.actor, status).await?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

#[utoipa::path(get, path = "/api/v1/staff/profiles", tag = "staff",
    responses((status = 200, body = [StaffProfileDto]), (status = 401), (status = 403)))]
pub async fn list_profiles(
    State(pool): State<PgPool>,
    user: AuthUser,
) -> Result<Response, AppError> {
    let rows = profile::list(&pool, &user.actor).await?;
    let body: Vec<StaffProfileDto> = rows.into_iter().map(Into::into).collect();
    Ok(no_store(StatusCode::OK, HeaderMap::new(), body))
}

#[utoipa::path(post, path = "/api/v1/staff", tag = "staff", request_body = StaffProfileWrite,
    responses((status = 201, body = StaffProfileDto), (status = 401), (status = 403),
              (status = 409, description = "staff.account_taken"), (status = 422)))]
pub async fn create_staff(
    State(pool): State<PgPool>,
    user: AuthUser,
    Json(body): Json<StaffProfileWrite>,
) -> Result<Response, AppError> {
    let created = profile::create(&pool, &user.actor, &to_input(body)?).await?;
    let version = created.profile.version;
    Ok(no_store(
        StatusCode::CREATED,
        with_etag(version),
        StaffProfileDto::from(created),
    ))
}

#[utoipa::path(get, path = "/api/v1/staff/{id}", tag = "staff", params(("id" = Uuid, Path)),
    responses((status = 200, body = StaffProfileDto), (status = 401), (status = 403), (status = 404)))]
pub async fn get_staff(
    State(pool): State<PgPool>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let v = profile::get(&pool, &user.actor, id).await?;
    let version = v.profile.version;
    Ok(no_store(
        StatusCode::OK,
        with_etag(version),
        StaffProfileDto::from(v),
    ))
}

#[utoipa::path(put, path = "/api/v1/staff/{id}", tag = "staff", params(("id" = Uuid, Path)),
    request_body = StaffProfileWrite,
    responses((status = 200, body = StaffProfileDto), (status = 401), (status = 403), (status = 404),
              (status = 409, description = "staff.account_taken"), (status = 412), (status = 422)))]
pub async fn update_staff(
    State(pool): State<PgPool>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    IfMatch(version): IfMatch,
    Json(body): Json<StaffProfileWrite>,
) -> Result<Response, AppError> {
    let v = profile::update(&pool, &user.actor, id, version, &to_input(body)?).await?;
    let version = v.profile.version;
    Ok(no_store(
        StatusCode::OK,
        with_etag(version),
        StaffProfileDto::from(v),
    ))
}

#[utoipa::path(put, path = "/api/v1/staff/{id}/pay", tag = "staff", params(("id" = Uuid, Path)),
    request_body = StaffPayWrite,
    responses((status = 200, body = StaffPayDto), (status = 401), (status = 403), (status = 404), (status = 422)))]
pub async fn set_staff_pay(
    State(pool): State<PgPool>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<StaffPayWrite>,
) -> Result<Response, AppError> {
    let terms = profile::set_pay(
        &pool,
        &user.actor,
        id,
        &PayInput {
            base_salary: body.base_salary,
            commission_rate_bp: body.commission_rate_bp,
            loyalty_bonus_monthly: body.loyalty_bonus_monthly,
            meal_allowance: body.meal_allowance,
        },
    )
    .await?;
    Ok(no_store(
        StatusCode::OK,
        HeaderMap::new(),
        StaffPayDto::from(terms),
    ))
}

#[utoipa::path(get, path = "/api/v1/staff/accounts", tag = "staff",
    responses((status = 200, body = [StaffAccountDto]), (status = 401), (status = 403)))]
pub async fn list_accounts(
    State(pool): State<PgPool>,
    user: AuthUser,
) -> Result<Response, AppError> {
    let rows = profile::accounts(&pool, &user.actor).await?;
    let body: Vec<StaffAccountDto> = rows.into_iter().map(Into::into).collect();
    Ok(no_store(StatusCode::OK, HeaderMap::new(), body))
}
