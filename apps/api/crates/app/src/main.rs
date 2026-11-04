use anyhow::Context;
use app::config::{Config, Environment};
use app::middleware::HttpPolicy;
use app::{AppState, jobs};
use secrecy::ExposeSecret;
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use std::time::Duration;
use tracing_subscriber::EnvFilter;

/// Commands:
///   `app` | `app serve`                serve the API as app_rw (APP_DATABASE_URL)
///   `app migrate`                      apply migrations as the schema owner (DATABASE_URL)
///   `app create-tenant --slug S --name N [--branch B]`
///   `app seed-catalog --tenant S`     load the printed menu into a spa with no catalog yet
///   `app openapi`                      print the OpenAPI contract (written to apps/api/openapi.json)
///   `app add-user --tenant S --phone P --name N --role owner|manager|technician`
///                                      (password from env APP_NEW_USER_PASSWORD, never argv)
///   `app add-staff --tenant S --name N --position P`   closer / technician for the pickers
///   `app reset-mfa --phone P`          lost authenticator: clear TOTP + recovery codes, end
///                                      sessions; next login re-enrolls (verify identity first)
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Local development convenience: load .env if present. Real deployments set variables in the environment.
    let _ = dotenvy::dotenv();
    match std::env::args().nth(1).as_deref() {
        None | Some("serve") => serve().await,
        Some("migrate") => migrate().await,
        Some("create-tenant") => create_tenant().await,
        Some("add-user") => add_user().await,
        Some("add-staff") => add_staff().await,
        Some("seed-catalog") => seed_catalog().await,
        Some("reset-mfa") => reset_mfa().await,
        Some("openapi") => {
            print!("{}", app::openapi::json()?);
            Ok(())
        }
        Some(other) => anyhow::bail!(
            "unknown command {other:?} (expected: serve | migrate | create-tenant | add-user | add-staff | seed-catalog | reset-mfa | openapi)"
        ),
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
    let cipher = kernel::crypto::FieldCipher::from_base64(config.data_key.expose_secret())
        .map_err(|e| anyhow::anyhow!("APP_DATA_KEY: {e}"))?;
    let app = app::router(
        AppState {
            pool,
            cipher: std::sync::Arc::new(cipher),
        },
        &policy,
    );
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

async fn owner_pool() -> anyhow::Result<sqlx::PgPool> {
    let url = std::env::var("DATABASE_URL").context("DATABASE_URL (schema owner) is required")?;
    PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .context("cannot connect as schema owner")
}

/// Reads `--flag value` pairs after the command name.
fn flag(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().skip(2).collect();
    args.windows(2)
        .find(|pair| pair[0] == format!("--{name}"))
        .map(|pair| pair[1].clone())
}

fn required_flag(name: &str) -> anyhow::Result<String> {
    flag(name).with_context(|| format!("--{name} is required"))
}

async fn create_tenant() -> anyhow::Result<()> {
    init_tracing(Environment::Development);
    let (slug, name) = (required_flag("slug")?, required_flag("name")?);
    let branch = flag("branch").unwrap_or_else(|| "Cơ sở chính".to_owned());
    let pool = owner_pool().await?;
    let tenant = identity::provisioning::create_tenant(&pool, &slug, &name, &branch)
        .await
        .map_err(|e| anyhow::anyhow!("create-tenant failed: {}", e.code()))?;
    tracing::info!(%tenant, %slug, "tenant created with system roles");
    Ok(())
}

async fn add_user() -> anyhow::Result<()> {
    init_tracing(Environment::Development);
    let role_key = required_flag("role")?;
    let role = identity::SystemRole::from_key(&role_key)
        .with_context(|| format!("unknown role {role_key:?} (owner | manager | technician)"))?;
    let password = std::env::var("APP_NEW_USER_PASSWORD")
        .context("APP_NEW_USER_PASSWORD is required (kept out of argv and shell history)")?;
    let (tenant, phone, name) = (
        required_flag("tenant")?,
        required_flag("phone")?,
        required_flag("name")?,
    );
    let pool = owner_pool().await?;
    let user = identity::provisioning::add_user(
        &pool,
        identity::provisioning::NewUser {
            tenant_slug: &tenant,
            phone: &phone,
            full_name: &name,
            password: &password,
            role,
        },
    )
    .await
    .map_err(|e| anyhow::anyhow!("add-user failed: {}", e.code()))?;
    tracing::info!(%user, %tenant, role = role.key(), "user added");
    Ok(())
}

async fn add_staff() -> anyhow::Result<()> {
    init_tracing(Environment::Development);
    let (tenant, name, position) = (
        required_flag("tenant")?,
        required_flag("name")?,
        required_flag("position")?,
    );
    let pool = owner_pool().await?;
    let staff = org::provisioning::add_staff(&pool, &tenant, &name, &position)
        .await
        .map_err(|e| anyhow::anyhow!("add-staff failed: {}", e.code()))?;
    tracing::info!(%staff, %tenant, "staff added");
    Ok(())
}

async fn seed_catalog() -> anyhow::Result<()> {
    init_tracing(Environment::Development);
    let tenant = required_flag("tenant")?;
    let pool = owner_pool().await?;
    let report = catalog::service::seed_menu(&pool, &tenant)
        .await
        .map_err(|e| anyhow::anyhow!("seed-catalog failed: {}", e.code()))?;
    tracing::info!(
        categories = report.categories,
        services = report.services,
        inactive_without_price = report.inactive_without_price,
        %tenant,
        "menu seeded"
    );
    Ok(())
}

async fn reset_mfa() -> anyhow::Result<()> {
    init_tracing(Environment::Development);
    let phone = required_flag("phone")?;
    let pool = owner_pool().await?;
    let found = identity::provisioning::reset_mfa(&pool, &phone)
        .await
        .map_err(|e| anyhow::anyhow!("reset-mfa failed: {}", e.code()))?;
    anyhow::ensure!(found, "no account with that phone number");
    tracing::info!("MFA reset; the next login will ask to enroll an authenticator again");
    Ok(())
}

async fn migrate() -> anyhow::Result<()> {
    init_tracing(Environment::Development);
    let pool = owner_pool().await?;
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
