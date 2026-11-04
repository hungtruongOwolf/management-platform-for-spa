use anyhow::Context;
use app::config::{Config, Environment};
use app::middleware::HttpPolicy;
use app::{AppState, jobs};
use secrecy::ExposeSecret;
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use std::time::Duration;
use tracing_subscriber::EnvFilter;

/// `app` serves the API; `app migrate` applies migrations as the schema owner (DATABASE_URL).
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Local development convenience: load .env if present. Real deployments set variables in the environment.
    let _ = dotenvy::dotenv();
    match std::env::args().nth(1).as_deref() {
        None | Some("serve") => serve().await,
        Some("migrate") => migrate().await,
        Some(other) => anyhow::bail!("unknown command {other:?} (expected: serve | migrate)"),
    }
}

async fn serve() -> anyhow::Result<()> {
    let config = Config::from_env().context("invalid configuration")?;
    init_tracing(config.environment);

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(3))
        .connect(config.app_database_url.expose_secret())
        .await
        .context("cannot connect to the database as app_rw")?;
    jobs::spawn_idempotency_purge(pool.clone());

    let policy = HttpPolicy::standard(config.public_origin.clone());
    let app = app::router(AppState { pool }, &policy);
    let listener = tokio::net::TcpListener::bind(config.bind_addr)
        .await
        .with_context(|| format!("cannot bind {}", config.bind_addr))?;
    tracing::info!(addr = %config.bind_addr, env = ?config.environment, "api listening");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .context("server error")
}

async fn migrate() -> anyhow::Result<()> {
    init_tracing(Environment::Development);
    let url = std::env::var("DATABASE_URL")
        .context("DATABASE_URL (schema owner) is required for migrate")?;
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .context("cannot connect as schema owner")?;
    kernel::MIGRATOR
        .run(&pool)
        .await
        .context("migration failed")?;
    tracing::info!("migrations applied");
    Ok(())
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
