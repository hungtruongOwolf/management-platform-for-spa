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

impl AppError {
    pub const fn forbidden() -> Self {
        Self::Forbidden {
            code: "auth.forbidden",
        }
    }

    pub fn status(&self) -> StatusCode {
        match self {
            Self::Unauthenticated | Self::ReauthRequired | Self::InvalidCredentials => {
                StatusCode::UNAUTHORIZED
            }
            Self::Forbidden { .. } => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Validation(_) | Self::Unprocessable { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Conflict { .. } => StatusCode::CONFLICT,
            Self::StaleVersion => StatusCode::PRECONDITION_FAILED,
            Self::BadRequest { .. } => StatusCode::BAD_REQUEST,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::Database(_) | Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::Unauthenticated => "auth.unauthenticated",
            Self::ReauthRequired => "auth.reauth_required",
            Self::InvalidCredentials => "auth.invalid_credentials",
            Self::Forbidden { code }
            | Self::Unprocessable { code }
            | Self::Conflict { code }
            | Self::BadRequest { code } => code,
            Self::NotFound => "not_found",
            Self::Validation(_) => "validation_failed",
            Self::StaleVersion => "conflict.stale_version",
            Self::RateLimited => "rate_limited",
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
