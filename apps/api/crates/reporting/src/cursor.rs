//! Opaque keyset cursors: the sort key of the last row returned, `|`-joined and base64url-encoded.
//! The client only passes it back; a tampered cursor is a `422 cursor invalid`, never a 500, and
//! can only move the window inside the caller's own tenant (every query filters by tenant).

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, NaiveDate, Utc};
use kernel::{AppError, FieldError};
use uuid::Uuid;

/// Default and maximum page size for list endpoints (backend.md §1).
pub const DEFAULT_LIMIT: u32 = 20;
pub const MAX_LIMIT: u32 = 100;

pub fn limit(requested: Option<u32>) -> Result<i64, AppError> {
    match requested.unwrap_or(DEFAULT_LIMIT) {
        n @ 1..=MAX_LIMIT => Ok(i64::from(n)),
        _ => Err(AppError::Validation(vec![FieldError::new(
            "limit",
            "out_of_range",
        )])),
    }
}

pub fn encode(parts: &[String]) -> String {
    URL_SAFE_NO_PAD.encode(parts.join("|"))
}

fn invalid() -> AppError {
    AppError::Validation(vec![FieldError::new("cursor", "invalid")])
}

/// The `n` parts of a cursor, or `422 cursor invalid`.
pub fn decode(raw: &str, n: usize) -> Result<Vec<String>, AppError> {
    if raw.len() > 512 {
        return Err(invalid());
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(raw.trim())
        .map_err(|_| invalid())?;
    let text = String::from_utf8(bytes).map_err(|_| invalid())?;
    let parts: Vec<String> = text.split('|').map(str::to_owned).collect();
    if parts.len() == n {
        Ok(parts)
    } else {
        Err(invalid())
    }
}

pub fn uuid(part: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(part).map_err(|_| invalid())
}

pub fn date(part: &str) -> Result<NaiveDate, AppError> {
    part.parse().map_err(|_| invalid())
}

pub fn instant(part: &str) -> Result<DateTime<Utc>, AppError> {
    DateTime::parse_from_rfc3339(part)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|_| invalid())
}

pub fn integer(part: &str) -> Result<i32, AppError> {
    part.parse().map_err(|_| invalid())
}

/// Instants round-trip at full precision (Postgres keeps microseconds).
pub fn instant_part(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_rejects_tampering() -> Result<(), AppError> {
        let id = Uuid::now_v7();
        let at = Utc::now();
        let raw = encode(&[instant_part(at), id.to_string()]);
        let parts = decode(&raw, 2)?;
        assert_eq!(instant(&parts[0])?, at);
        assert_eq!(uuid(&parts[1])?, id);
        assert!(decode(&raw, 3).is_err(), "wrong arity");
        assert!(decode("%%%", 2).is_err(), "not base64");
        assert!(decode(&encode(&["x".into(), "y".into()]), 2).is_ok());
        assert!(uuid("y").is_err());
        assert!(decode(&"A".repeat(600), 1).is_err(), "bounded");
        Ok(())
    }

    #[test]
    fn page_size_is_bounded() {
        assert_eq!(limit(None).ok(), Some(20));
        assert_eq!(limit(Some(100)).ok(), Some(100));
        assert!(limit(Some(0)).is_err());
        assert!(limit(Some(101)).is_err());
    }
}
