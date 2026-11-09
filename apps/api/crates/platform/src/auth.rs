//! Operator authentication (ADR-0018 D1): username + password, then TOTP — always. Sessions are
//! short (idle 30 min, absolute 8 h) and every write needs a step-up (password + code within the
//! last 5 minutes). Shares the primitives of `identity` (Argon2id, TOTP, tokens, lockout) but none
//! of its tables: a spa account can never become an operator, nor an operator a spa member.

use crate::audit::{self, PlatformEntry};
use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;
use axum::http::{HeaderMap, Method, header};
use chrono::{DateTime, Duration, Utc};
use identity::mfa::{self, SecondFactor, TotpSecret};
use identity::service::ClientMeta;
use identity::token::{self, SessionToken};
use identity::{lockout, password};
use kernel::crypto::FieldCipher;
use kernel::{AppError, FieldError};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

/// `__Host-`: Secure, Path=/, no Domain, so it is host-only — on a separate admin host it is never
/// sent to the spa app's host (ADR-0018 D2). The spa API never reads it.
pub const OPERATOR_COOKIE: &str = "__Host-op-session";
pub const IDLE_TIMEOUT_MINUTES: i64 = 30;
pub const ABSOLUTE_TIMEOUT_HOURS: i64 = 8;
pub const MFA_PENDING_MINUTES: i64 = 10;
/// A write must follow a password + code confirmation within this window.
pub const STEP_UP_MINUTES: i64 = 5;
const TOUCH_INTERVAL_SECONDS: i64 = 60;

pub fn operator_cookie(secret: &str) -> String {
    let max_age = ABSOLUTE_TIMEOUT_HOURS * 60 * 60;
    format!(
        "{OPERATOR_COOKIE}={secret}; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age={max_age}"
    )
}

pub fn clear_operator_cookie() -> String {
    format!("{OPERATOR_COOKIE}=; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=0")
}

/// The operator cookie's secret, if the request carries one. A spa cookie is ignored.
pub fn operator_secret(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, value)| *name == OPERATOR_COOKIE && !value.is_empty())
        .map(|(_, value)| value.to_owned())
}

/// AAD of an operator's encrypted TOTP secret: differs from a spa account's
/// (`user.mfa_secret`), so a ciphertext never decrypts on the other kind of row.
fn secret_aad(operator: Uuid) -> Vec<u8> {
    kernel::crypto::record_aad("platform_operator.mfa", &[operator])
}

fn recovery_code_hash(operator: Uuid, normalised: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"platform_operator:");
    hasher.update(operator.as_bytes());
    hasher.update(normalised.as_bytes());
    hasher.finalize().into()
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, ToSchema)]
pub struct OperatorSummary {
    pub id: Uuid,
    pub username: String,
    pub full_name: String,
}

/// Body of `GET /admin/me`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, ToSchema)]
pub struct OperatorMe {
    pub operator: OperatorSummary,
    /// Writes are allowed until then without a new step-up (`null` = step-up needed now).
    pub step_up_until: Option<DateTime<Utc>>,
}

/// What the browser does after the password step: always a second factor.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OperatorLoginNext {
    MfaVerify,
    MfaEnroll,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, ToSchema)]
pub struct OperatorLogin {
    pub next: OperatorLoginNext,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, ToSchema)]
pub struct OperatorEnrollment {
    pub otpauth_uri: String,
    pub manual_key: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, ToSchema)]
pub struct OperatorEnrolled {
    pub operator: OperatorSummary,
    /// Shown this once; stored hashed.
    pub recovery_codes: Vec<String>,
}

/// A live, MFA-complete operator session.
#[derive(Debug, Clone)]
pub struct OperatorSession {
    pub session_id: Uuid,
    pub operator_id: Uuid,
    /// SHA-256 of the cookie secret: what the platform DB functions check (`platform_gate`).
    pub token_hash: [u8; 32],
    pub recent_auth_at: DateTime<Utc>,
    pub meta: ClientMeta,
}

impl OperatorSession {
    pub fn step_up_until(&self) -> DateTime<Utc> {
        self.recent_auth_at + Duration::minutes(STEP_UP_MINUTES)
    }

    pub fn stepped_up(&self, now: DateTime<Utc>) -> bool {
        now < self.step_up_until()
    }
}

/// A session that passed the password and still owes the second factor.
#[derive(Debug, Clone)]
pub struct PendingOperator {
    pub session_id: Uuid,
    pub operator_id: Uuid,
    pub enrolled: bool,
    pub meta: ClientMeta,
}

