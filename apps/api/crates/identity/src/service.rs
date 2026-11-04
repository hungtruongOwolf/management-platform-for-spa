//! Authentication use cases (ADR-0004): login, sessions, re-authentication, tenant switching,
//! and turning a session into a `kernel::Actor` for authorization.

use crate::repo::{self, NewSession, SessionRow, TenantRef};
use crate::token::{self, SessionToken};
use crate::{lockout, password};
use chrono::{DateTime, Duration, Utc};
use kernel::{Actor, AppError, Permission, PhoneE164, TenantId, TenantTx, UserId};
use serde::Serialize;
use sqlx::PgPool;
use std::net::IpAddr;
use utoipa::ToSchema;
use uuid::Uuid;

pub const IDLE_TIMEOUT_HOURS: i64 = 12;
pub const ABSOLUTE_TIMEOUT_DAYS: i64 = 30;
pub const REAUTH_WINDOW_MINUTES: i64 = 5;
/// Avoid a write on every request: refresh `last_seen_at` at most once a minute.
const TOUCH_INTERVAL_SECONDS: i64 = 60;

#[derive(Debug, Clone, Default)]
pub struct ClientMeta {
    pub ip: Option<IpAddr>,
    pub user_agent: Option<String>,
}

/// A validated, live session.
#[derive(Debug, Clone)]
pub struct Session {
    pub id: Uuid,
    pub user_id: UserId,
    pub active_tenant: Option<TenantId>,
    pub recent_auth_at: DateTime<Utc>,
}

impl Session {
    pub fn recently_authenticated(&self, now: DateTime<Utc>) -> bool {
        now - self.recent_auth_at <= Duration::minutes(REAUTH_WINDOW_MINUTES)
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, ToSchema)]
pub struct TenantSummary {
    #[schema(value_type = String, format = Uuid)]
    pub id: TenantId,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, ToSchema)]
pub struct UserSummary {
    #[schema(value_type = String, format = Uuid)]
    pub id: UserId,
    pub full_name: String,
    pub phone: String,
}

/// Body of `GET /me` and of a successful login.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, ToSchema)]
pub struct Me {
    pub user: UserSummary,
    pub active_tenant: Option<TenantSummary>,
    pub tenants: Vec<TenantSummary>,
    #[schema(value_type = Vec<String>)]
    pub permissions: Vec<&'static str>,
}

fn summary(t: TenantRef) -> TenantSummary {
    TenantSummary {
        id: t.id,
        name: t.name,
    }
}

async fn issue_session(
    pool: &PgPool,
    user: UserId,
    tenant: Option<TenantId>,
    meta: &ClientMeta,
) -> Result<SessionToken, AppError> {
    let token = token::generate().map_err(|e| AppError::Internal(e.to_string()))?;
    let now = Utc::now();
    let absolute = now + Duration::days(ABSOLUTE_TIMEOUT_DAYS);
    repo::insert_session(
        pool,
        NewSession {
            token_hash: &token.hash,
            user_id: user,
            active_tenant: tenant,
            idle_expires_at: (now + Duration::hours(IDLE_TIMEOUT_HOURS)).min(absolute),
            absolute_expires_at: absolute,
            ip: meta.ip,
            user_agent: meta.user_agent.as_deref(),
        },
    )
    .await?;
    Ok(token)
}

/// Verifies credentials and opens a new session. Any session the browser already held is revoked
/// (session rotation on login). Unknown phone, wrong password, locked or disabled account all
/// return the same `auth.invalid_credentials` after the same amount of hashing work.
pub async fn login(
    pool: &PgPool,
    phone: &str,
    password_input: &str,
    previous_token: Option<&str>,
    meta: &ClientMeta,
) -> Result<(SessionToken, Me), AppError> {
    let Ok(phone) = PhoneE164::parse(phone) else {
        run_blocking_dummy(password_input).await;
        return Err(AppError::InvalidCredentials);
    };
    let Some(account) = repo::account_by_phone(pool, &phone).await? else {
        run_blocking_dummy(password_input).await;
        return Err(AppError::InvalidCredentials);
    };

    let now = Utc::now();
    let locked = account.locked_until.is_some_and(|until| until > now);
    let candidate = password_input.to_owned();
    let stored = account.password_hash.clone();
    let matches = tokio::task::spawn_blocking(move || password::verify(&candidate, &stored))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    if locked || !account.active || !matches {
        if !locked && !matches {
            let failures = account.failed_logins.saturating_add(1);
            repo::record_failed_login(
                pool,
                account.id,
                failures,
                lockout::lock_until(failures, now),
            )
            .await?;
        }
        return Err(AppError::InvalidCredentials);
    }

    repo::reset_failed_logins(pool, account.id).await?;
    if let Some(previous) = previous_token {
        repo::revoke_session_by_hash(pool, &token::hash_secret(previous)).await?;
    }
    let tenants = repo::tenants_of_user(pool, account.id).await?;
    // With exactly one spa, select it; otherwise the user picks one (switch-tenant).
    let active = match tenants.as_slice() {
        [only] => Some(only.id),
        _ => None,
    };
    let token = issue_session(pool, account.id, active, meta).await?;
    let session = Session {
        id: Uuid::nil(),
        user_id: account.id,
        active_tenant: active,
        recent_auth_at: now,
    };
    let me = build_me(pool, &session, Some(tenants)).await?;
    Ok((token, me))
}

