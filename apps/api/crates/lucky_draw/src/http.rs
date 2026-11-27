//! Lucky draw endpoints (spec lucky-draw, contract architecture/lucky-draw.md §3). Every route needs
//! `lucky_draw.manage`; the right is checked first, before the id, the key and the body are read
//! (F17), so a caller without it learns nothing about what a route takes. Every write answers with
//! the whole draw (`LuckyDrawDetail`) and its version as `ETag`.

use crate::domain::{self, Eligible, Gender, OsRandom, Status};
use crate::repo::{ParticipantRow, ResultRow};
use crate::service::{self, Detail, ParticipantInput, PrizeInput, PrizeView, Summary};
use axum::body::Bytes;
use axum::extract::rejection::PathRejection;
use axum::extract::{FromRef, Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use identity::AuthUser;
use kernel::http::{IdempotencyKey, IfMatch, etag};
use kernel::idempotency::hash_route;
use kernel::{AppError, FieldError, Permission};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

/// Body limit of `POST /lucky-draws/{id}/participants`: 500 names of up to 120 characters (3 bytes
/// each in UTF-8) do not fit the API's default 64 KB. Applied by `app::middleware` to that route
/// alone ([`PARTICIPANTS_ROUTE`]).
pub const PARTICIPANTS_MAX_BODY_BYTES: usize = 256 * 1024;
/// The `POST` route (path template, `/api/v1` included) that takes [`PARTICIPANTS_MAX_BODY_BYTES`].
pub const PARTICIPANTS_ROUTE: &str = "/api/v1/lucky-draws/{id}/participants";

// ---------------------------------------------------------------- shapes (contract §3)

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LuckyDrawStatus {
    /// No result yet ("Chưa quay").
    NotStarted,
    /// Results, and some prize can still be drawn ("Đang quay").
    InProgress,
    /// Results, and no prize has both a slot and an eligible person left ("Đã xong").
    Finished,
}

impl From<Status> for LuckyDrawStatus {
    fn from(s: Status) -> Self {
        match s {
            Status::NotStarted => Self::NotStarted,
            Status::InProgress => Self::InProgress,
            Status::Finished => Self::Finished,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LuckyDrawGender {
    Female,
    Male,
}

/// Who a prize is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LuckyDrawEligible {
    /// "Cả nam và nữ".
    All,
    /// "Chỉ nữ".
    Female,
    /// "Chỉ nam".
    Male,
}

/// One line of `GET /lucky-draws`.
#[derive(Debug, Serialize, ToSchema)]
pub struct LuckyDrawSummary {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub participant_count: i64,
    /// Every prize's quantity, summed.
    pub prize_slots: i64,
    /// Results not forfeited.
    pub awarded: i64,
    pub status: LuckyDrawStatus,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LuckyDrawList {
    /// Newest first.
    pub items: Vec<LuckyDrawSummary>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LuckyDrawParticipant {
    pub id: Uuid,
    pub name: String,
    pub gender: LuckyDrawGender,
    pub position: i32,
    /// Send back as `If-Match` when editing this person.
    pub version: i64,
    /// Their result, forfeited or not: someone with a result is out of every later spin and can no
    /// longer be edited or deleted.
    #[schema(required = true)]
    pub result_id: Option<Uuid>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LuckyDrawPrize {
    pub id: Uuid,
    pub name: String,
    #[schema(required = true)]
    pub description: Option<String>,
    pub eligible: LuckyDrawEligible,
    pub quantity: i32,
    /// Results not forfeited.
    pub awarded: i64,
    /// `quantity − awarded`.
    pub remaining: i64,
    /// People still in the draw who may win it (no result yet, gender admitted).
    pub eligible_count: i64,
    pub position: i32,
    /// Send back as `If-Match` when editing this prize.
    pub version: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LuckyDrawResult {
    pub id: Uuid,
    /// Spin number in the draw: 1, 2, 3…
    pub seq: i32,
    pub prize_id: Uuid,
    pub participant_id: Uuid,
    pub drawn_at: DateTime<Utc>,
    /// "Vắng mặt, trả giải": the slot went back to the prize, the person stays out of the draw.
    #[schema(required = true)]
    pub forfeited_at: Option<DateTime<Utc>>,
}

/// A whole draw: what every read and write of it answers.
#[derive(Debug, Serialize, ToSchema)]
pub struct LuckyDrawDetail {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    /// Goes up on every write inside the draw; send back as `If-Match` when renaming it.
    pub version: i64,
    pub status: LuckyDrawStatus,
    /// `status = finished`: at least one result and no prize with a slot and an eligible person.
    pub finished: bool,
    /// In entry order (`position`).
    pub participants: Vec<LuckyDrawParticipant>,
    /// In order (`position`).
    pub prizes: Vec<LuckyDrawPrize>,
    /// In spin order (`seq`).
    pub results: Vec<LuckyDrawResult>,
}

/// The result a spin just recorded.
#[derive(Debug, Serialize, ToSchema)]
pub struct LuckyDrawSpinResult {
    pub id: Uuid,
    pub seq: i32,
    pub prize_id: Uuid,
    pub participant_id: Uuid,
    pub drawn_at: DateTime<Utc>,
}

/// `POST /lucky-draws/{id}/spins`: the result, and the whole draw after it.
#[derive(Debug, Serialize, ToSchema)]
pub struct SpinOutcome {
    pub result: LuckyDrawSpinResult,
    pub draw: LuckyDrawDetail,
}

/// Create or rename a draw.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LuckyDrawWrite {
    /// 1–80 characters.
    pub name: String,
}

/// One person: name and gender ("Nữ" / "Nam", required).
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LuckyDrawParticipantWrite {
    /// 1–120 characters.
    pub name: String,
    /// `female` | `male`; anything else is `participants[i].gender invalid`.
    #[schema(value_type = LuckyDrawGender)]
    pub gender: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LuckyDrawParticipantsAdd {
    /// 1–500 people, all added or none; at most 2.000 in the draw.
    pub participants: Vec<LuckyDrawParticipantWrite>,
}

/// Create or replace a prize.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LuckyDrawPrizeWrite {
    /// 1–140 characters.
    pub name: String,
    /// Up to 250 characters; blank or absent = none.
    #[serde(default)]
    pub description: Option<String>,
    /// `all` | `female` | `male`; cannot change once the prize has a result.
    #[schema(value_type = LuckyDrawEligible)]
    pub eligible: String,
    /// 1–1000, never below what the prize awarded.
    pub quantity: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LuckyDrawSpin {
    /// A prize of this draw; the server picks the winner.
    pub prize_id: Uuid,
}

// ---------------------------------------------------------------- mapping

fn gender_dto(code: &str) -> Result<LuckyDrawGender, AppError> {
    match Gender::parse(code) {
        Some(Gender::Female) => Ok(LuckyDrawGender::Female),
        Some(Gender::Male) => Ok(LuckyDrawGender::Male),
        None => Err(AppError::Internal(
            "unknown stored participant gender".into(),
        )),
    }
}

fn eligible_dto(code: &str) -> Result<LuckyDrawEligible, AppError> {
    match Eligible::parse(code) {
        Some(Eligible::All) => Ok(LuckyDrawEligible::All),
        Some(Eligible::Female) => Ok(LuckyDrawEligible::Female),
        Some(Eligible::Male) => Ok(LuckyDrawEligible::Male),
        None => Err(AppError::Internal(
            "unknown stored prize eligibility".into(),
        )),
    }
}

fn participant_dto(p: ParticipantRow) -> Result<LuckyDrawParticipant, AppError> {
    Ok(LuckyDrawParticipant {
        gender: gender_dto(&p.gender)?,
        id: p.id,
        name: p.name,
        position: p.position,
        version: p.version,
        result_id: p.result_id,
    })
}

fn prize_dto(p: PrizeView) -> Result<LuckyDrawPrize, AppError> {
    Ok(LuckyDrawPrize {
        eligible: eligible_dto(&p.row.eligible)?,
        id: p.row.id,
        name: p.row.name,
        description: p.row.description,
        quantity: p.row.quantity,
        awarded: p.row.awarded,
        remaining: i64::from(p.tally.remaining()),
        eligible_count: i64::from(p.tally.eligible_count),
        position: p.row.position,
        version: p.row.version,
    })
}

fn result_dto(r: ResultRow) -> LuckyDrawResult {
    LuckyDrawResult {
        id: r.id,
        seq: r.seq,
        prize_id: r.prize_id,
        participant_id: r.participant_id,
        drawn_at: r.drawn_at,
        forfeited_at: r.forfeited_at,
    }
}

fn detail_dto(d: Detail) -> Result<LuckyDrawDetail, AppError> {
    Ok(LuckyDrawDetail {
        id: d.draw.id,
        name: d.draw.name,
        created_at: d.draw.created_at,
        version: d.draw.version,
        status: d.status.into(),
        finished: d.status.finished(),
        participants: d
            .participants
            .into_iter()
            .map(participant_dto)
            .collect::<Result<_, _>>()?,
        prizes: d
            .prizes
            .into_iter()
            .map(prize_dto)
            .collect::<Result<_, _>>()?,
        results: d.results.into_iter().map(result_dto).collect(),
    })
}

fn summary_dto(s: Summary) -> LuckyDrawSummary {
    LuckyDrawSummary {
        id: s.id,
        name: s.name,
        created_at: s.created_at,
        participant_count: s.participant_count,
        prize_slots: s.prize_slots,
        awarded: s.awarded,
        status: s.status.into(),
    }
}

/// `body` as JSON with its version as `ETag`.
fn versioned<T: Serialize>(status: StatusCode, version: i64, body: T) -> Response {
    let mut response = (status, Json(body)).into_response();
    response.headers_mut().insert(header::ETAG, etag(version));
    response
}

fn draw_response(status: StatusCode, d: Detail) -> Result<Response, AppError> {
    let version = d.draw.version;
    Ok(versioned(status, version, detail_dto(d)?))
}

/// The right first (F17), then the path: an id that is not a UUID is `400`.
fn authorized<T>(user: &AuthUser, path: Result<Path<T>, PathRejection>) -> Result<T, AppError> {
    user.actor.require(Permission::LuckyDrawManage)?;
    path.map(|Path(p)| p).map_err(|_| AppError::BadRequest {
        code: "path.invalid",
    })
}

/// A JSON body as its canonical bytes (re-serialised, so whitespace or key order never makes a
/// retry an idempotency mismatch) and as `T`. Any malformed body is `422 body invalid`.
fn read_json<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<(Vec<u8>, T), AppError> {
    let invalid = || AppError::Validation(vec![FieldError::new("body", "invalid")]);
    let value: serde_json::Value = serde_json::from_slice(body).map_err(|_| invalid())?;
    let canonical = serde_json::to_vec(&value).map_err(|e| AppError::Internal(e.to_string()))?;
    let parsed = serde_json::from_value(value).map_err(|_| invalid())?;
    Ok((canonical, parsed))
}

fn fail_if_any(errors: Vec<FieldError>) -> Result<(), AppError> {
    if errors.is_empty() {
        Ok(())
    } else {
        Err(AppError::Validation(errors))
    }
}

fn participant_input(
    w: &LuckyDrawParticipantWrite,
    at: Option<usize>,
    errors: &mut Vec<FieldError>,
) -> Option<ParticipantInput> {
    let field = |f: &str| match at {
        Some(i) => format!("participants[{i}].{f}"),
        None => f.to_owned(),
    };
    let name = domain::required_text(&w.name, field("name"), domain::PARTICIPANT_NAME_MAX, errors);
    let gender = Gender::parse(&w.gender);
    if gender.is_none() {
        errors.push(FieldError::new(field("gender"), "invalid"));
    }
    Some(ParticipantInput {
        name,
        gender: gender?,
    })
}

fn prize_input(w: &LuckyDrawPrizeWrite) -> Result<PrizeInput, AppError> {
    let mut errors = Vec::new();
    let name = domain::required_text(&w.name, "name", domain::PRIZE_NAME_MAX, &mut errors);
    let description = domain::optional_text(
        w.description.as_deref(),
        "description",
        domain::DESCRIPTION_MAX,
        &mut errors,
    );
    let eligible = Eligible::parse(&w.eligible);
    if eligible.is_none() {
        errors.push(FieldError::new("eligible", "invalid"));
    }
    let quantity = i32::try_from(w.quantity)
        .ok()
        .filter(|_| domain::QUANTITY_RANGE.contains(&w.quantity));
    if quantity.is_none() {
        errors.push(FieldError::new("quantity", "out_of_range"));
    }
    fail_if_any(errors)?;
    match (eligible, quantity) {
        (Some(eligible), Some(quantity)) => Ok(PrizeInput {
            name,
            description,
            eligible,
            quantity,
        }),
        _ => Err(AppError::Internal("prize input checked above".into())),
    }
}

// ---------------------------------------------------------------- routes

pub fn routes<S>() -> Router<S>
where
    PgPool: FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/lucky-draws", get(list_draws).post(create_draw))
        .route(
            "/lucky-draws/{id}",
            get(get_draw).put(rename_draw).delete(delete_draw),
        )
        .route("/lucky-draws/{id}/participants", post(add_participants))
        .route(
            "/lucky-draws/{id}/participants/{pid}",
            put(update_participant).delete(delete_participant),
        )
        .route("/lucky-draws/{id}/prizes", post(create_prize))
        .route(
            "/lucky-draws/{id}/prizes/{qid}",
            put(update_prize).delete(delete_prize),
        )
        .route("/lucky-draws/{id}/spins", post(spin))
        .route(
            "/lucky-draws/{id}/results/{rid}/forfeit",
            post(forfeit_result),
        )
}

#[utoipa::path(get, path = "/api/v1/lucky-draws", tag = "lucky-draws",
    responses((status = 200, body = LuckyDrawList, description = "Every draw of the spa, newest first"),
        (status = 401), (status = 403, description = "auth.forbidden: needs lucky_draw.manage (the owner)")))]
pub async fn list_draws(State(pool): State<PgPool>, user: AuthUser) -> Result<Response, AppError> {
    let items = service::list(&pool, &user.actor).await?;
    Ok(Json(LuckyDrawList {
        items: items.into_iter().map(summary_dto).collect(),
    })
    .into_response())
}

#[utoipa::path(post, path = "/api/v1/lucky-draws", tag = "lucky-draws", request_body = LuckyDrawWrite,
    responses((status = 201, body = LuckyDrawDetail, description = "Created (ETag = version); audited as lucky_draw.create"),
        (status = 401), (status = 403),
        (status = 422, description = "validation_failed: name required / too_long, body invalid")))]
pub async fn create_draw(
    State(pool): State<PgPool>,
    user: AuthUser,
    body: Bytes,
) -> Result<Response, AppError> {
    user.actor.require(Permission::LuckyDrawManage)?;
    let (_, b): (_, LuckyDrawWrite) = read_json(&body)?;
    draw_response(
        StatusCode::CREATED,
        service::create(&pool, &user.actor, &b.name).await?,
    )
}

#[utoipa::path(get, path = "/api/v1/lucky-draws/{id}", tag = "lucky-draws", params(("id" = Uuid, Path)),
    responses((status = 200, body = LuckyDrawDetail, description = "ETag = version"),
        (status = 400, description = "path.invalid: id is not a UUID"), (status = 401), (status = 403),
        (status = 404, description = "No such draw in this spa")))]
