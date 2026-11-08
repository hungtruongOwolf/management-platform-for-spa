//! The single error type of the API and its one mapping to RFC 9457 `application/problem+json`.
//! Error `code`s are stable: the frontend translates them into Vietnamese messages.

use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FieldError {
    pub field: String,
    pub code: &'static str,
}

impl FieldError {
    pub fn new(field: impl Into<String>, code: &'static str) -> Self {
        Self {
            field: field.into(),
            code,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("authentication required")]
    Unauthenticated,
    #[error("re-authentication required")]
    ReauthRequired,
    /// Wrong phone or password — deliberately indistinguishable from each other.
    #[error("invalid credentials")]
    InvalidCredentials,
    /// Password accepted, second factor still owed: the session may only call the MFA endpoints.
    #[error("second factor required")]
    MfaRequired,
    /// Like `MfaRequired`, for an account that must first enrol an authenticator.
    #[error("second factor enrollment required")]
    MfaEnrollmentRequired,
    /// Wrong, expired, or already-used TOTP / recovery code.
    #[error("invalid second factor")]
    InvalidMfaCode,
    #[error("forbidden: {code}")]
    Forbidden { code: &'static str },
    #[error("not found")]
    NotFound,
    #[error("validation failed")]
    Validation(Vec<FieldError>),
    /// A business rule rejected an otherwise well-formed request (422), e.g. `payment.amount_mismatch`.
    #[error("unprocessable: {code}")]
    Unprocessable { code: &'static str },
    /// The request conflicts with current state (409), e.g. `package.no_sessions_left`.
    #[error("conflict: {code}")]
    Conflict { code: &'static str },
    /// Conflict with an existing record the client may open instead (e.g. `customer.phone_taken`).
    #[error("conflict: {code}")]
    ConflictWith {
        code: &'static str,
        existing_id: uuid::Uuid,
    },
    /// Conflict whose blocking records the client lists for the user (409), e.g.
    /// `staff.has_upcoming_bookings` with the bookings still assigned. `details` is built by the
    /// service from data the caller is already entitled to; never internals.
    #[error("conflict: {code}")]
    ConflictDetails {
        code: &'static str,
        details: serde_json::Value,
    },
    #[error("stale version")]
    StaleVersion,
    #[error("bad request: {code}")]
    BadRequest { code: &'static str },
    #[error("rate limited")]
    RateLimited,
    #[error("database error")]
    Database(#[from] sqlx::Error),
    #[error("internal error: {0}")]
    Internal(String),
}

/// SQLSTATEs where Postgres refuses a client-supplied text value as such: U+0000 or a character
/// the database encoding cannot hold (`22021`, `22P05`). JSON can carry an escaped NUL, Postgres
/// `text` cannot store it; that is bad input (`422`), not a server fault.
const UNSTORABLE_TEXT: [&str; 2] = ["22021", "22P05"];

fn is_unstorable_text(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|e| e.code())
        .is_some_and(|code| UNSTORABLE_TEXT.contains(&code.as_ref()))
}

impl AppError {
    pub const fn forbidden() -> Self {
        Self::Forbidden {
            code: "auth.forbidden",
        }
    }

    pub fn status(&self) -> StatusCode {
        match self {
            Self::Unauthenticated
            | Self::ReauthRequired
            | Self::InvalidCredentials
            | Self::MfaRequired
            | Self::MfaEnrollmentRequired
            | Self::InvalidMfaCode => StatusCode::UNAUTHORIZED,
            Self::Forbidden { .. } => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Validation(_) | Self::Unprocessable { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Conflict { .. } | Self::ConflictWith { .. } | Self::ConflictDetails { .. } => {
                StatusCode::CONFLICT
            }
            Self::StaleVersion => StatusCode::PRECONDITION_FAILED,
            Self::BadRequest { .. } => StatusCode::BAD_REQUEST,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::Database(e) if is_unstorable_text(e) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Database(_) | Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::Unauthenticated => "auth.unauthenticated",
            Self::ReauthRequired => "auth.reauth_required",
            Self::InvalidCredentials => "auth.invalid_credentials",
            Self::MfaRequired => "auth.mfa_required",
            Self::MfaEnrollmentRequired => "auth.mfa_enrollment_required",
            Self::InvalidMfaCode => "auth.invalid_mfa_code",
            Self::Forbidden { code }
            | Self::Unprocessable { code }
            | Self::Conflict { code }
            | Self::ConflictWith { code, .. }
            | Self::ConflictDetails { code, .. }
            | Self::BadRequest { code } => code,
            Self::NotFound => "not_found",
            Self::Validation(_) => "validation_failed",
            Self::StaleVersion => "conflict.stale_version",
            Self::RateLimited => "rate_limited",
            Self::Database(e) if is_unstorable_text(e) => "validation_failed",
            Self::Database(_) | Self::Internal(_) => "internal_error",
        }
    }

    pub fn problem(&self) -> Problem {
        let status = self.status();
        Problem {
            r#type: "about:blank",
            title: status.canonical_reason().unwrap_or("Error"),
            status: status.as_u16(),
            code: self.code(),
            errors: match self {
                Self::Validation(errors) => Some(errors.clone()),
                // The database cannot say which field; `body` tells the client to check its text.
                Self::Database(e) if is_unstorable_text(e) => {
                    Some(vec![FieldError::new("body", "invalid_character")])
                }
                _ => None,
            },
            existing_id: match self {
                Self::ConflictWith { existing_id, .. } => Some(*existing_id),
                _ => None,
            },
            details: match self {
                Self::ConflictDetails { details, .. } => Some(details.clone()),
                _ => None,
            },
        }
    }
}

/// RFC 9457 body. Deliberately has no free-text `detail`: internals never reach the client.
#[derive(Debug, Clone, Serialize)]
pub struct Problem {
    pub r#type: &'static str,
    pub title: &'static str,
    pub status: u16,
    pub code: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<FieldError>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub existing_id: Option<uuid::Uuid>,
    /// The records behind a `ConflictDetails` refusal (shape documented per code in OpenAPI).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl Problem {
    /// Problem body for a bare status produced outside handlers (404 fallback, 405, 408, 413).
    pub fn for_status(status: StatusCode) -> Self {
        let code = match status {
            StatusCode::NOT_FOUND => "not_found",
            StatusCode::METHOD_NOT_ALLOWED => "method_not_allowed",
            StatusCode::REQUEST_TIMEOUT => "request_timeout",
            StatusCode::PAYLOAD_TOO_LARGE => "payload_too_large",
            StatusCode::TOO_MANY_REQUESTS => "rate_limited",
            s if s.is_server_error() => "internal_error",
            _ => "error",
        };
        Self {
            r#type: "about:blank",
            title: status.canonical_reason().unwrap_or("Error"),
            status: status.as_u16(),
            code,
            errors: None,
            existing_id: None,
            details: None,
        }
    }
}

pub const PROBLEM_JSON: &str = "application/problem+json";

impl IntoResponse for Problem {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let mut response = (status, axum::Json(self)).into_response();
        response
            .headers_mut()
            .insert(header::CONTENT_TYPE, HeaderValue::from_static(PROBLEM_JSON));
        response
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match &self {
            Self::Database(error) if is_unstorable_text(error) => {
                tracing::warn!(%error, "rejected unstorable text");
            }
            Self::Database(error) => tracing::error!(%error, "database error"),
            Self::Internal(message) => tracing::error!(%message, "internal error"),
            _ => {}
        }
        self.problem().into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_errors_hide_details() {
        let problem = AppError::Internal("connection string postgres://secret@db".into()).problem();
        let json = serde_json::to_string(&problem).unwrap_or_default();
        assert_eq!(problem.status, 500);
        assert_eq!(problem.code, "internal_error");
        assert!(!json.contains("secret"));
    }

    #[test]
    fn validation_lists_field_errors() {
        let problem =
            AppError::Validation(vec![FieldError::new("phone", "invalid_format")]).problem();
        assert_eq!(problem.status, 422);
        assert_eq!(problem.errors.as_deref().map(<[FieldError]>::len), Some(1));
    }

    #[test]
    fn response_uses_problem_json_content_type() {
        let response = AppError::NotFound.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some(PROBLEM_JSON)
        );
    }

    #[test]
    fn conflict_with_points_at_the_existing_record() {
        let id = uuid::Uuid::from_u128(7);
        let problem = AppError::ConflictWith {
            code: "customer.phone_taken",
            existing_id: id,
        }
        .problem();
        let json = serde_json::to_value(&problem).unwrap_or_default();
        assert_eq!(
            (json["status"].clone(), json["code"].clone()),
            (
                serde_json::json!(409),
                serde_json::json!("customer.phone_taken")
            )
        );
        assert_eq!(json["existing_id"], serde_json::json!(id.to_string()));
        assert!(
            serde_json::to_value(AppError::NotFound.problem())
                .unwrap_or_default()
                .get("existing_id")
                .is_none()
        );
    }

    #[test]
    fn conflict_details_list_the_blocking_records() {
        let problem = AppError::ConflictDetails {
            code: "staff.has_upcoming_bookings",
            details: serde_json::json!({"bookings": [{"date": "2026-11-09"}]}),
        }
        .problem();
        let json = serde_json::to_value(&problem).unwrap_or_default();
        assert_eq!(json["status"], serde_json::json!(409));
        assert_eq!(
            json["code"],
            serde_json::json!("staff.has_upcoming_bookings")
        );
        assert_eq!(json["details"]["bookings"][0]["date"], "2026-11-09");
        assert!(
            serde_json::to_value(AppError::NotFound.problem())
                .unwrap_or_default()
                .get("details")
                .is_none()
        );
    }

    #[test]
    fn business_codes_map_to_expected_statuses() {
        assert_eq!(
            AppError::Conflict {
                code: "package.no_sessions_left"
            }
            .status(),
            StatusCode::CONFLICT
        );
        assert_eq!(
            AppError::Unprocessable {
                code: "payment.amount_mismatch"
            }
            .status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(AppError::StaleVersion.code(), "conflict.stale_version");
        assert_eq!(AppError::forbidden().code(), "auth.forbidden");
        assert_eq!(
            AppError::InvalidCredentials.status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            AppError::InvalidCredentials.code(),
            "auth.invalid_credentials"
        );
    }
}
