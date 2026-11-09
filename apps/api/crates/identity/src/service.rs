//! Authentication use cases (ADR-0004): login, sessions, re-authentication, tenant switching,
//! and turning a session into a `kernel::Actor` for authorization.

use crate::mfa::{self, SecondFactor, TotpSecret};
use crate::repo::{self, AccountRow, NewSession, SessionRow, TenantRef};
use crate::token::{self, SessionToken};
use crate::{lockout, password};
use chrono::{DateTime, Duration, Utc};
use kernel::crypto::FieldCipher;
use kernel::{Actor, AppError, FieldError, Permission, PhoneE164, TenantId, TenantTx, UserId};
use serde::Serialize;
use sqlx::PgPool;
use std::net::IpAddr;
use utoipa::ToSchema;
use uuid::Uuid;

pub const IDLE_TIMEOUT_HOURS: i64 = 12;
pub const ABSOLUTE_TIMEOUT_DAYS: i64 = 30;
pub const REAUTH_WINDOW_MINUTES: i64 = 5;
/// A password-only session must finish MFA within this time, then the user starts over.
pub const MFA_PENDING_MINUTES: i64 = 10;
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
    /// Hard end of the login this session belongs to; token rotation never moves it.
    pub absolute_expires_at: DateTime<Utc>,
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

/// What the browser must do after the password step.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LoginNext {
    /// Fully signed in; `me` is present.
    Done,
    /// Enter a TOTP or recovery code (`POST /auth/mfa/verify`).
    MfaVerify,
    /// MFA is mandatory for this account and not set up yet (`/auth/mfa/enroll/*`).
    MfaEnroll,
    /// Signed in with a temporary password (ADR-0016): set a new one (`POST /auth/password`)
    /// before anything else; every other route answers `403 auth.password_change_required`.
    PasswordChange,
}

/// Body of a successful password step. `me` is only present once fully signed in: a session that
/// still owes a second factor learns nothing about spas or permissions.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, ToSchema)]
pub struct LoginResponse {
    pub next: LoginNext,
    pub me: Option<Me>,
}

/// Enrollment material, shown only to the pending session that started enrollment.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, ToSchema)]
pub struct MfaEnrollment {
    /// `otpauth://totp/...` URI; the client renders it as a QR code.
    pub otpauth_uri: String,
    /// The same secret in base32, for typing into an authenticator app by hand.
    pub manual_key: String,
}

/// Result of confirming enrollment: signed in, plus the recovery codes (returned this once only).
#[derive(Debug, Clone, Serialize, PartialEq, Eq, ToSchema)]
pub struct MfaEnrolled {
    pub me: Me,
    pub recovery_codes: Vec<String>,
}

/// A session that passed the password step and still owes the second factor.
#[derive(Debug, Clone)]
pub struct PendingMfa {
    pub id: Uuid,
    pub user_id: UserId,
    pub enrolled: bool,
}

/// Why a session row is being issued. Every caller states it explicitly, because it decides
/// whether the new token counts as a fresh authentication (security review H1).
#[derive(Debug, Clone, Copy)]
enum Issue {
    /// Password accepted, second factor still owed: short-lived, usable only for MFA.
    MfaPending,
    /// Every required factor was just checked: a fresh login (new absolute expiry, re-auth now).
    Authenticated,
    /// Same login, new token (switch-tenant): keeps the re-auth time and the absolute expiry,
    /// so rotation can neither stand in for a password nor extend a session.
    Rotated {
        recent_auth_at: DateTime<Utc>,
        absolute_expires_at: DateTime<Utc>,
    },
}

/// `(idle_expires_at, absolute_expires_at, recent_auth_at)` for a session issued at `now`.
fn session_times(
    issue: Issue,
    now: DateTime<Utc>,
) -> (DateTime<Utc>, DateTime<Utc>, DateTime<Utc>) {
    let idle_from =
        |absolute: DateTime<Utc>| (now + Duration::hours(IDLE_TIMEOUT_HOURS)).min(absolute);
    match issue {
        Issue::MfaPending => {
            let deadline = now + Duration::minutes(MFA_PENDING_MINUTES);
            (deadline, deadline, now)
        }
        Issue::Authenticated => {
            let absolute = now + Duration::days(ABSOLUTE_TIMEOUT_DAYS);
            (idle_from(absolute), absolute, now)
        }
        Issue::Rotated {
            recent_auth_at,
            absolute_expires_at,
        } => (
            idle_from(absolute_expires_at),
            absolute_expires_at,
            recent_auth_at,
        ),
    }
}

