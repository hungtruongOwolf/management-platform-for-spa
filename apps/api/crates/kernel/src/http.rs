//! HTTP extractors shared by every context's routes.

use crate::error::AppError;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use uuid::Uuid;

pub const IDEMPOTENCY_KEY_HEADER: &str = "idempotency-key";

/// Required `Idempotency-Key: <uuid>` header on mutations that create money movements or consume sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdempotencyKey(pub Uuid);

impl<S: Send + Sync> FromRequestParts<S> for IdempotencyKey {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let raw = parts
            .headers
            .get(IDEMPOTENCY_KEY_HEADER)
            .ok_or(AppError::BadRequest {
                code: "idempotency.key_required",
            })?;
        raw.to_str()
            .ok()
            .and_then(|s| Uuid::parse_str(s.trim()).ok())
            .map(IdempotencyKey)
            .ok_or(AppError::BadRequest {
                code: "idempotency.key_invalid",
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;

    async fn extract(header: Option<&str>) -> Result<IdempotencyKey, AppError> {
        let mut builder = Request::post("/");
        if let Some(value) = header {
            builder = builder.header(IDEMPOTENCY_KEY_HEADER, value);
        }
        let (mut parts, ()) = builder
            .body(())
            .map_err(|e| AppError::Internal(e.to_string()))?
            .into_parts();
        IdempotencyKey::from_request_parts(&mut parts, &()).await
    }

    #[tokio::test]
    async fn requires_a_uuid_header() {
        assert!(matches!(
            extract(None).await,
            Err(AppError::BadRequest {
                code: "idempotency.key_required"
            })
        ));
        assert!(matches!(
            extract(Some("abc")).await,
            Err(AppError::BadRequest {
                code: "idempotency.key_invalid"
            })
        ));
        let key = Uuid::now_v7();
        assert_eq!(
            extract(Some(&key.to_string())).await.ok(),
            Some(IdempotencyKey(key))
        );
    }
}
