//! Operator accounts, CLI only (ADR-0018 D1): there is no route that creates an operator. The
//! password comes from the environment (never argv); MFA is enrolled at the first sign-in.

use identity::password;
use kernel::{AppError, FieldError};
use sqlx::PgPool;
use uuid::Uuid;

fn username_ok(username: &str) -> bool {
    let len = username.len();
    (3..=40).contains(&len)
        && username
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        && username.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
        })
}

/// `app add-operator --username U --name N`. `409 operator.username_taken` when it exists.
pub async fn add_operator(
    pool: &PgPool,
    username: &str,
    full_name: &str,
    password_input: &str,
) -> Result<Uuid, AppError> {
    let username = username.trim().to_ascii_lowercase();
    if !username_ok(&username) {
        return Err(AppError::Validation(vec![FieldError::new(
            "username",
            "invalid_format",
        )]));
    }
    let candidate = password_input.to_owned();
    let hashed = tokio::task::spawn_blocking(move || password::hash(&candidate))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .map_err(|e| AppError::Validation(vec![FieldError::new("password", e.code())]))?;
    let id = Uuid::now_v7();
    let inserted = sqlx::query!(
        "INSERT INTO app.platform_operator (id, username, full_name, password_hash)
         VALUES ($1, $2, $3, $4) ON CONFLICT (username) DO NOTHING",
        id,
        username,
        full_name.trim(),
        hashed
    )
    .execute(pool)
    .await?;
    if inserted.rows_affected() == 0 {
        return Err(AppError::Conflict {
            code: "operator.username_taken",
        });
    }
    Ok(id)
}

/// `app reset-operator-mfa --username U`: lost authenticator. Clears the secret and recovery
/// codes and ends every session; the next sign-in enrolls again. False for an unknown username.
pub async fn reset_operator_mfa(pool: &PgPool, username: &str) -> Result<bool, AppError> {
    let mut tx = pool.begin().await?;
    let Some(id) = sqlx::query_scalar!(
        "UPDATE app.platform_operator
         SET mfa_secret_ct = NULL, mfa_enabled_at = NULL, mfa_last_step = NULL, updated_at = now()
         WHERE username = $1 RETURNING id",
        username.trim().to_ascii_lowercase()
    )
    .fetch_optional(&mut *tx)
    .await?
    else {
        return Ok(false);
    };
    sqlx::query!(
        "DELETE FROM app.platform_recovery_code WHERE operator_id = $1",
        id
    )
    .execute(&mut *tx)
    .await?;
    end_sessions(&mut tx, id).await?;
    tx.commit().await?;
    Ok(true)
}

/// `app disable-operator --username U` (or `--enable`): a disabled operator cannot sign in and
/// loses every session.
pub async fn set_operator_active(
    pool: &PgPool,
    username: &str,
    active: bool,
) -> Result<bool, AppError> {
    let mut tx = pool.begin().await?;
    let Some(id) = sqlx::query_scalar!(
        "UPDATE app.platform_operator
         SET status = CASE WHEN $2 THEN 'active' ELSE 'disabled' END,
             failed_logins = 0, locked_until = NULL, updated_at = now()
         WHERE username = $1 RETURNING id",
        username.trim().to_ascii_lowercase(),
        active
    )
    .fetch_optional(&mut *tx)
    .await?
    else {
        return Ok(false);
    };
    if !active {
        end_sessions(&mut tx, id).await?;
    }
    tx.commit().await?;
    Ok(true)
}

async fn end_sessions(conn: &mut sqlx::PgConnection, operator: Uuid) -> Result<(), AppError> {
    sqlx::query!(
        "UPDATE app.platform_session SET revoked_at = now()
         WHERE operator_id = $1 AND revoked_at IS NULL",
        operator
    )
    .execute(conn)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usernames_are_short_lowercase_words() {
        for ok in ["hung", "em.ops", "ops_1", "a-b-c"] {
            assert!(username_ok(ok), "{ok}");
        }
        for bad in [
            "ab",
            "Hung",
            ".ops",
            "ops admin",
            "0905123456789012345678901234567890123456789",
            "ộps",
        ] {
            assert!(!username_ok(bad), "{bad}");
        }
    }
}