async fn run_blocking_dummy(password_input: &str) {
    let candidate = password_input.to_owned();
    let _ = tokio::task::spawn_blocking(move || password::verify_against_dummy(&candidate)).await;
}

/// Resolves a cookie secret into a live session, sliding the idle timeout.
pub async fn resolve(pool: &PgPool, secret: &str) -> Result<Session, AppError> {
    let row: SessionRow = repo::live_session(pool, &token::hash_secret(secret))
        .await?
        .ok_or(AppError::Unauthenticated)?;
    let now = Utc::now();
    if now - row.last_seen_at > Duration::seconds(TOUCH_INTERVAL_SECONDS) {
        let idle = (now + Duration::hours(IDLE_TIMEOUT_HOURS)).min(row.absolute_expires_at);
        repo::touch_session(pool, row.id, idle).await?;
    }
    Ok(Session {
        id: row.id,
        user_id: row.user_id,
        active_tenant: row.active_tenant,
        recent_auth_at: row.recent_auth_at,
    })
}

pub async fn logout(pool: &PgPool, session: &Session) -> Result<(), AppError> {
    repo::revoke_session(pool, session.id).await
}

pub async fn logout_all(pool: &PgPool, session: &Session) -> Result<u64, AppError> {
    repo::revoke_all_sessions(pool, session.user_id).await
}

/// Confirms the password again before sensitive actions (🔒 in the authz matrix).
pub async fn reauth(
    pool: &PgPool,
    session: &Session,
    password_input: &str,
) -> Result<(), AppError> {
    let account = repo::account_by_id(pool, session.user_id)
        .await?
        .ok_or(AppError::Unauthenticated)?;
    let candidate = password_input.to_owned();
    let stored = account.password_hash;
    let matches = tokio::task::spawn_blocking(move || password::verify(&candidate, &stored))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    if matches {
        repo::mark_recent_auth(pool, session.id).await
    } else {
        Err(AppError::InvalidCredentials)
    }
}

/// Moves the session to another spa the user belongs to. Privileges change, so the token rotates.
pub async fn switch_tenant(
    pool: &PgPool,
    session: &Session,
    tenant: TenantId,
    meta: &ClientMeta,
) -> Result<(SessionToken, Me), AppError> {
    let tenants = repo::tenants_of_user(pool, session.user_id).await?;
    if !tenants.iter().any(|t| t.id == tenant) {
        return Err(AppError::NotFound);
    }
    repo::revoke_session(pool, session.id).await?;
    let token = issue_session(pool, session.user_id, Some(tenant), meta).await?;
    let moved = Session {
        id: Uuid::nil(),
        user_id: session.user_id,
        active_tenant: Some(tenant),
        recent_auth_at: session.recent_auth_at,
    };
    let me = build_me(pool, &moved, Some(tenants)).await?;
    Ok((token, me))
}

pub async fn me(pool: &PgPool, session: &Session) -> Result<Me, AppError> {
    build_me(pool, session, None).await
}

async fn build_me(
    pool: &PgPool,
    session: &Session,
    tenants: Option<Vec<TenantRef>>,
) -> Result<Me, AppError> {
    let account = repo::account_by_id(pool, session.user_id)
        .await?
        .ok_or(AppError::Unauthenticated)?;
    let tenants = match tenants {
        Some(t) => t,
        None => repo::tenants_of_user(pool, session.user_id).await?,
    };
    let (active_tenant, permissions) = match session.active_tenant {
        Some(tenant) => match tenant_permissions(pool, session.user_id, tenant).await? {
            Some(perms) => (
                tenants
                    .iter()
                    .find(|t| t.id == tenant)
                    .map(|t| TenantSummary {
                        id: t.id,
                        name: t.name.clone(),
                    }),
                perms,
            ),
            None => (None, Vec::new()),
        },
        None => (None, Vec::new()),
    };
    let phone = PhoneE164::parse(&account.phone)
        .map(|p| p.display_national())
        .unwrap_or(account.phone);
    let mut permission_names: Vec<&'static str> = permissions.iter().map(|p| p.as_str()).collect();
    permission_names.sort_unstable();
    Ok(Me {
        user: UserSummary {
            id: account.id,
            full_name: account.full_name,
            phone,
        },
        active_tenant,
        tenants: tenants.into_iter().map(summary).collect(),
        permissions: permission_names,
    })
}

async fn tenant_permissions(
    pool: &PgPool,
    user: UserId,
    tenant: TenantId,
) -> Result<Option<Vec<Permission>>, AppError> {
    let mut tx = TenantTx::begin(pool, tenant).await?;
    let permissions = repo::member_permissions(&mut tx, user).await?;
    tx.commit().await?;
    Ok(permissions)
}

/// The authorization subject for a request: requires an active spa and an active membership in it.
pub async fn actor_for(
    pool: &PgPool,
    session: &Session,
    ip: Option<IpAddr>,
) -> Result<Actor, AppError> {
    let tenant = session.active_tenant.ok_or(AppError::Forbidden {
        code: "auth.no_active_tenant",
    })?;
    let permissions = tenant_permissions(pool, session.user_id, tenant)
        .await?
        .ok_or(AppError::Forbidden {
            code: "auth.not_a_member",
        })?;
    Ok(Actor::new(session.user_id, tenant, permissions, ip))
}
