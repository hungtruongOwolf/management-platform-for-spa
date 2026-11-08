//! Management view endpoints (spec management-views). All `GET`, all `Cache-Control: no-store`
//! (they carry customer and money data), all tenant-scoped by the session.

use crate::dto::{
    CustomerStats, DailyQuery, DailyReport, HistoryPage, HistoryQuery, InactivePage, InactiveQuery,
    LedgerPage, LedgerQuery, PackageOverviewPage, PackageUsage, PackagesQuery, RangeQuery,
    RangeReport,
};
use crate::notes_dto::{NotesPage, NotesQuery};
use crate::{customer, day, notes, overview};
use axum::extract::rejection::QueryRejection;
use axum::extract::{FromRef, Path, Query, State};
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use identity::AuthUser;
use kernel::crypto::FieldCipher;
use kernel::http::invalid_query;
use kernel::{AppError, Permission};
use serde::Serialize;
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
        .route("/reports/daily", get(daily_report))
        .route("/reports/range", get(range_report))
        .route("/reports/ledger", get(ledger))
        .route("/customers/inactive", get(inactive_customers))
        .route("/customers/{id}/history", get(customer_history))
        .route("/customers/{id}/summary", get(customer_summary))
        .route("/customer-packages/{id}/usage", get(package_usage))
        .route("/packages", get(list_packages))
        .route("/notes", get(list_notes))
}

fn no_store(body: impl Serialize) -> Response {
    let mut response = Json(body).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

#[utoipa::path(get, path = "/api/v1/reports/daily", tag = "reports", params(DailyQuery),
    responses(
        (status = 200, body = DailyReport, description = "Counts; `revenue` and `net_revenue` only with report.revenue.read"),
        (status = 401), (status = 403, description = "auth.forbidden: no report.read"),
        (status = 422, description = "validation_failed: date out_of_range")))]
pub async fn daily_report(
    State(pool): State<PgPool>,
    user: AuthUser,
    Query(query): Query<DailyQuery>,
) -> Result<Response, AppError> {
    Ok(no_store(day::daily(&pool, &user.actor, query.date).await?))
}

#[utoipa::path(get, path = "/api/v1/reports/range", tag = "reports", params(RangeQuery),
    responses(
        (status = 200, body = RangeReport, description = "Money fields only with report.revenue.read"),
        (status = 401), (status = 403, description = "auth.forbidden: no report.read"),
        (status = 422, description = "validation_failed: from after_to, to range_too_long (> 366 days by day/week, > 732 days by month/year; > 1830 with report.long_range), preset conflicts_with_range")))]
pub async fn range_report(
    State(pool): State<PgPool>,
    user: AuthUser,
    Query(query): Query<RangeQuery>,
) -> Result<Response, AppError> {
    Ok(no_store(day::range(&pool, &user.actor, &query).await?))
}

#[utoipa::path(get, path = "/api/v1/reports/ledger", tag = "reports", params(LedgerQuery),
    responses(
        (status = 200, body = LedgerPage),
        (status = 401), (status = 403, description = "auth.forbidden: no report.revenue.read"),
        (status = 422, description = "validation_failed: from after_to, to range_too_long (> 92 days), cursor invalid, limit out_of_range")))]
pub async fn ledger(
    State(pool): State<PgPool>,
    user: AuthUser,
    Query(query): Query<LedgerQuery>,
) -> Result<Response, AppError> {
    Ok(no_store(
        day::ledger(
            &pool,
            &user.actor,
            query.from,
            query.to,
            query.cursor.as_deref(),
            query.limit,
        )
        .await?,
    ))
}

#[utoipa::path(get, path = "/api/v1/customers/inactive", tag = "reports", params(InactiveQuery),
    responses(
        (status = 200, body = InactivePage),
        (status = 401), (status = 403, description = "auth.forbidden: needs customer.read + visit.read"),
        (status = 422, description = "validation_failed: days out_of_range, q too_short / too_long, cursor invalid, limit out_of_range")))]
