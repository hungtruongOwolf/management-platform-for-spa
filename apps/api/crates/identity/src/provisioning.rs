//! Platform provisioning (CLI only, schema-owner connection — runs outside tenant RLS, like an
//! operator action): create a spa with its default branch and system roles, add users to it.

use crate::password;
use crate::roles::SystemRole;
use kernel::{AppError, FieldError, PhoneE164, TenantId, UserId};
use sqlx::PgPool;
use uuid::Uuid;

pub async fn create_tenant(
    owner: &PgPool,
    slug: &str,
    name: &str,
    branch_name: &str,
) -> Result<TenantId, AppError> {
    let mut tx = owner.begin().await?;
    let tenant = TenantId::new();
    sqlx::query!(
        "INSERT INTO app.tenant (id, name, slug) VALUES ($1, $2, $3)",
        tenant.as_uuid(),
        name,
        slug
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        match e
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::code)
        {
            Some(code) if code == "23505" => AppError::Conflict {
                code: "tenant.slug_taken",
            },
            Some(code) if code == "23514" => {
                AppError::Validation(vec![FieldError::new("slug", "invalid_format")])
            }
            _ => AppError::Database(e),
        }
    })?;
    sqlx::query!(
        "INSERT INTO app.branch (id, tenant_id, name) VALUES ($1, $2, $3)",
        Uuid::now_v7(),
        tenant.as_uuid(),
        branch_name
    )
    .execute(&mut *tx)
    .await?;
    for role in SystemRole::ALL {
        let role_id = Uuid::now_v7();
        sqlx::query!(
            "INSERT INTO app.role (id, tenant_id, key, name, is_system, requires_mfa) VALUES ($1, $2, $3, $4, true, $5)",
            role_id,
            tenant.as_uuid(),
            role.key(),
            role.display_name(),
            role.requires_mfa()
        )
        .execute(&mut *tx)
        .await?;
        for permission in role.default_permissions() {
            sqlx::query!(
                "INSERT INTO app.role_permission (tenant_id, role_id, permission) VALUES ($1, $2, $3)",
                tenant.as_uuid(),
                role_id,
                permission.as_str()
            )
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;
    Ok(tenant)
}

pub struct NewUser<'a> {
    pub tenant_slug: &'a str,
    pub phone: &'a str,
    pub full_name: &'a str,
    pub password: &'a str,
    pub role: SystemRole,
}

/// Adds a person to a spa with a role. An existing account (same phone) is reused and keeps its password.
pub async fn add_user(owner: &PgPool, new: NewUser<'_>) -> Result<UserId, AppError> {
    let phone = PhoneE164::parse(new.phone)
        .map_err(|_| AppError::Validation(vec![FieldError::new("phone", "invalid_format")]))?;
    let mut tx = owner.begin().await?;
    let tenant = sqlx::query_scalar!("SELECT id FROM app.tenant WHERE slug = $1", new.tenant_slug)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound)?;

    let existing = sqlx::query_scalar!(
        "SELECT id FROM app.user_account WHERE phone_e164 = $1",
        phone.as_str()
    )
    .fetch_optional(&mut *tx)
    .await?;
    let user = match existing {
        Some(id) => UserId::from_uuid(id),
        None => {
            let candidate = new.password.to_owned();
            let hashed = tokio::task::spawn_blocking(move || password::hash(&candidate))
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?
                .map_err(|e| {
                    AppError::Validation(vec![FieldError::new(
                        "password",
                        match e {
                            password::PasswordError::TooShort => "too_short",
                            password::PasswordError::TooLong => "too_long",
                            password::PasswordError::Hashing => "invalid",
                        },
                    )])
                })?;
            let id = UserId::new();
            sqlx::query!(
                "INSERT INTO app.user_account (id, phone_e164, full_name, password_hash) VALUES ($1, $2, $3, $4)",
                id.as_uuid(),
                phone.as_str(),
                new.full_name,
                hashed
            )
            .execute(&mut *tx)
            .await?;
            id
        }
    };

    let membership = sqlx::query_scalar!(
        r#"INSERT INTO app.membership (id, tenant_id, user_id) VALUES ($1, $2, $3)
           ON CONFLICT (tenant_id, user_id) DO UPDATE SET status = 'active'
           RETURNING id"#,
        Uuid::now_v7(),
        tenant,
        user.as_uuid()
    )
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query!(
        r#"INSERT INTO app.membership_role (tenant_id, membership_id, role_id)
           SELECT $1, $2, id FROM app.role WHERE tenant_id = $1 AND key = $3
           ON CONFLICT DO NOTHING"#,
        tenant,
        membership,
        new.role.key()
    )
    .execute(&mut *tx)
    .await?;
    // A role that requires MFA must not be usable from a session that never passed a second
    // factor (security review L4): end every session, so the next login goes through MFA.
    let requires_mfa = sqlx::query_scalar!(
        "SELECT requires_mfa FROM app.role WHERE tenant_id = $1 AND key = $2",
        tenant,
        new.role.key()
    )
    .fetch_one(&mut *tx)
    .await?;
    if requires_mfa {
        revoke_sessions(&mut tx, user.as_uuid()).await?;
    }
    tx.commit().await?;
    Ok(user)
}

/// Operator action for a locked-out account (security review L2): anyone who knows a phone number
/// can trip the per-account lockout, so the operator can clear it after confirming the person
/// out of band. Sessions are untouched; the password is not changed. False for an unknown phone.
pub async fn unlock_account(owner: &PgPool, phone: &str) -> Result<bool, AppError> {
    let phone = PhoneE164::parse(phone)
        .map_err(|_| AppError::Validation(vec![FieldError::new("phone", "invalid_format")]))?;
    let done = sqlx::query!(
        "UPDATE app.user_account SET failed_logins = 0, locked_until = NULL, updated_at = now()
         WHERE phone_e164 = $1",
        phone.as_str()
    )
    .execute(owner)
    .await?;
    Ok(done.rows_affected() == 1)
}

async fn revoke_sessions(conn: &mut sqlx::PgConnection, user: Uuid) -> Result<u64, AppError> {
    let done = sqlx::query!(
        "UPDATE app.session SET revoked_at = now() WHERE user_id = $1 AND revoked_at IS NULL",
        user
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected())
}

/// Operator action for a lost authenticator: clears the TOTP secret and recovery codes and ends
/// every session, so the next login goes through enrollment again. Identity must be verified
/// out of band (in person / known phone) before running it. Returns false for an unknown phone.
pub async fn reset_mfa(owner: &PgPool, phone: &str) -> Result<bool, AppError> {
    let phone = PhoneE164::parse(phone)
        .map_err(|_| AppError::Validation(vec![FieldError::new("phone", "invalid_format")]))?;
    let mut tx = owner.begin().await?;
    let Some(user) = sqlx::query_scalar!(
        "UPDATE app.user_account
         SET mfa_secret_ct = NULL, mfa_enabled_at = NULL, mfa_last_step = NULL, updated_at = now()
         WHERE phone_e164 = $1 RETURNING id",
        phone.as_str()
    )
    .fetch_optional(&mut *tx)
    .await?
    else {
        return Ok(false);
    };
    sqlx::query!("DELETE FROM app.mfa_recovery_code WHERE user_id = $1", user)
        .execute(&mut *tx)
        .await?;
    revoke_sessions(&mut tx, user).await?;
    tx.commit().await?;
    Ok(true)
}
