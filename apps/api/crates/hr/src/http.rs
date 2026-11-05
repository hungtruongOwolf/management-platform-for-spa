//! HTTP for staff management: attendance, daily reviews, evaluations, bonuses/penalties,
//! payroll and self-service. Handlers only parse and render; services authorize.
//! Every response is `Cache-Control: no-store` (salary and personal data).

use crate::adjustment::{self, Adjustment, AdjustmentInput, AdjustmentKind, AdjustmentLog};
use crate::attendance::{self, AttendanceDay, AttendanceSheet, AttendanceStatus, Entry};
use crate::calc::Payslip;
use crate::evaluation::{
    self, CriterionInput, Evaluation, EvaluationInput, EvaluationSheet, Level,
};
use crate::me::{self, MyStaff};
use crate::month::Month;
use crate::payroll::{self, CloseInput, PayrollMonth};
use crate::review::{self, DailyReview, ReviewEntry, ReviewSheet};
use crate::rules::{self, PayrollRules};
use axum::body::Bytes;
use axum::extract::{FromRef, Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::NaiveDate;
use identity::AuthUser;
use kernel::http::IdempotencyKey;
use kernel::idempotency::hash_route;
use kernel::{AppError, FieldError};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

pub fn routes<S>() -> Router<S>
where
    PgPool: FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/attendance", get(get_attendance).put(put_attendance))
        .route(
            "/daily-reviews",
            get(get_daily_reviews).put(put_daily_reviews),
        )
        .route("/evaluations", get(get_evaluations).put(put_evaluation))
        .route("/adjustments", get(get_adjustments).post(post_adjustment))
        .route("/payroll", get(get_payroll))
        .route("/payroll/payslips/{id}", get(get_payslip))
        .route("/payroll/close", post(close_payroll))
        .route("/payroll/rules", get(get_rules).put(put_rules))
        .route("/me/staff", get(get_my_staff))
}

fn no_store(status: StatusCode, body: impl Serialize) -> Response {
    let mut response = (status, Json(body)).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response
}

fn stored(status: u16, body: serde_json::Value) -> Response {
    no_store(StatusCode::from_u16(status).unwrap_or(StatusCode::OK), body)
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct MonthQuery {
    /// `YYYY-MM`; default the current month (spa time zone).
    pub month: Option<String>,
}

fn month_of(q: &MonthQuery) -> Result<Option<Month>, AppError> {
    q.month
        .as_deref()
        .map(|m| Month::parse("month", m))
        .transpose()
}

fn clean_note(raw: Option<String>) -> Option<String> {
    raw.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty())
}

fn to_json<T: Serialize>(v: T) -> Result<serde_json::Value, AppError> {
    serde_json::to_value(v).map_err(|e| AppError::Internal(e.to_string()))
}

/// Re-serialised JSON, so whitespace or key order never makes a retry a mismatch.
fn canonical<T: for<'de> Deserialize<'de>>(body: &Bytes) -> Result<(T, Vec<u8>), AppError> {
    let invalid = || AppError::Validation(vec![FieldError::new("body", "invalid")]);
    let value: serde_json::Value = serde_json::from_slice(body).map_err(|_| invalid())?;
    let bytes = serde_json::to_vec(&value).map_err(|e| AppError::Internal(e.to_string()))?;
    let parsed = serde_json::from_value(value).map_err(|_| invalid())?;
    Ok((parsed, bytes))
}

// ------------------------------------------------------------------------------ attendance

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AttendanceEntryWrite {
    pub staff_id: Uuid,
    pub date: NaiveDate,
    /// `null` clears the day.
    pub status: Option<AttendanceStatus>,
    /// `HH:MM`, only when present.
    #[serde(default)]
    pub check_in: Option<String>,
    #[serde(default)]
    pub check_out: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AttendanceWrite {
    /// 1–100 cells.
    pub entries: Vec<AttendanceEntryWrite>,
}

#[utoipa::path(get, path = "/api/v1/attendance", tag = "staff", params(MonthQuery),
    responses((status = 200, body = AttendanceSheet), (status = 401), (status = 403), (status = 422)))]
