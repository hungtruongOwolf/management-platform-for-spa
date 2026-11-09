//! Argon2id password hashing. Hashing is CPU-heavy, so callers run it on the blocking pool.

use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use std::collections::HashSet;
use std::sync::OnceLock;

pub const MIN_LEN: usize = 10;
pub const MAX_LEN: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PasswordError {
    #[error("password too short")]
    TooShort,
    #[error("password too long")]
    TooLong,
    #[error("password is on the common-password list")]
    TooCommon,
    #[error("password hashing failed")]
    Hashing,
}

impl PasswordError {
    /// The `FieldError` code of a refused password (the web maps it to Vietnamese).
    pub const fn code(self) -> &'static str {
        match self {
            Self::TooShort => "too_short",
            Self::TooLong => "too_long",
            Self::TooCommon => "too_common",
            Self::Hashing => "invalid",
        }
    }
}

/// Common passwords, checked offline (ADR-0019 D18: no call to an outside service).
/// `common_passwords.txt`: the entries of 10+ characters (shorter ones fail the length rule
/// anyway) of the NCSC "100k most used passwords" list (Pwned Passwords) as published in
/// SecLists (MIT licence, danielmiessler/SecLists, Passwords/Common-Credentials), lower-cased,
/// one per line. `common_passwords_vi.txt`: Vietnamese ones and this spa's name, kept by hand.
const COMMON: [&str; 2] = [
    include_str!("common_passwords.txt"),
    include_str!("common_passwords_vi.txt"),
];

/// On the bundled list, ignoring case and surrounding spaces ("Password1234" = "password1234").
pub fn is_common(password: &str) -> bool {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    let set = SET.get_or_init(|| {
        COMMON
            .iter()
            .flat_map(|list| list.lines())
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect()
    });
    set.contains(password.trim().to_lowercase().as_str())
}

/// Length policy (no composition rules, per NIST SP 800-63B) plus the blocklist of common
/// passwords that SP 800-63B also asks for. Every password a person or an operator chooses goes
/// through here (change, CLI provisioning, operator CLI); generated temporary passwords pass it
/// trivially.
pub fn check_policy(password: &str) -> Result<(), PasswordError> {
    let len = password.chars().count();
    if len < MIN_LEN {
        Err(PasswordError::TooShort)
    } else if len > MAX_LEN {
        Err(PasswordError::TooLong)
    } else if is_common(password) {
        Err(PasswordError::TooCommon)
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
    fn refuses_a_common_password_whatever_its_case() {
        for common in [
            "password1234",
            "Password1234",
            " 1234567890 ",
            "QWERTYUIOP",
            "matkhau123",
            "VienPhucYDuong",
        ] {
            assert_eq!(
                check_policy(common),
                Err(PasswordError::TooCommon),
                "{common}"
            );
        }
        assert_eq!(check_policy("mat-khau-du-dai"), Ok(()));
        assert_eq!(PasswordError::TooCommon.code(), "too_common");
        // Every bundled entry is lower-case and trimmed, or the case-insensitive lookup misses it.
        for line in COMMON.iter().flat_map(|list| list.lines()) {
            assert_eq!(line, line.trim().to_lowercase(), "{line:?}");
        }
    }

    #[test]
    fn a_generated_temporary_password_is_never_common() {
        assert!(!is_common("ABCD-EFGH-JKLM"));
        assert!(!is_common("dummy-password-for-timing"));
    }

    #[test]
    fn garbage_hash_never_verifies() {
        assert!(!verify("anything-at-all", "not-a-phc-string"));
    }
}
