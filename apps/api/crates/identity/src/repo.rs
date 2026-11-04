//! SQL for identity. Global tables (accounts, sessions) are queried by unique keys only;
//! tenant tables (memberships, roles) only inside a `TenantTx`.

use chrono::{DateTime, Utc};
use kernel::{AppError, Permission, PhoneE164, TenantId, TenantTx, UserId};
use sqlx::PgPool;
use sqlx::types::ipnetwork::IpNetwork;
use std::str::FromStr;
use uuid::Uuid;

pub struct AccountRow {
    pub id: UserId,
    pub full_name: String,
    pub phone: String,
    pub password_hash: String,
    pub active: bool,
    pub failed_logins: i32,
    pub locked_until: Option<DateTime<Utc>>,
}

pub async fn account_by_phone(
    pool: &PgPool,
    phone: &PhoneE164,
) -> Result<Option<AccountRow>, AppError> {
    let row = sqlx::query!(
        r#"SELECT id, full_name, phone_e164, password_hash, status, failed_logins, locked_until
           FROM app.user_account WHERE phone_e164 = $1"#,
        phone.as_str()
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| AccountRow {
        id: UserId::from_uuid(r.id),
        full_name: r.full_name,
        phone: r.phone_e164,
        password_hash: r.password_hash,
        active: r.status == "active",
        failed_logins: r.failed_logins,
        locked_until: r.locked_until,
    }))
}

pub async fn account_by_id(pool: &PgPool, id: UserId) -> Result<Option<AccountRow>, AppError> {
    let row = sqlx::query!(
        r#"SELECT id, full_name, phone_e164, password_hash, status, failed_logins, locked_until
           FROM app.user_account WHERE id = $1"#,
        id.as_uuid()
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| AccountRow {
        id: UserId::from_uuid(r.id),
        full_name: r.full_name,
        phone: r.phone_e164,
        password_hash: r.password_hash,
        active: r.status == "active",
        failed_logins: r.failed_logins,
        locked_until: r.locked_until,
    }))
}

