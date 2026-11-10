//! `app seed-demo --tenant <slug> [--months N]`: fills a spa with one complete, realistic
//! scenario — N months (default 12) of front desk, packages, appointments and staff books up to
//! today — so every screen of every role has something to show (staging, trial sessions).
//! `app reset-demo` ([`reset`]) empties a spa's business data first. Operator tools only; no HTTP
//! route reaches them.
//!
//! Every record goes through the use cases (customers, sales, visits, voids, corrections, the
//! paper-book import, appointments, attendance, evaluations, payroll close…), acting as the spa's
//! owner or the demo manager, so validation, invariants, audit and append-only money rules all
//! hold. History is back-filled with the `*_as_of` forms of the use cases, which take the clock as
//! a parameter instead of reading it. Everything is relative to the run date and drawn from a
//! seeded generator: the same day gives the same spa.
//!
//! Speed: independent work (separate customers, separate days, separate months) runs side by side
//! on `workers` pool connections ([`run`]); a customer's own records stay in time order. Each
//! phase reports its SQL statements, i.e. its round trips (`server_timing::count_statements`).
//!
//! Demo customers use the phone block `0399 00 CCCC`: that is how a second run detects the first.

mod calendar;
mod data;
mod desk;
mod people;
mod plan;
mod reset;
mod rng;
mod run;
mod staff;
mod summary;

pub use calendar::CalendarReport;
pub use data::MANAGER_PHONE;
pub use desk::Tally;
pub use reset::{DELETED, KEPT, ResetReport, reset};
pub use staff::{FORMER_TECHNICIAN_PHONE, StaffReport, TECHNICIAN_PHONE};
pub use summary::{Summary, render, summary};

use anyhow::{Context, anyhow, bail};
use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use data::{PLANS, Sex};
use desk::{Chain, Package, packages_in};
use kernel::crypto::FieldCipher;
use kernel::idempotency::hash_request;
use kernel::{Actor, AppError, TenantId, TenantTx, UserId};
use people::{PAPER_CUSTOMERS, Person};
use rng::Rng;
use serde_json::json;
use sqlx::PgPool;
use std::collections::HashMap;
use std::future::Future;
use std::time::Instant;
use uuid::Uuid;

const SEED: u64 = 0x5EED_0000_D3E0_2026;
const DEMO_PHONE_PREFIX: &str = "+8439900";
pub const DEFAULT_MONTHS: u32 = 12;
pub const DEFAULT_WORKERS: usize = 8;
/// A leaver mid last month needs last month; reports compare up to two years.
const MONTHS: std::ops::RangeInclusive<u32> = 2..=24;

/// The days of the history: from the first day of the month `months - 1` before today's, to
/// today. Every month but the current one ends before today and is closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    pub start: NaiveDate,
    pub today: NaiveDate,
}

impl Window {
    pub fn new(today: NaiveDate, months: u32) -> Self {
        let first = today.with_day(1).unwrap_or(today);
        let start = first
            .checked_sub_months(chrono::Months::new(months.saturating_sub(1)))
            .unwrap_or(first);
        Self { start, today }
    }

    /// Days from the start to today.
    pub fn days(&self) -> i64 {
        (self.today - self.start).num_days()
    }
}

pub struct DemoOptions<'a> {
    pub tenant_slug: &'a str,
    /// Passwords of the demo logins (from env, never argv); needed only for a login that does
    /// not exist yet — an existing account is reused and keeps its password.
    pub manager_password: Option<&'a str>,
    pub technician_password: Option<&'a str>,
    pub former_technician_password: Option<&'a str>,
    /// Months of history, this one included (2–24).
    pub months: u32,
    /// Use cases in flight at once (the app pool needs a few more connections than this).
    pub workers: usize,
}

