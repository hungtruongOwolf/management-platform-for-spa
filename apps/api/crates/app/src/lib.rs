//! HTTP application wiring: router, configuration, and cross-cutting middleware.

pub mod config;
mod health;

use axum::Router;

/// Builds the full HTTP router. Kept separate from `main` so tests drive it in-process.
pub fn router() -> Router {
    Router::new().nest("/api/v1", Router::new().merge(health::routes()))
}

#[cfg(test)]
mod tests {
    use super::router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    async fn get(path: &str) -> (StatusCode, serde_json::Value) {
        let response = router()
            .oneshot(Request::get(path).body(Body::empty()).unwrap_or_default())
            .await
            .unwrap_or_else(|never| match never {});
        let status = response.status();
        let bytes = response
            .into_body()
            .collect()
            .await
            .map(|b| b.to_bytes())
            .unwrap_or_default();
        let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, json)
    }

    #[tokio::test]
    async fn health_reports_ok_with_version() {
        let (status, body) = get("/api/v1/health").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ok");
        assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
    }

    #[tokio::test]
    async fn unknown_route_is_not_found() {
        let (status, _) = get("/api/v1/does-not-exist").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}
