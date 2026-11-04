//! HTTP extractors shared by every context's routes.

use crate::error::AppError;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use uuid::Uuid;

pub const IDEMPOTENCY_KEY_HEADER: &str = "idempotency-key";

/// Required `If-Match: "<version>"` on updates of versioned records (optimistic locking).
/// A stale version yields `412 conflict.stale_version` from the use case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IfMatch(pub i64);

impl<S: Send + Sync> FromRequestParts<S> for IfMatch {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let raw = parts
            .headers
            .get(axum::http::header::IF_MATCH)
            .ok_or(AppError::BadRequest {
                code: "precondition.if_match_required",
            })?;
        raw.to_str()
            .ok()
            .map(|s| s.trim().trim_start_matches("W/").trim_matches('"'))
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|v| *v > 0)
            .map(IfMatch)
            .ok_or(AppError::BadRequest {
                code: "precondition.if_match_invalid",
            })
    }
}

/// `ETag` header value for a record version.
pub fn etag(version: i64) -> axum::http::HeaderValue {
    axum::http::HeaderValue::from_str(&format!("\"{version}\""))
        .unwrap_or_else(|_| axum::http::HeaderValue::from_static("\"0\""))
}

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

    async fn if_match(header: Option<&str>) -> Result<IfMatch, AppError> {
        let mut builder = Request::put("/");
        if let Some(value) = header {
            builder = builder.header(axum::http::header::IF_MATCH, value);
        }
        let (mut parts, ()) = builder
            .body(())
            .map_err(|e| AppError::Internal(e.to_string()))?
            .into_parts();
        IfMatch::from_request_parts(&mut parts, &()).await
    }

    #[tokio::test]
    async fn if_match_accepts_quoted_and_weak_versions() {
        assert_eq!(if_match(Some("\"3\"")).await.ok(), Some(IfMatch(3)));
        assert_eq!(if_match(Some("W/\"3\"")).await.ok(), Some(IfMatch(3)));
        assert_eq!(if_match(Some("7")).await.ok(), Some(IfMatch(7)));
        assert!(matches!(
            if_match(None).await,
            Err(AppError::BadRequest {
                code: "precondition.if_match_required"
            })
        ));
        for bad in ["*", "\"abc\"", "0", "-1"] {
            assert!(
                matches!(
                    if_match(Some(bad)).await,
                    Err(AppError::BadRequest {
                        code: "precondition.if_match_invalid"
                    })
                ),
                "{bad}"
            );
        }
        assert_eq!(etag(4).to_str().ok(), Some("\"4\""));
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
