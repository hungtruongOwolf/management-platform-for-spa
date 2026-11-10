use anyhow::Context;
use app::config::{Config, Environment};
use app::middleware::HttpPolicy;
use app::{AppState, jobs};
use secrecy::ExposeSecret;
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
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
///                                      sessions; next login re-enrolls where MFA is required
///                                      (verify identity first)
///   `app unlock --phone P`             clear a login lockout (verify identity first)
///   `app add-operator --username U --name N`
///                                      platform console operator (ADR-0018; password from env
///                                      APP_NEW_OPERATOR_PASSWORD, never argv); MFA enrolls at the
///                                      first sign-in at /admin
///   `app reset-operator-mfa --username U`   lost operator authenticator (verify identity first)
///   `app disable-operator --username U` | `app enable-operator --username U`
///   `app seed-demo --tenant S [--months N] [--workers W]`
///                                      fill a spa with one full demo scenario: N months (default
///                                      12) of visits, packages, appointments, staff books and
///                                      closed payroll up to today; refuses a spa that has demo
///                                      customers. Logins reused when they exist; new ones take their
///                                      password from env (APP_NEW_USER_PASSWORD manager,
///                                      APP_NEW_TECHNICIAN_PASSWORD, APP_NEW_FORMER_TECHNICIAN_PASSWORD)
///   `app reset-demo --tenant S`        delete every business row of a spa (customers, money,
///                                      packages, appointments, staff books, its audit log); keeps
///                                      the spa, roles, catalog, settings and logins. Needs
///                                      APP_ENV=development or staging, set explicitly
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
        Some("add-operator") => add_operator().await,
        Some("reset-operator-mfa") => reset_operator_mfa().await,
        Some("disable-operator") => set_operator_active(false).await,
        Some("enable-operator") => set_operator_active(true).await,
        Some("seed-demo") => seed_demo().await,
        Some("reset-demo") => reset_demo().await,
        Some("import-paper-book") => import_paper_book().await,
        Some("openapi") => {
            print!("{}", app::openapi::json()?);
            Ok(())
        }
        Some(other) => anyhow::bail!(
            "unknown command {other:?} (expected: serve | migrate | create-tenant | add-user | add-staff | seed-catalog | reset-mfa | unlock | add-operator | reset-operator-mfa | disable-operator | enable-operator | seed-demo | reset-demo | import-paper-book | openapi)"
        ),
    }
}

