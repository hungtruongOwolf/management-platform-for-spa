//! Cross-cutting HTTP protections applied to every route (threat model TB1).

use axum::Router;
use axum::extract::{Request, State};
use axum::http::{Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use kernel::AppError;
use kernel::error::{PROBLEM_JSON, Problem};
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceBuilder;
use tower_governor::GovernorLayer;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::SmartIpKeyExtractor;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

#[derive(Debug, Clone)]
pub struct HttpPolicy {
    pub public_origin: String,
    pub request_timeout: Duration,
    pub max_body_bytes: usize,
    /// Sustained requests per second per client IP, and the burst allowed on top.
    pub rate_per_second: u64,
    pub rate_burst: u32,
}

impl HttpPolicy {
    pub fn standard(public_origin: String) -> Self {
        Self {
            public_origin,
            request_timeout: Duration::from_secs(10),
            max_body_bytes: 64 * 1024,
            rate_per_second: 5,
            rate_burst: 60,
        }
    }
}

/// Wraps `router` with request ids, tracing, problem+json for bare errors, timeout,
/// per-IP rate limiting, a body size cap and the CSRF Origin check (outermost first).
pub fn apply(router: Router, policy: &HttpPolicy) -> Router {
    let governor = GovernorConfigBuilder::default()
        .per_second(policy.rate_per_second)
        .burst_size(policy.rate_burst)
        .key_extractor(SmartIpKeyExtractor)
        .finish()
        .map(Arc::new);
    let origin = Arc::new(policy.public_origin.clone());

    let router = router.layer(middleware::from_fn_with_state(origin, check_origin));
    let router = match governor {
        Some(config) => router.layer(GovernorLayer::new(config)),
        None => {
            tracing::error!("invalid rate limit policy; rate limiting disabled");
            router
        }
    };

    router.layer(
        ServiceBuilder::new()
            .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
            .layer(PropagateRequestIdLayer::x_request_id())
            .layer(TraceLayer::new_for_http())
            .layer(middleware::from_fn(problem_for_bare_errors))
            .layer(TimeoutLayer::with_status_code(
                StatusCode::REQUEST_TIMEOUT,
                policy.request_timeout,
            ))
            .layer(RequestBodyLimitLayer::new(policy.max_body_bytes)),
    )
}

/// State-changing requests must come from our own SPA origin (CSRF defence with SameSite cookies).
async fn check_origin(
    State(allowed): State<Arc<String>>,
    request: Request,
    next: Next,
) -> Response {
    let safe = matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    );
    if safe {
        return next.run(request).await;
    }
    let origin_ok = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|origin| origin == allowed.as_str());
    if origin_ok {
        next.run(request).await
    } else {
        AppError::Forbidden {
            code: "auth.bad_origin",
        }
        .into_response()
    }
}

/// Turns framework-generated errors (404 fallback, 405, 408, 413, 429, …) into problem+json,
/// so every error the client sees has the same shape and a stable `code`.
async fn problem_for_bare_errors(request: Request, next: Next) -> Response {
    let response = next.run(request).await;
    let status = response.status();
    let is_problem = response
        .headers()
        .get(header::CONTENT_TYPE)
        .is_some_and(|v| v.as_bytes().starts_with(PROBLEM_JSON.as_bytes()));
    if (status.is_client_error() || status.is_server_error()) && !is_problem {
        let mut replaced = Problem::for_status(status).into_response();
        if let Some(retry) = response.headers().get(header::RETRY_AFTER) {
            replaced
                .headers_mut()
                .insert(header::RETRY_AFTER, retry.clone());
        }
        return replaced;
    }
    response
}
