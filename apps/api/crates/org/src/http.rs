//! `GET /staff` — picker for closer / technician.

use crate::service::{self, Staff, StaffStatus};
use axum::extract::{FromRef, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use identity::AuthUser;
use kernel::AppError;
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

#[derive(Debug, Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
pub struct StaffQuery {
    /// Default `active` (pickers only offer people still working here).
    #[param(inline)]
    pub status: Option<StaffFilter>,
}

pub fn routes<S>() -> Router<S>
where
    PgPool: FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new().route("/staff", get(list_staff))
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
