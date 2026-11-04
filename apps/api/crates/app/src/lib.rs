//! HTTP application wiring: state, router, configuration, cross-cutting middleware and jobs.

pub mod config;
mod health;
pub mod jobs;
pub mod middleware;
pub mod openapi;

use axum::Router;
use middleware::HttpPolicy;
use sqlx::PgPool;

/// Shared state handed to every context's routes.
#[derive(Clone)]
pub struct AppState {
    /// Pool connected as `app_rw`; every use case opens a `kernel::TenantTx` from it.
    pub pool: PgPool,
}

impl axum::extract::FromRef<AppState> for PgPool {
    fn from_ref(state: &AppState) -> Self {
        state.pool.clone()
    }
}

/// Builds the full HTTP router with every protection applied. Kept separate from `main` so tests drive it in-process.
pub fn router(state: AppState, policy: &HttpPolicy) -> Router {
    let (auth, me) = identity::http::routes::<AppState>();
    let api = Router::<AppState>::new()
        .merge(health::routes())
        .merge(middleware::with_auth_rate_limit(auth))
        .merge(me)
        .merge(catalog::http::routes())
        .with_state(state);
    middleware::apply(Router::new().nest("/api/v1", api), policy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use axum::http::{Request, StatusCode, header};
    use axum::routing::post;
    use http_body_util::BodyExt;
    use std::net::SocketAddr;
    use tower::ServiceExt;

    const ORIGIN: &str = "http://localhost:5173";

    fn lazy_state() -> AppState {
        // Never connects: the routes under test do not touch the database.
        let pool = PgPool::connect_lazy("postgres://app_rw:x@127.0.0.1:1/none")
            .unwrap_or_else(|e| panic!("{e}"));
        AppState { pool }
    }

    fn policy() -> HttpPolicy {
        HttpPolicy::standard(ORIGIN.to_owned())
    }

    /// Router with an extra POST echo route, to exercise protections on state-changing requests.
    fn router_with_echo(policy: &HttpPolicy) -> Router {
        let echo = Router::new().route(
            "/echo",
            post(|body: String| async move { body.len().to_string() }),
        );
        middleware::apply(Router::new().nest("/api/v1", echo), policy)
    }

    fn with_peer(mut request: Request<Body>) -> Request<Body> {
        request
            .extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40_000))));
        request
    }

    async fn send(
        router: Router,
        request: Request<Body>,
    ) -> (StatusCode, axum::http::HeaderMap, serde_json::Value) {
        let response = router
            .oneshot(with_peer(request))
            .await
            .unwrap_or_else(|never| match never {});
        let (status, headers) = (response.status(), response.headers().clone());
        let bytes = response
            .into_body()
            .collect()
            .await
            .map(|b| b.to_bytes())
            .unwrap_or_default();
        (
            status,
            headers,
            serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
        )
    }

    fn get(path: &str) -> Request<Body> {
        Request::get(path).body(Body::empty()).unwrap_or_default()
    }

    #[tokio::test]
    async fn health_reports_ok_with_version_and_request_id() {
        let (status, headers, body) =
            send(router(lazy_state(), &policy()), get("/api/v1/health")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ok");
        assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
        assert!(headers.contains_key("x-request-id"));
    }

    #[tokio::test]
    async fn unknown_route_is_a_problem_json_404() {
        let (status, headers, body) = send(
            router(lazy_state(), &policy()),
            get("/api/v1/does-not-exist"),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(
            headers
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("application/problem+json")
        );
        assert_eq!(body["code"], "not_found");
    }

    fn post_echo(origin: Option<&str>, body: String) -> Request<Body> {
        let mut builder = Request::post("/api/v1/echo").header(header::CONTENT_TYPE, "text/plain");
        if let Some(o) = origin {
            builder = builder.header(header::ORIGIN, o);
        }
        builder.body(Body::from(body)).unwrap_or_default()
    }

    #[tokio::test]
    async fn state_changing_requests_require_our_origin() {
        let p = policy();
        let (status, _, body) = send(
            router_with_echo(&p),
            post_echo(Some("https://evil.example"), "x".into()),
        )
        .await;
        assert_eq!(
            (status, body["code"].as_str()),
            (StatusCode::FORBIDDEN, Some("auth.bad_origin"))
        );

        let (status, _, _) = send(router_with_echo(&p), post_echo(None, "x".into())).await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        let (status, _, _) = send(router_with_echo(&p), post_echo(Some(ORIGIN), "x".into())).await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn oversized_bodies_are_rejected_with_problem_json() {
        let p = policy();
        let big = "a".repeat(p.max_body_bytes + 1);
        let (status, _, body) = send(router_with_echo(&p), post_echo(Some(ORIGIN), big)).await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(body["code"], "payload_too_large");
    }

    #[tokio::test]
    async fn bursts_beyond_the_rate_limit_get_429() {
        let mut p = policy();
        p.rate_per_second = 1;
        p.rate_burst = 2;
        let app = router(lazy_state(), &p);
        let mut statuses = Vec::new();
        for _ in 0..4 {
            let (status, _, _) = send(app.clone(), get("/api/v1/health")).await;
            statuses.push(status);
        }
        assert_eq!(statuses[..2], [StatusCode::OK, StatusCode::OK]);
        assert!(
            statuses.contains(&StatusCode::TOO_MANY_REQUESTS),
            "{statuses:?}"
        );
    }
}
