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

/// Fingerprint of a request for idempotency: method, route template, the record the path names
/// (if any) and the canonical body. A key reused on another endpoint or record is therefore a
/// mismatch (422), never a replay of a different operation's response, even when the bodies
/// happen to be identical (e.g. `{"reason": …}` on cancel vs. void).
///
/// Each field is length-prefixed, so no two different inputs serialise to the same bytes.
pub fn hash_route(method: &str, route: &str, record: Option<Uuid>, body: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"vpyd.idempotency.v2");
    for part in [method.as_bytes(), route.as_bytes()] {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    match record {
        Some(id) => {
            hasher.update([1]);
            hasher.update(id.as_bytes());
        }
        None => hasher.update([0]),
    }
    hasher.update(body);
    hasher.finalize().into()
}

/// Body-only fingerprint, for callers that are not an HTTP route: operator tooling (`seed-demo`,
/// `import-paper-book`) and tests. Endpoints use [`hash_route`], which also binds method and route.
pub fn hash_request(body: &[u8]) -> [u8; 32] {
    Sha256::digest(body).into()
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
    fn route_hash_binds_method_route_and_record() {
        let id = Uuid::now_v7();
        let body = br#"{"reason":"x"}"#;
        let cancel = hash_route("POST", "/customer-packages/{id}/cancel", Some(id), body);
        assert_eq!(
            cancel,
            hash_route("POST", "/customer-packages/{id}/cancel", Some(id), body)
        );
        assert_ne!(
            cancel,
            hash_route("POST", "/customer-packages/{id}/extend", Some(id), body),
            "same body, other endpoint"
        );
        assert_ne!(
            cancel,
            hash_route("PUT", "/customer-packages/{id}/cancel", Some(id), body)
        );
        assert_ne!(
            cancel,
            hash_route(
                "POST",
                "/customer-packages/{id}/cancel",
                Some(Uuid::now_v7()),
                body
            )
        );
        assert_ne!(
            hash_route("POST", "/sales", None, body),
            hash_route("POST", "/visits", None, body)
        );
        // Length prefixes: moving bytes between fields never collides.
        assert_ne!(
            hash_route("POST", "/a", None, b"b"),
            hash_route("POST", "/ab", None, b"")
        );
        assert_ne!(
            hash_route("POS", "T/a", None, b""),
            hash_route("POST", "/a", None, b"")
        );
    }
}
