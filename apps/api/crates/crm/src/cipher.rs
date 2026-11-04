//! Field-level encryption for sensitive notes (data model: `*_ct bytea`). AES-256-GCM with a fresh
//! random nonce per write. Associated data binds a ciphertext to its tenant + record, so a ciphertext
//! copied onto another customer (or spa) fails to decrypt instead of silently leaking.
//! Layout: `b'v'`, key version, 12-byte nonce, ciphertext + tag.

use aes_gcm::aead::{Aead, Generate, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

const KEY_VERSION: u8 = 1;
const NONCE_LEN: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CipherError {
    #[error("data key must be 32 bytes, base64-encoded")]
    BadKey,
    #[error("encryption failed")]
    Encrypt,
    #[error("ciphertext is corrupt, from another record, or from an unknown key version")]
    Decrypt,
}

pub struct FieldCipher {
    cipher: Aes256Gcm,
}

impl std::fmt::Debug for FieldCipher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FieldCipher([redacted])")
    }
}

impl FieldCipher {
    /// From the base64 value of `APP_DATA_KEY` (generate with `openssl rand -base64 32`).
    pub fn from_base64(encoded: &str) -> Result<Self, CipherError> {
        let bytes = STANDARD
            .decode(encoded.trim())
            .map_err(|_| CipherError::BadKey)?;
        let key: [u8; 32] = bytes.try_into().map_err(|_| CipherError::BadKey)?;
        Ok(Self {
            cipher: Aes256Gcm::new(&Key::<Aes256Gcm>::from(key)),
        })
    }

    pub fn encrypt(&self, plaintext: &str, aad: &[u8]) -> Result<Vec<u8>, CipherError> {
        let nonce = Nonce::generate();
        let ciphertext = self
            .cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: plaintext.as_bytes(),
                    aad,
                },
            )
            .map_err(|_| CipherError::Encrypt)?;
        let mut out = Vec::with_capacity(2 + NONCE_LEN + ciphertext.len());
        out.extend_from_slice(&[b'v', KEY_VERSION]);
        out.extend_from_slice(nonce.as_slice());
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    pub fn decrypt(&self, blob: &[u8], aad: &[u8]) -> Result<String, CipherError> {
        let (header, rest) = blob.split_at_checked(2).ok_or(CipherError::Decrypt)?;
        if header != [b'v', KEY_VERSION] {
            return Err(CipherError::Decrypt);
        }
        let (nonce, ciphertext) = rest
            .split_at_checked(NONCE_LEN)
            .ok_or(CipherError::Decrypt)?;
        let nonce = Nonce::try_from(nonce).map_err(|_| CipherError::Decrypt)?;
        let plaintext = self
            .cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: ciphertext,
                    aad,
                },
            )
            .map_err(|_| CipherError::Decrypt)?;
        String::from_utf8(plaintext).map_err(|_| CipherError::Decrypt)
    }
}

/// Associated data for a customer's health note: tenant + customer id.
pub fn customer_aad(tenant: uuid::Uuid, customer: uuid::Uuid) -> Vec<u8> {
    let mut aad = Vec::with_capacity(32 + 16);
    aad.extend_from_slice(b"customer.health_note:");
    aad.extend_from_slice(tenant.as_bytes());
    aad.extend_from_slice(customer.as_bytes());
    aad
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY="; // 32 ASCII bytes, test only

    fn cipher() -> Result<FieldCipher, CipherError> {
        FieldCipher::from_base64(KEY)
    }

    #[test]
    fn round_trips_and_never_stores_plaintext() -> Result<(), CipherError> {
        let c = cipher()?;
        let aad = customer_aad(uuid::Uuid::nil(), uuid::Uuid::from_u128(1));
        let blob = c.encrypt("Huyết áp cao — tránh đá nóng", &aad)?;
        assert!(!String::from_utf8_lossy(&blob).contains("Huyết"));
        assert_eq!(c.decrypt(&blob, &aad)?, "Huyết áp cao — tránh đá nóng");
        Ok(())
    }

    #[test]
    fn same_text_encrypts_differently_each_time() -> Result<(), CipherError> {
        let c = cipher()?;
        assert_ne!(c.encrypt("x", b"a")?, c.encrypt("x", b"a")?);
        Ok(())
    }

    #[test]
    fn ciphertext_moved_to_another_customer_does_not_decrypt() -> Result<(), CipherError> {
        let c = cipher()?;
        let blob = c.encrypt(
            "ghi chú",
            &customer_aad(uuid::Uuid::nil(), uuid::Uuid::from_u128(1)),
        )?;
        let other = customer_aad(uuid::Uuid::nil(), uuid::Uuid::from_u128(2));
        assert_eq!(c.decrypt(&blob, &other), Err(CipherError::Decrypt));
        Ok(())
    }

    #[test]
    fn tampering_and_bad_keys_are_rejected() -> Result<(), CipherError> {
        let c = cipher()?;
        let mut blob = c.encrypt("ghi chú", b"a")?;
        if let Some(last) = blob.last_mut() {
            *last ^= 1;
        }
        assert_eq!(c.decrypt(&blob, b"a"), Err(CipherError::Decrypt));
        assert_eq!(c.decrypt(b"v", b"a"), Err(CipherError::Decrypt));
        assert!(FieldCipher::from_base64("too-short").is_err());
        assert_eq!(format!("{c:?}"), "FieldCipher([redacted])");
        Ok(())
    }
}
