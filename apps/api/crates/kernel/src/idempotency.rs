//! Idempotent mutations. A use case first `claim`s its key inside its `TenantTx`:
//!
//! - fresh key → runs, then `complete` stores the response in the same transaction;
//! - retried key with the same body → the stored response is replayed (nothing runs twice);
//! - same key with a different body → `422 idempotency.key_reuse_mismatch`.
//!
//! A concurrent duplicate blocks on the primary key until the first transaction finishes, so two
//! simultaneous submits can never both execute. If the first rolls back, the retry runs fresh.

use crate::db::TenantTx;
use crate::error::AppError;
use crate::permission::Actor;
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub enum Claim {
    Fresh,
    Replay { status: u16, body: Value },
}

pub fn hash_request(body: &[u8]) -> [u8; 32] {
    Sha256::digest(body).into()
}

/// Hash of a request acting on one record (`POST /visits/{id}/void`, …): the record id is part
/// of it, so a key reused on another record is a mismatch (422), never a replay of the first.
pub fn hash_request_on(record: Uuid, body: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(record.as_bytes());
    hasher.update(body);
    hasher.finalize().into()
}

pub async fn claim(
    tx: &mut TenantTx,
    actor: &Actor,
    key: Uuid,
    request_hash: [u8; 32],
) -> Result<Claim, AppError> {
    let tenant = tx.tenant().as_uuid();
    let inserted = sqlx::query_scalar!(
        r#"INSERT INTO app.idempotency_record (tenant_id, actor_user_id, key, request_hash)
           VALUES ($1, $2, $3, $4)
           ON CONFLICT DO NOTHING
           RETURNING key"#,
        tenant,
        actor.user_id.as_uuid(),
        key,
        &request_hash[..],
    )
    .fetch_optional(tx.conn())
    .await?;
    if inserted.is_some() {
        return Ok(Claim::Fresh);
    }

    let existing = sqlx::query!(
        r#"SELECT request_hash, response_status, response_body
           FROM app.idempotency_record
           WHERE tenant_id = $1 AND actor_user_id = $2 AND key = $3"#,
        tenant,
        actor.user_id.as_uuid(),
        key,
    )
    .fetch_one(tx.conn())
    .await?;

    if existing.request_hash != request_hash {
        return Err(AppError::Unprocessable {
            code: "idempotency.key_reuse_mismatch",
        });
    }
    match (existing.response_status, existing.response_body) {
        (Some(status), Some(body)) => Ok(Claim::Replay {
            status: u16::try_from(status).unwrap_or(200),
            body,
        }),
        // Committed without a stored response: only possible if a use case forgot `complete`.
        _ => Err(AppError::Conflict {
            code: "idempotency.in_progress",
        }),
    }
}

pub async fn complete(
    tx: &mut TenantTx,
    actor: &Actor,
    key: Uuid,
    status: u16,
    body: &Value,
) -> Result<(), AppError> {
    let status =
        i16::try_from(status).map_err(|_| AppError::Internal("status out of range".into()))?;
    let updated = sqlx::query!(
        r#"UPDATE app.idempotency_record
           SET response_status = $4, response_body = $5
           WHERE tenant_id = $1 AND actor_user_id = $2 AND key = $3"#,
        tx.tenant().as_uuid(),
        actor.user_id.as_uuid(),
        key,
        status,
        body,
    )
    .execute(tx.conn())
    .await?;
    if updated.rows_affected() == 1 {
        Ok(())
    } else {
        Err(AppError::Internal(
            "idempotency record missing on complete".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_stable_and_body_sensitive() {
        assert_eq!(hash_request(b"{\"a\":1}"), hash_request(b"{\"a\":1}"));
        assert_ne!(hash_request(b"{\"a\":1}"), hash_request(b"{\"a\":2}"));
    }

    #[test]
    fn record_hash_binds_the_record() {
        let (a, b) = (Uuid::now_v7(), Uuid::now_v7());
        assert_eq!(hash_request_on(a, b"{}"), hash_request_on(a, b"{}"));
        assert_ne!(hash_request_on(a, b"{}"), hash_request_on(b, b"{}"));
    }
}