pub async fn get_draw(
    State(pool): State<PgPool>,
    user: AuthUser,
    id: Result<Path<Uuid>, PathRejection>,
) -> Result<Response, AppError> {
    let id = authorized(&user, id)?;
    draw_response(StatusCode::OK, service::get(&pool, &user.actor, id).await?)
}

#[utoipa::path(put, path = "/api/v1/lucky-draws/{id}", tag = "lucky-draws", request_body = LuckyDrawWrite,
    params(("id" = Uuid, Path), ("If-Match" = String, Header, description = "The draw's version")),
    responses((status = 200, body = LuckyDrawDetail, description = "Renamed; audited as lucky_draw.rename"),
        (status = 400, description = "precondition.if_match_required / if_match_invalid; path.invalid"),
        (status = 401), (status = 403), (status = 404),
        (status = 412, description = "conflict.stale_version"),
        (status = 422, description = "validation_failed: name required / too_long, body invalid")))]
pub async fn rename_draw(
    State(pool): State<PgPool>,
    user: AuthUser,
    id: Result<Path<Uuid>, PathRejection>,
    version: Result<IfMatch, AppError>,
    body: Bytes,
) -> Result<Response, AppError> {
    let id = authorized(&user, id)?;
    let IfMatch(version) = version?;
    let (_, b): (_, LuckyDrawWrite) = read_json(&body)?;
    draw_response(
        StatusCode::OK,
        service::rename(&pool, &user.actor, id, version, &b.name).await?,
    )
}