pub async fn get_attendance(
    State(pool): State<PgPool>,
    user: AuthUser,
    Query(q): Query<MonthQuery>,
) -> Result<Response, AppError> {
    let sheet = attendance::sheet(&pool, &user.actor, month_of(&q)?).await?;
    Ok(no_store(StatusCode::OK, sheet))
}

#[utoipa::path(put, path = "/api/v1/attendance", tag = "staff", request_body = AttendanceWrite,
    responses((status = 200, body = [AttendanceDay]), (status = 401), (status = 403),
              (status = 409, description = "payroll.period_closed"), (status = 422)))]
pub async fn put_attendance(
    State(pool): State<PgPool>,
    user: AuthUser,
    Json(body): Json<AttendanceWrite>,
) -> Result<Response, AppError> {
    let entries = body
        .entries
        .into_iter()
        .enumerate()
        .map(|(i, e)| {
            Ok(Entry {
                staff_id: e.staff_id,
                date: e.date,
                status: e.status,
                check_in: org::http::parse_hhmm(
                    &format!("entries[{i}].check_in"),
                    e.check_in.as_deref(),
                )?,
                check_out: org::http::parse_hhmm(
                    &format!("entries[{i}].check_out"),
                    e.check_out.as_deref(),
                )?,
                note: clean_note(e.note),
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    let saved = attendance::record(&pool, &user.actor, &entries).await?;
    Ok(no_store(StatusCode::OK, saved))
}

// ---------------------------------------------------------------------------- daily review

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DailyReviewEntryWrite {
    pub staff_id: Uuid,
    pub date: NaiveDate,
    #[serde(default)]
    pub compliments: u32,
    #[serde(default)]
    pub complaints: u32,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DailyReviewWrite {
    /// 1–100 days; all zero and no note clears the day.
    pub entries: Vec<DailyReviewEntryWrite>,
}

#[utoipa::path(get, path = "/api/v1/daily-reviews", tag = "staff", params(MonthQuery),
    responses((status = 200, body = ReviewSheet), (status = 401), (status = 403), (status = 422)))]
pub async fn get_daily_reviews(
    State(pool): State<PgPool>,
    user: AuthUser,
    Query(q): Query<MonthQuery>,
) -> Result<Response, AppError> {
    let sheet = review::sheet(&pool, &user.actor, month_of(&q)?).await?;
    Ok(no_store(StatusCode::OK, sheet))
}

#[utoipa::path(put, path = "/api/v1/daily-reviews", tag = "staff", request_body = DailyReviewWrite,
    responses((status = 200, body = [DailyReview]), (status = 401), (status = 403),
              (status = 409, description = "payroll.period_closed"), (status = 422)))]
pub async fn put_daily_reviews(
    State(pool): State<PgPool>,
    user: AuthUser,
    Json(body): Json<DailyReviewWrite>,
) -> Result<Response, AppError> {
    let entries: Vec<ReviewEntry> = body
        .entries
        .into_iter()
        .map(|e| ReviewEntry {
            staff_id: e.staff_id,
            date: e.date,
            compliments: e.compliments,
            complaints: e.complaints,
            note: clean_note(e.note),
        })
        .collect();
    let saved = review::record(&pool, &user.actor, &entries).await?;
    Ok(no_store(StatusCode::OK, saved))
}

// ------------------------------------------------------------------------------ evaluations

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CriterionWrite {
    /// One of the six criterion keys (`GET /evaluations` → `criteria`).
    pub key: String,
    pub level: Level,
    /// Only for `fail`: a score below the 30 % ceiling.
    #[serde(default)]
    pub points: Option<u32>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct EvaluationWrite {
    pub staff_id: Uuid,
    /// `YYYY-MM`, not after the current month.
    pub month: String,
    pub criteria: Vec<CriterionWrite>,
    #[serde(default)]
    pub serious_issue: bool,
    /// Required when the result is Mức 3 or Chưa đạt.
    #[serde(default)]
    pub coaching_plan: Option<String>,
    #[serde(default)]
    pub comment: Option<String>,
}

#[utoipa::path(get, path = "/api/v1/evaluations", tag = "staff", params(MonthQuery),
    responses((status = 200, body = EvaluationSheet), (status = 401), (status = 403), (status = 422)))]
