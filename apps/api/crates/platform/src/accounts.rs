//! Accounts across spas (spec US8, ADR-0018): find a login by phone or name, lock / unlock it,
//! reset its password (a temporary one, ADR-0016) or its MFA, end its sessions. Replaces the
//! CLI-only recovery of an owner's account. Never returns a hash, a secret or business data.
//!
//! Every action is one transaction: the account change, the platform audit row and one row in
//! the audit trail of each spa the account belongs to (`actor_kind = 'platform_admin'`), written
//! under that spa's RLS scope.

use crate::audit::{self, PlatformEntry};
use crate::auth::OperatorSession;
use crate::gate_refusal;
use crate::tenants::Reason;
use chrono::{DateTime, Utc};
use identity::account::TemporaryPassword;
use kernel::{AppError, FieldError, PhoneE164};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

/// One spa an account belongs to.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AccountMembership {
    pub tenant_id: Uuid,
    pub tenant_name: String,
    pub tenant_status: String,
    pub status: String,
    pub role_keys: Vec<String>,
    pub is_owner: bool,
}

/// A login as the console shows it: who, which spas, and the state of its security — nothing
/// else.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AccountRow {
    pub id: Uuid,
    pub full_name: String,
    pub phone: String,
    /// `active` | `disabled` (locked by an operator).
    pub status: String,
    /// Temporarily locked after wrong passwords, until then.
    pub locked_until: Option<DateTime<Utc>>,
    pub mfa_enabled: bool,
    pub must_change_password: bool,
    pub created_at: DateTime<Utc>,
    pub memberships: Vec<AccountMembership>,
}

struct Membership {
    user_id: Uuid,
    view: AccountMembership,
    other_active_owners: i64,
}

async fn memberships(
    conn: &mut PgConnection,
    op: &OperatorSession,
    users: &[Uuid],
) -> Result<Vec<Membership>, AppError> {
    let rows = sqlx::query!(
        r#"SELECT user_id AS "user_id!", tenant_id AS "tenant_id!", tenant_name AS "tenant_name!",
                  tenant_status AS "tenant_status!", membership_status AS "membership_status!",
                  role_keys AS "role_keys!", is_owner AS "is_owner!",
                  other_active_owners AS "other_active_owners!"
           FROM app.platform_account_memberships($1, $2)"#,
        &op.token_hash[..],
        users
    )
    .fetch_all(conn)
    .await
    .map_err(gate_refusal)?;
    Ok(rows
        .into_iter()
        .map(|r| Membership {
            user_id: r.user_id,
            other_active_owners: r.other_active_owners,
            view: AccountMembership {
                tenant_id: r.tenant_id,
                tenant_name: r.tenant_name,
                tenant_status: r.tenant_status,
                status: r.membership_status,
                role_keys: r.role_keys,
                is_owner: r.is_owner,
            },
        })
        .collect())
}

struct UserRow {
    id: Uuid,
    full_name: String,
    phone_e164: String,
    status: String,
    locked_until: Option<DateTime<Utc>>,
    mfa_enabled: bool,
    must_change_password: bool,
    created_at: DateTime<Utc>,
}

fn row(user: UserRow, memberships: &[Membership]) -> AccountRow {
    let phone = PhoneE164::parse(&user.phone_e164)
        .map(|p| p.display_national())
        .unwrap_or(user.phone_e164);
    AccountRow {
        id: user.id,
        full_name: user.full_name,
        phone,
        status: user.status,
        locked_until: user.locked_until,
        mfa_enabled: user.mfa_enabled,
        must_change_password: user.must_change_password,
        created_at: user.created_at,
        memberships: memberships
            .iter()
            .filter(|m| m.user_id == user.id)
            .map(|m| m.view.clone())
            .collect(),
    }
}

#[derive(Debug, Clone, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AccountQuery {
    /// A phone number (or its start) or part of a name, at least 3 characters.
    pub q: String,
}