/// One step of the run: how long it took and how many SQL statements (round trips) it sent.
#[derive(Debug, Clone, PartialEq)]
pub struct Phase {
    pub name: &'static str,
    pub seconds: f64,
    pub statements: u32,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct DemoReport {
    pub window_start: Option<NaiveDate>,
    pub today: Option<NaiveDate>,
    pub catalog_seeded: bool,
    pub plans_created: usize,
    pub customers: usize,
    pub opening_balances: usize,
    pub desk: Tally,
    pub calendar: CalendarReport,
    pub staff: StaffReport,
    pub cancelled_with_refund: usize,
    pub extended: usize,
    pub phases: Vec<Phase>,
}

/// Seeds the demo spa. `owner` is the schema-owner pool (provisioning, as the other CLI commands);
/// `app` is the `app_rw` pool every use case runs on, with RLS.
pub async fn seed(
    owner: &PgPool,
    app: &PgPool,
    cipher: &FieldCipher,
    options: &DemoOptions<'_>,
) -> anyhow::Result<DemoReport> {
    anyhow::ensure!(
        MONTHS.contains(&options.months),
        "--months must be between {} and {}",
        MONTHS.start(),
        MONTHS.end()
    );
    let slug = options.tenant_slug;
    let tenant = TenantId::from_uuid(
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM app.tenant WHERE slug = $1")
            .bind(slug)
            .fetch_optional(owner)
            .await?
            .with_context(|| format!("no spa with slug {slug:?}"))?,
    );
    refuse_seeded(owner, tenant).await?;
    for (phone, password, env) in [
        (
            MANAGER_PHONE,
            options.manager_password,
            "APP_NEW_USER_PASSWORD",
        ),
        (
            TECHNICIAN_PHONE,
            options.technician_password,
            "APP_NEW_TECHNICIAN_PASSWORD",
        ),
        (
            FORMER_TECHNICIAN_PHONE,
            options.former_technician_password,
            "APP_NEW_FORMER_TECHNICIAN_PASSWORD",
        ),
    ] {
        if password.is_none() && !login_exists(owner, phone).await? {
            bail!(
                "{env} is required: the demo login {phone} does not exist yet; nothing was changed"
            );
        }
    }
    let mut report = DemoReport::default();
    let mut phases = Vec::new();
    let started = Instant::now();
    let owner_actor = member_actor(app, owner_of(owner, tenant).await?, tenant).await?;
    report.catalog_seeded = ensure_catalog(owner, tenant, slug).await?;
    report.plans_created = ensure_plans(app, &owner_actor).await?;
    let catalog = Catalog::load(app, &owner_actor).await?;

    let logins = staff::Logins {
        manager: ensure_login(
            owner,
            slug,
            MANAGER_PHONE,
            data::MANAGER_NAME,
            identity::SystemRole::Manager,
            options.manager_password,
            "APP_NEW_USER_PASSWORD",
        )
        .await?,
        technician: ensure_login(
            owner,
            slug,
            TECHNICIAN_PHONE,
            "Nguyễn Thị Lan",
            identity::SystemRole::Technician,
            options.technician_password,
            "APP_NEW_TECHNICIAN_PASSWORD",
        )
        .await?,
        former: ensure_login(
            owner,
            slug,
            FORMER_TECHNICIAN_PHONE,
            "Đỗ Thị Ngân",
            identity::SystemRole::Technician,
            options.former_technician_password,
            "APP_NEW_FORMER_TECHNICIAN_PASSWORD",
        )
        .await?,
    };
    let manager = member_actor(app, logins.manager, tenant).await?;
    let tz = {
        let mut tx = TenantTx::begin(app, tenant).await.map_err(fail("open"))?;
        let profile = kernel::tenant::profile(&mut tx)
            .await
            .map_err(fail("read the spa"))?;
        tx.commit().await.map_err(fail("commit"))?;
        profile.timezone
    };
    let now = Utc::now();
    let today = kernel::BusinessDate::of_instant(now, tz).as_naive();
    let window = Window::new(today, options.months);
    report.window_start = Some(window.start);
    report.today = Some(today);

    let roster = staff::roster(today);
    let staff_ids = staff::setup(
        owner,
        app,
        &owner_actor,
        slug,
        &roster,
        &logins,
        &mut report.staff,
    )
    .await?;
    phases.push(Phase {
        name: "setup (catalog, logins, roster)",
        seconds: started.elapsed().as_secs_f64(),
        statements: 0,
    });

    let mut rng = Rng::new(SEED);
    let attendance = staff::Attendance::plan(window, &roster, &mut rng);
    let count = 40 + 15 * usize::try_from(options.months).unwrap_or(12);
    let people = people::people(window, count, &mut rng);
    let ctx = Ctx {
        app,
        cipher,
        owner: owner_actor,
        manager,
        catalog,
        roster,
        staff: staff_ids,
        attendance,
        tz,
        window,
        now,
        workers: options.workers.max(1),
    };
    let today_minutes = today_minutes(ctx.now_minute(), &mut rng);
    let schedule = plan::schedule(window, &people, &today_minutes, &mut rng);

    let mut ids = timed(&mut phases, "customers", ctx.create_customers(&people)).await?;
    report.customers = ids.iter().filter(|id| !id.is_nil()).count();
    let mut initial: Vec<Vec<Package>> = vec![Vec::new(); people.len()];
    report.opening_balances = timed(
        &mut phases,
        "paper-book opening balances",
        ctx.import_paper_books(&people, &mut ids, &mut initial),
    )
    .await?;
    report.customers = ids.iter().filter(|id| !id.is_nil()).count();

    let mut chains: Vec<(usize, Vec<plan::Visit>)> = schedule.into_iter().enumerate().collect();
    // Longest chains first, so no worker is left with a long one at the end.
    chains.sort_by_key(|(c, visits)| (std::cmp::Reverse(visits.len()), *c));
    let ctx_ref = &ctx;
    let (ids_ref, people_ref, initial_ref) = (&ids, &people, &initial);
    let results = timed(
        &mut phases,
        "front desk history (visits, sales, voids, corrections, follow-ups)",
        run::each(chains, ctx.workers, |(c, visits)| async move {
            Chain::new(
                ctx_ref,
                c,
                ids_ref[c],
                &people_ref[c],
                initial_ref[c].clone(),
            )
            .run(&visits)
            .await
        }),
    )
    .await?;
    let mut packages: Vec<Package> = Vec::new();
    for (p, tally) in results {
        packages.extend(p);
        report.desk.add(&tally);
    }
    packages.sort_by_key(|p| (p.customer, p.starts_on));

    report.calendar = timed(
        &mut phases,
        "appointments",
        calendar::write(&ctx, &people, &ids, &mut rng),
    )
    .await?;
    let (cancelled, extended) = timed(
        &mut phases,
        "package cancellations and extension",
        ctx.package_lifecycle(&packages, &initial, &people),
    )
    .await?;
    report.cancelled_with_refund = cancelled;
    report.extended = extended;

    let books = staff::Books {
        app,
        owner: &ctx.owner,
        manager: &ctx.manager,
        window,
        roster: &ctx.roster,
        staff: &ctx.staff,
        attendance: &ctx.attendance,
        workers: ctx.workers,
    };
    let mut staff_report = std::mem::take(&mut report.staff);
    timed(
        &mut phases,
        "staff books and payroll close",
        books.write(&mut rng, &mut staff_report),
    )
    .await?;
    report.staff = staff_report;

    // "Quên mật khẩu?" from the technician: the owner's overview lists it.
    identity::service::request_password_reset(
        app,
        TECHNICIAN_PHONE,
        &identity::service::ClientMeta {
            ip: None,
            user_agent: Some("seed-demo".to_owned()),
        },
    )
    .await
    .map_err(fail("request a password reset"))?;
    report.phases = phases;
    Ok(report)
}

/// Refuses a spa that already has demo customers: nothing is written.
async fn refuse_seeded(owner: &PgPool, tenant: TenantId) -> anyhow::Result<()> {
    let seeded: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM app.customer WHERE tenant_id = $1 AND phone_e164 LIKE $2)",
    )
    .bind(tenant.as_uuid())
    .bind(format!("{DEMO_PHONE_PREFIX}%"))
    .fetch_one(owner)
    .await?;
    if seeded {
        bail!(
            "this spa already has demo data (customers with phones 0399 00…); nothing was \
             changed. Empty it first with `app reset-demo --tenant <slug>`."
        );
    }
    Ok(())
}

