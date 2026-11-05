//! Customer endpoints (US2). The cipher reaches handlers through state (`Arc<FieldCipher>: FromRef<S>`).

use crate::cipher::FieldCipher;
use crate::follow_up::{self, Channel, FollowUp, FollowUpInput, Outcome};
use crate::repo::SummaryRow;
use crate::service::{self, CustomerView, Gender, PhoneCheck, ProfileInput};
use axum::body::Bytes;
use axum::extract::{FromRef, Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, put};
use axum::{Json, Router};
use chrono::{DateTime, NaiveDate, Utc};
use identity::AuthUser;
use kernel::http::{IdempotencyKey, IfMatch, etag};
use kernel::idempotency::hash_route;
use kernel::{AppError, PhoneE164};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GenderDto {
    Female,
    Male,
}

impl From<GenderDto> for Gender {
    fn from(g: GenderDto) -> Self {
        match g {
            GenderDto::Female => Gender::Female,
            GenderDto::Male => Gender::Male,
        }
    }
}

/// The stored `customer.gender` text as the API shows it.
pub fn gender_dto(value: Option<&str>) -> Option<GenderDto> {
    Gender::parse(value?).map(|g| match g {
        Gender::Female => GenderDto::Female,
        Gender::Male => GenderDto::Male,
    })
}

fn national(phone_e164: &str) -> String {
    PhoneE164::parse(phone_e164)
        .map(|p| p.display_national())
        .unwrap_or_else(|_| phone_e164.to_owned())
}

/// Search hit for the counter: never contains notes.
#[derive(Debug, Serialize, ToSchema)]
pub struct CustomerSummaryDto {
    pub id: Uuid,
    pub full_name: String,
    /// National display form, e.g. "0905 123 456".
    pub phone: String,
    pub gender: Option<GenderDto>,
}

