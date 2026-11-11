//! Management view endpoints (spec management-views). All `GET`, all `Cache-Control: no-store`
//! (they carry customer and money data), all tenant-scoped by the session.

use crate::activity_dto::{ActivityPage, ActivityQuery};
use crate::dto::{
    CustomerStats, DailyQuery, DailyReport, HistoryPage, HistoryQuery, InactivePage, InactiveQuery,
    LedgerPage, LedgerQuery, PackageOverviewPage, PackageUsage, PackagesQuery, RangeQuery,
    RangeReport,
};
use crate::history::RecordRoute;
use crate::history_dto::RecordHistory;
use crate::notes_dto::{NotesPage, NotesQuery};
use crate::{activity, customer, day, history, notes, overview};
use axum::extract::rejection::{PathRejection, QueryRejection};
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
        .route("/audit-log", get(list_activity))
        .route("/record-history/visit/{id}", get(visit_history))
        .route("/record-history/sale/{id}", get(sale_history))
        .route("/record-history/refund/{id}", get(refund_history))
        .route(
            "/record-history/customer-package/{id}",
            get(customer_package_history),
        )
        .route("/record-history/customer/{id}", get(customer_record_history))
}

/// Every `/record-history/*` route: the rights first (a caller without them gets `403` whatever
/// the id, malformed or not), then the id (`400` when it is not a UUID), then the record.
async fn record_history(
    pool: &PgPool,
    user: &AuthUser,
    route: RecordRoute,
    id: Result<Path<Uuid>, PathRejection>,
) -> Result<Response, AppError> {
    history::require_route(&user.actor, route)?;
    let id = match id {
        Ok(Path(id)) => id,
        Err(rejection) => return Ok(rejection.into_response()),
    };
    Ok(no_store(history::read(pool, &user.actor, route, id).await?))
}

#[utoipa::path(get, path = "/api/v1/record-history/visit/{id}", tag = "reports",
    params(("id" = Uuid, Path)),
    responses(
        (status = 200, body = RecordHistory, description = "The visit's history, its whole \"Sửa lượt\" chain included"),
        (status = 400, description = "id is not a UUID"), (status = 401),
        (status = 403, description = "auth.forbidden: needs record.history.read + visit.read"),
        (status = 404)))]
pub async fn visit_history(
    State(pool): State<PgPool>,
    user: AuthUser,
    id: Result<Path<Uuid>, PathRejection>,
) -> Result<Response, AppError> {
    record_history(&pool, &user, RecordRoute::Visit, id).await
}

#[utoipa::path(get, path = "/api/v1/record-history/sale/{id}", tag = "reports",
    params(("id" = Uuid, Path)),
    responses(
        (status = 200, body = RecordHistory, description = "A one-off sale's visit history, or a package sale's package history"),
        (status = 400, description = "id is not a UUID"), (status = 401),
        (status = 403, description = "auth.forbidden: needs record.history.read + sale.read, then visit.read (one-off sale) or customer.read (package sale)"),
        (status = 404)))]
pub async fn sale_history(
    State(pool): State<PgPool>,
    user: AuthUser,
    id: Result<Path<Uuid>, PathRejection>,
) -> Result<Response, AppError> {
    record_history(&pool, &user, RecordRoute::Sale, id).await
}

#[utoipa::path(get, path = "/api/v1/record-history/refund/{id}", tag = "reports",
    params(("id" = Uuid, Path)),
    responses(
        (status = 200, body = RecordHistory, description = "The refunded package's history"),
        (status = 400, description = "id is not a UUID"), (status = 401),
        (status = 403, description = "auth.forbidden: needs record.history.read + sale.read + customer.read"),
        (status = 404)))]
pub async fn refund_history(
    State(pool): State<PgPool>,
    user: AuthUser,
    id: Result<Path<Uuid>, PathRejection>,
) -> Result<Response, AppError> {
    record_history(&pool, &user, RecordRoute::Refund, id).await
}