struct SessionRow {
    id: Uuid,
    operator_id: Uuid,
    last_seen_at: DateTime<Utc>,
    absolute_expires_at: DateTime<Utc>,
    recent_auth_at: DateTime<Utc>,
    mfa_pending: bool,
    mfa_enrolled: bool,
}

async fn live_session(pool: &PgPool, hash: &[u8; 32]) -> Result<SessionRow, AppError> {
    let row = sqlx::query_as!(
        SessionRow,
        r#"SELECT s.id, s.operator_id, s.last_seen_at, s.absolute_expires_at, s.recent_auth_at,
                  s.mfa_pending, (o.mfa_enabled_at IS NOT NULL) AS "mfa_enrolled!"
           FROM app.platform_session s
           JOIN app.platform_operator o ON o.id = s.operator_id AND o.status = 'active'
           WHERE s.token_hash = $1 AND s.revoked_at IS NULL
             AND now() < s.idle_expires_at AND now() < s.absolute_expires_at"#,
        &hash[..]
    )
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::Unauthenticated)?;
    let now = Utc::now();
    if !row.mfa_pending && now - row.last_seen_at > Duration::seconds(TOUCH_INTERVAL_SECONDS) {
        let idle = (now + Duration::minutes(IDLE_TIMEOUT_MINUTES)).min(row.absolute_expires_at);
        sqlx::query!(
            "UPDATE app.platform_session SET last_seen_at = now(), idle_expires_at = $2 WHERE id = $1",
            row.id,
            idle
        )
        .execute(pool)
        .await?;
    }
    Ok(row)
}

fn secret_of(parts: &Parts) -> Result<String, AppError> {
    operator_secret(&parts.headers).ok_or(AppError::Unauthenticated)
}

/// Resolves the operator cookie into a full session (no step-up check).
pub async fn resolve(
    pool: &PgPool,
    secret: &str,
    meta: ClientMeta,
) -> Result<OperatorSession, AppError> {
    let token_hash = token::hash_secret(secret);
    let row = live_session(pool, &token_hash).await?;
    if row.mfa_pending {
        return Err(if row.mfa_enrolled {
            AppError::MfaRequired
        } else {
            AppError::MfaEnrollmentRequired
        });
    }
    Ok(OperatorSession {
        session_id: row.id,
        operator_id: row.operator_id,
        token_hash,
        recent_auth_at: row.recent_auth_at,
        meta,
    })
}

/// An operator acting in the console. Any method but `GET`/`HEAD` also needs a recent step-up
/// (`403 platform.step_up_required`): checked here, so no handler can forget it, and again by
/// the DB functions that write.
pub struct Operator(pub OperatorSession);

impl<S> FromRequestParts<S> for Operator
where
    PgPool: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let secret = secret_of(parts)?;
        let pool = PgPool::from_ref(state);
        let session = resolve(&pool, &secret, identity::http::client_meta(parts)).await?;
        let safe = matches!(parts.method, Method::GET | Method::HEAD);
        if !safe && !session.stepped_up(Utc::now()) {
            return Err(AppError::Forbidden {
                code: "platform.step_up_required",
            });
        }
        Ok(Self(session))
    }
}

/// A full operator session without the step-up rule: `GET /admin/me`, re-auth and logout-like
/// auth routes only.
pub struct OperatorAuth(pub OperatorSession);

impl<S> FromRequestParts<S> for OperatorAuth
where
    PgPool: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let secret = secret_of(parts)?;
        let pool = PgPool::from_ref(state);
        resolve(&pool, &secret, identity::http::client_meta(parts))
            .await
            .map(Self)
    }
}

/// A session waiting for its second factor (and only that): the MFA routes.
pub struct PendingOperatorSession(pub PendingOperator);

impl<S> FromRequestParts<S> for PendingOperatorSession
where
    PgPool: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let secret = secret_of(parts)?;
        let pool = PgPool::from_ref(state);
        let row = live_session(&pool, &token::hash_secret(&secret)).await?;
        if !row.mfa_pending {
            return Err(AppError::Conflict {
                code: "auth.mfa_not_pending",
            });
        }
        Ok(Self(PendingOperator {
            session_id: row.id,
            operator_id: row.operator_id,
            enrolled: row.mfa_enrolled,
            meta: identity::http::client_meta(parts),
        }))
    }
}