/// Runs one phase, recording its time and SQL statement count.
async fn timed<T>(
    phases: &mut Vec<Phase>,
    name: &'static str,
    work: impl Future<Output = anyhow::Result<T>>,
) -> anyhow::Result<T> {
    let started = Instant::now();
    let (out, statements) = crate::server_timing::count_statements(work).await;
    phases.push(Phase {
        name,
        seconds: started.elapsed().as_secs_f64(),
        statements,
    });
    tracing::info!(
        phase = name,
        seconds = started.elapsed().as_secs_f64(),
        statements,
        "demo phase done"
    );
    out
}

/// Today's visit times up to now (local minutes): from 08:00 (or half an hour ago when it is
/// earlier), the last one minutes ago so it is still under way on `/quay`.
fn today_minutes(now_minute: u32, rng: &mut Rng) -> Vec<u32> {
    let end = now_minute.saturating_sub(1);
    let start = (8 * 60).min(end.saturating_sub(30));
    let span = end - start;
    let n = (3 + span / 75).min(12);
    let step = (span / n).max(1);
    let mut out: Vec<u32> = (1..n)
        .map(|i| {
            start + step * i
                - u32::try_from(rng.below(usize::try_from(step / 2).unwrap_or(0))).unwrap_or(0)
        })
        .collect();
    out.push(
        end.saturating_sub(u32::try_from(rng.below(10)).unwrap_or(0))
            .max(start),
    );
    out
}

