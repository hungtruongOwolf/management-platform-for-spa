//! `GET /staff` — picker for closer / technician; the staff roster (`staff.read`), profiles,
//! employment (end / rehire), login link and pay terms (`staff.manage`).

use crate::employment::Period;
use crate::profile::{
    self, Account, Change, EndInput, HistoryEntry, PayInput, PayTerms, ProfileInput, ProfileView,
};
use crate::service::{self, Staff, StaffStatus};
use axum::extract::{FromRef, Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
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

impl StaffFilter {
    const fn status(self) -> Option<StaffStatus> {
        match self {
            Self::Active => Some(StaffStatus::Active),
            Self::Left => Some(StaffStatus::Left),
            Self::All => None,
        }
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct StaffQuery {
    /// Default `active` (pickers only offer people still working here).
    #[param(inline)]
    pub status: Option<StaffFilter>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct RosterQuery {
    /// Default `all` (Đang làm / Đã nghỉ / Tất cả).
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

/// One employment period (ADR-0012). `end_reason` only for `staff.manage` holders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct EmploymentPeriodDto {
    /// `null` = before the books (start date never recorded).
    pub started_on: Option<NaiveDate>,
    /// `null` = still working.
    pub ended_on: Option<NaiveDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_reason: Option<String>,
}

impl EmploymentPeriodDto {
    /// Dates only (no reason): for views that must not carry HR details.
    pub fn dates(p: &Period) -> Self {
        Self {
            started_on: p.started_on,
            ended_on: p.ended_on,
            end_reason: None,
        }
    }
}

/// Personal details of a profile (`staff.manage` only).
#[derive(Debug, Serialize, ToSchema)]
pub struct StaffDetailsDto {
    /// National form, e.g. `0905 123 456`.
    pub phone: Option<String>,
    pub note: Option<String>,
    pub user_id: Option<Uuid>,
    pub user_name: Option<String>,
}

/// A roster entry / staff profile (`staff.read`). `details` only for `staff.manage`; `pay` only
/// for `payroll.read` holders who have set terms (`pay_visible` says whether pay is visible at
/// all). Absent means "not yours to see", never "hidden by the UI".
#[derive(Debug, Serialize, ToSchema)]
pub struct StaffProfileDto {
    pub id: Uuid,
    pub full_name: String,
    pub position: String,
    pub status: StaffStatusDto,
    /// Start of the current (or, once left, the latest) period: "Ngày vào làm".
    pub joined_on: Option<NaiveDate>,
    /// Start of the first period ("vào làm lần đầu"), differs from `joined_on` after a rehire.
    pub first_joined_on: Option<NaiveDate>,
    /// Last day of the latest period, once the person has left.
    pub left_on: Option<NaiveDate>,
    /// Oldest first.
    pub periods: Vec<EmploymentPeriodDto>,
    /// Days worked, summed over the periods up to today; `null` = a start date is unknown.
    pub tenure_days: Option<i64>,
    /// Completed calendar months, summed over the periods.
    pub tenure_months: Option<i64>,
    /// Whether a login is linked (decision C: an unlinked manager counts as a technician).
    pub login_linked: bool,
    /// `HH:MM`; `null` = the spa default (payroll rules).
    pub shift_start: Option<String>,
    pub version: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<StaffDetailsDto>,
    pub pay_visible: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pay: Option<StaffPayDto>,
}

impl From<ProfileView> for StaffProfileDto {
    fn from(v: ProfileView) -> Self {
        let p = v.profile;
        let details = v.details_visible;
        Self {
            id: p.id,
            full_name: p.full_name,
            position: p.position,
            status: p.status.into(),
            joined_on: p.joined_on,
            first_joined_on: p.employment.first_started_on(),
            left_on: p.left_on,
            periods: p
                .employment
                .periods()
                .iter()
                .map(|period| EmploymentPeriodDto {
                    end_reason: period.end_reason.clone().filter(|_| details),
                    ..EmploymentPeriodDto::dates(period)
                })
                .collect(),
            tenure_days: v.tenure.map(|t| t.days),
            tenure_months: v.tenure.map(|t| t.months),
            login_linked: p.user_id.is_some(),
            shift_start: p.shift_start.map(|t| t.format("%H:%M").to_string()),
            version: p.version,
            details: details.then(|| StaffDetailsDto {
                phone: p.phone.map(|ph| ph.display_national()),
                note: p.note,
                user_id: p.user_id,
                user_name: p.user_name,
            }),
            pay_visible: v.pay_visible,
            pay: v.pay.map(Into::into),
        }
    }
}

/// Create (hire) or edit a profile. On edit, `joined_on` is the start of the current period.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StaffProfileWrite {
    pub full_name: String,
    pub position: String,
    #[serde(default)]
    pub phone: Option<String>,
    /// Start of the current period; `null` = not recorded (tenure unknown).
    #[serde(default)]
    pub joined_on: Option<NaiveDate>,
    /// `HH:MM`.
    #[serde(default)]
    pub shift_start: Option<String>,
    /// HR note (≤ 1000), owner only.
    #[serde(default)]
    pub note: Option<String>,
}

/// Hire: a profile plus, optionally, the login to link right away.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StaffCreate {
    pub full_name: String,
    pub position: String,
    #[serde(default)]
    pub phone: Option<String>,
    /// First day of work; `null` = not recorded (the UI fills in today).
    #[serde(default)]
    pub joined_on: Option<NaiveDate>,
    /// `HH:MM`.
    #[serde(default)]
    pub shift_start: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    /// Login to link (an active member of this spa, not linked to anyone else).
    #[serde(default)]
    pub user_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StaffAccountWrite {
    /// `null` unlinks.
    pub user_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StaffEndWrite {
    /// Last working day (≤ today).
    pub ended_on: NaiveDate,
    pub reason: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StaffRehireWrite {
    /// First day of the new period (after the last period's end).
    pub started_on: NaiveDate,
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

#[derive(Debug, Serialize, ToSchema)]
pub struct StaffChangeDto {
    /// Audit key: `position`, `joined_on`, `left_on`, `status`, `user_id`, `base_salary`, …
    pub field: String,
    #[schema(value_type = Object)]
    pub before: serde_json::Value,
    #[schema(value_type = Object)]
    pub after: serde_json::Value,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StaffHistoryEntryDto {
    pub at: DateTime<Utc>,
    /// `staff.create`, `staff.update`, `staff.account_link`, `staff.end`, `staff.rehire`,
    /// `staff.pay_update` (only for `payroll.read`).
    pub action: String,
    pub actor_name: Option<String>,
    pub changes: Vec<StaffChangeDto>,
    /// End-of-employment reason.
    pub reason: Option<String>,
}

impl From<HistoryEntry> for StaffHistoryEntryDto {
    fn from(h: HistoryEntry) -> Self {
        Self {
            at: h.at,
            action: h.action,
            actor_name: h.actor_name,
            changes: h
                .changes
                .into_iter()
                .map(|c: Change| StaffChangeDto {
                    field: c.field,
                    before: c.before,
                    after: c.after,
                })
                .collect(),
            reason: h.reason,
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
        joined_on: b.joined_on,
        note: b.note,
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
        .route("/staff/{id}/account", put(set_staff_account))
        .route("/staff/{id}/end", post(end_employment))
        .route("/staff/{id}/rehire", post(rehire_staff))
        .route("/staff/{id}/history", get(staff_history))
}

fn no_store(status: StatusCode, headers: HeaderMap, body: impl Serialize) -> Response {
    let mut response = (status, headers, Json(body)).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response
}

fn profile_response(status: StatusCode, v: ProfileView) -> Response {
    let mut h = HeaderMap::new();
    h.insert(header::ETAG, etag(v.profile.version));
    no_store(status, h, StaffProfileDto::from(v))
}

#[utoipa::path(get, path = "/api/v1/staff", tag = "staff", params(StaffQuery),
    responses((status = 200, body = [StaffDto]), (status = 401), (status = 403)))]
pub async fn list_staff(
    State(pool): State<PgPool>,
    user: AuthUser,
    Query(query): Query<StaffQuery>,
) -> Result<Json<Vec<StaffDto>>, AppError> {
    let status = query.status.unwrap_or(StaffFilter::Active).status();
    let rows = service::list(&pool, &user.actor, status).await?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

#[utoipa::path(get, path = "/api/v1/staff/profiles", tag = "staff", params(RosterQuery),
    responses((status = 200, body = [StaffProfileDto]), (status = 401), (status = 403)))]
pub async fn list_profiles(
    State(pool): State<PgPool>,
    user: AuthUser,
    Query(query): Query<RosterQuery>,
) -> Result<Response, AppError> {
    let status = query.status.unwrap_or(StaffFilter::All).status();
    let rows = profile::list(&pool, &user.actor, status).await?;
    let body: Vec<StaffProfileDto> = rows.into_iter().map(Into::into).collect();
    Ok(no_store(StatusCode::OK, HeaderMap::new(), body))
}

#[utoipa::path(post, path = "/api/v1/staff", tag = "staff", request_body = StaffCreate,
    responses((status = 201, body = StaffProfileDto), (status = 401), (status = 403),
              (status = 409, description = "staff.account_taken"), (status = 422)))]
pub async fn create_staff(
    State(pool): State<PgPool>,
    user: AuthUser,
    Json(body): Json<StaffCreate>,
) -> Result<Response, AppError> {
    let user_id = body.user_id;
    let input = to_input(StaffProfileWrite {
        full_name: body.full_name,
        position: body.position,
        phone: body.phone,
        joined_on: body.joined_on,
        shift_start: body.shift_start,
        note: body.note,
    })?;
    let created = profile::create(&pool, &user.actor, &input, user_id).await?;
    Ok(profile_response(StatusCode::CREATED, created))
}

#[utoipa::path(get, path = "/api/v1/staff/{id}", tag = "staff", params(("id" = Uuid, Path)),
    responses((status = 200, body = StaffProfileDto), (status = 401), (status = 403), (status = 404)))]
pub async fn get_staff(
    State(pool): State<PgPool>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let v = profile::get(&pool, &user.actor, id).await?;
    Ok(profile_response(StatusCode::OK, v))
}

#[utoipa::path(put, path = "/api/v1/staff/{id}", tag = "staff", params(("id" = Uuid, Path)),
    request_body = StaffProfileWrite,
    responses((status = 200, body = StaffProfileDto), (status = 401), (status = 403), (status = 404),
              (status = 412), (status = 422)))]
