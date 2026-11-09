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
    pub locked_until: Option<DateTime<Utc>>,
    /// Encrypted TOTP secret (enrolled, or enrollment started but not confirmed).
    pub mfa_secret_ct: Option<Vec<u8>>,
    pub mfa_enabled: bool,
    /// Signed in with a temporary password (ADR-0016): must set their own first.
    pub must_change_password: bool,
}

pub async fn account_by_phone(
    pool: &PgPool,
    phone: &PhoneE164,
) -> Result<Option<AccountRow>, AppError> {
    let row = sqlx::query!(
        r#"SELECT id, full_name, phone_e164, password_hash, status, locked_until,
                  mfa_secret_ct, mfa_enabled_at, must_change_password
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
        locked_until: r.locked_until,
        mfa_secret_ct: r.mfa_secret_ct,
        mfa_enabled: r.mfa_enabled_at.is_some(),
        must_change_password: r.must_change_password,
    }))
}

pub async fn account_by_id(pool: &PgPool, id: UserId) -> Result<Option<AccountRow>, AppError> {
    let row = sqlx::query!(
        r#"SELECT id, full_name, phone_e164, password_hash, status, locked_until,
                  mfa_secret_ct, mfa_enabled_at, must_change_password
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
        locked_until: r.locked_until,
        mfa_secret_ct: r.mfa_secret_ct,
        mfa_enabled: r.mfa_enabled_at.is_some(),
        must_change_password: r.must_change_password,
    }))
}

/// Counts one failed password or second-factor attempt and returns the new total. The increment
/// happens in SQL so parallel guesses cannot overwrite each other's count and dodge the lockout.
pub async fn increment_failed_logins(pool: &PgPool, id: UserId) -> Result<i32, AppError> {
    let failures = sqlx::query_scalar!(
        "UPDATE app.user_account SET failed_logins = failed_logins + 1, updated_at = now()
         WHERE id = $1 RETURNING failed_logins",
        id.as_uuid()
    )
    .fetch_one(pool)
    .await?;
    Ok(failures)
}