struct OperatorRow {
    id: Uuid,
    username: String,
    full_name: String,
    password_hash: String,
    active: bool,
    locked_until: Option<DateTime<Utc>>,
    mfa_secret_ct: Option<Vec<u8>>,
    mfa_enabled: bool,
}

async fn operator_by_username(
    pool: &PgPool,
    username: &str,
) -> Result<Option<OperatorRow>, AppError> {
    Ok(sqlx::query_as!(
        OperatorRow,
        r#"SELECT id, username, full_name, password_hash, status = 'active' AS "active!",
                  locked_until, mfa_secret_ct, mfa_enabled_at IS NOT NULL AS "mfa_enabled!"
           FROM app.platform_operator WHERE username = $1"#,
        username
    )
    .fetch_optional(pool)
    .await?)
}

async fn operator_by_id(pool: &PgPool, id: Uuid) -> Result<OperatorRow, AppError> {
    sqlx::query_as!(
        OperatorRow,
        r#"SELECT id, username, full_name, password_hash, status = 'active' AS "active!",
                  locked_until, mfa_secret_ct, mfa_enabled_at IS NOT NULL AS "mfa_enabled!"
           FROM app.platform_operator WHERE id = $1"#,
        id
    )
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::Unauthenticated)
}

fn summary(row: &OperatorRow) -> OperatorSummary {
    OperatorSummary {
        id: row.id,
        username: row.username.clone(),
        full_name: row.full_name.clone(),
    }
}

#[derive(Clone, Copy)]
enum Issue {
    MfaPending,
    Authenticated,
}

async fn issue_session(
    pool: &PgPool,
    operator: Uuid,
    meta: &ClientMeta,
    issue: Issue,
) -> Result<SessionToken, AppError> {
    let token = token::generate().map_err(|e| AppError::Internal(e.to_string()))?;
    let now = Utc::now();
    let (idle, absolute) = match issue {
        Issue::MfaPending => {
            let deadline = now + Duration::minutes(MFA_PENDING_MINUTES);
            (deadline, deadline)
        }
        Issue::Authenticated => (
            now + Duration::minutes(IDLE_TIMEOUT_MINUTES),
            now + Duration::hours(ABSOLUTE_TIMEOUT_HOURS),
        ),
    };
    let ip = meta.ip.map(sqlx::types::ipnetwork::IpNetwork::from);
    sqlx::query!(
        r#"INSERT INTO app.platform_session
             (id, token_hash, operator_id, idle_expires_at, absolute_expires_at, recent_auth_at,
              mfa_pending, ip, user_agent)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
        Uuid::now_v7(),
        &token.hash[..],
        operator,
        idle,
        absolute,
        now,
        matches!(issue, Issue::MfaPending),
        ip,
        meta.user_agent.as_deref()
    )
    .execute(pool)
    .await?;
    Ok(token)
}

async fn revoke_session(pool: &PgPool, session: Uuid) -> Result<(), AppError> {
    sqlx::query!(
        "UPDATE app.platform_session SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL",
        session
    )
    .execute(pool)
    .await?;
    Ok(())
}

async fn record_failure(pool: &PgPool, operator: Uuid) -> Result<(), AppError> {
    let failures = sqlx::query_scalar!(
        "UPDATE app.platform_operator SET failed_logins = failed_logins + 1, updated_at = now()
         WHERE id = $1 RETURNING failed_logins",
        operator
    )
    .fetch_one(pool)
    .await?;
    if let Some(until) = lockout::lock_until(failures, Utc::now()) {
        sqlx::query!(
            "UPDATE app.platform_operator SET locked_until = $2 WHERE id = $1",
            operator,
            until
        )
        .execute(pool)
        .await?;
    }
    Ok(())
}

async fn reset_failures(pool: &PgPool, operator: Uuid) -> Result<(), AppError> {
    sqlx::query!(
        "UPDATE app.platform_operator SET failed_logins = 0, locked_until = NULL, updated_at = now()
         WHERE id = $1",
        operator
    )
    .execute(pool)
    .await?;
    Ok(())
}

async fn verify_password(candidate: &str, stored: &str) -> Result<bool, AppError> {
    let (candidate, stored) = (candidate.to_owned(), stored.to_owned());
    tokio::task::spawn_blocking(move || password::verify(&candidate, &stored))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))
}

async fn dummy_work(candidate: &str) {
    let candidate = candidate.to_owned();
    let _ = tokio::task::spawn_blocking(move || password::verify_against_dummy(&candidate)).await;
}