async fn issue_session(
    pool: &PgPool,
    user: UserId,
    tenant: Option<TenantId>,
    meta: &ClientMeta,
    issue: Issue,
) -> Result<SessionToken, AppError> {
    let token = token::generate().map_err(|e| AppError::Internal(e.to_string()))?;
    let (idle, absolute, recent_auth_at) = session_times(issue, Utc::now());
    repo::insert_session(
        pool,
        NewSession {
            token_hash: &token.hash,
            user_id: user,
            active_tenant: tenant,
            idle_expires_at: idle,
            absolute_expires_at: absolute,
            recent_auth_at,
            ip: meta.ip,
            user_agent: meta.user_agent.as_deref(),
            mfa_pending: matches!(issue, Issue::MfaPending),
        },
    )
    .await?;
    Ok(token)
}

/// Opens a full session after every required factor has been checked.
async fn open_full_session(
    pool: &PgPool,
    user: UserId,
    meta: &ClientMeta,
) -> Result<(SessionToken, Me), AppError> {
    let tenants = repo::tenants_of_user(pool, user).await?;
    // With exactly one spa, select it; otherwise the user picks one (switch-tenant).
    let active = match tenants.as_slice() {
        [only] => Some(only.id),
        _ => None,
    };
    let token = issue_session(pool, user, active, meta, Issue::Authenticated).await?;
    let now = Utc::now();
    let session = Session {
        id: Uuid::nil(),
        user_id: user,
        active_tenant: active,
        recent_auth_at: now,
        absolute_expires_at: now + Duration::days(ABSOLUTE_TIMEOUT_DAYS),
    };
    let me = build_me(pool, &session, Some(tenants)).await?;
    Ok((token, me))
}

/// Counts a failed attempt (password or second factor) and locks the account progressively.
async fn record_failure(pool: &PgPool, user: UserId) -> Result<(), AppError> {
    let failures = repo::increment_failed_logins(pool, user).await?;
    if let Some(until) = lockout::lock_until(failures, Utc::now()) {
        repo::lock_account_until(pool, user, until).await?;
    }
    Ok(())
}

/// Verifies credentials and opens a new session. Any session the browser already held is revoked
/// (session rotation on login). Unknown phone, wrong password, locked or disabled account all
/// return the same `auth.invalid_credentials` after the same amount of hashing work.
///
/// When MFA applies (a role requires it, or the user enrolled), the new session is only
/// "pending MFA": usable for the MFA endpoints and logout, expiring after `MFA_PENDING_MINUTES`.
pub async fn login(
    pool: &PgPool,
    phone: &str,
    password_input: &str,
    previous_token: Option<&str>,
    meta: &ClientMeta,
) -> Result<(SessionToken, LoginResponse), AppError> {
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
            record_failure(pool, account.id).await?;
        }
        return Err(AppError::InvalidCredentials);
    }

    if let Some(previous) = previous_token {
        repo::revoke_session_by_hash(pool, &token::hash_secret(previous)).await?;
    }
    if account.mfa_enabled || repo::user_requires_mfa(pool, account.id).await? {
        // The failure counter is reset only after the second factor: otherwise re-entering the
        // (stolen) password would reset the lockout between rounds of code guessing.
        let token = issue_session(pool, account.id, None, meta, Issue::MfaPending).await?;
        let next = if account.mfa_enabled {
            LoginNext::MfaVerify
        } else {
            LoginNext::MfaEnroll
        };
        return Ok((token, LoginResponse { next, me: None }));
    }

    repo::reset_failed_logins(pool, account.id).await?;
    let (token, me) = open_full_session(pool, account.id, meta).await?;
    if account.must_change_password {
        // The session exists, but `resolve` keeps it off every route but the password change.
        return Ok((
            token,
            LoginResponse {
                next: LoginNext::PasswordChange,
                me: None,
            },
        ));
    }
    Ok((
        token,
        LoginResponse {
            next: LoginNext::Done,
            me: Some(me),
        },
    ))
}