#[utoipa::path(delete, path = "/api/v1/lucky-draws/{id}", tag = "lucky-draws", params(("id" = Uuid, Path)),
    responses((status = 204, description = "Deleted with its participants, prizes and results; audited as lucky_draw.delete (name and counts)"),
        (status = 400), (status = 401), (status = 403), (status = 404)))]
pub async fn delete_draw(
    State(pool): State<PgPool>,
    user: AuthUser,
    id: Result<Path<Uuid>, PathRejection>,
) -> Result<StatusCode, AppError> {
    let id = authorized(&user, id)?;
    service::delete(&pool, &user.actor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(post, path = "/api/v1/lucky-draws/{id}/participants", tag = "lucky-draws",
    request_body = LuckyDrawParticipantsAdd, params(("id" = Uuid, Path)),
    responses((status = 201, body = LuckyDrawDetail, description = "All added, in order; audited as lucky_draw.participants_add (the count)"),
        (status = 400), (status = 401), (status = 403), (status = 404),
        (status = 413, description = "payload_too_large: body over 256 KB"),
        (status = 422, description = "validation_failed, nobody added: participants[i].name required / too_long, participants[i].gender invalid, participants count_out_of_range (0 or over 500 in one request), participants too_many (over 2.000 in the draw), body invalid")))]
pub async fn add_participants(
    State(pool): State<PgPool>,
    user: AuthUser,
    id: Result<Path<Uuid>, PathRejection>,
    body: Bytes,
) -> Result<Response, AppError> {
    let id = authorized(&user, id)?;
    let (_, b): (_, LuckyDrawParticipantsAdd) = read_json(&body)?;
    if b.participants.is_empty() || b.participants.len() > domain::PARTICIPANTS_PER_REQUEST {
        return Err(AppError::Validation(vec![FieldError::new(
            "participants",
            "count_out_of_range",
        )]));
    }
    let mut errors = Vec::new();
    let people: Vec<ParticipantInput> = b
        .participants
        .iter()
        .enumerate()
        .filter_map(|(i, w)| participant_input(w, Some(i), &mut errors))
        .collect();
    fail_if_any(errors)?;
    draw_response(
        StatusCode::CREATED,
        service::add_participants(&pool, &user.actor, id, people).await?,
    )
}

#[utoipa::path(put, path = "/api/v1/lucky-draws/{id}/participants/{pid}", tag = "lucky-draws",
    request_body = LuckyDrawParticipantWrite,
    params(("id" = Uuid, Path), ("pid" = Uuid, Path),
        ("If-Match" = String, Header, description = "The person's version")),
    responses((status = 200, body = LuckyDrawDetail, description = "Saved; audited as lucky_draw.participant_update"),
        (status = 400), (status = 401), (status = 403), (status = 404),
        (status = 409, description = "lucky_draw.participant_drawn: they have a result (forfeited or not)"),
        (status = 412, description = "conflict.stale_version"),
        (status = 422, description = "validation_failed: name required / too_long, gender invalid, body invalid")))]