pub async fn update_staff(
    State(pool): State<PgPool>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    IfMatch(version): IfMatch,
    Json(body): Json<StaffProfileWrite>,
) -> Result<Response, AppError> {
    let v = profile::update(&pool, &user.actor, id, version, &to_input(body)?).await?;
    Ok(profile_response(StatusCode::OK, v))
}

#[utoipa::path(put, path = "/api/v1/staff/{id}/account", tag = "staff", params(("id" = Uuid, Path)),
    request_body = StaffAccountWrite,
    responses((status = 200, body = StaffProfileDto), (status = 401), (status = 403), (status = 404),
              (status = 409, description = "staff.account_taken"), (status = 412), (status = 422)))]
pub async fn set_staff_account(
    State(pool): State<PgPool>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    IfMatch(version): IfMatch,
    Json(body): Json<StaffAccountWrite>,
) -> Result<Response, AppError> {
    let v = profile::set_account(&pool, &user.actor, id, version, body.user_id).await?;
    Ok(profile_response(StatusCode::OK, v))
}

#[utoipa::path(post, path = "/api/v1/staff/{id}/end", tag = "staff", params(("id" = Uuid, Path)),
    request_body = StaffEndWrite,
    responses((status = 200, body = StaffProfileDto), (status = 401), (status = 403), (status = 404),
              (status = 409, description = "staff.already_left | staff.has_upcoming_bookings"),
              (status = 412),
              (status = 422, description = "staff.cannot_end_self | staff.linked_to_owner | field errors")))]
