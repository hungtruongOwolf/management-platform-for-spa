//! Second factor (ADR-0004): TOTP per RFC 6238 (HMAC-SHA-1, 6 digits, 30 s steps, ±1 step skew)
//! and single-use recovery codes. Pure functions only; persistence and replay state live in
//! `service`/`repo`.

use crate::token::RandomUnavailable;
use kernel::UserId;
use sha2::{Digest, Sha256};
use totp_rs::{Algorithm, Builder, Secret, Totp};

pub const ISSUER: &str = "Viên Phúc Y Đường";
pub const STEP_SECONDS: u64 = 30;
pub const RECOVERY_CODE_COUNT: usize = 10;
/// 160-bit secrets (the RFC 4226 recommendation).
const SECRET_BYTES: usize = 20;
/// 16 characters of a 32-symbol alphabet = 80 random bits per recovery code.
const RECOVERY_CODE_CHARS: usize = 16;
/// Crockford-style base32 without I, L, O, U: nothing that reads ambiguously on paper.
const RECOVERY_ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

/// A TOTP secret as stored (encrypted) in `mfa_secret_ct`: RFC 4648 base32, no padding.
pub struct TotpSecret(String);

impl TotpSecret {
    pub fn generate() -> Result<Self, RandomUnavailable> {
        let mut bytes = [0_u8; SECRET_BYTES];
        getrandom::fill(&mut bytes).map_err(|_| RandomUnavailable)?;
        Ok(Self(Secret::from(bytes).to_base32()))
    }

    pub fn from_base32(encoded: String) -> Self {
        Self(encoded)
    }

    pub fn base32(&self) -> &str {
        &self.0
    }

    fn totp(&self, account: &str) -> Option<Totp> {
        let secret = Secret::try_from_base32(&self.0).ok()?;
        Builder::new()
            .with_algorithm(Algorithm::SHA1)
            .with_digits(6)
            .with_skew(1)
            .with_step_duration(STEP_SECONDS)
            .with_secret(secret)
            .with_issuer(Some(ISSUER))
            .with_account_name(account.replace(':', " "))
            .build()
            .ok()
    }

    /// The time step a 6-digit code is valid for at `unix_time` (±1 step), or None.
    /// The caller must reject a step ≤ the last one accepted (RFC 6238 §5.2: one use per step).
    pub fn matching_step(&self, code: &str, unix_time: u64) -> Option<u64> {
        self.totp("-")?.check(code, unix_time)
    }

    /// The code for `unix_time` (tests and the dev CLI only need it; login never generates codes).
    pub fn code_at(&self, unix_time: u64) -> Option<String> {
        self.totp("-").map(|t| t.generate(unix_time).to_string())
    }

    /// `otpauth://totp/...` for authenticator apps; the QR code is drawn client-side from it.
    pub fn otpauth_uri(&self, account: &str) -> Option<String> {
        self.totp(account)?.to_url().ok()
    }
}

/// What the user typed as a second factor.
#[derive(Debug, PartialEq, Eq)]
pub enum SecondFactor {
    Totp(String),
    Recovery(String),
}

impl SecondFactor {
    /// Six digits (spaces allowed) are a TOTP code; anything else is tried as a recovery code.
    pub fn parse(input: &str) -> Option<Self> {
        let compact: String = input
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '-')
            .collect();
        if compact.is_empty() || compact.len() > 64 {
            return None;
        }
        if compact.len() == 6 && compact.bytes().all(|b| b.is_ascii_digit()) {
            return Some(Self::Totp(compact));
        }
        Some(Self::Recovery(compact.to_ascii_lowercase()))
    }
}

/// Fresh recovery codes, formatted for display as `xxxx-xxxx-xxxx-xxxx`.
pub fn generate_recovery_codes() -> Result<Vec<String>, RandomUnavailable> {
    (0..RECOVERY_CODE_COUNT)
        .map(|_| {
            let mut bytes = [0_u8; RECOVERY_CODE_CHARS];
            getrandom::fill(&mut bytes).map_err(|_| RandomUnavailable)?;
            let chars: Vec<char> = bytes
                .iter()
                .map(|b| char::from(RECOVERY_ALPHABET[usize::from(b & 31)]))
                .collect();
            Ok(chars
                .chunks(4)
                .map(|group| group.iter().collect::<String>())
                .collect::<Vec<_>>()
                .join("-"))
        })
        .collect()
}

