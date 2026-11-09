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
    let more = i64::try_from(rows.len()).unwrap_or(i64::MAX) > limit;
    let mut items: Vec<PlatformAuditItem> = rows
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
    if more {
        items.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
    }
    let next_before = more.then(|| items.last().map(|i| i.at)).flatten();
    Ok(PlatformAuditPage { items, next_before })
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
