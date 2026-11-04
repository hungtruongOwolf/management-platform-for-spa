//! Opaque session tokens: 256 random bits for the cookie, SHA-256 of it in the database.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("system random source unavailable")]
pub struct RandomUnavailable;

/// A freshly minted token: `secret` goes into the cookie, `hash` into `app.session.token_hash`.
pub struct SessionToken {
    pub secret: String,
    pub hash: [u8; 32],
}

pub fn generate() -> Result<SessionToken, RandomUnavailable> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| RandomUnavailable)?;
    let secret = URL_SAFE_NO_PAD.encode(bytes);
    let hash = hash_secret(&secret);
    Ok(SessionToken { secret, hash })
}

pub fn hash_secret(secret: &str) -> [u8; 32] {
    Sha256::digest(secret.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_unique_url_safe_and_hash_consistently() -> Result<(), RandomUnavailable> {
        let (a, b) = (generate()?, generate()?);
        assert_ne!(a.secret, b.secret);
        assert_eq!(a.secret.len(), 43); // 32 bytes, base64url without padding
        assert!(
            a.secret
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
        assert_eq!(hash_secret(&a.secret), a.hash);
        Ok(())
    }
}