/// Maps a use-case error to a readable failure naming the step.
fn fail(step: &'static str) -> impl FnOnce(AppError) -> anyhow::Error {
    move |e| anyhow!("{step}: {} ({e:?})", e.code())
}

fn idempotency() -> (Uuid, [u8; 32]) {
    let key = Uuid::now_v7();
    (key, hash_request(key.as_bytes()))
}

pub(crate) async fn owner_of(owner: &PgPool, tenant: TenantId) -> anyhow::Result<UserId> {
    let user: Option<Uuid> = sqlx::query_scalar(
        "SELECT m.user_id FROM app.membership m
         JOIN app.membership_role mr ON mr.tenant_id = m.tenant_id AND mr.membership_id = m.id
         JOIN app.role r ON r.tenant_id = mr.tenant_id AND r.id = mr.role_id
         WHERE m.tenant_id = $1 AND m.status = 'active' AND r.key = 'owner'
         ORDER BY m.created_at, m.id LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .fetch_optional(owner)
    .await?;
    user.map(UserId::from_uuid).context(
        "the spa has no owner yet: run `app add-user --role owner` first \
         (refunds, payroll and the paper-book import act as the owner)",
    )
}

/// Acts as `user` with exactly the permissions their roles grant (audit names them).
pub(crate) async fn member_actor(
    app: &PgPool,
    user: UserId,
    tenant: TenantId,
) -> anyhow::Result<Actor> {
    let permissions = identity::service::tenant_permissions(app, user, tenant)
        .await
        .map_err(fail("read permissions"))?
        .context("user is not an active member of the spa")?;
    Ok(Actor::new(user, tenant, permissions, None))
}

async fn login_exists(owner: &PgPool, phone: &str) -> anyhow::Result<bool> {
    let e164 = kernel::PhoneE164::parse(phone).map_err(|_| anyhow!("demo phone {phone}"))?;
    Ok(
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM app.user_account WHERE phone_e164 = $1)")
            .bind(e164.as_str())
            .fetch_one(owner)
            .await?,
    )
}

/// The login with `phone` as a member of the spa with `role`: an existing account is reused and
/// keeps its password (and MFA); a new one needs `password`, read by the CLI from `env`.
async fn ensure_login(
    owner: &PgPool,
    slug: &str,
    phone: &str,
    name: &str,
    role: identity::SystemRole,
    password: Option<&str>,
    env: &str,
) -> anyhow::Result<UserId> {
    let password = match (login_exists(owner, phone).await?, password) {
        (false, None) => bail!("{env} is required: the demo login {phone} does not exist yet"),
        (false, Some(p)) => p,
        // `add_user` never reads the password of an account that exists.
        (true, p) => p.unwrap_or_default(),
    };
    identity::provisioning::add_user(
        owner,
        identity::provisioning::NewUser {
            tenant_slug: slug,
            phone,
            full_name: name,
            password,
            role,
        },
    )
    .await
    .map_err(fail("add a demo login"))
}

async fn ensure_catalog(owner: &PgPool, tenant: TenantId, slug: &str) -> anyhow::Result<bool> {
    let services: i64 = sqlx::query_scalar("SELECT count(*) FROM app.service WHERE tenant_id = $1")
        .bind(tenant.as_uuid())
        .fetch_one(owner)
        .await?;
    if services > 0 {
        return Ok(false);
    }
    catalog::service::seed_menu(owner, slug)
        .await
        .map_err(fail("seed the menu"))?;
    Ok(true)
}

/// The catalog the seed sells from: services by name with their prices, and the demo plans (in
/// [`PLANS`] order) with their current price.
pub(crate) struct Catalog {
    services: HashMap<String, (Uuid, catalog::ServicePrices)>,
    plans: Vec<(Uuid, i64)>,
}