pub async fn get_evaluations(
    State(pool): State<PgPool>,
    user: AuthUser,
    Query(q): Query<MonthQuery>,
) -> Result<Response, AppError> {
    let sheet = evaluation::sheet(&pool, &user.actor, month_of(&q)?).await?;
    Ok(no_store(StatusCode::OK, sheet))
}

#[utoipa::path(put, path = "/api/v1/evaluations", tag = "staff", request_body = EvaluationWrite,
    responses((status = 200, body = Evaluation), (status = 401), (status = 403),
              (status = 409, description = "payroll.period_closed"), (status = 422)))]
pub async fn put_evaluation(
    State(pool): State<PgPool>,
    user: AuthUser,
    Json(body): Json<EvaluationWrite>,
) -> Result<Response, AppError> {
    let input = EvaluationInput {
        staff_id: body.staff_id,
        month: Month::parse("month", &body.month)?,
        criteria: body
            .criteria
            .into_iter()
            .map(|c| CriterionInput {
                key: c.key,
                level: c.level,
                points: c.points,
            })
            .collect(),
        serious_issue: body.serious_issue,
        coaching_plan: body.coaching_plan,
        comment: body.comment,
    };
    let saved = evaluation::record(&pool, &user.actor, &input).await?;
    Ok(no_store(StatusCode::OK, saved))
}

// ------------------------------------------------------------------------------ adjustments

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AdjustmentCreate {
    pub staff_id: Uuid,
    pub kind: AdjustmentKind,
    /// Đồng, 1–100.000.000; give `amount`, `points` or both.
    #[serde(default)]
    pub amount: Option<i64>,
    #[serde(default)]
    pub points: Option<u32>,
    pub reason: String,
    /// Default today; its month must be open.
    #[serde(default)]
    pub occurred_on: Option<NaiveDate>,
    /// `YYYY-MM` of a closed month this entry corrects.
    #[serde(default)]
    pub corrects_month: Option<String>,
}

#[utoipa::path(get, path = "/api/v1/adjustments", tag = "staff", params(MonthQuery),
    responses((status = 200, body = AdjustmentLog), (status = 401), (status = 403), (status = 422)))]
pub async fn get_adjustments(
    State(pool): State<PgPool>,
    user: AuthUser,
    Query(q): Query<MonthQuery>,
) -> Result<Response, AppError> {
    let log = adjustment::log(&pool, &user.actor, month_of(&q)?).await?;
    Ok(no_store(StatusCode::OK, log))
}

#[utoipa::path(post, path = "/api/v1/adjustments", tag = "staff", request_body = AdjustmentCreate,
    params(("Idempotency-Key" = Uuid, Header)),
    responses((status = 201, body = Adjustment), (status = 400), (status = 401), (status = 403),
              (status = 409, description = "payroll.period_closed"), (status = 422)))]
pub async fn post_adjustment(
    State(pool): State<PgPool>,
    user: AuthUser,
    IdempotencyKey(key): IdempotencyKey,
    body: Bytes,
) -> Result<Response, AppError> {
    let (b, bytes): (AdjustmentCreate, _) = canonical(&body)?;
    let hash = hash_route("POST", "/adjustments", None, &bytes);
    let input = AdjustmentInput {
        staff_id: b.staff_id,
        kind: b.kind,
        amount: b.amount,
        points: b.points,
        reason: b.reason,
        occurred_on: b.occurred_on,
        corrects_month: b
            .corrects_month
            .as_deref()
            .map(|m| Month::parse("corrects_month", m))
            .transpose()?,
    };
    let (status, body) = adjustment::record(&pool, &user.actor, key, hash, &input, to_json).await?;
    Ok(stored(status, body))
}

// ---------------------------------------------------------------------------------- payroll

#[utoipa::path(get, path = "/api/v1/payroll", tag = "staff", params(MonthQuery),
    responses((status = 200, body = PayrollMonth), (status = 401), (status = 403), (status = 422)))]
