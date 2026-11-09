//! Logins created and looked after by the spa's owner (ADR-0016): a login made from a staff
//! profile with a temporary password, an owner's reset, a role change, and forgotten-password
//! requests (option A). The caller (the roster, `org::account`) authorizes and audits; these
//! run inside its transaction. Passwords never leave here except as the one-time
//! [`TemporaryPassword`] the caller returns to the owner.

use crate::password::{self, PasswordError};
use crate::repo::{self, ResetRequestRow};
use crate::roles::SystemRole;
use crate::token::RandomUnavailable;
use kernel::{AppError, FieldError, PhoneE164, TenantTx, UserId};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// 32 symbols without look-alikes (no I, O, 0, 1): each random byte maps without bias (`b & 31`).
const ALPHABET: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
/// 12 symbols = 60 bits, written `ABCD-EFGH-JKLM` (the hyphens are part of the password).
const SYMBOLS: usize = 12;

/// A fresh temporary password: returned once to the owner, stored only as its Argon2id hash.
/// `Debug` never prints it.
pub struct TemporaryPassword(String);

impl TemporaryPassword {
    pub fn generate() -> Result<Self, RandomUnavailable> {
        let mut bytes = [0_u8; SYMBOLS];
        getrandom::fill(&mut bytes).map_err(|_| RandomUnavailable)?;
        let mut out = String::with_capacity(SYMBOLS + 2);
        for (i, b) in bytes.iter().enumerate() {
            if i > 0 && i % 4 == 0 {
                out.push('-');
            }
            out.push(char::from(ALPHABET[usize::from(b & 31)]));
        }
        Ok(Self(out))
    }

    /// The value, for the one response that hands it to the owner.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for TemporaryPassword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TemporaryPassword([redacted])")
    }
}

/// The roles the owner gives from the roster (the owner role is never handed out here).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LoginRole {
    /// Quản lý.
    Manager,
    /// Kỹ thuật viên.
    Technician,
}