pub async fn record_failed_login(
    pool: &PgPool,
    id: UserId,
    failed_logins: i32,
    locked_until: Option<DateTime<Utc>>,
) -> Result<(), AppError> {
    sqlx::query!(
        "UPDATE app.user_account SET failed_logins = $2, locked_until = $3, updated_at = now() WHERE id = $1",
        id.as_uuid(),
        failed_logins,
        locked_until
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn reset_failed_logins(pool: &PgPool, id: UserId) -> Result<(), AppError> {
    sqlx::query!(
        "UPDATE app.user_account SET failed_logins = 0, locked_until = NULL, updated_at = now()
         WHERE id = $1 AND (failed_logins <> 0 OR locked_until IS NOT NULL)",
        id.as_uuid()
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub struct NewSession<'a> {
    pub token_hash: &'a [u8; 32],
    pub user_id: UserId,
    pub active_tenant: Option<TenantId>,
    pub idle_expires_at: DateTime<Utc>,
    pub absolute_expires_at: DateTime<Utc>,
    pub ip: Option<std::net::IpAddr>,
    pub user_agent: Option<&'a str>,
}

pub async fn insert_session(pool: &PgPool, s: NewSession<'_>) -> Result<Uuid, AppError> {
    let id = Uuid::now_v7();
    sqlx::query!(
        r#"INSERT INTO app.session
             (id, token_hash, user_id, active_tenant_id, idle_expires_at, absolute_expires_at, ip, user_agent)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
        id,
        &s.token_hash[..],
        s.user_id.as_uuid(),
        s.active_tenant.map(|t| t.as_uuid()),
        s.idle_expires_at,
        s.absolute_expires_at,
        s.ip.map(IpNetwork::from),
        s.user_agent,
    )
    .execute(pool)
    .await?;
    Ok(id)
}

pub struct SessionRow {
    pub id: Uuid,
    pub user_id: UserId,
    pub active_tenant: Option<TenantId>,
    pub last_seen_at: DateTime<Utc>,
    pub absolute_expires_at: DateTime<Utc>,
    pub recent_auth_at: DateTime<Utc>,
}

/// A live session: not revoked, not past either expiry, owner account active.
pub async fn live_session(
    pool: &PgPool,
    token_hash: &[u8; 32],
) -> Result<Option<SessionRow>, AppError> {
    let row = sqlx::query!(
        r#"SELECT s.id, s.user_id, s.active_tenant_id, s.last_seen_at, s.absolute_expires_at, s.recent_auth_at
           FROM app.session s JOIN app.user_account u ON u.id = s.user_id
           WHERE s.token_hash = $1 AND s.revoked_at IS NULL
             AND now() < s.idle_expires_at AND now() < s.absolute_expires_at
             AND u.status = 'active'"#,
        &token_hash[..]
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| SessionRow {
        id: r.id,
        user_id: UserId::from_uuid(r.user_id),
        active_tenant: r.active_tenant_id.map(TenantId::from_uuid),
        last_seen_at: r.last_seen_at,
        absolute_expires_at: r.absolute_expires_at,
        recent_auth_at: r.recent_auth_at,
    }))
}

pub async fn touch_session(
    pool: &PgPool,
    id: Uuid,
    idle_expires_at: DateTime<Utc>,
) -> Result<(), AppError> {
    sqlx::query!(
        "UPDATE app.session SET last_seen_at = now(), idle_expires_at = least($2, absolute_expires_at) WHERE id = $1",
        id,
        idle_expires_at
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn mark_recent_auth(pool: &PgPool, id: Uuid) -> Result<(), AppError> {
    sqlx::query!(
        "UPDATE app.session SET recent_auth_at = now() WHERE id = $1",
        id
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn revoke_session(pool: &PgPool, id: Uuid) -> Result<(), AppError> {
    sqlx::query!(
        "UPDATE app.session SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL",
        id
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn revoke_session_by_hash(pool: &PgPool, token_hash: &[u8; 32]) -> Result<(), AppError> {
    sqlx::query!(
        "UPDATE app.session SET revoked_at = now() WHERE token_hash = $1 AND revoked_at IS NULL",
        &token_hash[..]
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn revoke_all_sessions(pool: &PgPool, user: UserId) -> Result<u64, AppError> {
    let done = sqlx::query!(
        "UPDATE app.session SET revoked_at = now() WHERE user_id = $1 AND revoked_at IS NULL",
        user.as_uuid()
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected())
}

pub struct TenantRef {
    pub id: TenantId,
    pub name: String,
}

pub async fn tenants_of_user(pool: &PgPool, user: UserId) -> Result<Vec<TenantRef>, AppError> {
    let rows = sqlx::query!(
        r#"SELECT tenant_id AS "tenant_id!", tenant_name AS "tenant_name!" FROM app.tenants_of_user($1)"#,
        user.as_uuid()
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| TenantRef {
            id: TenantId::from_uuid(r.tenant_id),
            name: r.tenant_name,
        })
        .collect())
}

/// Permissions of an active member in the transaction's tenant. `None` = not an active member.
pub async fn member_permissions(
    tx: &mut TenantTx,
    user: UserId,
) -> Result<Option<Vec<Permission>>, AppError> {
    let tenant = tx.tenant().as_uuid();
    let member = sqlx::query_scalar!(
        "SELECT id FROM app.membership WHERE tenant_id = $1 AND user_id = $2 AND status = 'active'",
        tenant,
        user.as_uuid()
    )
    .fetch_optional(tx.conn())
    .await?;
    let Some(membership_id) = member else {
        return Ok(None);
    };
    let names = sqlx::query_scalar!(
        r#"SELECT DISTINCT rp.permission
           FROM app.membership_role mr
           JOIN app.role_permission rp ON rp.tenant_id = mr.tenant_id AND rp.role_id = mr.role_id
           WHERE mr.tenant_id = $1 AND mr.membership_id = $2"#,
        tenant,
        membership_id
    )
    .fetch_all(tx.conn())
    .await?;
    let permissions = names
        .iter()
        .filter_map(|name| match Permission::from_str(name) {
            Ok(p) => Some(p),
            Err(_) => {
                tracing::warn!(permission = %name, "ignoring unknown permission in role");
                None
            }
        })
        .collect();
    Ok(Some(permissions))
}