#[utoipa::path(get, path = "/api/v1/record-history/customer-package/{id}", tag = "reports",
    params(("id" = Uuid, Path)),
    responses(
        (status = 200, body = RecordHistory),
        (status = 400, description = "id is not a UUID"), (status = 401),
        (status = 403, description = "auth.forbidden: needs record.history.read + customer.read"),
        (status = 404)))]
pub async fn customer_package_history(
    State(pool): State<PgPool>,
    user: AuthUser,
    id: Result<Path<Uuid>, PathRejection>,
) -> Result<Response, AppError> {
    record_history(&pool, &user, RecordRoute::CustomerPackage, id).await
}

#[utoipa::path(get, path = "/api/v1/record-history/customer/{id}", tag = "reports",
    params(("id" = Uuid, Path)),
    responses(
        (status = 200, body = RecordHistory, description = "The customer profile's history; old / new phone values only for audit.read"),
        (status = 400, description = "id is not a UUID"), (status = 401),
        (status = 403, description = "auth.forbidden: needs record.history.read + customer.read"),
        (status = 404)))]
pub async fn customer_record_history(
    State(pool): State<PgPool>,
    user: AuthUser,
    id: Result<Path<Uuid>, PathRejection>,
) -> Result<Response, AppError> {
    record_history(&pool, &user, RecordRoute::Customer, id).await
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
    query: Result<Query<DailyQuery>, QueryRejection>,
) -> Result<Response, AppError> {
    // The right first, then the query (F17), on every route here.
    user.actor.require(Permission::ReportRead)?;
    let Query(query) = query.map_err(invalid_query)?;
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
    query: Result<Query<RangeQuery>, QueryRejection>,
) -> Result<Response, AppError> {
    user.actor.require(Permission::ReportRead)?;
    let Query(query) = query.map_err(invalid_query)?;
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
    query: Result<Query<LedgerQuery>, QueryRejection>,
) -> Result<Response, AppError> {
    user.actor.require(Permission::ReportRevenueRead)?;
    let Query(query) = query.map_err(invalid_query)?;
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
    query: Result<Query<InactiveQuery>, QueryRejection>,
) -> Result<Response, AppError> {
    user.actor.require(Permission::CustomerRead)?;
    user.actor.require(Permission::VisitRead)?;
    let Query(query) = query.map_err(invalid_query)?;
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

#[utoipa::path(get, path = "/api/v1/audit-log", tag = "reports", params(ActivityQuery),
    responses(
        (status = 200, body = ActivityPage, description = "The spa's audit trail, newest first: structured entries (who, what, about whom, changed values, money, reason); never a note's text"),
        (status = 401), (status = 403, description = "auth.forbidden: no audit.read (owner only)"),
        (status = 422, description = "validation_failed: actor invalid, from after_to, to range_too_long (> 366 days), cursor invalid, limit out_of_range")))]
pub async fn list_activity(
    State(pool): State<PgPool>,
    user: AuthUser,
    query: Result<Query<ActivityQuery>, QueryRejection>,
) -> Result<Response, AppError> {
    // The right first, then the query (F17).
    user.actor.require(Permission::AuditRead)?;
    let Query(query) = query.map_err(invalid_query)?;
    Ok(no_store(activity::list(&pool, &user.actor, &query).await?))
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
    query: Result<Query<HistoryQuery>, QueryRejection>,
) -> Result<Response, AppError> {
    customer::require_history(&user.actor)?;
    let Query(query) = query.map_err(invalid_query)?;
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
    query: Result<Query<PackagesQuery>, QueryRejection>,
) -> Result<Response, AppError> {
    user.actor.require(Permission::CustomerRead)?;
    let Query(query) = query.map_err(invalid_query)?;
    Ok(no_store(
        overview::packages(&pool, &user.actor, &query).await?,
    ))
}