/// `GET /admin/accounts?q=`: at most 50 logins whose phone starts with the digits, or whose
/// name contains the words (accents ignored).
pub async fn search(
    pool: &PgPool,
    op: &OperatorSession,
    query: &AccountQuery,
) -> Result<Vec<AccountRow>, AppError> {
    let q = query.q.trim();
    if q.chars().count() < 3 {
        return Err(AppError::Validation(vec![FieldError::new(
            "q",
            "too_short",
        )]));
    }
    let digits: String = q.chars().filter(char::is_ascii_digit).collect();
    let phone_prefix = (digits.len() >= 3 && digits.len() * 2 >= q.len()).then(|| {
        match digits.strip_prefix('0') {
            Some(rest) => format!("+84{rest}"),
            None if digits.starts_with("84") => format!("+{digits}"),
            None => format!("+84{digits}"),
        }
    });
    let mut conn = pool.acquire().await?;
    let users = sqlx::query_as!(
        UserRow,
        r#"SELECT id, full_name, phone_e164, status, locked_until,
                  mfa_enabled_at IS NOT NULL AS "mfa_enabled!", must_change_password, created_at
           FROM app.user_account
           WHERE ($1::text IS NOT NULL AND phone_e164 LIKE $1 || '%')
              OR ($1::text IS NULL AND app.search_fold(full_name) LIKE '%' || app.search_fold($2) || '%')
           ORDER BY full_name, id
           LIMIT 50"#,
        phone_prefix,
        q
    )
    .fetch_all(&mut *conn)
    .await?;
    let ids: Vec<Uuid> = users.iter().map(|u| u.id).collect();
    let spas = memberships(&mut conn, op, &ids).await?;
    Ok(users.into_iter().map(|u| row(u, &spas)).collect())
}

async fn load(
    conn: &mut PgConnection,
    op: &OperatorSession,
    id: Uuid,
) -> Result<(AccountRow, Vec<Membership>), AppError> {
    let user = sqlx::query_as!(
        UserRow,
        r#"SELECT id, full_name, phone_e164, status, locked_until,
                  mfa_enabled_at IS NOT NULL AS "mfa_enabled!", must_change_password, created_at
           FROM app.user_account WHERE id = $1 FOR UPDATE"#,
        id
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)?;
    let spas = memberships(conn, op, &[id]).await?;
    Ok((row(user, &spas), spas))
}

/// What an account action did, for both audit trails.
struct Done<'a> {
    platform_action: &'static str,
    spa_action: &'static str,
    detail: Value,
    reason: &'a str,
}

async fn audit_everywhere(
    conn: &mut PgConnection,
    op: &OperatorSession,
    account: &AccountRow,
    done: Done<'_>,
) -> Result<(), AppError> {
    let ip = op.meta.ip.map(sqlx::types::ipnetwork::IpNetwork::from);
    for spa in &account.memberships {
        // The spa's RLS scope for this one insert (transaction-local, like TenantTx).
        sqlx::query!(
            "SELECT set_config('app.tenant_id', $1, true)",
            spa.tenant_id.to_string()
        )
        .fetch_one(&mut *conn)
        .await?;
        sqlx::query!(
            r#"INSERT INTO app.audit_log
                 (id, tenant_id, actor_user_id, actor_kind, action, entity_type, entity_id, after, reason, ip)
               VALUES ($1, $2, $3, 'platform_admin', $4, 'user', $5, $6, $7, $8)"#,
            Uuid::now_v7(),
            spa.tenant_id,
            op.operator_id,
            done.spa_action,
            account.id,
            done.detail,
            done.reason,
            ip
        )
        .execute(&mut *conn)
        .await?;
    }
    sqlx::query!("SELECT set_config('app.tenant_id', '', true)")
        .fetch_one(&mut *conn)
        .await?;
    audit::record(
        conn,
        op.operator_id,
        op.meta.ip,
        PlatformEntry {
            action: done.platform_action,
            tenant: None,
            target_type: Some("user"),
            target_id: Some(account.id),
            detail: Some(done.detail),
            reason: Some(done.reason),
        },
    )
    .await
}

async fn end_sessions(conn: &mut PgConnection, user: Uuid) -> Result<i64, AppError> {
    let done = sqlx::query!(
        "UPDATE app.session SET revoked_at = now() WHERE user_id = $1 AND revoked_at IS NULL",
        user
    )
    .execute(conn)
    .await?;
    Ok(i64::try_from(done.rows_affected()).unwrap_or(i64::MAX))
}

/// The action an operator takes on an account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountAction {
    Lock,
    Unlock,
    ResetMfa,
    RevokeSessions,
}

/// Body of `POST /admin/accounts/{id}/revoke-sessions`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SessionsRevoked {
    pub revoked: i64,
    pub account: AccountRow,
}

