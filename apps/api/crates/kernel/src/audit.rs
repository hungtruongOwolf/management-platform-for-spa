//! Append-only audit trail written in the same transaction as the change it describes.

use crate::db::TenantTx;
use crate::error::AppError;
use crate::permission::Actor;
use serde_json::Value;
use uuid::Uuid;

/// One audited change. `before`/`after` must already have sensitive fields (health notes) redacted.
#[derive(Debug, Clone)]
pub struct AuditEntry<'a> {
    pub action: &'a str,
    pub entity_type: &'a str,
    pub entity_id: Uuid,
    pub before: Option<Value>,
    pub after: Option<Value>,
    pub reason: Option<&'a str>,
}

pub async fn record(
    tx: &mut TenantTx,
    actor: &Actor,
    entry: AuditEntry<'_>,
) -> Result<(), AppError> {
    let ip = actor.ip.map(sqlx::types::ipnetwork::IpNetwork::from);
    sqlx::query!(
        r#"INSERT INTO app.audit_log
             (id, tenant_id, actor_user_id, actor_kind, action, entity_type, entity_id, before, after, reason, ip)
           VALUES ($1, $2, $3, 'user', $4, $5, $6, $7, $8, $9, $10)"#,
        Uuid::now_v7(),
        tx.tenant().as_uuid(),
        actor.user_id.as_uuid(),
        entry.action,
        entry.entity_type,
        entry.entity_id,
        entry.before,
        entry.after,
        entry.reason,
        ip,
    )
    .execute(tx.conn())
    .await?;
    Ok(())
}

/// An audit entry with no signed-in actor (`actor_kind = 'system'`), e.g. a forgotten-password
/// request from the login screen: who asked is not authenticated, so no user is named as actor.
pub async fn record_system(
    tx: &mut TenantTx,
    ip: Option<std::net::IpAddr>,
    entry: AuditEntry<'_>,
) -> Result<(), AppError> {
    let ip = ip.map(sqlx::types::ipnetwork::IpNetwork::from);
    sqlx::query!(
        r#"INSERT INTO app.audit_log
             (id, tenant_id, actor_user_id, actor_kind, action, entity_type, entity_id, before, after, reason, ip)
           VALUES ($1, $2, NULL, 'system', $3, $4, $5, $6, $7, $8, $9)"#,
        Uuid::now_v7(),
        tx.tenant().as_uuid(),
        entry.action,
        entry.entity_type,
        entry.entity_id,
        entry.before,
        entry.after,
        entry.reason,
        ip,
    )
    .execute(tx.conn())
    .await?;
    Ok(())
}

/// Placeholder written instead of sensitive values (health notes, visit notes).
pub const REDACTED: &str = "[redacted]";
