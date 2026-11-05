//! Follow-up lists: every package of the spa by derived status (expiring soon first when asked),
//! and the customers who have not come back (re-engagement). Status comes from the SQL function
//! `app.package_status` (0013), the same rule as `packages::PackageStatus::derive`.

use crate::cursor;
use crate::dto::{
    InactiveCustomer, InactivePage, InactiveScope, PackageOverviewCounts, PackageOverviewPage,
    PackageOverviewRow, PackageStatusFilter, PackagesQuery,
};
use chrono::{DateTime, NaiveDate, Utc};
use crm::text::{like_escape, search_key};
use kernel::{Actor, AppError, FieldError, Permission, TenantTx};
use packages::http::PackageStatusDto;
use sqlx::PgPool;
use uuid::Uuid;

pub const DEFAULT_EXPIRING_DAYS: u32 = 30;
pub const DEFAULT_INACTIVE_DAYS: u32 = 30;
const MAX_DAYS: u32 = 365;
const MAX_QUERY_CHARS: usize = 100;

fn field(name: &'static str, code: &'static str) -> AppError {
    AppError::Validation(vec![FieldError::new(name, code)])
}

fn days(value: Option<u32>, default: u32, name: &'static str) -> Result<u32, AppError> {
    match value.unwrap_or(default) {
        n @ 1..=MAX_DAYS => Ok(n),
        _ => Err(field(name, "out_of_range")),
    }
}

/// `q` → (name pattern, phone pattern), like `GET /customers?q=`: digits search the phone,
/// anything else the accent-folded name.
fn customer_filter(q: Option<&str>) -> Result<(Option<String>, Option<String>), AppError> {
    let q = q.map(str::trim).unwrap_or_default();
    if q.is_empty() {
        return Ok((None, None));
    }
    if q.chars().count() > MAX_QUERY_CHARS {
        return Err(field("q", "too_long"));
    }
    if q.chars()
        .all(|c| c.is_ascii_digit() || c.is_whitespace() || matches!(c, '.' | '-' | '+'))
    {
        let digits: String = q.chars().filter(char::is_ascii_digit).collect();
        let national = digits
            .strip_prefix("84")
            .filter(|_| q.starts_with('+'))
            .or_else(|| digits.strip_prefix('0'))
            .unwrap_or(&digits);
        if national.len() < 3 {
            return Err(field("q", "too_short"));
        }
        return Ok((None, Some(format!("+84%{}%", like_escape(national)))));
    }
    let key = search_key(q);
    if key.chars().count() < 2 {
        return Err(field("q", "too_short"));
    }
    Ok((Some(format!("%{}%", like_escape(&key))), None))
}

fn status(s: &str) -> PackageStatusDto {
    match s {
        "cancelled" => PackageStatusDto::Cancelled,
        "exhausted" => PackageStatusDto::Exhausted,
        "expired" => PackageStatusDto::Expired,
        _ => PackageStatusDto::Active,
    }
}

struct Row {
    id: Uuid,
    customer_id: Uuid,
    full_name: String,
    phone_e164: String,
    name: String,
    status: String,
    sessions_total: i64,
    sessions_remaining: i64,
    starts_on: NaiveDate,
    expires_on: Option<NaiveDate>,
    cancelled_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    last_used_on: Option<NaiveDate>,
}

impl Row {
    fn into_dto(self, today: NaiveDate) -> PackageOverviewRow {
        PackageOverviewRow {
            id: self.id,
            customer_id: self.customer_id,
            customer_name: self.full_name,
            customer_phone: self.phone_e164,
            name: self.name,
            status: status(&self.status),
            sessions_total: self.sessions_total,
            sessions_remaining: self.sessions_remaining,
            starts_on: self.starts_on,
            expires_on: self.expires_on,
            days_to_expiry: self.expires_on.map(|e| (e - today).num_days()),
            last_used_on: self.last_used_on,
            cancelled_at: self.cancelled_at,
        }
    }
}