pub async fn update_participant(
    State(pool): State<PgPool>,
    user: AuthUser,
    ids: Result<Path<(Uuid, Uuid)>, PathRejection>,
    version: Result<IfMatch, AppError>,
    body: Bytes,
) -> Result<Response, AppError> {
    let (id, participant) = authorized(&user, ids)?;
    let IfMatch(version) = version?;
    let (_, b): (_, LuckyDrawParticipantWrite) = read_json(&body)?;
    let mut errors = Vec::new();
    let input = participant_input(&b, None, &mut errors);
    fail_if_any(errors)?;
    let input = input.ok_or_else(|| AppError::Internal("participant checked above".into()))?;
    draw_response(
        StatusCode::OK,
        service::update_participant(&pool, &user.actor, id, participant, version, input).await?,
    )
}

#[utoipa::path(delete, path = "/api/v1/lucky-draws/{id}/participants/{pid}", tag = "lucky-draws",
    params(("id" = Uuid, Path), ("pid" = Uuid, Path)),
    responses((status = 200, body = LuckyDrawDetail, description = "Deleted; audited as lucky_draw.participant_delete"),
        (status = 400), (status = 401), (status = 403), (status = 404),
        (status = 409, description = "lucky_draw.participant_drawn")))]
pub async fn delete_participant(
    State(pool): State<PgPool>,
    user: AuthUser,
    ids: Result<Path<(Uuid, Uuid)>, PathRejection>,
) -> Result<Response, AppError> {
    let (id, participant) = authorized(&user, ids)?;
    draw_response(
        StatusCode::OK,
        service::delete_participant(&pool, &user.actor, id, participant).await?,
    )
}