async fn run_blocking_dummy(password_input: &str) {
    let candidate = password_input.to_owned();
    let _ = tokio::task::spawn_blocking(move || password::verify_against_dummy(&candidate)).await;
}

/// A live session of either kind, sliding the idle timeout (never past the absolute expiry).
async fn live(pool: &PgPool, secret: &str) -> Result<SessionRow, AppError> {
    let row: SessionRow = repo::live_session(pool, &token::hash_secret(secret))
        .await?
        .ok_or(AppError::Unauthenticated)?;
    let now = Utc::now();
    if now - row.last_seen_at > Duration::seconds(TOUCH_INTERVAL_SECONDS) {
        let idle = (now + Duration::hours(IDLE_TIMEOUT_HOURS)).min(row.absolute_expires_at);
        repo::touch_session(pool, row.id, idle).await?;
    }
    Ok(row)
}

/// Resolves a cookie secret into a live, fully authenticated session. A session that still owes
/// its second factor is refused here, which is what keeps it off `/me` and every business route.
pub async fn resolve(pool: &PgPool, secret: &str) -> Result<Session, AppError> {
    let session = resolve_for_password_change(pool, secret).await?;
    if session.1 {
        // A temporary password (ADR-0016): only `POST /auth/password` and logout take it.
        return Err(AppError::PasswordChangeRequired);
    }
    Ok(session.0)
}

/// Like [`resolve`], also for an account that must still replace its temporary password: the
/// one extractor `POST /auth/password` uses. `.1` = the password must be changed.
pub async fn resolve_for_password_change(
    pool: &PgPool,
    secret: &str,
) -> Result<(Session, bool), AppError> {
    let row = live(pool, secret).await?;
    if row.mfa_pending {
        return Err(if row.mfa_enrolled {
            AppError::MfaRequired
        } else {
            AppError::MfaEnrollmentRequired
        });
    }
    let must_change = row.must_change_password;
    Ok((
        Session {
            id: row.id,
            user_id: row.user_id,
            active_tenant: row.active_tenant,
            recent_auth_at: row.recent_auth_at,
            absolute_expires_at: row.absolute_expires_at,
        },
        must_change,
    ))
}

/// Resolves a cookie secret into a session that is waiting for its second factor (and only that).
pub async fn resolve_pending(pool: &PgPool, secret: &str) -> Result<PendingMfa, AppError> {
    let row = live(pool, secret).await?;
    if !row.mfa_pending {
        return Err(AppError::Conflict {
            code: "auth.mfa_not_pending",
        });
    }
    Ok(PendingMfa {
        id: row.id,
        user_id: row.user_id,
        enrolled: row.mfa_enrolled,
    })
}

/// Ends the session behind `secret`, whether full or pending MFA.
pub async fn logout(pool: &PgPool, secret: &str) -> Result<(), AppError> {
    let row = live(pool, secret).await?;
    repo::revoke_session(pool, row.id).await
}

pub async fn logout_all(pool: &PgPool, session: &Session) -> Result<u64, AppError> {
    repo::revoke_all_sessions(pool, session.user_id).await
}

/// Confirms the password again before account-security actions (🔒 in the authz matrix: password
/// and MFA settings; business actions never ask, EM decision 2026-11-05). Accounts with
/// MFA must also give a current TOTP (or recovery) code. Failures count toward the lockout, so a
/// stolen session cannot be used to guess the password or the code.
pub async fn reauth(
    pool: &PgPool,
    cipher: &FieldCipher,
    session: &Session,
    password_input: &str,
    code: Option<&str>,
) -> Result<(), AppError> {
    let account = repo::account_by_id(pool, session.user_id)
        .await?
        .ok_or(AppError::Unauthenticated)?;
    if account.locked_until.is_some_and(|until| until > Utc::now()) {
        return Err(AppError::InvalidCredentials);
    }
    let candidate = password_input.to_owned();
    let stored = account.password_hash.clone();
    let matches = tokio::task::spawn_blocking(move || password::verify(&candidate, &stored))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    if !matches {
        record_failure(pool, account.id).await?;
        return Err(AppError::InvalidCredentials);
    }
    if account.mfa_enabled {
        let code =
            code.ok_or_else(|| AppError::Validation(vec![FieldError::new("code", "required")]))?;
        if !second_factor_ok(pool, cipher, &account, code, true).await? {
            record_failure(pool, account.id).await?;
            return Err(AppError::InvalidMfaCode);
        }
    }
    repo::mark_recent_auth(pool, session.id).await
}

