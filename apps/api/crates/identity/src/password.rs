//! Argon2id password hashing. Hashing is CPU-heavy, so callers run it on the blocking pool.

use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use std::sync::OnceLock;

pub const MIN_LEN: usize = 10;
pub const MAX_LEN: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PasswordError {
    #[error("password too short")]
    TooShort,
    #[error("password too long")]
    TooLong,
    #[error("password hashing failed")]
    Hashing,
}

/// Length-only policy (no composition rules, per NIST SP 800-63B). Breached-list check comes with spec #4.
pub fn check_policy(password: &str) -> Result<(), PasswordError> {
    let len = password.chars().count();
    if len < MIN_LEN {
        Err(PasswordError::TooShort)
    } else if len > MAX_LEN {
        Err(PasswordError::TooLong)
    } else {
        Ok(())
    }
}

pub fn hash(password: &str) -> Result<String, PasswordError> {
    check_policy(password)?;
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|h| h.to_string())
        .map_err(|_| PasswordError::Hashing)
}

pub fn verify(password: &str, stored_hash: &str) -> bool {
    PasswordHash::new(stored_hash).ok().is_some_and(|parsed| {
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    })
}

/// Spends the same work as a real verification when the account does not exist,
/// so response time does not reveal which phone numbers are registered.
pub fn verify_against_dummy(password: &str) {
    static DUMMY: OnceLock<String> = OnceLock::new();
    let dummy = DUMMY.get_or_init(|| hash("dummy-password-for-timing").unwrap_or_default());
    let _ = verify(password, dummy);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_with_argon2id_and_verifies() {
        let hashed = hash("mat-khau-du-dai").unwrap_or_default();
        assert!(hashed.starts_with("$argon2id$"));
        assert!(verify("mat-khau-du-dai", &hashed));
        assert!(!verify("mat-khau-sai-roi", &hashed));
    }

    #[test]
    fn same_password_gets_a_different_salt_each_time() {
        assert_ne!(hash("mat-khau-du-dai").ok(), hash("mat-khau-du-dai").ok());
    }

    #[test]
    fn enforces_length_policy_counting_characters_not_bytes() {
        assert_eq!(check_policy("ngắn"), Err(PasswordError::TooShort));
        assert_eq!(check_policy("đủ mười ký tự"), Ok(()));
        assert_eq!(
            check_policy(&"a".repeat(MAX_LEN + 1)),
            Err(PasswordError::TooLong)
        );
    }

    #[test]
    fn garbage_hash_never_verifies() {
        assert!(!verify("anything-at-all", "not-a-phc-string"));
    }
}