pub async fn get_payroll(
    State(pool): State<PgPool>,
    user: AuthUser,
    Query(q): Query<MonthQuery>,
) -> Result<Response, AppError> {
    let m = payroll::month(&pool, &user.actor, month_of(&q)?).await?;
    Ok(no_store(StatusCode::OK, m))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PayslipDto {
    pub closed: bool,
    pub payslip: Payslip,
}

#[utoipa::path(get, path = "/api/v1/payroll/payslips/{id}", tag = "staff",
    params(("id" = Uuid, Path, description = "Staff id"), MonthQuery),
    responses((status = 200, body = PayslipDto), (status = 401), (status = 403), (status = 404), (status = 422)))]
pub async fn get_payslip(
    State(pool): State<PgPool>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Query(q): Query<MonthQuery>,
) -> Result<Response, AppError> {
    let (payslip, closed) = payroll::payslip(&pool, &user.actor, month_of(&q)?, id).await?;
    Ok(no_store(StatusCode::OK, PayslipDto { closed, payslip }))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PayrollClose {
    /// `YYYY-MM` of an ended month.
    pub month: String,
    /// Close although some employed days have no attendance (default false → 422).
    #[serde(default)]
    pub allow_incomplete: bool,
}

#[utoipa::path(post, path = "/api/v1/payroll/close", tag = "staff", request_body = PayrollClose,
    params(("Idempotency-Key" = Uuid, Header)),
    responses((status = 200, body = PayrollMonth), (status = 400), (status = 401), (status = 403),
              (status = 409, description = "payroll.period_closed"),
              (status = 422, description = "payroll.month_not_ended | payroll.unrecorded_days")))]
pub async fn close_payroll(
    State(pool): State<PgPool>,
    user: AuthUser,
    IdempotencyKey(key): IdempotencyKey,
    body: Bytes,
) -> Result<Response, AppError> {
    let (b, bytes): (PayrollClose, _) = canonical(&body)?;
    let hash = hash_route("POST", "/payroll/close", None, &bytes);
    let input = CloseInput {
        month: Month::parse("month", &b.month)?,
        allow_incomplete: b.allow_incomplete,
    };
    let (status, body) = payroll::close(&pool, &user.actor, key, hash, &input, to_json).await?;
    Ok(stored(status, body))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PayrollRulesDto {
    pub rules: PayrollRules,
    /// 0 = never saved (all defaults).
    pub version: i64,
}

#[utoipa::path(get, path = "/api/v1/payroll/rules", tag = "staff",
    responses((status = 200, body = PayrollRulesDto), (status = 401), (status = 403)))]
pub async fn get_rules(State(pool): State<PgPool>, user: AuthUser) -> Result<Response, AppError> {
    let (rules, version) = rules::get(&pool, &user.actor).await?;
    Ok(no_store(StatusCode::OK, PayrollRulesDto { rules, version }))
}

#[utoipa::path(put, path = "/api/v1/payroll/rules", tag = "staff", request_body = PayrollRules,
    responses((status = 200, body = PayrollRulesDto), (status = 401), (status = 403), (status = 422)))]
pub async fn put_rules(
    State(pool): State<PgPool>,
    user: AuthUser,
    body: Bytes,
) -> Result<Response, AppError> {
    let (rules, _): (PayrollRules, _) = canonical(&body)?;
    let (rules, version) = rules::put(&pool, &user.actor, &rules).await?;
    Ok(no_store(StatusCode::OK, PayrollRulesDto { rules, version }))
}

// ------------------------------------------------------------------------------ self-service

#[utoipa::path(get, path = "/api/v1/me/staff", tag = "staff", params(MonthQuery),
    responses((status = 200, body = MyStaff), (status = 401), (status = 403), (status = 422)))]
pub async fn get_my_staff(
    State(pool): State<PgPool>,
    user: AuthUser,
    Query(q): Query<MonthQuery>,
) -> Result<Response, AppError> {
    let mine = me::my_month(&pool, &user.actor, month_of(&q)?).await?;
    Ok(no_store(StatusCode::OK, mine))
}
