//! `GET/POST /appointments`, `POST /appointments/group`, `GET/PATCH /appointments/{id}`,
//! `POST /appointments/{id}/arrive|undo-arrival|move-group|complete|cancel|no-show`,
//! `PUT /appointment-settings`.

use super::dto::{
    AppointmentArrive, AppointmentClose, AppointmentComplete, AppointmentCompleted,
    AppointmentCreate, AppointmentDto, AppointmentGroupCreate, AppointmentGroupDto,
    AppointmentGroupMove, AppointmentGroupMoved, AppointmentSettingsDto, AppointmentSettingsUpdate,
    AppointmentUpdate, CalendarDto, CalendarQuery, MyCalendar,
};
use super::service::{self, Outcome};
use axum::body::Bytes;
use axum::extract::{FromRef, Path, Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use identity::AuthUser;
use kernel::crypto::FieldCipher;
use kernel::http::{IdempotencyKey, IfMatch, etag};
use kernel::idempotency::hash_route;
use kernel::{AppError, FieldError};
use serde::de::DeserializeOwned;
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
        .route("/appointments", get(calendar).post(book))
        .route("/me/appointments", get(my_calendar))
        .route("/appointments/group", post(book_group))
        .route("/appointment-settings", put(update_settings))
        .route(
            "/appointments/{id}",
            get(read_appointment).patch(update_appointment),
        )
        .route("/appointments/{id}/arrive", post(arrive))
        .route("/appointments/{id}/undo-arrival", post(undo_arrival))
        .route("/appointments/{id}/move-group", post(move_group))
        .route("/appointments/{id}/complete", post(complete))
        .route("/appointments/{id}/cancel", post(cancel))
        .route("/appointments/{id}/no-show", post(no_show))
}

fn no_store(status: StatusCode, body: impl serde::Serialize, version: Option<i64>) -> Response {
    let mut response = (status, Json(body)).into_response();
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if let Some(v) = version {
        headers.insert(header::ETAG, etag(v));
    }
    response
}

/// Parses the body and hashes its canonical JSON (key order and whitespace never make a retry a
/// "different" request).
fn parse_canonical<T: DeserializeOwned>(body: &Bytes) -> Result<(T, Vec<u8>), AppError> {
    let invalid = || AppError::Validation(vec![FieldError::new("body", "invalid")]);
    let value: Value = serde_json::from_slice(body).map_err(|_| invalid())?;
    let canonical = serde_json::to_vec(&value).map_err(|e| AppError::Internal(e.to_string()))?;
    let input = serde_json::from_value(value).map_err(|_| invalid())?;
    Ok((input, canonical))
}

#[utoipa::path(get, path = "/api/v1/appointments", tag = "appointments", params(CalendarQuery),
    responses((status = 200, body = CalendarDto), (status = 401), (status = 403),
        (status = 422, description = "to before_from / range_too_long (max 14 days)")))]
pub async fn calendar(
    State(pool): State<PgPool>,
    user: AuthUser,
    Query(query): Query<CalendarQuery>,
) -> Result<Response, AppError> {
    let cal = service::calendar(&pool, &user.actor, &query).await?;
    Ok(no_store(StatusCode::OK, cal, None))
}

/// The signed-in technician's own calendar (read-only). The person is the session's staff link;
/// there is no parameter to pick anyone else (`?staff_id=` → 400).
#[utoipa::path(get, path = "/api/v1/me/appointments", tag = "appointments", params(CalendarQuery),
    responses((status = 200, body = MyCalendar), (status = 400), (status = 401), (status = 403),
        (status = 422, description = "to before_from / range_too_long (max 14 days)")))]
pub async fn my_calendar(
    State(pool): State<PgPool>,
    State(cipher): State<Arc<FieldCipher>>,
    user: AuthUser,
    Query(query): Query<CalendarQuery>,
) -> Result<Response, AppError> {
    let cal = service::my_calendar(&pool, &cipher, &user.actor, &query).await?;
    Ok(no_store(StatusCode::OK, cal, None))
}

#[utoipa::path(post, path = "/api/v1/appointments", tag = "appointments", request_body = AppointmentCreate,
    params(("Idempotency-Key" = Uuid, Header, description = "One key per booking; a retry replays the first response")),
    responses((status = 201, body = AppointmentDto), (status = 400), (status = 401),
        (status = 403, description = "auth.forbidden / day.locked (group_with: the host is on a past day, non-owner)"),
        (status = 404, description = "customer or service not in this spa"),
        (status = 409, description = "appointment.staff_busy (existing_id = the overlapping appointment), appointment.customer_busy (the same customer, or newcomer phone outside the group, already has an open booking overlapping it: existing_id), customer.phone_taken (newcomer's phone already has a record: existing_id)"),
        (status = 422, description = "validation_failed (starts_at not_on_slot / too_far, duration_min out_of_range, guest.*), appointment.outside_opening_hours, appointment.in_the_past (a past business day, owner too: record a back-dated visit instead), staff.inactive")))]