pub async fn end_employment(
    State(pool): State<PgPool>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    IfMatch(version): IfMatch,
    Json(body): Json<StaffEndWrite>,
) -> Result<Response, AppError> {
    let input = EndInput {
        ended_on: body.ended_on,
        reason: body.reason,
    };
    let v = profile::end(&pool, &user.actor, id, version, &input).await?;
    Ok(profile_response(StatusCode::OK, v))
}

#[utoipa::path(post, path = "/api/v1/staff/{id}/rehire", tag = "staff", params(("id" = Uuid, Path)),
    request_body = StaffRehireWrite,
    responses((status = 200, body = StaffProfileDto), (status = 401), (status = 403), (status = 404),
              (status = 409, description = "staff.already_active"), (status = 412), (status = 422)))]
pub async fn rehire_staff(
    State(pool): State<PgPool>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    IfMatch(version): IfMatch,
    Json(body): Json<StaffRehireWrite>,
) -> Result<Response, AppError> {
    let v = profile::rehire(&pool, &user.actor, id, version, body.started_on).await?;
    Ok(profile_response(StatusCode::OK, v))
}

#[utoipa::path(get, path = "/api/v1/staff/{id}/history", tag = "staff", params(("id" = Uuid, Path)),
    responses((status = 200, body = [StaffHistoryEntryDto]), (status = 401), (status = 403), (status = 404)))]
pub async fn staff_history(
    State(pool): State<PgPool>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let rows = profile::history(&pool, &user.actor, id).await?;
    let body: Vec<StaffHistoryEntryDto> = rows.into_iter().map(Into::into).collect();
    Ok(no_store(StatusCode::OK, HeaderMap::new(), body))
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