pub async fn inactive_customers(
    State(pool): State<PgPool>,
    user: AuthUser,
    Query(query): Query<InactiveQuery>,
) -> Result<Response, AppError> {
    Ok(no_store(
        overview::inactive(&pool, &user.actor, &query).await?,
    ))
}

#[utoipa::path(get, path = "/api/v1/notes", tag = "reports", params(NotesQuery),
    responses(
        (status = 200, body = NotesPage, description = "Every note the caller may read, newest first; each kind under its own permission (encrypted notes decrypted only for customer.health_notes.read)"),
        (status = 401), (status = 403, description = "auth.forbidden: no report.read"),
        (status = 422, description = "validation_failed: q too_short / too_long, from after_to, to range_too_long (> 366 days), cursor invalid, limit out_of_range")))]
pub async fn list_notes(
    State(pool): State<PgPool>,
    State(cipher): State<Arc<FieldCipher>>,
    user: AuthUser,
    query: Result<Query<NotesQuery>, QueryRejection>,
) -> Result<Json<NotesPage>, AppError> {
    // The right first, then the query (F17).
    user.actor.require(Permission::ReportRead)?;
    let Query(query) = query.map_err(invalid_query)?;
    // No handler-set Cache-Control: the default `private, no-store` (middleware) applies.
    Ok(Json(
        notes::list(&pool, &cipher, &user.actor, &query).await?,
    ))
}

#[utoipa::path(get, path = "/api/v1/customers/{id}/history", tag = "reports",
    params(("id" = Uuid, Path), HistoryQuery),
    responses(
        (status = 200, body = HistoryPage),
        (status = 401), (status = 403, description = "auth.forbidden: needs customer.read + visit.read + sale.read"),
        (status = 404), (status = 422, description = "validation_failed: q too_short / too_long, from after_to, cursor invalid, limit out_of_range")))]
pub async fn customer_history(
    State(pool): State<PgPool>,
    State(cipher): State<Arc<FieldCipher>>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Query(query): Query<HistoryQuery>,
) -> Result<Response, AppError> {
    Ok(no_store(
        customer::history(&pool, &cipher, &user.actor, id, &query).await?,
    ))
}

#[utoipa::path(get, path = "/api/v1/customers/{id}/summary", tag = "reports",
    params(("id" = Uuid, Path)),
    responses(
        (status = 200, body = CustomerStats),
        (status = 401), (status = 403, description = "auth.forbidden: needs customer.read + visit.read + sale.read"),
        (status = 404)))]
pub async fn customer_summary(
    State(pool): State<PgPool>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    Ok(no_store(customer::summary(&pool, &user.actor, id).await?))
}

#[utoipa::path(get, path = "/api/v1/customer-packages/{id}/usage", tag = "reports",
    params(("id" = Uuid, Path)),
    responses(
        (status = 200, body = PackageUsage, description = "`sale_note` only for customer.health_notes.read"),
        (status = 401), (status = 403, description = "auth.forbidden: needs customer.read + visit.read"),
        (status = 404)))]
pub async fn package_usage(
    State(pool): State<PgPool>,
    State(cipher): State<Arc<FieldCipher>>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    Ok(no_store(
        customer::package_usage(&pool, &cipher, &user.actor, id).await?,
    ))
}

#[utoipa::path(get, path = "/api/v1/packages", tag = "reports", params(PackagesQuery),
    responses(
        (status = 200, body = PackageOverviewPage),
        (status = 401), (status = 403, description = "auth.forbidden: no customer.read"),
        (status = 422, description = "validation_failed: q too_short / too_long, expiring_within_days out_of_range, cursor invalid, limit out_of_range")))]
pub async fn list_packages(
    State(pool): State<PgPool>,
    user: AuthUser,
    Query(query): Query<PackagesQuery>,
) -> Result<Response, AppError> {
    Ok(no_store(
        overview::packages(&pool, &user.actor, &query).await?,
    ))
}