impl Catalog {
    async fn load(app: &PgPool, actor: &Actor) -> anyhow::Result<Self> {
        let services = catalog::service::services(app, actor, true)
            .await
            .map_err(fail("list services"))?
            .into_iter()
            .map(|s| (s.name, (s.id, s.prices)))
            .collect();
        let listed = catalog::service::package_plans(app, actor, true)
            .await
            .map_err(fail("list package plans"))?;
        let plans = PLANS
            .iter()
            .map(|plan| {
                listed
                    .iter()
                    .find(|p| p.name == plan.name)
                    .map(|p| (p.id, p.price.as_i64()))
                    .with_context(|| format!("package plan {:?} missing or inactive", plan.name))
            })
            .collect::<anyhow::Result<_>>()?;
        Ok(Self { services, plans })
    }

    fn service(&self, name: &str) -> anyhow::Result<Uuid> {
        self.services.get(name).map(|(id, _)| *id).with_context(|| {
            format!("service {name:?} is not on sale: the demo expects the printed menu")
        })
    }

    /// The service of the plan's first item (today's session of a package sold in a visit).
    fn plan_service(&self, plan: usize) -> anyhow::Result<Uuid> {
        let item = PLANS[plan].items.first().context("plan without items")?;
        self.service(item.service)
    }

    /// The listed price of a one-off service for this customer, as the server will charge it.
    fn price(&self, service: Uuid, sex: Sex) -> anyhow::Result<i64> {
        let (_, prices) = self
            .services
            .values()
            .find(|(id, _)| *id == service)
            .context("service vanished")?;
        let gender = match sex {
            Sex::Female => catalog::Gender::Female,
            Sex::Male => catalog::Gender::Male,
        };
        let (_, amount) = prices
            .price_for(Some(gender))
            .map_err(|e| anyhow!("price of {service}: {e:?}"))?;
        Ok(amount.as_i64())
    }
}

