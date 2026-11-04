//! Customer endpoints (US2). The cipher reaches handlers through state (`Arc<FieldCipher>: FromRef<S>`).

use crate::cipher::FieldCipher;
use crate::repo::SummaryRow;
use crate::service::{self, CustomerView, Gender, ProfileInput};
use axum::extract::{FromRef, Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, put};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use identity::AuthUser;
use kernel::http::{IfMatch, etag};
use kernel::{AppError, PhoneE164};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
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

fn gender_dto(value: Option<&str>) -> Option<GenderDto> {
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
#[serde(deny_unknown_fields)]
pub struct SearchQuery {
    /// ≥ 3 digits of the phone, or ≥ 2 letters of the name (accents optional). Empty = most recent.
    pub q: Option<String>,
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
        .route("/customers/{id}", get(read).put(update))
        .route("/customers/{id}/health-note", put(set_health_note))
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