pub async fn book(
    State(pool): State<PgPool>,
    user: AuthUser,
    IdempotencyKey(key): IdempotencyKey,
    body: Bytes,
) -> Result<Response, AppError> {
    let (input, canonical): (AppointmentCreate, _) = parse_canonical(&body)?;
    let (status, body) = service::book(
        &pool,
        &user.actor,
        key,
        hash_route("POST", "/appointments", None, &canonical),
        &input,
    )
    .await?;
    let version = body["version"].as_i64();
    Ok(no_store(
        StatusCode::from_u16(status).unwrap_or(StatusCode::CREATED),
        body,
        version,
    ))
}

#[utoipa::path(post, path = "/api/v1/appointments/group", tag = "appointments", request_body = AppointmentGroupCreate,
    params(("Idempotency-Key" = Uuid, Header, description = "One key per group booking; a retry replays the first response")),
    responses((status = 201, body = AppointmentGroupDto), (status = 400), (status = 401),
        (status = 403, description = "auth.forbidden / day.locked"),
        (status = 404, description = "a customer or service not in this spa"),
        (status = 409, description = "appointment.staff_busy (existing_id; also two people of the group on one technician), appointment.customer_busy (existing_id), customer.phone_taken (existing_id)"),
        (status = 422, description = "validation_failed: people count_out_of_range (2–6), people[i].customer_id duplicate, people[i].starts_at not_same_day, and every 422 of POST /appointments under people[i].*")))]
pub async fn book_group(
    State(pool): State<PgPool>,
    user: AuthUser,
    IdempotencyKey(key): IdempotencyKey,
    body: Bytes,
) -> Result<Response, AppError> {
    let (input, canonical): (AppointmentGroupCreate, _) = parse_canonical(&body)?;
    let (status, body) = service::book_group(
        &pool,
        &user.actor,
        key,
        hash_route("POST", "/appointments/group", None, &canonical),
        &input,
    )
    .await?;
    Ok(no_store(
        StatusCode::from_u16(status).unwrap_or(StatusCode::CREATED),
        body,
        None,
    ))
}

#[utoipa::path(put, path = "/api/v1/appointment-settings", tag = "appointments", request_body = AppointmentSettingsUpdate,
    responses((status = 200, body = AppointmentSettingsDto), (status = 401),
        (status = 403, description = "auth.forbidden (owner only: appointment.settings)"),
        (status = 422, description = "max_per_slot required / out_of_range (1–200 or null)")))]
pub async fn update_settings(
    State(pool): State<PgPool>,
    user: AuthUser,
    Json(input): Json<AppointmentSettingsUpdate>,
) -> Result<Response, AppError> {
    let settings = service::update_settings(&pool, &user.actor, &input).await?;
    Ok(no_store(StatusCode::OK, settings, None))
}

#[utoipa::path(get, path = "/api/v1/appointments/{id}", tag = "appointments", params(("id" = Uuid, Path)),
    responses((status = 200, body = AppointmentDto), (status = 401), (status = 403), (status = 404)))]
pub async fn read_appointment(
    State(pool): State<PgPool>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let a = service::get(&pool, &user.actor, id).await?;
    let version = a.version;
    Ok(no_store(StatusCode::OK, a, Some(version)))
}

#[utoipa::path(patch, path = "/api/v1/appointments/{id}", tag = "appointments", request_body = AppointmentUpdate,
    params(("id" = Uuid, Path), ("If-Match" = String, Header)),
    responses((status = 200, body = AppointmentDto), (status = 401),
        (status = 403, description = "auth.forbidden / day.locked"), (status = 404),
        (status = 409, description = "appointment.closed, appointment.staff_busy, appointment.customer_busy (checked when the booking moves or gets its customer record)"),
        (status = 412),
        (status = 422, description = "validation_failed (customer_id duplicate: that profile is already in the group), appointment.outside_opening_hours, appointment.in_the_past (moved to a past day)")))]
pub async fn update_appointment(
    State(pool): State<PgPool>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    IfMatch(version): IfMatch,
    Json(input): Json<AppointmentUpdate>,
) -> Result<Response, AppError> {
    let a = service::update(&pool, &user.actor, id, version, &input).await?;
    let version = a.version;
    Ok(no_store(StatusCode::OK, a, Some(version)))
}

#[utoipa::path(post, path = "/api/v1/appointments/{id}/arrive", tag = "appointments", request_body = AppointmentArrive,
    params(("id" = Uuid, Path)),
    responses((status = 200, body = AppointmentDto), (status = 401),
        (status = 403, description = "auth.forbidden / day.locked"), (status = 404),
        (status = 409, description = "appointment.closed / already_arrived / staff_busy"),
        (status = 422, description = "appointment.not_today, staff.inactive")))]