fn unix_now() -> Result<u64, AppError> {
    u64::try_from(Utc::now().timestamp()).map_err(|e| AppError::Internal(e.to_string()))
}

fn stored_secret(cipher: &FieldCipher, account: &AccountRow) -> Result<TotpSecret, AppError> {
    let ct = account
        .mfa_secret_ct
        .as_deref()
        .ok_or(AppError::Internal("MFA secret missing".to_owned()))?;
    cipher
        .decrypt(ct, &mfa::secret_aad(account.id))
        .map(TotpSecret::from_base32)
        .map_err(|e| AppError::Internal(format!("MFA secret: {e}")))
}

/// Checks a TOTP code (accepting each time step at most once) or, when `allow_recovery`, consumes
/// a recovery code. `Ok(false)` = wrong, expired, or replayed.
async fn second_factor_ok(
    pool: &PgPool,
    cipher: &FieldCipher,
    account: &AccountRow,
    input: &str,
    allow_recovery: bool,
) -> Result<bool, AppError> {
    match SecondFactor::parse(input) {
        Some(SecondFactor::Totp(code)) => {
            let secret = stored_secret(cipher, account)?;
            let Some(step) = secret.matching_step(&code, unix_now()?) else {
                return Ok(false);
            };
            let step = i64::try_from(step).map_err(|e| AppError::Internal(e.to_string()))?;
            repo::accept_totp_step(pool, account.id, step).await
        }
        Some(SecondFactor::Recovery(code)) if allow_recovery => {
            let hash = mfa::recovery_code_hash(account.id, &code);
            let used = repo::consume_recovery_code(pool, account.id, &hash).await?;
            if used {
                tracing::info!(user = %account.id, "MFA recovery code used");
            }
            Ok(used)
        }
        _ => Ok(false),
    }
}

async fn pending_account(pool: &PgPool, pending: &PendingMfa) -> Result<AccountRow, AppError> {
    let account = repo::account_by_id(pool, pending.user_id)
        .await?
        .ok_or(AppError::Unauthenticated)?;
    if account.locked_until.is_some_and(|until| until > Utc::now()) {
        // Locked mid-flow (e.g. by too many wrong codes): no attempt is evaluated, none counted.
        return Err(AppError::InvalidMfaCode);
    }
    Ok(account)
}

/// Completes a pending login with a TOTP or recovery code. On success the pending session is
/// revoked and a fresh full session issued (token rotation, as on any privilege change).
pub async fn verify_mfa(
    pool: &PgPool,
    cipher: &FieldCipher,
    pending: &PendingMfa,
    code: &str,
    meta: &ClientMeta,
) -> Result<(SessionToken, Me), AppError> {
    if !pending.enrolled {
        return Err(AppError::MfaEnrollmentRequired);
    }
    let account = pending_account(pool, pending).await?;
    if !second_factor_ok(pool, cipher, &account, code, true).await? {
        record_failure(pool, account.id).await?;
        return Err(AppError::InvalidMfaCode);
    }
    repo::reset_failed_logins(pool, account.id).await?;
    repo::revoke_session(pool, pending.id).await?;
    open_full_session(pool, account.id, meta).await
}