/// `GET /packages?status=&expiring_within_days=&q=&cursor=&limit=` (`customer.read`).
/// Without `expiring_within_days`: newest first. With it: only active packages expiring within
/// that window, soonest first.
pub async fn packages(
    pool: &PgPool,
    actor: &Actor,
    query: &PackagesQuery,
) -> Result<PackageOverviewPage, AppError> {
    actor.require(Permission::CustomerRead)?;
    let limit = cursor::limit(query.limit)?;
    let window = days(
        query.expiring_within_days,
        DEFAULT_EXPIRING_DAYS,
        "expiring_within_days",
    )?;
    let expiring = query.expiring_within_days.is_some();
    let window_i32 = i32::try_from(window).map_err(|_| field("expiring_within_days", "out_of_range"))?;
    let (name_like, phone_like) = customer_filter(query.q.as_deref())?;
    let status_filter = query.status.map(PackageStatusFilter::as_str);
    let after = query
        .cursor
        .as_deref()
        .map(|raw| cursor::decode(raw, 2))
        .transpose()?;

    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let today = kernel::tenant::profile(&mut tx)
        .await?
        .business_date
        .as_naive();
    let tenant = tx.tenant().as_uuid();

    let mut rows: Vec<Row> = if expiring {
        let (a_date, a_id) = match &after {
            Some(p) => (Some(cursor::date(&p[0])?), Some(cursor::uuid(&p[1])?)),
            None => (None, None),
        };
        sqlx::query_as!(
            Row,
            r#"SELECT ps.id AS "id!", ps.customer_id AS "customer_id!", c.full_name, c.phone_e164,
                      ps.name_snapshot AS "name!",
                      app.package_status(ps.cancelled_at, ps.sessions_remaining, ps.expires_on, $2) AS "status!",
                      ps.sessions_total AS "sessions_total!", ps.sessions_remaining AS "sessions_remaining!",
                      ps.starts_on AS "starts_on!", ps.expires_on, ps.cancelled_at, ps.created_at AS "created_at!",
                      (SELECT max(v.business_date) FROM app.visit_line l
                       JOIN app.visit v ON v.tenant_id = l.tenant_id AND v.id = l.visit_id
                       WHERE l.tenant_id = ps.tenant_id AND l.customer_package_id = ps.id
                         AND v.status = 'completed') AS last_used_on
               FROM app.customer_package_state ps
               JOIN app.customer c ON c.tenant_id = ps.tenant_id AND c.id = ps.customer_id
               WHERE ps.tenant_id = $1 AND c.anonymized_at IS NULL
                 AND ($3::text IS NULL OR c.search_text LIKE $3)
                 AND ($4::text IS NULL OR c.phone_e164 LIKE $4)
                 AND app.package_status(ps.cancelled_at, ps.sessions_remaining, ps.expires_on, $2) = 'active'
                 AND ps.expires_on <= $2 + $5::int
                 AND ($6::date IS NULL OR (ps.expires_on, ps.id) > ($6, $7))
               ORDER BY ps.expires_on, ps.id
               LIMIT $8"#,
            tenant,
            today,
            name_like,
            phone_like,
            window_i32,
            a_date,
            a_id,
            limit + 1
        )
        .fetch_all(tx.conn())
        .await?
    } else {
        let (a_at, a_id) = match &after {
            Some(p) => (Some(cursor::instant(&p[0])?), Some(cursor::uuid(&p[1])?)),
            None => (None, None),
        };
        sqlx::query_as!(
            Row,
            r#"SELECT ps.id AS "id!", ps.customer_id AS "customer_id!", c.full_name, c.phone_e164,
                      ps.name_snapshot AS "name!",
                      app.package_status(ps.cancelled_at, ps.sessions_remaining, ps.expires_on, $2) AS "status!",
                      ps.sessions_total AS "sessions_total!", ps.sessions_remaining AS "sessions_remaining!",
                      ps.starts_on AS "starts_on!", ps.expires_on, ps.cancelled_at, ps.created_at AS "created_at!",
                      (SELECT max(v.business_date) FROM app.visit_line l
                       JOIN app.visit v ON v.tenant_id = l.tenant_id AND v.id = l.visit_id
                       WHERE l.tenant_id = ps.tenant_id AND l.customer_package_id = ps.id
                         AND v.status = 'completed') AS last_used_on
               FROM app.customer_package_state ps
               JOIN app.customer c ON c.tenant_id = ps.tenant_id AND c.id = ps.customer_id
               WHERE ps.tenant_id = $1 AND c.anonymized_at IS NULL
                 AND ($3::text IS NULL OR c.search_text LIKE $3)
                 AND ($4::text IS NULL OR c.phone_e164 LIKE $4)
                 AND ($5::text IS NULL
                      OR app.package_status(ps.cancelled_at, ps.sessions_remaining, ps.expires_on, $2) = $5)
                 AND ($6::timestamptz IS NULL OR (ps.created_at, ps.id) < ($6, $7))
               ORDER BY ps.created_at DESC, ps.id DESC
               LIMIT $8"#,
            tenant,
            today,
            name_like,
            phone_like,
            status_filter,
            a_at,
            a_id,
            limit + 1
        )
        .fetch_all(tx.conn())
        .await?
    };
    // `status` narrows the expiring list too (only `active` can match it).
    if expiring {
        if let Some(wanted) = status_filter {
            rows.retain(|r| r.status == wanted);
        }
    }

    let mut counts = PackageOverviewCounts::default();
    for c in sqlx::query!(
        r#"SELECT app.package_status(ps.cancelled_at, ps.sessions_remaining, ps.expires_on, $2) AS "status!",
                  count(*) AS "n!",
                  count(*) FILTER (WHERE ps.expires_on <= $2 + $5::int) AS "expiring!"
           FROM app.customer_package_state ps
           JOIN app.customer c ON c.tenant_id = ps.tenant_id AND c.id = ps.customer_id
           WHERE ps.tenant_id = $1 AND c.anonymized_at IS NULL
             AND ($3::text IS NULL OR c.search_text LIKE $3)
             AND ($4::text IS NULL OR c.phone_e164 LIKE $4)
           GROUP BY 1"#,
        tenant,
        today,
        name_like,
        phone_like,
        window_i32
    )
    .fetch_all(tx.conn())
    .await?
    {
        match c.status.as_str() {
            "active" => {
                counts.active = c.n;
                counts.expiring_soon = c.expiring;
            }
            "expired" => counts.expired = c.n,
            "exhausted" => counts.exhausted = c.n,
            _ => counts.cancelled = c.n,
        }
    }
    tx.commit().await?;

    let page = usize::try_from(limit).unwrap_or(usize::MAX);
    let more = rows.len() > page;
    rows.truncate(page);
    let next_cursor = more.then(|| rows.last()).flatten().and_then(|r| {
        if expiring {
            r.expires_on
                .map(|e| cursor::encode(&[e.to_string(), r.id.to_string()]))
        } else {
            Some(cursor::encode(&[
                cursor::instant_part(r.created_at),
                r.id.to_string(),
            ]))
        }
    });
    Ok(PackageOverviewPage {
        items: rows.into_iter().map(|r| r.into_dto(today)).collect(),
        counts,
        expiring_within_days: window,
        next_cursor,
    })
}