/// `POST /admin/auth/login`. Unknown username, wrong password, locked or disabled operator: the
/// same `auth.invalid_credentials` after the same hashing work. Success only ever yields a
/// session pending MFA.
pub async fn login(
    pool: &PgPool,
    username: &str,
    password_input: &str,
    previous_secret: Option<&str>,
    meta: &ClientMeta,
) -> Result<(SessionToken, OperatorLogin), AppError> {
    let username = username.trim().to_ascii_lowercase();
    let Some(operator) = operator_by_username(pool, &username).await? else {
        dummy_work(password_input).await;
        return Err(AppError::InvalidCredentials);
    };
    let locked = operator
        .locked_until
        .is_some_and(|until| until > Utc::now());
    let matches = verify_password(password_input, &operator.password_hash).await?;
    if locked || !operator.active || !matches {
        if !locked && !matches {
            record_failure(pool, operator.id).await?;
        }
        return Err(AppError::InvalidCredentials);
    }
    if let Some(previous) = previous_secret {
        sqlx::query!(
            "UPDATE app.platform_session SET revoked_at = now()
             WHERE token_hash = $1 AND revoked_at IS NULL",
            &token::hash_secret(previous)[..]
        )
        .execute(pool)
        .await?;
    }
    // The failure counter is reset only after the second factor (as for spa owners).
    let token = issue_session(pool, operator.id, meta, Issue::MfaPending).await?;
    let next = if operator.mfa_enabled {
        OperatorLoginNext::MfaVerify
    } else {
        OperatorLoginNext::MfaEnroll
    };
    Ok((token, OperatorLogin { next }))
}

fn unix_now() -> Result<u64, AppError> {
    u64::try_from(Utc::now().timestamp()).map_err(|e| AppError::Internal(e.to_string()))
}

/// TOTP (each step once) or, with `allow_recovery`, a recovery code. `Ok(false)` = wrong.
async fn second_factor_ok(
    pool: &PgPool,
    cipher: &FieldCipher,
    operator: &OperatorRow,
    input: &str,
    allow_recovery: bool,
) -> Result<bool, AppError> {
    match SecondFactor::parse(input) {
        Some(SecondFactor::Totp(code)) => {
            let ct = operator
                .mfa_secret_ct
                .as_deref()
                .ok_or(AppError::Internal("operator MFA secret missing".to_owned()))?;
            let secret = cipher
                .decrypt(ct, &secret_aad(operator.id))
                .map(TotpSecret::from_base32)
                .map_err(|e| AppError::Internal(format!("operator MFA secret: {e}")))?;
            let Some(step) = secret.matching_step(&code, unix_now()?) else {
                return Ok(false);
            };
            let step = i64::try_from(step).map_err(|e| AppError::Internal(e.to_string()))?;
            let accepted = sqlx::query!(
                "UPDATE app.platform_operator SET mfa_last_step = $2
                 WHERE id = $1 AND (mfa_last_step IS NULL OR mfa_last_step < $2)",
                operator.id,
                step
            )
            .execute(pool)
            .await?;
            Ok(accepted.rows_affected() == 1)
        }
        Some(SecondFactor::Recovery(code)) if allow_recovery => {
            let hash = recovery_code_hash(operator.id, &code);
            let used = sqlx::query!(
                "UPDATE app.platform_recovery_code SET used_at = now()
                 WHERE operator_id = $1 AND code_hash = $2 AND used_at IS NULL",
                operator.id,
                &hash[..]
            )
            .execute(pool)
            .await?;
            if used.rows_affected() == 1 {
                tracing::info!(operator = %operator.id, "operator recovery code used");
            }
            Ok(used.rows_affected() == 1)
        }
        _ => Ok(false),
    }
}

async fn pending_operator(
    pool: &PgPool,
    pending: &PendingOperator,
) -> Result<OperatorRow, AppError> {
    let operator = operator_by_id(pool, pending.operator_id).await?;
    if operator
        .locked_until
        .is_some_and(|until| until > Utc::now())
        || !operator.active
    {
        return Err(AppError::InvalidMfaCode);
    }
    Ok(operator)
}

async fn audit_auth(
    pool: &PgPool,
    operator: Uuid,
    meta: &ClientMeta,
    action: &'static str,
) -> Result<(), AppError> {
    let mut conn = pool.acquire().await?;
    audit::record(
        &mut conn,
        operator,
        meta.ip,
        PlatformEntry {
            action,
            ..PlatformEntry::default()
        },
    )
    .await
}