impl LoginRole {
    pub const fn system(self) -> SystemRole {
        match self {
            Self::Manager => SystemRole::Manager,
            Self::Technician => SystemRole::Technician,
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match SystemRole::from_key(key)? {
            SystemRole::Manager => Some(Self::Manager),
            SystemRole::Technician => Some(Self::Technician),
            SystemRole::Owner => None,
        }
    }
}

fn policy_error(field: &str, e: PasswordError) -> AppError {
    AppError::Validation(vec![FieldError::new(field, e.code())])
}

/// Argon2id on the blocking pool.
pub async fn hash(field: &str, plain: &str) -> Result<String, AppError> {
    let candidate = plain.to_owned();
    tokio::task::spawn_blocking(move || password::hash(&candidate))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .map_err(|e| policy_error(field, e))
}

/// The password policy for a password the person chooses (ADR-0016): 10–128 characters (counted
/// as characters, no composition rules, NIST SP 800-63B), not the current one, not the login
/// phone in any written form.
pub fn check_new_password(new: &str, current: &str, phone: &PhoneE164) -> Result<(), AppError> {
    password::check_policy(new).map_err(|e| policy_error("new_password", e))?;
    if new == current {
        return Err(AppError::Validation(vec![FieldError::new(
            "new_password",
            "same_as_current",
        )]));
    }
    let digits: String = new.chars().filter(char::is_ascii_digit).collect();
    let e164 = phone.as_str().trim_start_matches('+').to_owned();
    let national = format!("0{}", e164.get(2..).unwrap_or_default());
    if new
        .chars()
        .all(|c| c.is_ascii_digit() || c.is_whitespace() || "+-.".contains(c))
        && (digits == e164 || digits == national)
    {
        return Err(AppError::Validation(vec![FieldError::new(
            "new_password",
            "is_phone",
        )]));
    }
    Ok(())
}

fn random_failure(_: RandomUnavailable) -> AppError {
    AppError::Internal("system random source unavailable".to_owned())
}

/// Creates a login for a staff profile: phone (a valid Vietnamese mobile, used by no account
/// yet: accounts are global, and the owner must never take over someone else's), name, role,
/// temporary password; an active membership of the transaction's spa. The caller links it.
pub async fn create_login(
    tx: &mut TenantTx,
    phone: &str,
    full_name: &str,
    role: LoginRole,
) -> Result<(UserId, TemporaryPassword), AppError> {
    let phone = PhoneE164::parse(phone)
        .map_err(|_| AppError::Validation(vec![FieldError::new("phone", "invalid_format")]))?;
    if repo::phone_taken(tx, &phone).await? {
        return Err(AppError::Conflict {
            code: "account.phone_taken",
        });
    }
    let temporary = TemporaryPassword::generate().map_err(random_failure)?;
    let hashed = hash("password", temporary.expose()).await?;
    let user = repo::insert_login(tx, &phone, full_name, &hashed, role.system().key()).await?;
    Ok((user, temporary))
}

/// A new temporary password for `user`; every session ends and the next sign-in must set a new
/// password. The caller checks that the owner may reset this login.
pub async fn reset_password(
    tx: &mut TenantTx,
    user: UserId,
) -> Result<TemporaryPassword, AppError> {
    let temporary = TemporaryPassword::generate().map_err(random_failure)?;
    let hashed = hash("password", temporary.expose()).await?;
    repo::set_temporary_password(tx, user, &hashed).await?;
    Ok(temporary)
}

/// The login's role in the transaction's spa, when it is one of the roster's roles.
pub async fn role_of(tx: &mut TenantTx, user: UserId) -> Result<Option<LoginRole>, AppError> {
    let keys = repo::role_keys(tx, user).await?;
    Ok(match keys.as_slice() {
        [only] => LoginRole::from_key(only),
        _ => None,
    })
}

/// Replaces the login's role in the transaction's spa (permissions are read on every request).
pub async fn set_role(tx: &mut TenantTx, user: UserId, role: LoginRole) -> Result<(), AppError> {
    repo::replace_role(tx, user, role.system().key()).await
}

/// A forgotten-password request (option A) as the owner sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResetRequest {
    pub id: Uuid,
    pub user_id: UserId,
    pub requested_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

impl From<ResetRequestRow> for ResetRequest {
    fn from(r: ResetRequestRow) -> Self {
        Self {
            id: r.id,
            user_id: UserId::from_uuid(r.user_id),
            requested_at: r.requested_at,
            expires_at: r.expires_at,
        }
    }
}

/// Open, unexpired requests of the transaction's spa.
pub async fn reset_requests(tx: &mut TenantTx) -> Result<Vec<ResetRequest>, AppError> {
    Ok(repo::open_reset_requests(tx)
        .await?
        .into_iter()
        .map(ResetRequest::from)
        .collect())
}

/// "Bỏ qua": closes one open request; `None` when there is no such open request.
pub async fn dismiss_request(
    tx: &mut TenantTx,
    id: Uuid,
    by: UserId,
) -> Result<Option<UserId>, AppError> {
    Ok(
        repo::close_reset_requests(tx, Some(id), None, "dismissed", by)
            .await?
            .pop()
            .map(UserId::from_uuid),
    )
}

/// A reset answers every open request of that login.
pub async fn close_requests_of(
    tx: &mut TenantTx,
    user: UserId,
    by: UserId,
) -> Result<(), AppError> {
    repo::close_reset_requests(tx, None, Some(user), "reset", by).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temporary_passwords_are_long_readable_and_unique() -> Result<(), RandomUnavailable> {
        let (a, b) = (
            TemporaryPassword::generate()?,
            TemporaryPassword::generate()?,
        );
        assert_ne!(a.expose(), b.expose());
        assert_eq!(a.expose().len(), 14);
        assert!(password::check_policy(a.expose()).is_ok());
        assert!(
            a.expose()
                .bytes()
                .all(|c| c == b'-' || ALPHABET.contains(&c))
        );
        assert_eq!(format!("{a:?}"), "TemporaryPassword([redacted])");
        Ok(())
    }

    #[test]
    fn a_new_password_is_long_new_and_not_the_phone() -> Result<(), Box<dyn std::error::Error>> {
        let phone = PhoneE164::parse("0905123456")?;
        assert!(check_new_password("mat-khau-moi-cua-toi", "cu-da-dung-lau", &phone).is_ok());
        let code = |r: Result<(), AppError>| match r {
            Err(AppError::Validation(e)) => e.first().map(|f| f.code),
            _ => None,
        };
        assert_eq!(
            code(check_new_password("ngan", "x", &phone)),
            Some("too_short")
        );
        assert_eq!(
            code(check_new_password(
                "giong-het-cu-1",
                "giong-het-cu-1",
                &phone
            )),
            Some("same_as_current")
        );
        for p in [
            "0905123456",
            "84905123456",
            "+84 905 123 456",
            "0905.123.456",
        ] {
            assert_eq!(
                code(check_new_password(p, "x", &phone)),
                Some("is_phone"),
                "{p}"
            );
        }
        assert!(check_new_password("09051234567890", "x", &phone).is_ok());
        Ok(())
    }

    #[test]
    fn the_roster_hands_out_manager_and_technician_only() {
        assert_eq!(LoginRole::from_key("manager"), Some(LoginRole::Manager));
        assert_eq!(
            LoginRole::from_key("technician"),
            Some(LoginRole::Technician)
        );
        assert_eq!(LoginRole::from_key("owner"), None);
        assert_eq!(LoginRole::Manager.system().key(), "manager");
    }
}