impl From<SummaryRow> for CustomerSummaryDto {
    fn from(r: SummaryRow) -> Self {
        Self {
            id: r.id,
            phone: national(&r.phone_e164),
            full_name: r.full_name,
            gender: gender_dto(r.gender.as_deref()),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CustomerDto {
    pub id: Uuid,
    pub full_name: String,
    pub phone: String,
    pub phone_e164: String,
    pub gender: Option<GenderDto>,
    pub birth_year: Option<i16>,
    pub address: Option<String>,
    pub source: Option<String>,
    pub note: Option<String>,
    /// Present only for callers with `customer.health_notes.read`; null = no note.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>)]
    pub health_note: Option<Option<String>>,
    /// Send back as `If-Match` when updating.
    pub version: i64,
    pub created_at: DateTime<Utc>,
}

impl From<CustomerView> for CustomerDto {
    fn from(v: CustomerView) -> Self {
        let r = v.row;
        Self {
            id: r.id,
            phone: national(&r.phone_e164),
            phone_e164: r.phone_e164,
            gender: gender_dto(r.gender.as_deref()),
            full_name: r.full_name,
            birth_year: r.birth_year,
            address: r.address,
            source: r.source,
            note: r.note,
            health_note: v.health_note,
            version: r.version,
            created_at: r.created_at,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CustomerCreate {
    pub full_name: String,
    pub phone: String,
    pub gender: Option<GenderDto>,
    pub birth_year: Option<i16>,
    pub address: Option<String>,
    pub source: Option<String>,
    pub note: Option<String>,
    /// Requires `customer.health_notes.write`.
    pub health_note: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CustomerUpdate {
    pub full_name: String,
    pub phone: String,
    pub gender: Option<GenderDto>,
    pub birth_year: Option<i16>,
    pub address: Option<String>,
    pub source: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HealthNoteWrite {
    /// null or empty clears the note.
    pub health_note: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct SearchQuery {
    /// ≥ 3 digits of the phone, or ≥ 2 letters of the name (accents optional). Empty = most recent.
    pub q: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct PhoneCheckQuery {
    /// The number as typed (any spacing, 0… or +84…). Normalised like `POST /customers`.
    pub phone: Option<String>,
}

/// `GET /customers/phone-check`. `taken` → `existing_id` + `full_name` name the profile to open
/// instead of creating a duplicate (create would answer `409 customer.phone_taken`).
#[derive(Debug, Serialize, ToSchema)]
pub struct PhoneCheckDto {
    /// The input is a complete, valid phone number (false while still typing).
    pub valid: bool,
    pub taken: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub existing_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_name: Option<String>,
}

impl From<PhoneCheck> for PhoneCheckDto {
    fn from(c: PhoneCheck) -> Self {
        match c {
            PhoneCheck::Invalid => Self {
                valid: false,
                taken: false,
                existing_id: None,
                full_name: None,
            },
            PhoneCheck::Free => Self {
                valid: true,
                taken: false,
                existing_id: None,
                full_name: None,
            },
            PhoneCheck::Taken { id, full_name } => Self {
                valid: true,
                taken: true,
                existing_id: Some(id),
                full_name: Some(full_name),
            },
        }
    }
}

// ------------------------------------------------------------------------------ follow-ups

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FollowUpChannelDto {
    Phone,
    Zalo,
    InPerson,
    Other,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FollowUpOutcomeDto {
    /// Reached; will come.
    WillCome,
    /// Reached; not interested.
    NotInterested,
    NoAnswer,
    Other,
}

impl From<FollowUpChannelDto> for Channel {
    fn from(c: FollowUpChannelDto) -> Self {
        match c {
            FollowUpChannelDto::Phone => Self::Phone,
            FollowUpChannelDto::Zalo => Self::Zalo,
            FollowUpChannelDto::InPerson => Self::InPerson,
            FollowUpChannelDto::Other => Self::Other,
        }
    }
}

impl From<Channel> for FollowUpChannelDto {
    fn from(c: Channel) -> Self {
        match c {
            Channel::Phone => Self::Phone,
            Channel::Zalo => Self::Zalo,
            Channel::InPerson => Self::InPerson,
            Channel::Other => Self::Other,
        }
    }
}

impl From<FollowUpOutcomeDto> for Outcome {
    fn from(o: FollowUpOutcomeDto) -> Self {
        match o {
            FollowUpOutcomeDto::WillCome => Self::WillCome,
            FollowUpOutcomeDto::NotInterested => Self::NotInterested,
            FollowUpOutcomeDto::NoAnswer => Self::NoAnswer,
            FollowUpOutcomeDto::Other => Self::Other,
        }
    }
}

impl From<Outcome> for FollowUpOutcomeDto {
    fn from(o: Outcome) -> Self {
        match o {
            Outcome::WillCome => Self::WillCome,
            Outcome::NotInterested => Self::NotInterested,
            Outcome::NoAnswer => Self::NoAnswer,
            Outcome::Other => Self::Other,
        }
    }
}

/// Body of `POST /customers/{id}/follow-ups`. The recorder is the signed-in user.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FollowUpCreate {
    /// Day of the contact; default today (spa time zone). Not in the future, at most 30 days back.
    pub contacted_on: Option<NaiveDate>,
    pub channel: FollowUpChannelDto,
    pub outcome: FollowUpOutcomeDto,
    /// ≤ 500 characters. A contact note, not a health note.
    pub note: Option<String>,
    /// When to try again (≥ `contacted_on`, ≤ 365 days after it). Omitted: `no_answer` → contact
    /// day + 3, `will_come` → + 7, `not_interested` / `other` → none.
    pub next_follow_up_on: Option<NaiveDate>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FollowUpDto {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub contacted_on: NaiveDate,
    pub channel: FollowUpChannelDto,
    pub outcome: FollowUpOutcomeDto,
    pub note: Option<String>,
    pub next_follow_up_on: Option<NaiveDate>,
    /// The user who recorded it.
    pub by_user_id: Uuid,
    pub by_name: String,
    pub created_at: DateTime<Utc>,
}

impl From<FollowUp> for FollowUpDto {
    fn from(f: FollowUp) -> Self {
        Self {
            id: f.id,
            customer_id: f.customer_id,
            contacted_on: f.contacted_on,
            channel: f.channel.into(),
            outcome: f.outcome.into(),
            note: f.note,
            next_follow_up_on: f.next_follow_up_on,
            by_user_id: f.by_user_id,
            by_name: f.by_name,
            created_at: f.created_at,
        }
    }
}

/// The next date the rules would store for a contact made today, per outcome (`null` = none).
/// The form pre-fills `next_follow_up_on` with it; whatever the user leaves there is sent.
#[derive(Debug, Serialize, ToSchema)]
pub struct SuggestedNextOn {
    /// Today + 7: check in if they still have not come.
    pub will_come: Option<NaiveDate>,
    /// None: they leave the "due" list for `not_interested_pause_days`, then come back once.
    pub not_interested: Option<NaiveDate>,
    /// Today + 3.
    pub no_answer: Option<NaiveDate>,
    pub other: Option<NaiveDate>,
}

/// `GET /customers/{id}/follow-ups`: newest contact first.
#[derive(Debug, Serialize, ToSchema)]
pub struct FollowUpList {
    /// At most 100.
    pub items: Vec<FollowUpDto>,
    /// More than 100 exist (only the newest are listed).
    pub has_more: bool,
    /// The spa's business day the suggestions are computed from.
    pub today: NaiveDate,
    pub suggested_next_on_by_outcome: SuggestedNextOn,
    /// Days a `not_interested` customer stays out of `GET /customers/inactive?follow_up=due`.
    pub not_interested_pause_days: u64,
}

fn profile(
    full_name: String,
    phone: String,
    gender: Option<GenderDto>,
    birth_year: Option<i16>,
    address: Option<String>,
    source: Option<String>,
    note: Option<String>,
) -> ProfileInput {
    ProfileInput {
        full_name,
        phone,
        gender: gender.map(Into::into),
        birth_year,
        address,
        source,
        note,
    }
}

fn versioned(status: StatusCode, dto: CustomerDto) -> Response {
    let version = dto.version;
    let mut response = (status, Json(dto)).into_response();
    response.headers_mut().insert(header::ETAG, etag(version));
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response
}

pub fn routes<S>() -> Router<S>
where
    PgPool: FromRef<S>,
    Arc<FieldCipher>: FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/customers", get(search).post(create))
        .route("/customers/phone-check", get(phone_check))
        .route("/customers/{id}", get(read).put(update))
        .route("/customers/{id}/health-note", put(set_health_note))
        .route(
            "/customers/{id}/follow-ups",
            get(list_follow_ups).post(record_follow_up),
        )
}

fn no_store(status: StatusCode, body: impl Serialize) -> Response {
    let mut response = (status, Json(body)).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response
}

#[utoipa::path(get, path = "/api/v1/customers/phone-check", tag = "customers", params(PhoneCheckQuery),
    responses(
        (status = 200, body = PhoneCheckDto, description = "valid=false while the number is incomplete; taken → existing_id + full_name"),
        (status = 401), (status = 403, description = "auth.forbidden: no customer.read"),
        (status = 422, description = "validation_failed: phone too_long")))]
pub async fn phone_check(
    State(pool): State<PgPool>,
    user: AuthUser,
    Query(query): Query<PhoneCheckQuery>,
) -> Result<Response, AppError> {
    let check = service::phone_check(&pool, &user.actor, query.phone.as_deref()).await?;
    Ok(no_store(StatusCode::OK, PhoneCheckDto::from(check)))
}

#[utoipa::path(post, path = "/api/v1/customers/{id}/follow-ups", tag = "customers", request_body = FollowUpCreate,
    params(("id" = Uuid, Path),
        ("Idempotency-Key" = Uuid, Header, description = "One key per follow-up form; a retry replays the first response")),
    responses(
        (status = 201, body = FollowUpDto, description = "Recorded (append-only), audited as customer.follow_up"),
        (status = 400, description = "idempotency.key_required / key_invalid"),
        (status = 401), (status = 403, description = "auth.forbidden: no customer.follow_up"), (status = 404),
        (status = 422, description = "validation_failed: contacted_on in_future / too_old, next_follow_up_on before_contact / too_far, note too_long, body invalid; idempotency.key_reuse_mismatch")))]
pub async fn record_follow_up(
    State(pool): State<PgPool>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    IdempotencyKey(key): IdempotencyKey,
    body: Bytes,
) -> Result<Response, AppError> {
    let invalid = || AppError::Validation(vec![kernel::FieldError::new("body", "invalid")]);
    // Hash the re-serialised JSON, so whitespace or key order never makes a retry a mismatch.
    let value: serde_json::Value = serde_json::from_slice(&body).map_err(|_| invalid())?;
    let canonical = serde_json::to_vec(&value).map_err(|e| AppError::Internal(e.to_string()))?;
    let b: FollowUpCreate = serde_json::from_value(value).map_err(|_| invalid())?;
    let hash = hash_route("POST", "/customers/{id}/follow-ups", Some(id), &canonical);
    let input = FollowUpInput {
        contacted_on: b.contacted_on,
        channel: b.channel.into(),
        outcome: b.outcome.into(),
        note: b.note,
        next_follow_up_on: b.next_follow_up_on,
    };
    let (status, body) = follow_up::record(&pool, &user.actor, key, hash, id, &input, |f| {
        serde_json::to_value(FollowUpDto::from(f)).map_err(|e| AppError::Internal(e.to_string()))
    })
    .await?;
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::CREATED);
    Ok(no_store(status, body))
}

#[utoipa::path(get, path = "/api/v1/customers/{id}/follow-ups", tag = "customers",
    params(("id" = Uuid, Path)),
    responses(
        (status = 200, body = FollowUpList),
        (status = 401), (status = 403, description = "auth.forbidden: no customer.read"), (status = 404)))]
pub async fn list_follow_ups(
    State(pool): State<PgPool>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let (items, has_more, s) = follow_up::list(&pool, &user.actor, id).await?;
    Ok(no_store(
        StatusCode::OK,
        FollowUpList {
            items: items.into_iter().map(Into::into).collect(),
            has_more,
            today: s.today,
            suggested_next_on_by_outcome: SuggestedNextOn {
                will_come: s.will_come,
                not_interested: s.not_interested,
                no_answer: s.no_answer,
                other: s.other,
            },
            not_interested_pause_days: follow_up::NOT_INTERESTED_PAUSE_DAYS,
        },
    ))
}

#[utoipa::path(get, path = "/api/v1/customers", tag = "customers", params(SearchQuery),
    responses((status = 200, body = [CustomerSummaryDto]), (status = 401), (status = 403), (status = 422)))]
pub async fn search(
    State(pool): State<PgPool>,
    user: AuthUser,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<CustomerSummaryDto>>, AppError> {
    let rows = service::search(&pool, &user.actor, query.q.as_deref()).await?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

#[utoipa::path(post, path = "/api/v1/customers", tag = "customers", request_body = CustomerCreate,
    responses((status = 201, body = CustomerDto), (status = 403), (status = 409, description = "customer.phone_taken with existing_id"), (status = 422)))]
pub async fn create(
    State(pool): State<PgPool>,
    State(cipher): State<Arc<FieldCipher>>,
    user: AuthUser,
    Json(b): Json<CustomerCreate>,
) -> Result<Response, AppError> {
    let input = profile(
        b.full_name,
        b.phone,
        b.gender,
        b.birth_year,
        b.address,
        b.source,
        b.note,
    );
    let v = service::create(&pool, &cipher, &user.actor, input, b.health_note.as_deref()).await?;
    Ok(versioned(StatusCode::CREATED, v.into()))
}

#[utoipa::path(get, path = "/api/v1/customers/{id}", tag = "customers", params(("id" = Uuid, Path)),
    responses((status = 200, body = CustomerDto), (status = 401), (status = 403), (status = 404)))]