async fn ensure_plans(app: &PgPool, owner: &Actor) -> anyhow::Result<usize> {
    let services: HashMap<String, Uuid> = catalog::service::services(app, owner, true)
        .await
        .map_err(fail("list services"))?
        .into_iter()
        .map(|s| (s.name, s.id))
        .collect();
    let existing = catalog::service::package_plans(app, owner, false)
        .await
        .map_err(fail("list package plans"))?;
    let mut created = 0;
    for plan in PLANS {
        if existing.iter().any(|p| p.name == plan.name) {
            continue;
        }
        let items = plan
            .items
            .iter()
            .map(|i| {
                Ok(catalog::commands::PlanItemInput {
                    service_id: *services
                        .get(i.service)
                        .with_context(|| format!("service {:?} missing", i.service))?,
                    sessions: i.sessions,
                    bonus_sessions: i.bonus_sessions,
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        catalog::commands::create_plan(
            app,
            owner,
            catalog::commands::PlanInput {
                name: plan.name.to_owned(),
                price: kernel::Money::vnd(plan.price).map_err(|e| anyhow!("{e:?}"))?,
                validity_months: plan.validity_months,
                gift_note: plan.gift_note.map(str::to_owned),
                active: true,
                items,
            },
        )
        .await
        .map_err(fail("create a package plan"))?;
        created += 1;
    }
    Ok(created)
}

/// What every phase of the run shares.
pub(crate) struct Ctx<'a> {
    app: &'a PgPool,
    cipher: &'a FieldCipher,
    owner: Actor,
    manager: Actor,
    catalog: Catalog,
    roster: Vec<staff::Member>,
    staff: staff::Staff,
    attendance: staff::Attendance,
    tz: Tz,
    window: Window,
    /// The run's clock, read once: "today" and today's times never move during a run.
    now: DateTime<Utc>,
    workers: usize,
}

impl Ctx<'_> {
    fn at(&self, day: NaiveDate, minutes: u32) -> anyhow::Result<DateTime<Utc>> {
        let local = day
            .and_hms_opt(minutes / 60, minutes % 60, 0)
            .context("time of day")?;
        Ok(self
            .tz
            .from_local_datetime(&local)
            .earliest()
            .context("local time")?
            .with_timezone(&Utc))
    }

    /// Local minutes after midnight of the run's clock.
    fn now_minute(&self) -> u32 {
        let local = self.now.with_timezone(&self.tz);
        local.hour() * 60 + local.minute()
    }

    /// The manager's staff record, who closes most package sales (D9).
    fn manager_staff(&self) -> Uuid {
        self.staff.ids[0]
    }

    fn roster_works(&self, staff: Uuid, day: NaiveDate) -> bool {
        self.staff
            .ids
            .iter()
            .position(|id| *id == staff)
            .is_some_and(|m| self.roster[m].works_on(day))
    }

    fn technicians_on(&self, day: NaiveDate) -> Vec<Uuid> {
        self.attendance
            .technicians_on(&self.roster, day)
            .into_iter()
            .map(|m| self.staff.ids[m])
            .collect()
    }

    /// Technicians still working here (bookings ahead).
    fn active_technicians(&self) -> Vec<Uuid> {
        (0..self.roster.len())
            .filter(|&m| self.roster[m].technician && self.roster[m].left.is_none())
            .map(|m| self.staff.ids[m])
            .collect()
    }

    /// Every customer but the two the paper-book import creates, side by side. Returns their
    /// ids by index (nil for the paper-book ones until they are imported).
    async fn create_customers(&self, people: &[Person]) -> anyhow::Result<Vec<Uuid>> {
        let items: Vec<usize> = (0..people.len()).collect();
        run::each(items, self.workers, |i| async move {
            if i < 2 {
                return Ok(Uuid::nil());
            }
            let p = &people[i];
            let (key, hash) = idempotency();
            let input = || crm::service::ProfileInput {
                full_name: p.full_name.clone(),
                phone: p.phone.clone(),
                gender: p.gender_recorded.then_some(crm_gender(p.sex)),
                birth_year: p.birth_year,
                birth_date: p.birth_date,
                address: p.address.map(str::to_owned),
                source: p.source.map(str::to_owned),
                note: None,
            };
            let view = run::retry(|| {
                crm::service::create(
                    self.app,
                    self.cipher,
                    &self.manager,
                    key,
                    hash,
                    input(),
                    p.health_note,
                )
            })
            .await
            .map_err(fail("create a customer"))?;
            Ok(view.row.id)
        })
        .await
    }

    /// Four customers carried over from the paper books on go-live day, through the import's
    /// use case (as the owner): two new profiles created by the import itself, two existing.
    /// Their packages are the scripted states: expiring in five days (some sessions kept), expired
    /// three days ago (used today with an override), two sessions left (used up during the
    /// history), expired 20 days ago (extended today).
    async fn import_paper_books(
        &self,
        people: &[Person],
        ids: &mut [Uuid],
        initial: &mut [Vec<Package>],
    ) -> anyhow::Result<usize> {
        let today = self.window.today;
        let day = |offset: i64| (today + Duration::days(offset)).to_string();
        let item = |service: &str, purchased: u32, bonus: u32, remaining: u32| {
            Ok::<_, anyhow::Error>(json!({
                "service_id": self.catalog.service(service)?, "sessions_purchased": purchased,
                "sessions_bonus": bonus, "sessions_remaining": remaining,
            }))
        };
        let profile = |i: usize| {
            let p = &people[i];
            json!({
                "full_name": p.full_name, "phone": p.phone,
                "gender": p.gender_recorded.then_some(match p.sex {
                    Sex::Female => "female",
                    Sex::Male => "male",
                }),
                "birth_year": p.birth_year, "address": p.address, "source": p.source,
                "health_note": p.health_note,
            })
        };
        let before_window = (self.window.start - today).num_days() - 30;
        let imports = [
            (
                0,
                json!({"customer": profile(0), "packages": [
                    {"package_plan_id": self.catalog.plans[0].0, "sold_price": 790_000,
                     "starts_on": day(-176), "expires_on": day(5), "gift_note": "Đã nhận dầu gội",
                     "items": [item("Thư Giãn", 10, 2, 6)?]},
                    {"name": "Xông chân 10 buổi", "sold_price": 1_200_000,
                     "starts_on": day(-200), "expires_on": day(-3),
                     "items": [item("Xông chân thảo dược + Massage chân", 10, 0, 3)?]},
                ]}),
                // The expired one keeps two sessions: one for today's override, one so it stays
                // "expired" rather than "used up".
                vec![2, 2],
            ),
            (
                1,
                json!({"customer": profile(1), "packages": [
                    {"package_plan_id": self.catalog.plans[2].0, "sold_price": 2_500_000,
                     "starts_on": day(-60), "expires_on": day(305),
                     "items": [item("Thân Tâm An Định", 10, 2, 9)?]},
                ]}),
                vec![0],
            ),
            (
                2,
                json!({"customer_id": ids[2], "packages": [
                    {"name": "Cổ vai gáy 10 buổi", "sold_price": 2_000_000,
                     "starts_on": day(before_window),
                     "items": [item("An Kiên Lạc", 10, 0, 2)?]},
                ]}),
                vec![0],
            ),
            (
                3,
                json!({"customer_id": ids[3], "packages": [
                    {"name": "Massage body 10 buổi", "sold_price": 2_200_000,
                     "starts_on": day(-220), "expires_on": day(-20),
                     "items": [item("Thân Tâm An Định", 10, 0, 4)?]},
                ]}),
                vec![4],
            ),
        ];
        for (customer, body, keep) in imports {
            let input: packages::opening_balance::OpeningBalanceCreate =
                serde_json::from_value(body).context("opening balance body")?;
            let (key, hash) = idempotency();
            let (_, reply) = run::retry(|| {
                sales::paper::import_opening_balance(
                    self.app,
                    self.cipher,
                    &self.owner,
                    key,
                    hash,
                    &input,
                )
            })
            .await
            .map_err(fail("import a paper-book package"))?;
            if customer < 2 {
                ids[customer] = serde_json::from_value(reply["customer_id"].clone())
                    .context("imported customer id")?;
            }
            let mut imported = packages_in(&reply, customer)?;
            for (p, k) in imported.iter_mut().zip(keep) {
                p.keep = k;
            }
            initial[customer].extend(imported);
        }
        Ok(PAPER_CUSTOMERS)
    }

    /// As the owner: two part-used packages cancelled with a refund of the unused share (cash
    /// about three weeks ago, transfer about three months ago, see [`cancellation`]), and today
    /// the expired paper-book package of customer 3 extended.
    async fn package_lifecycle(
        &self,
        packages: &[Package],
        initial: &[Vec<Package>],
        people: &[Person],
    ) -> anyhow::Result<(usize, usize)> {
        let today = self.window.today;
        let last_visits: Vec<NaiveDate> = people.iter().map(|p| p.last_day).collect();
        let mut cancelled = 0;
        let mut used: Vec<usize> = Vec::new();
        for (days_ago, method) in [(21, "cash"), (90, "bank_transfer")] {
            let Some((p, day)) = cancellation(packages, &last_visits, &used, self.window, days_ago)
            else {
                continue;
            };
            used.push(p.customer);
            let unused = i64::from(p.remaining());
            let total = i64::from(p.sessions_total.max(1));
            let refund = p.sold_price * unused / total / 10_000 * 10_000;
            let reason = if last_visits.get(p.customer).is_some_and(|last| *last < day) {
                "Khách chuyển công tác vào TP.HCM, hoàn tiền các buổi chưa dùng"
            } else {
                "Khách không hợp liệu trình, chuyển sang dịch vụ lẻ; hoàn tiền các buổi chưa dùng"
            };
            let input: packages::lifecycle::PackageCancel = serde_json::from_value(json!({
                "reason": reason,
                "refund_amount": refund, "refund_method": method,
            }))
            .context("cancel body")?;
            let at = self.at(day, 16 * 60)?;
            let (key, hash) = idempotency();
            run::retry(|| {
                packages::lifecycle::cancel_as_of(
                    self.app,
                    &self.owner,
                    key,
                    hash,
                    p.id,
                    &input,
                    at,
                )
            })
            .await
            .map_err(fail("cancel a package with refund"))?;
            cancelled += 1;
        }
        let mut extended = 0;
        if let Some(p) = initial.get(3).and_then(|list| list.first()) {
            let current = packages::service::get(self.app, &self.owner, p.id)
                .await
                .map_err(fail("read a package"))?;
            let input = packages::lifecycle::PackageExtend {
                expires_on: today + Duration::days(60),
                reason: Some("Khách nằm viện 1 tháng, chủ spa đồng ý gia hạn 2 tháng".to_owned()),
            };
            let (key, hash) = idempotency();
            run::retry(|| {
                packages::lifecycle::extend(
                    self.app,
                    &self.owner,
                    key,
                    hash,
                    p.id,
                    current.version,
                    &input,
                )
            })
            .await
            .map_err(fail("extend a package"))?;
            extended += 1;
        }
        Ok((cancelled, extended))
    }
}

/// The package to cancel with a refund about `days_ago` days back, and the day: a part-used package
/// sold at the desk, live that day, cancelled after its last used session (so no visit takes a
/// session from it later). Customers who had stopped coming by then come first (they moved away),
/// then the day nearest the target. `None` when no package qualifies (a very short history).
fn cancellation<'p>(
    packages: &'p [Package],
    last_visits: &[NaiveDate],
    taken: &[usize],
    window: Window,
    days_ago: i64,
) -> Option<(&'p Package, NaiveDate)> {
    let target = window.today - Duration::days(days_ago);
    packages
        .iter()
        .filter(|p| {
            p.from_sale
                && !taken.contains(&p.customer)
                && p.remaining() >= 2
                && p.remaining() < p.sessions_total
        })
        .filter_map(|p| {
            let day = target.max(p.last_used? + Duration::days(1));
            (day < window.today && p.usable_on(day)).then_some((p, day))
        })
        .min_by_key(|(p, day)| {
            let still_coming = last_visits.get(p.customer).is_none_or(|last| last >= day);
            (still_coming, (*day - target).num_days().abs())
        })
}

const fn crm_gender(sex: Sex) -> crm::service::Gender {
    match sex {
        Sex::Female => crm::service::Gender::Female,
        Sex::Male => crm::service::Gender::Male,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_starts_on_the_first_of_the_month_months_minus_one_ago() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 9).unwrap_or_default();
        let w = Window::new(today, 12);
        assert_eq!(
            w.start,
            NaiveDate::from_ymd_opt(2025, 12, 1).unwrap_or_default()
        );
        assert_eq!(Window::new(today, 2).start.to_string(), "2026-10-01");
    }

    fn package(customer: usize, starts: i64, expires: i64, last_used: i64) -> Package {
        let today = NaiveDate::from_ymd_opt(2026, 11, 10).unwrap_or_default();
        Package {
            id: Uuid::from_u128(u128::try_from(customer).unwrap_or(0) + 1),
            customer,
            items: vec![desk::Item {
                id: Uuid::nil(),
                service_id: Uuid::nil(),
                remaining: 6,
            }],
            starts_on: today - Duration::days(starts),
            expires_on: Some(today - Duration::days(expires)),
            sold_price: 2_000_000,
            sessions_total: 10,
            from_sale: true,
            cancelled: false,
            keep: 0,
            last_used: Some(today - Duration::days(last_used)),
        }
    }

    #[test]
    fn a_cancellation_lands_after_the_last_session_and_prefers_a_customer_who_left() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 10).unwrap_or_default();
        let w = Window::new(today, 12);
        let ago = |n: i64| today - Duration::days(n);
        // Customer 0 still comes (used 10 days ago), customer 1 left 70 days ago, customer 2's
        // package had expired by then.
        let packages = [
            package(0, 200, -100, 10),
            package(1, 200, -100, 75),
            package(2, 300, 40, 120),
        ];
        let last_visits = [today, ago(70), ago(110)];
        let pick = |list: &[Package], taken: &[usize], days_ago: i64| {
            cancellation(list, &last_visits, taken, w, days_ago).map(|(p, day)| (p.customer, day))
        };
        assert_eq!(
            pick(&packages, &[], 21),
            Some((1, ago(21))),
            "the one who left, on the target day"
        );
        // Without them: the day after the last session of a customer still coming, never before.
        assert_eq!(pick(&packages, &[1], 21), Some((0, ago(9))));
        // Customer 2's package expired 40 days ago: live 90 days back, not 21 days back.
        assert_eq!(pick(&packages, &[0, 1], 90), Some((2, ago(90))));
        assert_eq!(pick(&packages[2..], &[], 21), None);
    }

    #[test]
    fn today_times_end_just_before_now_and_increase() {
        for now in [7 * 60 + 5, 10 * 60, 17 * 60 + 40, 23 * 60] {
            let t = today_minutes(now, &mut Rng::new(5));
            assert!(t.len() >= 3, "{now}: {t:?}");
            assert!(t.windows(2).all(|w| w[0] <= w[1]), "{t:?}");
            assert!(t.iter().all(|m| *m < now), "{t:?}");
            assert!(
                now - t.last().copied().unwrap_or(0) <= 11,
                "the last one is under way"
            );
        }
    }
}