/// Lock / unlock (US8 AC8.2), reset MFA (AC8.4), end sessions (AC8.5).
pub async fn act(
    pool: &PgPool,
    op: &OperatorSession,
    id: Uuid,
    action: AccountAction,
    reason: &Reason,
) -> Result<SessionsRevoked, AppError> {
    let reason = reason.checked()?;
    let mut tx = pool.begin().await?;
    let (account, spas) = load(&mut tx, op, id).await?;
    let (platform_action, spa_action, revoked) = match action {
        AccountAction::Lock => {
            if account.status == "disabled" {
                return Err(AppError::Conflict {
                    code: "account.status_unchanged",
                });
            }
            // At least one owner per active spa (ADR-0018 D5): suspend the spa first.
            if spas.iter().any(|m| {
                m.view.is_owner
                    && m.view.status == "active"
                    && m.view.tenant_status == "active"
                    && m.other_active_owners == 0
            }) {
                return Err(AppError::Conflict {
                    code: "platform.last_owner",
                });
            }
            sqlx::query!(
                "UPDATE app.user_account SET status = 'disabled', updated_at = now() WHERE id = $1",
                id
            )
            .execute(&mut *tx)
            .await?;
            (
                "platform.account.lock",
                "account.locked_by_platform",
                end_sessions(&mut tx, id).await?,
            )
        }
        AccountAction::Unlock => {
            if account.status == "active" && account.locked_until.is_none() {
                return Err(AppError::Conflict {
                    code: "account.status_unchanged",
                });
            }
            sqlx::query!(
                "UPDATE app.user_account
                 SET status = 'active', failed_logins = 0, locked_until = NULL, updated_at = now()
                 WHERE id = $1",
                id
            )
            .execute(&mut *tx)
            .await?;
            ("platform.account.unlock", "account.unlocked_by_platform", 0)
        }
        AccountAction::ResetMfa => {
            if !account.mfa_enabled {
                return Err(AppError::Conflict {
                    code: "account.mfa_not_enabled",
                });
            }
            sqlx::query!(
                "UPDATE app.user_account
                 SET mfa_secret_ct = NULL, mfa_enabled_at = NULL, mfa_last_step = NULL, updated_at = now()
                 WHERE id = $1",
                id
            )
            .execute(&mut *tx)
            .await?;
            sqlx::query!("DELETE FROM app.mfa_recovery_code WHERE user_id = $1", id)
                .execute(&mut *tx)
                .await?;
            (
                "platform.account.reset_mfa",
                "account.mfa_reset_by_platform",
                end_sessions(&mut tx, id).await?,
            )
        }
        AccountAction::RevokeSessions => (
            "platform.account.revoke_sessions",
            "account.sessions_revoked_by_platform",
            end_sessions(&mut tx, id).await?,
        ),
    };
    audit_everywhere(
        &mut tx,
        op,
        &account,
        Done {
            platform_action,
            spa_action,
            detail: json!({"revoked_sessions": revoked}),
            reason,
        },
    )
    .await?;
    let (account, _) = load(&mut tx, op, id).await?;
    tx.commit().await?;
    Ok(SessionsRevoked { revoked, account })
}

/// Body of `POST /admin/accounts/{id}/reset-password`.
#[derive(Debug, Serialize, ToSchema)]
pub struct PasswordReset {
    /// Shown this once (`no-store`); the person must replace it at the next sign-in.
    pub temporary_password: String,
    pub account: AccountRow,
}

/// AC8.3: a temporary password (ADR-0016), `must_change_password`, every session ended, the
/// lockout cleared. The owner's own recovery, which used to be CLI-only.
pub async fn reset_password(
    pool: &PgPool,
    op: &OperatorSession,
    id: Uuid,
    reason: &Reason,
) -> Result<PasswordReset, AppError> {
    let reason = reason.checked()?;
    let temporary = TemporaryPassword::generate().map_err(|e| AppError::Internal(e.to_string()))?;
    let hashed = identity::account::hash("temporary_password", temporary.expose()).await?;
    let mut tx = pool.begin().await?;
    let (account, _) = load(&mut tx, op, id).await?;
    if account.status == "disabled" {
        return Err(AppError::Conflict {
            code: "account.locked",
        });
    }
    sqlx::query!(
        "UPDATE app.user_account
         SET password_hash = $2, must_change_password = true, failed_logins = 0, locked_until = NULL,
             updated_at = now()
         WHERE id = $1",
        id,
        hashed
    )
    .execute(&mut *tx)
    .await?;
    let revoked = end_sessions(&mut tx, id).await?;
    audit_everywhere(
        &mut tx,
        op,
        &account,
        Done {
            platform_action: "platform.account.reset_password",
            spa_action: "account.password_reset_by_platform",
            detail: json!({"revoked_sessions": revoked}),
            reason,
        },
    )
    .await?;
    let (account, _) = load(&mut tx, op, id).await?;
    tx.commit().await?;
    Ok(PasswordReset {
        temporary_password: temporary.expose().to_owned(),
        account,
    })
}