/// Starts (or restarts) enrollment for an account that must use MFA and has not enrolled yet.
/// A new secret replaces any unconfirmed one; an enabled authenticator is never replaced here.
pub async fn start_enrollment(
    pool: &PgPool,
    cipher: &FieldCipher,
    pending: &PendingMfa,
) -> Result<MfaEnrollment, AppError> {
    const ALREADY: AppError = AppError::Conflict {
        code: "auth.mfa_already_enrolled",
    };
    if pending.enrolled {
        return Err(ALREADY);
    }
    let account = pending_account(pool, pending).await?;
    let secret = TotpSecret::generate().map_err(|e| AppError::Internal(e.to_string()))?;
    let ct = cipher
        .encrypt(secret.base32(), &mfa::secret_aad(account.id))
        .map_err(|e| AppError::Internal(e.to_string()))?;
    if !repo::store_pending_mfa_secret(pool, account.id, &ct).await? {
        return Err(ALREADY);
    }
    let label = PhoneE164::parse(&account.phone)
        .map(|p| p.display_national())
        .unwrap_or(account.phone);
    let otpauth_uri = secret
        .otpauth_uri(&label)
        .ok_or(AppError::Internal("cannot build otpauth URI".to_owned()))?;
    Ok(MfaEnrollment {
        otpauth_uri,
        manual_key: secret.base32().to_owned(),
    })
}

/// Confirms enrollment with the first code from the authenticator: enables MFA, issues recovery
/// codes (returned once, stored hashed) and completes the login with a fresh full session.
pub async fn confirm_enrollment(
    pool: &PgPool,
    cipher: &FieldCipher,
    pending: &PendingMfa,
    code: &str,
    meta: &ClientMeta,
) -> Result<(SessionToken, MfaEnrolled), AppError> {
    if pending.enrolled {
        return Err(AppError::Conflict {
            code: "auth.mfa_already_enrolled",
        });
    }
    let account = pending_account(pool, pending).await?;
    let Some(verified_ct) = account.mfa_secret_ct.clone() else {
        return Err(AppError::Conflict {
            code: "auth.mfa_enrollment_not_started",
        });
    };
    // Recovery codes cannot confirm an authenticator: only a TOTP code proves it was set up.
    if !second_factor_ok(pool, cipher, &account, code, false).await? {
        record_failure(pool, account.id).await?;
        return Err(AppError::InvalidMfaCode);
    }

    let recovery_codes =
        mfa::generate_recovery_codes().map_err(|e| AppError::Internal(e.to_string()))?;
    let hashes: Vec<[u8; 32]> = recovery_codes
        .iter()
        .filter_map(|c| match SecondFactor::parse(c) {
            Some(SecondFactor::Recovery(normalised)) => {
                Some(mfa::recovery_code_hash(account.id, &normalised))
            }
            _ => None,
        })
        .collect();
    let mut tx = pool.begin().await?;
    if !repo::enable_mfa(&mut *tx, account.id, &verified_ct).await? {
        return Err(AppError::Conflict {
            code: "auth.mfa_enrollment_changed",
        });
    }
    repo::replace_recovery_codes(&mut tx, account.id, &hashes).await?;
    tx.commit().await?;

    repo::reset_failed_logins(pool, account.id).await?;
    repo::revoke_session(pool, pending.id).await?;
    let (token, me) = open_full_session(pool, account.id, meta).await?;
    Ok((token, MfaEnrolled { me, recovery_codes }))
}