pub async fn arrive(
    State(pool): State<PgPool>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<AppointmentArrive>,
) -> Result<Response, AppError> {
    let a = service::arrive(&pool, &user.actor, id, &input).await?;
    let version = a.version;
    Ok(no_store(StatusCode::OK, a, Some(version)))
}

#[utoipa::path(post, path = "/api/v1/appointments/{id}/undo-arrival", tag = "appointments",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = AppointmentDto), (status = 401),
        (status = 403, description = "auth.forbidden / day.locked"), (status = 404),
        (status = 409, description = "appointment.closed (done: a visit exists, or no-show / cancelled), appointment.not_arrived"),
        (status = 422, description = "appointment.not_today")))]
pub async fn undo_arrival(
    State(pool): State<PgPool>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let a = service::undo_arrival(&pool, &user.actor, id).await?;
    let version = a.version;
    Ok(no_store(StatusCode::OK, a, Some(version)))
}

#[utoipa::path(post, path = "/api/v1/appointments/{id}/move-group", tag = "appointments",
    request_body = AppointmentGroupMove,
    params(("id" = Uuid, Path), ("If-Match" = String, Header)),
    responses((status = 200, body = AppointmentGroupMoved), (status = 401),
        (status = 403, description = "auth.forbidden / day.locked"), (status = 404),
        (status = 409, description = "appointment.closed, appointment.already_arrived ({id} is no longer booked)"),
        (status = 412),
        (status = 422, description = "appointment.not_in_group; validation_failed with errors[].field = appointments.<member id> and the member's refusal as code (appointment.staff_busy, appointment.customer_busy, appointment.in_the_past, appointment.outside_opening_hours, staff.inactive, …): nobody moved")))]
pub async fn move_group(
    State(pool): State<PgPool>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    IfMatch(version): IfMatch,
    Json(input): Json<AppointmentGroupMove>,
) -> Result<Response, AppError> {
    let moved = service::move_group(&pool, &user.actor, id, version, &input).await?;
    Ok(no_store(StatusCode::OK, moved, None))
}

#[utoipa::path(post, path = "/api/v1/appointments/{id}/complete", tag = "appointments", request_body = AppointmentComplete,
    params(("id" = Uuid, Path),
        ("Idempotency-Key" = Uuid, Header, description = "One key per completion; a retry replays the first response")),
    responses((status = 200, body = AppointmentCompleted), (status = 400), (status = 401),
        (status = 403, description = "auth.forbidden / day.locked / discount.limit_exceeded"), (status = 404),
        (status = 409, description = "appointment.closed, package.no_sessions_left / expired / cancelled"),
        (status = 422, description = "appointment.customer_required, and every 422 of POST /visits")))]
pub async fn complete(
    State(pool): State<PgPool>,
    State(cipher): State<Arc<FieldCipher>>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    IdempotencyKey(key): IdempotencyKey,
    body: Bytes,
) -> Result<Response, AppError> {
    let (input, canonical): (AppointmentComplete, _) = parse_canonical(&body)?;
    let (status, body) = service::complete(
        &pool,
        &cipher,
        &user.actor,
        key,
        hash_route("POST", "/appointments/{id}/complete", Some(id), &canonical),
        id,
        input,
    )
    .await?;
    let version = body["appointment"]["version"].as_i64();
    Ok(no_store(
        StatusCode::from_u16(status).unwrap_or(StatusCode::OK),
        body,
        version,
    ))
}

#[utoipa::path(post, path = "/api/v1/appointments/{id}/cancel", tag = "appointments", request_body = AppointmentClose,
    params(("id" = Uuid, Path)),
    responses((status = 200, body = AppointmentDto), (status = 401),
        (status = 403, description = "auth.forbidden / day.locked"), (status = 404),
        (status = 409, description = "appointment.closed"), (status = 422, description = "reason required")))]
pub async fn cancel(
    State(pool): State<PgPool>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<AppointmentClose>,
) -> Result<Response, AppError> {
    let a = service::close(
        &pool,
        &user.actor,
        id,
        Outcome::Cancelled,
        input.reason.as_deref(),
    )
    .await?;
    let version = a.version;
    Ok(no_store(StatusCode::OK, a, Some(version)))
}

#[utoipa::path(post, path = "/api/v1/appointments/{id}/no-show", tag = "appointments", request_body = AppointmentClose,
    params(("id" = Uuid, Path)),
    responses((status = 200, body = AppointmentDto), (status = 401),
        (status = 403, description = "auth.forbidden / day.locked"), (status = 404),
        (status = 409, description = "appointment.closed / already_arrived"),
        (status = 422, description = "reason required, appointment.not_started")))]
pub async fn no_show(
    State(pool): State<PgPool>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<AppointmentClose>,
) -> Result<Response, AppError> {
    let a = service::close(
        &pool,
        &user.actor,
        id,
        Outcome::NoShow,
        input.reason.as_deref(),
    )
    .await?;
    let version = a.version;
    Ok(no_store(StatusCode::OK, a, Some(version)))
}
