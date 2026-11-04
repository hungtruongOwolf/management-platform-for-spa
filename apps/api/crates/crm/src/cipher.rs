//! Field-level encryption for health notes. The cipher itself lives in `kernel::crypto` (single
//! source of truth, also used for MFA secrets); this module only names the CRM associated data.

pub use kernel::crypto::{CipherError, FieldCipher};

/// Associated data for a customer's health note: tenant + customer id. A ciphertext copied onto
/// another customer (or spa) fails to decrypt instead of silently leaking.
pub fn customer_aad(tenant: uuid::Uuid, customer: uuid::Uuid) -> Vec<u8> {
    kernel::crypto::record_aad("customer.health_note", &[tenant, customer])
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY="; // 32 ASCII bytes, test only

    #[test]
    fn aad_layout_is_unchanged_so_existing_notes_still_decrypt() {
        let (tenant, customer) = (uuid::Uuid::from_u128(7), uuid::Uuid::from_u128(9));
        let mut expected = b"customer.health_note:".to_vec();
        expected.extend_from_slice(tenant.as_bytes());
        expected.extend_from_slice(customer.as_bytes());
        assert_eq!(customer_aad(tenant, customer), expected);
    }

    #[test]
    fn note_moved_to_another_customer_does_not_decrypt() -> Result<(), CipherError> {
        let c = FieldCipher::from_base64(KEY)?;
        let blob = c.encrypt(
            "Huyết áp cao — tránh đá nóng",
            &customer_aad(uuid::Uuid::nil(), uuid::Uuid::from_u128(1)),
        )?;
        let other = customer_aad(uuid::Uuid::nil(), uuid::Uuid::from_u128(2));
        assert_eq!(c.decrypt(&blob, &other), Err(CipherError::Decrypt));
        Ok(())
    }
}