#[utoipa::path(post, path = "/api/v1/lucky-draws/{id}/prizes", tag = "lucky-draws",
    request_body = LuckyDrawPrizeWrite, params(("id" = Uuid, Path)),
    responses((status = 201, body = LuckyDrawDetail, description = "Added last; audited as lucky_draw.prize_create"),
        (status = 400), (status = 401), (status = 403), (status = 404),
        (status = 422, description = "validation_failed: name required / too_long, description too_long, eligible invalid, quantity out_of_range, prizes too_many (over 50), body invalid")))]
pub async fn create_prize(
    State(pool): State<PgPool>,
    user: AuthUser,
    id: Result<Path<Uuid>, PathRejection>,
    body: Bytes,
) -> Result<Response, AppError> {
    let id = authorized(&user, id)?;
    let (_, b): (_, LuckyDrawPrizeWrite) = read_json(&body)?;
    let input = prize_input(&b)?;
    draw_response(
        StatusCode::CREATED,
        service::create_prize(&pool, &user.actor, id, input).await?,
    )
}

#[utoipa::path(put, path = "/api/v1/lucky-draws/{id}/prizes/{qid}", tag = "lucky-draws",
    request_body = LuckyDrawPrizeWrite,
    params(("id" = Uuid, Path), ("qid" = Uuid, Path),
        ("If-Match" = String, Header, description = "The prize's version")),
    responses((status = 200, body = LuckyDrawDetail, description = "Saved; audited as lucky_draw.prize_update"),
        (status = 400), (status = 401), (status = 403), (status = 404),
        (status = 409, description = "lucky_draw.prize_drawn: `eligible` changed on a prize with a result (forfeited or not)"),
        (status = 412, description = "conflict.stale_version"),
        (status = 422, description = "lucky_draw.quantity_below_awarded; validation_failed as for POST")))]