/// Stored form of a recovery code: SHA-256 over the account id and the normalised code.
/// Binding to the account means a hash cannot be replayed against another account.
pub fn recovery_code_hash(user: UserId, normalised: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(user.as_uuid().as_bytes());
    hasher.update(normalised.as_bytes());
    hasher.finalize().into()
}

/// AAD for the encrypted TOTP secret: a ciphertext is only valid on its own account row.
pub fn secret_aad(user: UserId) -> Vec<u8> {
    kernel::crypto::record_aad("user.mfa_secret", &[user.as_uuid()])
}

#[cfg(test)]
mod tests {
    use super::*;

    // RFC 6238 Appendix B test secret ("12345678901234567890"), SHA-1.
    const RFC_SECRET: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    fn rfc() -> TotpSecret {
        TotpSecret::from_base32(RFC_SECRET.to_owned())
    }

    #[test]
    fn matches_rfc_6238_reference_vectors() {
        // Appendix B lists 8 digits; the 6-digit code is the last six.
        assert_eq!(rfc().code_at(59).as_deref(), Some("287082"));
        assert_eq!(rfc().code_at(1_111_111_109).as_deref(), Some("081804"));
        assert_eq!(rfc().code_at(2_000_000_000).as_deref(), Some("279037"));
    }

    #[test]
    fn accepts_one_step_of_skew_and_reports_the_step() {
        let s = rfc();
        let now = 1_111_111_109;
        let step = now / STEP_SECONDS;
        let code = s.code_at(now).unwrap_or_default();
        assert_eq!(s.matching_step(&code, now), Some(step));
        assert_eq!(s.matching_step(&code, now + STEP_SECONDS), Some(step));
        assert_eq!(s.matching_step(&code, now - STEP_SECONDS), Some(step));
        assert_eq!(s.matching_step(&code, now + 2 * STEP_SECONDS), None);
        assert_eq!(s.matching_step("000000x", now), None);
    }

    #[test]
    fn generated_secrets_are_160_bit_and_unique() -> Result<(), RandomUnavailable> {
        let (a, b) = (TotpSecret::generate()?, TotpSecret::generate()?);
        assert_ne!(a.base32(), b.base32());
        assert_eq!(a.base32().len(), 32); // 20 bytes → 32 base32 chars
        Ok(())
    }

    #[test]
    fn otpauth_uri_names_issuer_and_account() {
        let uri = rfc().otpauth_uri("0905 000 001").unwrap_or_default();
        assert!(uri.starts_with("otpauth://totp/"), "{uri}");
        assert!(uri.contains(&format!("secret={RFC_SECRET}")), "{uri}");
        assert!(uri.contains("issuer=Vi%C3%AAn"), "{uri}");
    }

    #[test]
    fn parses_totp_and_recovery_input() {
        assert_eq!(
            SecondFactor::parse(" 123 456 "),
            Some(SecondFactor::Totp("123456".into()))
        );
        assert_eq!(
            SecondFactor::parse("ABCD-efgh-2345-6789"),
            Some(SecondFactor::Recovery("abcdefgh23456789".into()))
        );
        assert_eq!(SecondFactor::parse("  "), None);
        assert_eq!(SecondFactor::parse(&"a".repeat(65)), None);
    }

    #[test]
    fn recovery_codes_are_unique_formatted_and_hash_per_account() -> Result<(), RandomUnavailable> {
        let codes = generate_recovery_codes()?;
        assert_eq!(codes.len(), RECOVERY_CODE_COUNT);
        let unique: std::collections::HashSet<_> = codes.iter().collect();
        assert_eq!(unique.len(), RECOVERY_CODE_COUNT);
        let first = codes.first().cloned().unwrap_or_default();
        assert_eq!(first.len(), 19);
        assert_eq!(first.matches('-').count(), 3);

        let Some(SecondFactor::Recovery(normalised)) = SecondFactor::parse(&first) else {
            panic!("recovery code must parse as recovery");
        };
        let (a, b) = (UserId::new(), UserId::new());
        assert_ne!(
            recovery_code_hash(a, &normalised),
            recovery_code_hash(b, &normalised)
        );
        Ok(())
    }
}