/// Moves the session to another spa the user belongs to. Privileges change, so the token rotates,
/// but it is the same login: the re-auth time and the absolute expiry carry over (review H1).
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
    let issue = Issue::Rotated {
        recent_auth_at: session.recent_auth_at,
        absolute_expires_at: session.absolute_expires_at,
    };
    let token = issue_session(pool, session.user_id, Some(tenant), meta, issue).await?;
    let moved = Session {
        id: Uuid::nil(),
        active_tenant: Some(tenant),
        ..session.clone()
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

/// Permissions of `user` in `tenant` from their roles; `None` = not an active member of an active
/// spa. Requests get them through [`actor_for`]; operator tooling (`app seed-demo`) acts as a
/// named member with exactly these, so audit and permission checks stay truthful.
pub async fn tenant_permissions(
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
    let mut tx = TenantTx::begin(pool, tenant).await?;
    let access = repo::member_access(&mut tx, session.user_id).await?;
    tx.commit().await?;
    let access = access.ok_or(AppError::Forbidden {
        code: "auth.not_a_member",
    })?;
    // The spa settings ride along, so the request's own transactions skip re-reading them.
    let actor = Actor::new(session.user_id, tenant, access.permissions, ip);
    Ok(match access.facts {
        Some(facts) => actor.with_tenant_facts(facts),
        None => actor,
    })
}

/// `POST /auth/password` (ADR-0016): the signed-in person replaces their own password — the
/// temporary one the owner handed over, or their usual one. The current password is checked like
/// a login (a wrong one counts toward the lockout), the new one against
/// [`crate::account::check_new_password`]. Every session of the account ends and this browser
/// gets a fresh one (token rotation). Audited in each spa of the account, never with a password.
pub async fn change_password(
    pool: &PgPool,
    session: &Session,
    current_input: &str,
    new_input: &str,
    meta: &ClientMeta,
) -> Result<(SessionToken, Me), AppError> {
    let account = repo::account_by_id(pool, session.user_id)
        .await?
        .ok_or(AppError::Unauthenticated)?;
    if account.locked_until.is_some_and(|until| until > Utc::now()) {
        return Err(AppError::InvalidCredentials);
    }
    let candidate = current_input.to_owned();
    let stored = account.password_hash.clone();
    let matches = tokio::task::spawn_blocking(move || password::verify(&candidate, &stored))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    if !matches {
        record_failure(pool, account.id).await?;
        return Err(AppError::InvalidCredentials);
    }
    let phone = PhoneE164::parse(&account.phone)
        .map_err(|_| AppError::Internal("stored phone unreadable".to_owned()))?;
    crate::account::check_new_password(new_input, current_input, &phone)?;
    let hashed = crate::account::hash("new_password", new_input).await?;
    repo::set_own_password(pool, account.id, &hashed).await?;
    repo::revoke_all_sessions(pool, account.id).await?;
    let tenants = repo::tenants_of_user(pool, account.id).await?;
    for t in &tenants {
        let mut tx = TenantTx::begin(pool, t.id).await?;
        let actor = Actor::new(account.id, t.id, Vec::new(), meta.ip);
        kernel::audit::record(
            &mut tx,
            &actor,
            kernel::audit::AuditEntry {
                action: "account.password_change",
                entity_type: "user",
                entity_id: account.id.as_uuid(),
                before: Some(serde_json::json!({"temporary": account.must_change_password})),
                after: None,
                reason: None,
            },
        )
        .await?;
        tx.commit().await?;
    }
    open_full_session(pool, account.id, meta).await
}

/// `POST /auth/password-reset-requests` (option A, ADR-0016), public: "Quên mật khẩu?". Records
/// a request in each spa where the phone's login is an active member without `staff.manage`
/// (an owner recovers another way), at most one open request and one an hour per login and spa,
/// expiring after 24 h. The caller answers the same, after the same hashing work, whether or not
/// anything was recorded: nothing here tells which phones have an account.
pub async fn request_password_reset(
    pool: &PgPool,
    phone_input: &str,
    meta: &ClientMeta,
) -> Result<(), AppError> {
    // Equal work on every path (one Argon2 verification dominates).
    run_blocking_dummy(phone_input).await;
    let Ok(phone) = PhoneE164::parse(phone_input) else {
        return Ok(());
    };
    let Some(account) = repo::account_by_phone(pool, &phone).await? else {
        return Ok(());
    };
    if !account.active {
        return Ok(());
    }
    for t in repo::tenants_of_user(pool, account.id).await? {
        let mut tx = TenantTx::begin(pool, t.id).await?;
        let Some(perms) = repo::member_permissions(&mut tx, account.id).await? else {
            continue;
        };
        if perms.contains(&Permission::StaffManage) {
            continue;
        }
        if repo::open_reset_request(&mut tx, account.id).await? {
            kernel::audit::record_system(
                &mut tx,
                meta.ip,
                kernel::audit::AuditEntry {
                    action: "account.password_reset_requested",
                    entity_type: "user",
                    entity_id: account.id.as_uuid(),
                    before: None,
                    after: None,
                    reason: None,
                },
            )
            .await?;
        }
        tx.commit().await?;
    }
    Ok(())
}
