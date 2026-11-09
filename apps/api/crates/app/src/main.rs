use anyhow::Context;
use app::config::{Config, Environment};
use app::middleware::HttpPolicy;
use app::{AppState, jobs};
use secrecy::ExposeSecret;
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use std::time::Duration;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::Layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

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
///   `app unlock --phone P`             clear a login lockout (verify identity first)
///   `app seed-demo --tenant S [--force-again]`
///                                      fill a spa with demo data (staff, a manager login 0905000002
///                                      whose password comes from APP_NEW_USER_PASSWORD, customers,
///                                      sales, three weeks of visits); refuses a spa that has it
///   `app import-paper-book --tenant S --file F.csv [--dry-run]`
///                                      one-off migration of the paper package books (columns in
///                                      memory/wiki/product/specs/management-views.md §7); acts as
///                                      the spa's owner, skips packages already imported
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
        Some("unlock") => unlock().await,
        Some("seed-demo") => seed_demo().await,
        Some("import-paper-book") => import_paper_book().await,
        Some("openapi") => {
            print!("{}", app::openapi::json()?);
            Ok(())
        }
        Some(other) => anyhow::bail!(
            "unknown command {other:?} (expected: serve | migrate | create-tenant | add-user | add-staff | seed-catalog | reset-mfa | unlock | seed-demo | import-paper-book | openapi)"
        ),
    }
}

async fn serve() -> anyhow::Result<()> {
    let config = Config::from_env().context("invalid configuration")?;
    init_tracing(config.environment);

    // Serverless hosts (Vercel) run several small instances against one Supabase pooler: keep each
    // instance's share small there (`APP_DB_MAX_CONNECTIONS`); a single VPS keeps the default.
    let max_connections = std::env::var("APP_DB_MAX_CONNECTIONS")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(10);
    let pool = PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(3))
        .connect(config.app_database_url.expose_secret())
        .await
        .context("cannot connect to the database as app_rw")?;
    app::db_guard::check_runtime_role(&pool)
        .await
        .context("refusing to start: APP_DATABASE_URL must be the app_rw role (RLS applies)")?;
    jobs::spawn_maintenance(pool.clone());

    let mut policy = HttpPolicy::standard(config.public_origin.clone());
    if let Some(limit) = config.rate_limit {
        // Development only (Config refuses it elsewhere): the E2E/Lighthouse stack.
        tracing::warn!(?limit, "API rate limit overridden for the local test stack");
        policy.rate_per_second = limit.per_second;
        policy.rate_burst = limit.burst;
    }
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

/// A bare `--switch` after the command name.
fn switch(name: &str) -> bool {
    std::env::args().skip(2).any(|a| a == format!("--{name}"))
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

async fn unlock() -> anyhow::Result<()> {
    init_tracing(Environment::Development);
    let phone = required_flag("phone")?;
    let pool = owner_pool().await?;
    let found = identity::provisioning::unlock_account(&pool, &phone)
        .await
        .map_err(|e| anyhow::anyhow!("unlock failed: {}", e.code()))?;
    anyhow::ensure!(found, "no account with that phone number");
    tracing::info!("login lockout cleared");
    Ok(())
}

async fn seed_demo() -> anyhow::Result<()> {
    init_tracing(Environment::Development);
    // Invented customers and money must never mix with a real spa's books.
    anyhow::ensure!(
        std::env::var("APP_ENV").as_deref() != Ok("production"),
        "seed-demo refuses APP_ENV=production: demo data is for development and staging only"
    );
    let tenant = required_flag("tenant")?;
    let password = std::env::var("APP_NEW_USER_PASSWORD").context(
        "APP_NEW_USER_PASSWORD is required: the demo manager's password (kept out of argv)",
    )?;
    let data_key = std::env::var("APP_DATA_KEY")
        .context("APP_DATA_KEY is required: health notes are encrypted with the API's key")?;
    let cipher = kernel::crypto::FieldCipher::from_base64(&data_key)
        .map_err(|e| anyhow::anyhow!("APP_DATA_KEY: {e}"))?;
    let owner = owner_pool().await?;
    let app_url = std::env::var("APP_DATABASE_URL")
        .context("APP_DATABASE_URL (app_rw) is required: demo data goes through the use cases")?;
    let app = PgPoolOptions::new()
        .max_connections(2)
        .connect(&app_url)
        .await
        .context("cannot connect as app_rw")?;
    app::db_guard::check_runtime_role(&app)
        .await
        .context("APP_DATABASE_URL must be the app_rw role (RLS applies)")?;
    let report = app::demo::seed(
        &owner,
        &app,
        &cipher,
        &app::demo::DemoOptions {
            tenant_slug: &tenant,
            manager_password: &password,
            force_again: switch("force-again"),
        },
    )
    .await?;
    tracing::info!(%tenant, ?report, "demo data seeded; manager login {}", app::demo::MANAGER_PHONE);
    Ok(())
}

async fn import_paper_book() -> anyhow::Result<()> {
    init_tracing(Environment::Development);
    let (tenant, file) = (required_flag("tenant")?, required_flag("file")?);
    let csv = std::fs::read_to_string(&file).with_context(|| format!("cannot read {file}"))?;
    let data_key = std::env::var("APP_DATA_KEY")
        .context("APP_DATA_KEY is required: new customers go through the API's use case")?;
    let cipher = kernel::crypto::FieldCipher::from_base64(&data_key)
        .map_err(|e| anyhow::anyhow!("APP_DATA_KEY: {e}"))?;
    let owner = owner_pool().await?;
    let app_url = std::env::var("APP_DATABASE_URL")
        .context("APP_DATABASE_URL (app_rw) is required: the import goes through the use case")?;
    let app = PgPoolOptions::new()
        .max_connections(2)
        .connect(&app_url)
        .await
        .context("cannot connect as app_rw")?;
    app::db_guard::check_runtime_role(&app)
        .await
        .context("APP_DATABASE_URL must be the app_rw role (RLS applies)")?;
    let dry_run = switch("dry-run");
    let report = app::paper_book::import(
        &owner,
        &app,
        &cipher,
        &app::paper_book::ImportOptions {
            tenant_slug: &tenant,
            csv: &csv,
            dry_run,
        },
    )
    .await?;
    tracing::info!(%tenant, dry_run, ?report, "paper-book import finished");
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
/// The log filter is per layer, so `RUST_LOG` never decides whether `Server-Timing` can count
/// SQL statements (app::server_timing listens to sqlx's DEBUG events for itself only).
fn init_tracing(environment: Environment) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let registry =
        tracing_subscriber::registry().with(app::server_timing::DbTimingLayer::filtered());
    if environment == Environment::Development {
        registry
            .with(tracing_subscriber::fmt::layer().with_filter(filter))
            .init();
    } else {
        registry
            .with(tracing_subscriber::fmt::layer().json().with_filter(filter))
            .init();
    }
}

async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::error!(%error, "failed to listen for shutdown signal");
    }
}