pub async fn update_prize(
    State(pool): State<PgPool>,
    user: AuthUser,
    ids: Result<Path<(Uuid, Uuid)>, PathRejection>,
    version: Result<IfMatch, AppError>,
    body: Bytes,
) -> Result<Response, AppError> {
    let (id, prize) = authorized(&user, ids)?;
    let IfMatch(version) = version?;
    let (_, b): (_, LuckyDrawPrizeWrite) = read_json(&body)?;
    let input = prize_input(&b)?;
    draw_response(
        StatusCode::OK,
        service::update_prize(&pool, &user.actor, id, prize, version, input).await?,
    )
}

#[utoipa::path(delete, path = "/api/v1/lucky-draws/{id}/prizes/{qid}", tag = "lucky-draws",
    params(("id" = Uuid, Path), ("qid" = Uuid, Path)),
    responses((status = 200, body = LuckyDrawDetail, description = "Deleted; audited as lucky_draw.prize_delete"),
        (status = 400), (status = 401), (status = 403), (status = 404),
        (status = 409, description = "lucky_draw.prize_drawn: it has a result (forfeited or not)")))]
pub async fn delete_prize(
    State(pool): State<PgPool>,
    user: AuthUser,
    ids: Result<Path<(Uuid, Uuid)>, PathRejection>,
) -> Result<Response, AppError> {
    let (id, prize) = authorized(&user, ids)?;
    draw_response(
        StatusCode::OK,
        service::delete_prize(&pool, &user.actor, id, prize).await?,
    )
}