/// `POST /admin/auth/mfa/verify`: completes the login with a fresh full session.
pub async fn verify_mfa(
    pool: &PgPool,
    cipher: &FieldCipher,
    pending: &PendingOperator,
    code: &str,
) -> Result<(SessionToken, OperatorSummary), AppError> {
    if !pending.enrolled {
        return Err(AppError::MfaEnrollmentRequired);
    }
    let operator = pending_operator(pool, pending).await?;
    if !second_factor_ok(pool, cipher, &operator, code, true).await? {
        record_failure(pool, operator.id).await?;
        return Err(AppError::InvalidMfaCode);
    }
    reset_failures(pool, operator.id).await?;
    revoke_session(pool, pending.session_id).await?;
    let token = issue_session(pool, operator.id, &pending.meta, Issue::Authenticated).await?;
    audit_auth(pool, operator.id, &pending.meta, "platform.auth.login").await?;
    Ok((token, summary(&operator)))
}

/// `POST /admin/auth/mfa/enroll/start`: a new secret replaces an unconfirmed one; an enabled
/// authenticator is never replaced here (CLI `reset-operator-mfa`).
pub async fn start_enrollment(
    pool: &PgPool,
    cipher: &FieldCipher,
    pending: &PendingOperator,
) -> Result<OperatorEnrollment, AppError> {
    const ALREADY: AppError = AppError::Conflict {
        code: "auth.mfa_already_enrolled",
    };
    if pending.enrolled {
        return Err(ALREADY);
    }
    let operator = pending_operator(pool, pending).await?;
    let secret = TotpSecret::generate().map_err(|e| AppError::Internal(e.to_string()))?;
    let ct = cipher
        .encrypt(secret.base32(), &secret_aad(operator.id))
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let stored = sqlx::query!(
        "UPDATE app.platform_operator SET mfa_secret_ct = $2, updated_at = now()
         WHERE id = $1 AND mfa_enabled_at IS NULL",
        operator.id,
        ct
    )
    .execute(pool)
    .await?;
    if stored.rows_affected() != 1 {
        return Err(ALREADY);
    }
    let label = format!("admin:{}", operator.username);
    let otpauth_uri = secret
        .otpauth_uri(&label)
        .ok_or(AppError::Internal("cannot build otpauth URI".to_owned()))?;
    Ok(OperatorEnrollment {
        otpauth_uri,
        manual_key: secret.base32().to_owned(),
    })
}

/// `POST /admin/auth/mfa/enroll/confirm`: the first TOTP code enables MFA, issues recovery codes
/// (returned once) and completes the login.
pub async fn confirm_enrollment(
    pool: &PgPool,
    cipher: &FieldCipher,
    pending: &PendingOperator,
    code: &str,
) -> Result<(SessionToken, OperatorEnrolled), AppError> {
    if pending.enrolled {
        return Err(AppError::Conflict {
            code: "auth.mfa_already_enrolled",
        });
    }
    let operator = pending_operator(pool, pending).await?;
    let Some(verified_ct) = operator.mfa_secret_ct.clone() else {
        return Err(AppError::Conflict {
            code: "auth.mfa_enrollment_not_started",
        });
    };
    if !second_factor_ok(pool, cipher, &operator, code, false).await? {
        record_failure(pool, operator.id).await?;
        return Err(AppError::InvalidMfaCode);
    }
    let codes = mfa::generate_recovery_codes().map_err(|e| AppError::Internal(e.to_string()))?;
    let mut tx = pool.begin().await?;
    let enabled = sqlx::query!(
        "UPDATE app.platform_operator SET mfa_enabled_at = now(), updated_at = now()
         WHERE id = $1 AND mfa_enabled_at IS NULL AND mfa_secret_ct = $2",
        operator.id,
        verified_ct
    )
    .execute(&mut *tx)
    .await?;
    if enabled.rows_affected() != 1 {
        return Err(AppError::Conflict {
            code: "auth.mfa_enrollment_changed",
        });
    }
    sqlx::query!(
        "DELETE FROM app.platform_recovery_code WHERE operator_id = $1",
        operator.id
    )
    .execute(&mut *tx)
    .await?;
    for code in &codes {
        let Some(SecondFactor::Recovery(normalised)) = SecondFactor::parse(code) else {
            continue;
        };
        sqlx::query!(
            "INSERT INTO app.platform_recovery_code (id, operator_id, code_hash) VALUES ($1, $2, $3)",
            Uuid::now_v7(),
            operator.id,
            &recovery_code_hash(operator.id, &normalised)[..]
        )
        .execute(&mut *tx)
        .await?;
    }
    audit::record(
        &mut tx,
        operator.id,
        pending.meta.ip,
        PlatformEntry {
            action: "platform.auth.mfa_enrolled",
            ..PlatformEntry::default()
        },
    )
    .await?;
    tx.commit().await?;
    reset_failures(pool, operator.id).await?;
    revoke_session(pool, pending.session_id).await?;
    let token = issue_session(pool, operator.id, &pending.meta, Issue::Authenticated).await?;
    audit_auth(pool, operator.id, &pending.meta, "platform.auth.login").await?;
    Ok((
        token,
        OperatorEnrolled {
            operator: summary(&operator),
            recovery_codes: codes,
        },
    ))
}