async fn serve() -> anyhow::Result<()> {
    let config = Config::from_env().context("invalid configuration")?;
    init_tracing(config.environment);

    let pool = app::db_pool::options(config.db_max_connections, app::db_pool::PING_WHEN_IDLE_FOR)
        .connect(config.app_database_url.expose_secret())
        .await
        .context("cannot connect to the database as app_rw")?;
    app::db_guard::check_runtime_role(&pool)
        .await
        .context("refusing to start: APP_DATABASE_URL must be the app_rw role (RLS applies)")?;
    jobs::spawn_maintenance(pool.clone());

    let mut policy = HttpPolicy::standard(config.public_origin.clone());
    policy.admin_origin = config.admin_origin.clone();
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

async fn add_operator() -> anyhow::Result<()> {
    init_tracing(Environment::Development);
    let password = std::env::var("APP_NEW_OPERATOR_PASSWORD")
        .context("APP_NEW_OPERATOR_PASSWORD is required (kept out of argv and shell history)")?;
    let (username, name) = (required_flag("username")?, required_flag("name")?);
    let pool = owner_pool().await?;
    let operator = platform::operator::add_operator(&pool, &username, &name, &password)
        .await
        .map_err(|e| anyhow::anyhow!("add-operator failed: {}", e.code()))?;
    tracing::warn!(
        %operator, %username,
        "operator created: sign in at /admin now and enroll the authenticator before anyone else can"
    );
    Ok(())
}

async fn reset_operator_mfa() -> anyhow::Result<()> {
    init_tracing(Environment::Development);
    let username = required_flag("username")?;
    let pool = owner_pool().await?;
    let done = platform::operator::reset_operator_mfa(&pool, &username)
        .await
        .map_err(|e| anyhow::anyhow!("reset-operator-mfa failed: {}", e.code()))?;
    anyhow::ensure!(done, "no operator {username:?}");
    tracing::info!(%username, "operator MFA cleared; sessions ended");
    Ok(())
}

async fn set_operator_active(active: bool) -> anyhow::Result<()> {
    init_tracing(Environment::Development);
    let username = required_flag("username")?;
    let pool = owner_pool().await?;
    let done = platform::operator::set_operator_active(&pool, &username, active)
        .await
        .map_err(|e| anyhow::anyhow!("operator status change failed: {}", e.code()))?;
    anyhow::ensure!(done, "no operator {username:?}");
    tracing::info!(%username, active, "operator status changed");
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
    tracing::info!(
        "MFA reset; the next login enrolls an authenticator again where the spa requires MFA"
    );
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

/// Invented customers and money must never mix with a real spa's books.
fn refuse_production(command: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        std::env::var("APP_ENV").as_deref() != Ok("production"),
        "{command} refuses APP_ENV=production: demo data is for development and staging only"
    );
    Ok(())
}

/// A password read from env for a demo login that may not exist yet (`None` when unset).
fn optional_secret(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

async fn seed_demo() -> anyhow::Result<()> {
    init_tracing(Environment::Development);
    refuse_production("seed-demo")?;
    let tenant = required_flag("tenant")?;
    let months = flag("months")
        .map(|m| m.parse::<u32>().context("--months must be a whole number"))
        .transpose()?
        .unwrap_or(app::demo::DEFAULT_MONTHS);
    let workers = flag("workers")
        .map(|w| {
            w.parse::<usize>()
                .context("--workers must be a whole number")
        })
        .transpose()?
        .unwrap_or(app::demo::DEFAULT_WORKERS);
    let manager = optional_secret("APP_NEW_USER_PASSWORD");
    let technician = optional_secret("APP_NEW_TECHNICIAN_PASSWORD");
    let former = optional_secret("APP_NEW_FORMER_TECHNICIAN_PASSWORD");
    let data_key = std::env::var("APP_DATA_KEY")
        .context("APP_DATA_KEY is required: health notes are encrypted with the API's key")?;
    let cipher = kernel::crypto::FieldCipher::from_base64(&data_key)
        .map_err(|e| anyhow::anyhow!("APP_DATA_KEY: {e}"))?;
    let owner = owner_pool().await?;
    let app_url = std::env::var("APP_DATABASE_URL")
        .context("APP_DATABASE_URL (app_rw) is required: demo data goes through the use cases")?;
    // Each worker holds one connection per use case; a few spare for the reads in between.
    let app = PgPoolOptions::new()
        .max_connections(u32::try_from(workers).unwrap_or(8).saturating_add(2))
        .acquire_timeout(std::time::Duration::from_secs(60))
        .connect(&app_url)
        .await
        .context("cannot connect as app_rw")?;
    app::db_guard::check_runtime_role(&app)
        .await
        .context("APP_DATABASE_URL must be the app_rw role (RLS applies)")?;
    let started = std::time::Instant::now();
    // Boxed: the seed's future is too large for the main thread's stack (see `demo::run`).
    let report = Box::pin(app::demo::seed(
        &owner,
        &app,
        &cipher,
        &app::demo::DemoOptions {
            tenant_slug: &tenant,
            manager_password: manager.as_deref(),
            technician_password: technician.as_deref(),
            former_technician_password: former.as_deref(),
            months,
            workers,
        },
    ))
    .await?;
    let summary = app::demo::summary(&owner, &tenant).await?;
    println!(
        "{}",
        app::demo::render(&report, &summary, started.elapsed())
    );
    tracing::info!(
        %tenant,
        "demo data seeded; logins: manager {}, technician {}, former technician {}",
        app::demo::MANAGER_PHONE,
        app::demo::TECHNICIAN_PHONE,
        app::demo::FORMER_TECHNICIAN_PHONE
    );
    Ok(())
}

async fn reset_demo() -> anyhow::Result<()> {
    init_tracing(Environment::Development);
    // Deleting is worse than inventing: an unset APP_ENV is not enough, it must say so.
    let env = std::env::var("APP_ENV").unwrap_or_default();
    anyhow::ensure!(
        matches!(env.as_str(), "development" | "staging"),
        "reset-demo deletes a spa's business data: set APP_ENV=development or APP_ENV=staging \
         explicitly (got {env:?})"
    );
    let tenant = required_flag("tenant")?;
    let owner = owner_pool().await?;
    let report = app::demo::reset(&owner, &tenant).await?;
    println!("reset-demo {tenant}: deleted {} rows", report.total());
    for (table, rows) in &report.deleted {
        println!("  {table:<28} {rows:>8}");
    }
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