pub async fn lock_account_until(
    pool: &PgPool,
    id: UserId,
    until: DateTime<Utc>,
) -> Result<(), AppError> {
    sqlx::query!(
        "UPDATE app.user_account SET locked_until = greatest(locked_until, $2), updated_at = now()
         WHERE id = $1",
        id.as_uuid(),
        until
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
    /// When the user last proved the password (and second factor). The column has no default
    /// (migration 0011): a token rotation must carry the old value, never get a free "now".
    pub recent_auth_at: DateTime<Utc>,
    pub ip: Option<std::net::IpAddr>,
    pub user_agent: Option<&'a str>,
    /// Password accepted, second factor still owed.
    pub mfa_pending: bool,
}

pub async fn insert_session(pool: &PgPool, s: NewSession<'_>) -> Result<Uuid, AppError> {
    let id = Uuid::now_v7();
    sqlx::query!(
        r#"INSERT INTO app.session
             (id, token_hash, user_id, active_tenant_id, idle_expires_at, absolute_expires_at,
              recent_auth_at, ip, user_agent, mfa_pending)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#,
        id,
        &s.token_hash[..],
        s.user_id.as_uuid(),
        s.active_tenant.map(|t| t.as_uuid()),
        s.idle_expires_at,
        s.absolute_expires_at,
        s.recent_auth_at,
        s.ip.map(IpNetwork::from),
        s.user_agent,
        s.mfa_pending,
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
    pub mfa_pending: bool,
    pub mfa_enrolled: bool,
    pub must_change_password: bool,
}

/// A live session: not revoked, not past either expiry, owner account active.
pub async fn live_session(
    pool: &PgPool,
    token_hash: &[u8; 32],
) -> Result<Option<SessionRow>, AppError> {
    let row = sqlx::query!(
        r#"SELECT s.id, s.user_id, s.active_tenant_id, s.last_seen_at, s.absolute_expires_at, s.recent_auth_at,
                  s.mfa_pending, u.mfa_enabled_at IS NOT NULL AS "mfa_enrolled!", u.must_change_password
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
        mfa_pending: r.mfa_pending,
        mfa_enrolled: r.mfa_enrolled,
        must_change_password: r.must_change_password,
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

/// Whether any of the user's active memberships holds a role that requires MFA (migration 0006).
pub async fn user_requires_mfa(pool: &PgPool, user: UserId) -> Result<bool, AppError> {
    let required = sqlx::query_scalar!(
        r#"SELECT app.user_requires_mfa($1) AS "required!""#,
        user.as_uuid()
    )
    .fetch_one(pool)
    .await?;
    Ok(required)
}

/// The account's only active memberships are in suspended spas (ADR-0018, `app.user_spa_suspended`).
pub async fn user_spa_suspended(pool: &PgPool, user: UserId) -> Result<bool, AppError> {
    let suspended = sqlx::query_scalar!(
        r#"SELECT app.user_spa_suspended($1) AS "suspended!""#,
        user.as_uuid()
    )
    .fetch_one(pool)
    .await?;
    Ok(suspended)
}

/// Stores a not-yet-confirmed TOTP secret. Refuses (false) once MFA is enabled: an enrolled
/// authenticator is only replaced by an operator reset, never by a password-only session.
pub async fn store_pending_mfa_secret(
    pool: &PgPool,
    user: UserId,
    secret_ct: &[u8],
) -> Result<bool, AppError> {
    let done = sqlx::query!(
        "UPDATE app.user_account SET mfa_secret_ct = $2, updated_at = now()
         WHERE id = $1 AND mfa_enabled_at IS NULL",
        user.as_uuid(),
        secret_ct
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Records `step` as used. False when this or a later step was already accepted (replay).
/// Comparison and write are one statement, so two concurrent requests cannot both win.
pub async fn accept_totp_step(
    conn: impl sqlx::PgExecutor<'_>,
    user: UserId,
    step: i64,
) -> Result<bool, AppError> {
    let done = sqlx::query!(
        "UPDATE app.user_account SET mfa_last_step = $2
         WHERE id = $1 AND (mfa_last_step IS NULL OR mfa_last_step < $2)",
        user.as_uuid(),
        step
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Turns MFA on for the exact secret the confirming code was checked against. False if MFA is
/// already on or the secret was replaced meanwhile (a concurrent enroll/start).
pub async fn enable_mfa(
    conn: impl sqlx::PgExecutor<'_>,
    user: UserId,
    verified_secret_ct: &[u8],
) -> Result<bool, AppError> {
    let done = sqlx::query!(
        "UPDATE app.user_account SET mfa_enabled_at = now(), updated_at = now()
         WHERE id = $1 AND mfa_enabled_at IS NULL AND mfa_secret_ct = $2",
        user.as_uuid(),
        verified_secret_ct
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected() == 1)
}

pub async fn replace_recovery_codes(
    conn: &mut sqlx::PgConnection,
    user: UserId,
    hashes: &[[u8; 32]],
) -> Result<(), AppError> {
    sqlx::query!(
        "DELETE FROM app.mfa_recovery_code WHERE user_id = $1",
        user.as_uuid()
    )
    .execute(&mut *conn)
    .await?;
    for hash in hashes {
        sqlx::query!(
            "INSERT INTO app.mfa_recovery_code (id, user_id, code_hash) VALUES ($1, $2, $3)",
            Uuid::now_v7(),
            user.as_uuid(),
            &hash[..]
        )
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Marks a recovery code used. False if it does not exist, belongs to someone else, or was used.
pub async fn consume_recovery_code(
    pool: &PgPool,
    user: UserId,
    hash: &[u8; 32],
) -> Result<bool, AppError> {
    let done = sqlx::query!(
        "UPDATE app.mfa_recovery_code SET used_at = now()
         WHERE user_id = $1 AND code_hash = $2 AND used_at IS NULL",
        user.as_uuid(),
        &hash[..]
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected() == 1)
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

/// Permissions of an active member of an active spa (the transaction's tenant). `None` = not an
/// active member, or the spa is suspended: a suspended spa grants nothing, even to live sessions.
/// A member whose linked staff profile has left holds only `kernel::permission::FORMER_STAFF`
/// (ADR-0014): derived here on every request from the roster, never from the client.
pub async fn member_permissions(
    tx: &mut TenantTx,
    user: UserId,
) -> Result<Option<Vec<Permission>>, AppError> {
    // One statement (perf page §3.4: every authenticated request paid two round trips here). A
    // member with no role still gets a row, with an empty list; no row = not an active member.
    // `(tenant_id, user_id)` is unique, so at most one row. `staff.status` mirrors the latest
    // employment period (0025): `left` = no open period (`staff_user_idx` answers the lookup).
    let member = sqlx::query!(
        r#"SELECT COALESCE(
                    array_agg(DISTINCT rp.permission) FILTER (WHERE rp.permission IS NOT NULL),
                    '{}'
                  ) AS "permissions!",
                  EXISTS (SELECT 1 FROM app.staff s
                          WHERE s.tenant_id = m.tenant_id AND s.user_id = m.user_id
                            AND s.status = 'left') AS "former_staff!"
           FROM app.membership m
           JOIN app.tenant t ON t.id = m.tenant_id
           LEFT JOIN app.membership_role mr ON mr.tenant_id = m.tenant_id AND mr.membership_id = m.id
           LEFT JOIN app.role_permission rp ON rp.tenant_id = mr.tenant_id AND rp.role_id = mr.role_id
           WHERE m.tenant_id = $1 AND m.user_id = $2 AND m.status = 'active' AND t.status = 'active'
           GROUP BY m.id"#,
        tx.tenant().as_uuid(),
        user.as_uuid()
    )
    .fetch_optional(tx.conn())
    .await?;
    let Some(member) = member else {
        return Ok(None);
    };
    let roles = member
        .permissions
        .iter()
        .filter_map(|name| match Permission::from_str(name) {
            Ok(p) => Some(p),
            Err(_) => {
                tracing::warn!(permission = %name, "ignoring unknown permission in role");
                None
            }
        })
        .collect();
    Ok(Some(kernel::permission::effective_permissions(
        roles,
        member.former_staff,
    )))
}

// ------------------------------------------------------------------ passwords (ADR-0016)

/// The person's own new password: clears the temporary-password flag and any lockout.
pub async fn set_own_password(pool: &PgPool, user: UserId, hash: &str) -> Result<(), AppError> {
    sqlx::query!(
        "UPDATE app.user_account
         SET password_hash = $2, must_change_password = false, password_changed_at = now(),
             failed_logins = 0, locked_until = NULL, updated_at = now()
         WHERE id = $1",
        user.as_uuid(),
        hash
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// A temporary password set by the owner: must be changed at the next sign-in; the lockout is
/// cleared (the person asked for help) and every session ends.
pub async fn set_temporary_password(
    tx: &mut TenantTx,
    user: UserId,
    hash: &str,
) -> Result<(), AppError> {
    sqlx::query!(
        "UPDATE app.user_account
         SET password_hash = $2, must_change_password = true, failed_logins = 0,
             locked_until = NULL, updated_at = now()
         WHERE id = $1",
        user.as_uuid(),
        hash
    )
    .execute(tx.conn())
    .await?;
    sqlx::query!(
        "UPDATE app.session SET revoked_at = now() WHERE user_id = $1 AND revoked_at IS NULL",
        user.as_uuid()
    )
    .execute(tx.conn())
    .await?;
    Ok(())
}

/// Whether any account uses this phone (accounts are global: one phone, one login).
pub async fn phone_taken(tx: &mut TenantTx, phone: &PhoneE164) -> Result<bool, AppError> {
    Ok(sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM app.user_account WHERE phone_e164 = $1) AS "e!""#,
        phone.as_str()
    )
    .fetch_one(tx.conn())
    .await?)
}

/// A new login with a temporary password, an active membership of the transaction's spa and one
/// system role (`key`).
pub async fn insert_login(
    tx: &mut TenantTx,
    phone: &PhoneE164,
    full_name: &str,
    hash: &str,
    role_key: &str,
) -> Result<UserId, AppError> {
    let tenant = tx.tenant().as_uuid();
    let user = UserId::new();
    sqlx::query!(
        "INSERT INTO app.user_account (id, phone_e164, full_name, password_hash, must_change_password)
         VALUES ($1, $2, $3, $4, true)",
        user.as_uuid(),
        phone.as_str(),
        full_name,
        hash
    )
    .execute(tx.conn())
    .await?;
    let membership = Uuid::now_v7();
    sqlx::query!(
        "INSERT INTO app.membership (id, tenant_id, user_id) VALUES ($1, $2, $3)",
        membership,
        tenant,
        user.as_uuid()
    )
    .execute(tx.conn())
    .await?;
    let roles = sqlx::query!(
        "INSERT INTO app.membership_role (tenant_id, membership_id, role_id)
         SELECT $1, $2, id FROM app.role WHERE tenant_id = $1 AND key = $3",
        tenant,
        membership,
        role_key
    )
    .execute(tx.conn())
    .await?;
    if roles.rows_affected() != 1 {
        return Err(AppError::Internal(format!("role {role_key} missing")));
    }
    Ok(user)
}

/// Replaces the roles of `user` in the transaction's spa with the system role `key`.
pub async fn replace_role(tx: &mut TenantTx, user: UserId, role_key: &str) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    let membership = sqlx::query_scalar!(
        "SELECT id FROM app.membership WHERE tenant_id = $1 AND user_id = $2 FOR UPDATE",
        tenant,
        user.as_uuid()
    )
    .fetch_optional(tx.conn())
    .await?
    .ok_or(AppError::NotFound)?;
    sqlx::query!(
        "DELETE FROM app.membership_role WHERE tenant_id = $1 AND membership_id = $2",
        tenant,
        membership
    )
    .execute(tx.conn())
    .await?;
    let roles = sqlx::query!(
        "INSERT INTO app.membership_role (tenant_id, membership_id, role_id)
         SELECT $1, $2, id FROM app.role WHERE tenant_id = $1 AND key = $3",
        tenant,
        membership,
        role_key
    )
    .execute(tx.conn())
    .await?;
    if roles.rows_affected() != 1 {
        return Err(AppError::Internal(format!("role {role_key} missing")));
    }
    Ok(())
}

/// Keys of the system roles `user` holds in the transaction's spa.
pub async fn role_keys(tx: &mut TenantTx, user: UserId) -> Result<Vec<String>, AppError> {
    Ok(sqlx::query_scalar!(
        "SELECT r.key FROM app.membership m
         JOIN app.membership_role mr ON mr.tenant_id = m.tenant_id AND mr.membership_id = m.id
         JOIN app.role r ON r.tenant_id = mr.tenant_id AND r.id = mr.role_id
         WHERE m.tenant_id = $1 AND m.user_id = $2
         ORDER BY r.key",
        tx.tenant().as_uuid(),
        user.as_uuid()
    )
    .fetch_all(tx.conn())
    .await?)
}

// ------------------------------------------------------- forgotten-password requests (option A)

/// Records a request unless one is open, or one was made in the last hour (at most one a spa an
/// hour reaches the owner). Expired open requests are closed first. `true` = a new request.
pub async fn open_reset_request(tx: &mut TenantTx, user: UserId) -> Result<bool, AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        "UPDATE app.password_reset_request
         SET closed_at = now(), outcome = 'expired'
         WHERE tenant_id = $1 AND user_id = $2 AND closed_at IS NULL AND expires_at <= now()",
        tenant,
        user.as_uuid()
    )
    .execute(tx.conn())
    .await?;
    let inserted = sqlx::query!(
        "INSERT INTO app.password_reset_request (id, tenant_id, user_id, expires_at)
         SELECT $1, $2, $3, now() + interval '24 hours'
         WHERE NOT EXISTS (SELECT 1 FROM app.password_reset_request
                           WHERE tenant_id = $2 AND user_id = $3
                             AND (closed_at IS NULL OR requested_at > now() - interval '1 hour'))
         ON CONFLICT DO NOTHING",
        Uuid::now_v7(),
        tenant,
        user.as_uuid()
    )
    .execute(tx.conn())
    .await?;
    Ok(inserted.rows_affected() == 1)
}

pub struct ResetRequestRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub requested_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

/// Open, unexpired requests of the transaction's spa, oldest first.
pub async fn open_reset_requests(tx: &mut TenantTx) -> Result<Vec<ResetRequestRow>, AppError> {
    Ok(sqlx::query_as!(
        ResetRequestRow,
        "SELECT id, user_id, requested_at, expires_at FROM app.password_reset_request
         WHERE tenant_id = $1 AND closed_at IS NULL AND expires_at > now()
         ORDER BY requested_at, id",
        tx.tenant().as_uuid()
    )
    .fetch_all(tx.conn())
    .await?)
}

/// Closes the open request `id` (or, with `user`, every open request of that login) with
/// `outcome`; returns the users whose request closed.
pub async fn close_reset_requests(
    tx: &mut TenantTx,
    id: Option<Uuid>,
    user: Option<UserId>,
    outcome: &str,
    by: UserId,
) -> Result<Vec<Uuid>, AppError> {
    Ok(sqlx::query_scalar!(
        "UPDATE app.password_reset_request
         SET closed_at = now(), outcome = $4, closed_by = $5
         WHERE tenant_id = $1 AND closed_at IS NULL
           AND ($2::uuid IS NULL OR id = $2) AND ($3::uuid IS NULL OR user_id = $3)
         RETURNING user_id",
        tx.tenant().as_uuid(),
        id,
        user.map(|u| u.as_uuid()),
        outcome,
        by.as_uuid()
    )
    .fetch_all(tx.conn())
    .await?)
}