/// `POST /admin/auth/reauth`: password **and** a current code (or recovery code), always. Opens
/// the step-up window for writes. Failures count toward the lockout.
pub async fn reauth(
    pool: &PgPool,
    cipher: &FieldCipher,
    session: &OperatorSession,
    password_input: &str,
    code: &str,
) -> Result<(), AppError> {
    if code.trim().is_empty() {
        return Err(AppError::Validation(vec![FieldError::new(
            "code", "required",
        )]));
    }
    let operator = operator_by_id(pool, session.operator_id).await?;
    if operator
        .locked_until
        .is_some_and(|until| until > Utc::now())
    {
        return Err(AppError::InvalidCredentials);
    }
    if !verify_password(password_input, &operator.password_hash).await? {
        record_failure(pool, operator.id).await?;
        return Err(AppError::InvalidCredentials);
    }
    if !second_factor_ok(pool, cipher, &operator, code, true).await? {
        record_failure(pool, operator.id).await?;
        return Err(AppError::InvalidMfaCode);
    }
    sqlx::query!(
        "UPDATE app.platform_session SET recent_auth_at = now() WHERE id = $1",
        session.session_id
    )
    .execute(pool)
    .await?;
    audit_auth(pool, operator.id, &session.meta, "platform.auth.reauth").await
}

/// `POST /admin/auth/logout`: ends the session behind the cookie, full or pending.
pub async fn logout(pool: &PgPool, secret: &str, meta: &ClientMeta) -> Result<(), AppError> {
    let row = live_session(pool, &token::hash_secret(secret)).await?;
    revoke_session(pool, row.id).await?;
    if !row.mfa_pending {
        audit_auth(pool, row.operator_id, meta, "platform.auth.logout").await?;
    }
    Ok(())
}

pub async fn me(pool: &PgPool, session: &OperatorSession) -> Result<OperatorMe, AppError> {
    let operator = operator_by_id(pool, session.operator_id).await?;
    let now = Utc::now();
    Ok(OperatorMe {
        operator: summary(&operator),
        step_up_until: session.stepped_up(now).then(|| session.step_up_until()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn the_operator_cookie_is_host_only_strict_and_short() {
        let cookie = operator_cookie("abc");
        for flag in [
            "__Host-op-session=abc",
            "Path=/",
            "HttpOnly",
            "Secure",
            "SameSite=Strict",
            "Max-Age=28800",
        ] {
            assert!(cookie.contains(flag), "{flag} missing in {cookie}");
        }
        assert!(!cookie.contains("Domain"));
        assert!(clear_operator_cookie().contains("Max-Age=0"));
    }

    #[test]
    fn a_spa_session_cookie_is_not_an_operator_cookie() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("__Host-session=spa-token"),
        );
        assert_eq!(operator_secret(&headers), None);
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("__Host-session=spa; __Host-op-session=op"),
        );
        assert_eq!(operator_secret(&headers).as_deref(), Some("op"));
    }

    #[test]
    fn operator_secrets_and_codes_are_bound_to_the_operator() {
        let (a, b) = (Uuid::now_v7(), Uuid::now_v7());
        assert_ne!(secret_aad(a), secret_aad(b));
        assert_ne!(
            secret_aad(a),
            mfa::secret_aad(kernel::UserId::from_uuid(a)),
            "an operator ciphertext must not decrypt on a spa account with the same id"
        );
        assert_ne!(recovery_code_hash(a, "x"), recovery_code_hash(b, "x"));
    }
}
