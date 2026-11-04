//! Liveness endpoint used by the SPA smoke check, Caddy, and uptime monitoring.

use axum::{Json, Router, routing::get};
use serde::Serialize;

#[derive(Serialize)]
struct Health {
    status: &'static str,
    version: &'static str,
}

pub(crate) fn routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new().route("/health", get(health))
}

async fn health() -> Json<Health> {
    Json(Health {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}