/// `GET /customers/inactive?days=&scope=&cursor=&limit=` (`customer.read` + `visit.read`):
/// customers not seen for at least `days` days, longest absence first. "Seen" = last completed
/// visit, or the latest package start for a customer who never came.
pub async fn inactive(
    pool: &PgPool,
    actor: &Actor,
    days_absent: Option<u32>,
    scope: Option<InactiveScope>,
    after: Option<&str>,
    limit: Option<u32>,
) -> Result<InactivePage, AppError> {
    actor.require(Permission::CustomerRead)?;
    actor.require(Permission::VisitRead)?;
    let limit = cursor::limit(limit)?;
    let threshold = days(days_absent, DEFAULT_INACTIVE_DAYS, "days")?;
    let threshold_i32 = i32::try_from(threshold).map_err(|_| field("days", "out_of_range"))?;
    let with_sessions = scope.unwrap_or_default() == InactiveScope::WithSessions;
    let (a_date, a_id) = match after {
        Some(raw) => {
            let p = cursor::decode(raw, 2)?;
            (Some(cursor::date(&p[0])?), Some(cursor::uuid(&p[1])?))
        }
        None => (None, None),
    };
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let today = kernel::tenant::profile(&mut tx)
        .await?
        .business_date
        .as_naive();
    let tenant = tx.tenant().as_uuid();
    let mut rows = sqlx::query!(
        r#"WITH last_visit AS (
             SELECT v.customer_id, max(v.business_date) AS last_on
             FROM app.visit v
             WHERE v.tenant_id = $1 AND v.status = 'completed'
             GROUP BY v.customer_id
           ), pk AS (
             SELECT ps.customer_id,
                    count(*) FILTER (WHERE app.package_status(ps.cancelled_at, ps.sessions_remaining, ps.expires_on, $2) = 'active') AS active,
                    coalesce(sum(ps.sessions_remaining) FILTER (WHERE app.package_status(ps.cancelled_at, ps.sessions_remaining, ps.expires_on, $2) = 'active'), 0)::bigint AS remaining,
                    max(ps.starts_on) AS last_start
             FROM app.customer_package_state ps
             WHERE ps.tenant_id = $1
             GROUP BY ps.customer_id
           ), cand AS (
             SELECT c.id, c.full_name, c.phone_e164, lv.last_on,
                    coalesce(lv.last_on, pk.last_start) AS last_seen,
                    coalesce(pk.active, 0) AS active, coalesce(pk.remaining, 0) AS remaining
             FROM app.customer c
             LEFT JOIN last_visit lv ON lv.customer_id = c.id
             LEFT JOIN pk ON pk.customer_id = c.id
             WHERE c.tenant_id = $1 AND c.anonymized_at IS NULL
           )
           SELECT id AS "id!", full_name AS "full_name!", phone_e164 AS "phone!", last_on,
                  last_seen AS "last_seen!", active AS "active!", remaining AS "remaining!"
           FROM cand
           WHERE last_seen IS NOT NULL AND last_seen <= $2 - $3::int
             AND (NOT $4::bool OR remaining > 0)
             AND ($5::date IS NULL OR (last_seen, id) > ($5, $6))
           ORDER BY last_seen, id
           LIMIT $7"#,
        tenant,
        today,
        threshold_i32,
        with_sessions,
        a_date,
        a_id,
        limit + 1
    )
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    let page = usize::try_from(limit).unwrap_or(usize::MAX);
    let more = rows.len() > page;
    rows.truncate(page);
    let next_cursor = more
        .then(|| rows.last())
        .flatten()
        .map(|r| cursor::encode(&[r.last_seen.to_string(), r.id.to_string()]));
    Ok(InactivePage {
        days: threshold,
        items: rows
            .into_iter()
            .map(|r| InactiveCustomer {
                customer_id: r.id,
                full_name: r.full_name,
                phone: r.phone,
                last_visit_on: r.last_on,
                last_seen_on: r.last_seen,
                days_absent: (today - r.last_seen).num_days(),
                active_packages: r.active,
                sessions_remaining: r.remaining,
            })
            .collect(),
        next_cursor,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn q_searches_phone_digits_or_folded_names() -> Result<(), AppError> {
        assert_eq!(customer_filter(None)?, (None, None));
        assert_eq!(
            customer_filter(Some("0905 123"))?,
            (None, Some("+84%905123%".to_owned()))
        );
        assert_eq!(
            customer_filter(Some("Hà"))?,
            (Some("%ha%".to_owned()), None)
        );
        assert!(customer_filter(Some("09")).is_err());
        assert!(customer_filter(Some("a")).is_err());
        assert!(customer_filter(Some(&"x".repeat(101))).is_err());
        assert!(customer_filter(Some("50%_")).is_ok(), "LIKE wildcards are escaped");
        Ok(())
    }

    #[test]
    fn day_windows_are_bounded() {
        assert_eq!(days(None, 30, "days").ok(), Some(30));
        assert!(days(Some(0), 30, "days").is_err());
        assert!(days(Some(366), 30, "days").is_err());
    }
}