pub async fn read(
    State(pool): State<PgPool>,
    State(cipher): State<Arc<FieldCipher>>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let v = service::get(&pool, &cipher, &user.actor, id).await?;
    Ok(versioned(StatusCode::OK, v.into()))
}

#[utoipa::path(put, path = "/api/v1/customers/{id}", tag = "customers", request_body = CustomerUpdate,
    params(("id" = Uuid, Path), ("If-Match" = String, Header)),
    responses((status = 200, body = CustomerDto), (status = 403), (status = 404), (status = 409), (status = 412), (status = 422)))]
pub async fn update(
    State(pool): State<PgPool>,
    State(cipher): State<Arc<FieldCipher>>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    IfMatch(version): IfMatch,
    Json(b): Json<CustomerUpdate>,
) -> Result<Response, AppError> {
    let input = profile(
        b.full_name,
        b.phone,
        b.gender,
        b.birth_year,
        b.address,
        b.source,
        b.note,
    );
    let v = service::update(&pool, &cipher, &user.actor, id, version, input).await?;
    Ok(versioned(StatusCode::OK, v.into()))
}

#[utoipa::path(put, path = "/api/v1/customers/{id}/health-note", tag = "customers", request_body = HealthNoteWrite,
    params(("id" = Uuid, Path), ("If-Match" = String, Header)),
    responses((status = 200, body = CustomerDto), (status = 403), (status = 404), (status = 412), (status = 422)))]
pub async fn set_health_note(
    State(pool): State<PgPool>,
    State(cipher): State<Arc<FieldCipher>>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    IfMatch(version): IfMatch,
    Json(b): Json<HealthNoteWrite>,
) -> Result<Response, AppError> {
    let v = service::set_health_note(
        &pool,
        &cipher,
        &user.actor,
        id,
        version,
        b.health_note.as_deref(),
    )
    .await?;
    Ok(versioned(StatusCode::OK, v.into()))
}
