use anyhow::Context;
use app::config::{Config, Environment};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Local development convenience: load .env if present. Real deployments set variables in the environment.
    let _ = dotenvy::dotenv();
    let config = Config::from_env().context("invalid configuration")?;
    init_tracing(config.environment);

    let app = app::router().layer(TraceLayer::new_for_http());
    let listener = tokio::net::TcpListener::bind(config.bind_addr)
        .await
        .with_context(|| format!("cannot bind {}", config.bind_addr))?;
    tracing::info!(addr = %config.bind_addr, env = ?config.environment, "api listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server error")
}

/// Human-readable logs in development, JSON everywhere else (for log shipping).
fn init_tracing(environment: Environment) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    if environment == Environment::Development {
        builder.init();
    } else {
        builder.json().init();
    }
}

async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::error!(%error, "failed to listen for shutdown signal");
    }
}