#[utoipa::path(post, path = "/api/v1/lucky-draws/{id}/spins", tag = "lucky-draws",
    request_body = LuckyDrawSpin,
    params(("id" = Uuid, Path),
        ("Idempotency-Key" = Uuid, Header, description = "One key per spin attempt: a retry (network error, double click) answers with the result the first request recorded, never a second spin")),
    responses((status = 201, body = SpinOutcome, description = "The server picked the winner uniformly among the people still eligible (OS CSPRNG) and recorded the draw's next spin; audited as lucky_draw.spin"),
        (status = 400, description = "idempotency.key_required / key_invalid; path.invalid"),
        (status = 401), (status = 403),
        (status = 404, description = "No such draw, or the prize is not one of this draw's"),
        (status = 409, description = "lucky_draw.prize_exhausted: no slot left; lucky_draw.no_eligible: nobody left who may win it"),
        (status = 422, description = "validation_failed: body invalid; idempotency.key_reuse_mismatch")))]
pub async fn spin(
    State(pool): State<PgPool>,
    user: AuthUser,
    id: Result<Path<Uuid>, PathRejection>,
    key: Result<IdempotencyKey, AppError>,
    body: Bytes,
) -> Result<Response, AppError> {
    let id = authorized(&user, id)?;
    let IdempotencyKey(key) = key?;
    let (canonical, b): (_, LuckyDrawSpin) = read_json(&body)?;
    let hash = hash_route("POST", "/lucky-draws/{id}/spins", Some(id), &canonical);
    let (result, d) =
        service::spin(&pool, &user.actor, key, hash, id, b.prize_id, &mut OsRandom).await?;
    let version = d.draw.version;
    Ok(versioned(
        StatusCode::CREATED,
        version,
        SpinOutcome {
            result: LuckyDrawSpinResult {
                id: result.id,
                seq: result.seq,
                prize_id: result.prize_id,
                participant_id: result.participant_id,
                drawn_at: result.drawn_at,
            },
            draw: detail_dto(d)?,
        },
    ))
}

#[utoipa::path(post, path = "/api/v1/lucky-draws/{id}/results/{rid}/forfeit", tag = "lucky-draws",
    params(("id" = Uuid, Path), ("rid" = Uuid, Path)),
    responses((status = 200, body = LuckyDrawDetail, description = "Forfeited: the prize gets the slot back, the person stays out of the draw; audited as lucky_draw.forfeit"),
        (status = 400), (status = 401), (status = 403), (status = 404),
        (status = 409, description = "lucky_draw.result_forfeited: already forfeited")))]
pub async fn forfeit_result(
    State(pool): State<PgPool>,
    user: AuthUser,
    ids: Result<Path<(Uuid, Uuid)>, PathRejection>,
) -> Result<Response, AppError> {
    let (id, result) = authorized(&user, ids)?;
    draw_response(
        StatusCode::OK,
        service::forfeit(&pool, &user.actor, id, result).await?,
    )
}
