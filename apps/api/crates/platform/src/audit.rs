//! The platform audit trail (ADR-0018 D6): one row per operator action, written in the same
//! transaction as the action, append-only. `detail` never carries a password, hash, secret or a
//! full phone number.

use chrono::{DateTime, Utc};
use kernel::AppError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgConnection, PgPool};
use std::net::IpAddr;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

#[derive(Debug, Clone, Default)]
pub struct PlatformEntry<'a> {
    /// `platform.<area>.<verb>`.
    pub action: &'a str,
    pub tenant: Option<Uuid>,
    pub target_type: Option<&'a str>,
    pub target_id: Option<Uuid>,
    pub detail: Option<Value>,
    pub reason: Option<&'a str>,
}

pub async fn record(
    conn: &mut PgConnection,
    operator: Uuid,
    ip: Option<IpAddr>,
    entry: PlatformEntry<'_>,
) -> Result<(), AppError> {
    let ip = ip.map(sqlx::types::ipnetwork::IpNetwork::from);
    sqlx::query!(
        r#"INSERT INTO app.platform_audit_log
             (id, operator_id, action, tenant_id, target_type, target_id, detail, reason, ip)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
        Uuid::now_v7(),
        operator,
        entry.action,
        entry.tenant,
        entry.target_type,
        entry.target_id,
        entry.detail,
        entry.reason,
        ip
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// The last three digits of a phone number: enough to tell two accounts apart in the trail.
pub fn phone_tail(e164: &str) -> String {
    let digits: Vec<char> = e164.chars().filter(char::is_ascii_digit).collect();
    let tail: String = digits[digits.len().saturating_sub(3)..].iter().collect();
    format!("…{tail}")
}

/// What an operator typed into the account search, kept for the trail without the personal data
/// it names (ADR-0019 D17): a phone keeps its first three digits (the network prefix), a name
/// keeps the first letter of each word. Enough to see that a search happened and of what kind,
/// not whose number or name it was.
pub fn masked_query(q: &str, by_phone: bool) -> String {
    if by_phone {
        let digits: Vec<char> = q.chars().filter(char::is_ascii_digit).collect();
        let head: String = digits.iter().take(3).collect();
        let hidden = "*".repeat(digits.len().saturating_sub(3));
        return format!("{head}{hidden}");
    }
    q.split_whitespace()
        .filter_map(|word| word.chars().next())
        .map(|first| format!("{first}…"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Debug, Clone, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AuditQuery {
    /// Only entries strictly older than this instant (the previous page's `next_before`).
    pub before: Option<DateTime<Utc>>,
    /// 1–100, default 50.
    pub limit: Option<i64>,
}

impl AuditQuery {
    pub fn limit(&self) -> Result<i64, AppError> {
        match self.limit {
            None => Ok(50),
            Some(n) if (1..=100).contains(&n) => Ok(n),
            Some(_) => Err(AppError::Validation(vec![kernel::FieldError::new(
                "limit",
                "out_of_range",
            )])),
        }
    }
}

/// One page of a newest-first trail from up to `limit + 1` rows, and the `before` of the next
/// page. The next page asks for `at < next_before`, so the page never ends in the middle of the
/// entries of one instant (a transaction writes several with the same `now()`): the rest of that
/// instant would be skipped. A single instant with more than `limit` entries cannot be kept
/// whole; that page is cut at `limit`.
fn page<T>(
    mut rows: Vec<T>,
    limit: i64,
    at: impl Fn(&T) -> DateTime<Utc>,
) -> (Vec<T>, Option<DateTime<Utc>>) {
    let limit = usize::try_from(limit).unwrap_or(usize::MAX);
    let Some(first_left_out) = rows.get(limit).map(&at) else {
        return (rows, None);
    };
    rows.truncate(limit);
    if let Some(last_whole) = rows.iter().rposition(|r| at(r) != first_left_out) {
        rows.truncate(last_whole + 1);
    }
    let next_before = rows.last().map(&at);
    (rows, next_before)
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PlatformAuditItem {
    pub id: Uuid,
    pub at: DateTime<Utc>,
    pub operator_id: Uuid,
    pub operator_username: String,
    pub action: String,
    pub tenant_id: Option<Uuid>,
    pub target_type: Option<String>,
    pub target_id: Option<Uuid>,
    pub detail: Option<Value>,
    pub reason: Option<String>,
    pub ip: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PlatformAuditPage {
    pub items: Vec<PlatformAuditItem>,
    /// Pass as `before` for the next (older) page; absent on the last page.
    pub next_before: Option<DateTime<Utc>>,
}

/// `GET /admin/audit`: newest first.
pub async fn list(pool: &PgPool, query: &AuditQuery) -> Result<PlatformAuditPage, AppError> {
    let limit = query.limit()?;
    let rows = sqlx::query!(
        r#"SELECT a.id, a.at, a.operator_id, o.username, a.action, a.tenant_id, a.target_type,
                  a.target_id, a.detail, a.reason, host(a.ip) AS ip
           FROM app.platform_audit_log a
           JOIN app.platform_operator o ON o.id = a.operator_id
           WHERE $1::timestamptz IS NULL OR a.at < $1
           ORDER BY a.at DESC, a.id DESC
           LIMIT $2"#,
        query.before,
        limit + 1
    )
    .fetch_all(pool)
    .await?;
    let items: Vec<PlatformAuditItem> = rows
        .into_iter()
        .map(|r| PlatformAuditItem {
            id: r.id,
            at: r.at,
            operator_id: r.operator_id,
            operator_username: r.username,
            action: r.action,
            tenant_id: r.tenant_id,
            target_type: r.target_type,
            target_id: r.target_id,
            detail: r.detail,
            reason: r.reason,
            ip: r.ip,
        })
        .collect();
    let (items, next_before) = page(items, limit, |i| i.at);
    Ok(PlatformAuditPage { items, next_before })
}

/// One entry of a spa's own audit trail, as support sees it (spec US9 AC9.2): metadata only. No
/// `before` / `after`, no `reason` (free text that may name a customer), no `entity_id` (may be a
/// customer's id), no IP. The full entry needs the owner's support grant (ADR-0003, later).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SpaAuditItem {
    pub id: Uuid,
    pub at: DateTime<Utc>,
    pub action: String,
    pub entity_type: String,
    /// `user` | `platform_admin` | `system`.
    pub actor_kind: String,
    /// The spa member's name, or the operator's username.
    pub actor_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SpaAuditPage {
    pub items: Vec<SpaAuditItem>,
    pub next_before: Option<DateTime<Utc>>,
}

/// `GET /admin/tenants/{id}/audit`: read in the spa's own RLS scope; the read itself is audited
/// (`platform.tenant_audit.read`).
pub async fn of_tenant(
    pool: &PgPool,
    op: &crate::auth::OperatorSession,
    tenant: Uuid,
    query: &AuditQuery,
) -> Result<SpaAuditPage, AppError> {
    let limit = query.limit()?;
    crate::tenants::get(pool, op, tenant).await?;
    let mut tx = kernel::TenantTx::begin(pool, kernel::TenantId::from_uuid(tenant)).await?;
    let rows = sqlx::query!(
        r#"SELECT a.id, a.at, a.action, a.entity_type, a.actor_kind,
                  COALESCE(u.full_name, o.username) AS actor_name
           FROM app.audit_log a
           LEFT JOIN app.user_account u ON a.actor_kind = 'user' AND u.id = a.actor_user_id
           LEFT JOIN app.platform_operator o
             ON a.actor_kind = 'platform_admin' AND o.id = a.actor_user_id
           WHERE a.tenant_id = $1 AND ($2::timestamptz IS NULL OR a.at < $2)
           ORDER BY a.at DESC, a.id DESC
           LIMIT $3"#,
        tenant,
        query.before,
        limit + 1
    )
    .fetch_all(tx.conn())
    .await?;
    let items: Vec<SpaAuditItem> = rows
        .into_iter()
        .map(|r| SpaAuditItem {
            id: r.id,
            at: r.at,
            action: r.action,
            entity_type: r.entity_type,
            actor_kind: r.actor_kind,
            actor_name: r.actor_name,
        })
        .collect();
    let (items, next_before) = page(items, limit, |i| i.at);
    record(
        tx.conn(),
        op.operator_id,
        op.meta.ip,
        PlatformEntry {
            action: "platform.tenant_audit.read",
            tenant: Some(tenant),
            target_type: Some("tenant"),
            target_id: Some(tenant),
            detail: Some(serde_json::json!({"rows": items.len()})),
            reason: None,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(SpaAuditPage { items, next_before })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_last_three_digits_of_a_phone_are_kept() {
        assert_eq!(phone_tail("+84905123456"), "…456");
        assert_eq!(phone_tail("12"), "…12");
    }

    #[test]
    fn a_search_is_kept_without_the_number_or_name() {
        assert_eq!(masked_query("0905 123 456", true), "090*******");
        assert_eq!(masked_query("+84905", true), "849**");
        assert_eq!(masked_query("Nguyễn  Văn An", false), "N… V… A…");
        assert_eq!(masked_query("%%%", false), "%…");
        assert!(!masked_query("Nguyễn Văn An", false).contains("guy"));
    }

    #[test]
    fn a_page_never_splits_one_instant() {
        let t = |s: i64| DateTime::from_timestamp(s, 0).unwrap_or_default();
        // Newest first; three entries share the instant 7 (one transaction).
        let rows = vec![t(9), t(8), t(7), t(7), t(7), t(6)];
        let (items, next) = page(rows.clone(), 3, |r| *r);
        assert_eq!((items, next), (vec![t(9), t(8)], Some(t(8))));
        // The next page (`at < 8`) starts with the whole instant.
        let rest: Vec<_> = rows.iter().copied().filter(|r| *r < t(8)).collect();
        let (items, next) = page(rest, 3, |r| *r);
        assert_eq!((items, next), (vec![t(7), t(7), t(7)], Some(t(7))));
        // No more rows than the limit: no next page.
        assert_eq!(page(vec![t(9), t(8)], 3, |r| *r), (vec![t(9), t(8)], None));
        // A boundary between two instants cuts at the limit as before.
        assert_eq!(
            page(vec![t(9), t(8), t(7), t(6)], 3, |r| *r),
            (vec![t(9), t(8), t(7)], Some(t(7)))
        );
        // One instant with more entries than a page: cut at the limit.
        assert_eq!(
            page(vec![t(7), t(7), t(7)], 2, |r| *r),
            (vec![t(7), t(7)], Some(t(7)))
        );
    }

    #[test]
    fn page_size_is_bounded() {
        let q = |limit| AuditQuery {
            before: None,
            limit,
        };
        assert_eq!(q(None).limit().ok(), Some(50));
        assert_eq!(q(Some(100)).limit().ok(), Some(100));
        assert!(q(Some(0)).limit().is_err());
        assert!(q(Some(101)).limit().is_err());
    }
}
