//! `app seed-demo --tenant <slug>`: fills a spa with realistic demo data so every R1-1 screen has
//! something to show (staging, trial sessions). Operator tool only; no HTTP route reaches it.
//!
//! Every record goes through the front desk's own use cases (customers, package sales, visits,
//! voids, the paper-book import, cancel + refund), acting as the spa's owner or the demo
//! manager, so validation, invariants, audit and append-only money rules all hold. Three weeks of
//! history are back-filled with `sales::service::create_sale_as_of` and
//! `operations::service::record_as_of`, which take the clock as a parameter instead of reading it.
//!
//! Demo customers use the reserved phone block `0399 RR CCCC` (RR = run, CCCC = customer): that
//! is how a second run detects the first one.

mod data;
mod rng;

pub use data::MANAGER_PHONE;

use anyhow::{Context, anyhow, bail};
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use data::{CUSTOMERS, Gender, PLANS, Sex};
use kernel::crypto::FieldCipher;
use kernel::idempotency::hash_request;
use kernel::{Actor, AppError, TenantId, TenantTx, UserId};
use operations::dto::{VisitCreate, VisitLineCreate, VisitLineSourceDto};
use rng::Rng;
use sales::dto::{DiscountCreate, PaymentCreate, PaymentMethodDto, SaleCreate, SaleLineCreate};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// Days of history before today.
const HISTORY_DAYS: i64 = 20;
const SEED: u64 = 0x5EED_0000_D3E0_2026;
const DEMO_PHONE_PREFIX: &str = "+84399";

pub struct DemoOptions<'a> {
    pub tenant_slug: &'a str,
    /// Password of the demo manager login (from `APP_NEW_USER_PASSWORD`, never argv).
    pub manager_password: &'a str,
    /// Seed another batch although demo customers exist (new phone block, same staff and plans).
    pub force_again: bool,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DemoReport {
    pub run: u32,
    pub catalog_seeded: bool,
    pub plans_created: usize,
    pub staff_added: usize,
    pub customers: usize,
    pub health_notes: usize,
    pub opening_balances: usize,
    pub package_sales: usize,
    pub discounted_sales: usize,
    pub visits: usize,
    pub visits_today: usize,
    pub voided_sales: usize,
    pub voided_visits: usize,
    pub cancelled_with_refund: usize,
}

/// Seeds the demo spa. `owner` is the schema-owner pool (provisioning, as the other CLI commands);
/// `app` is the `app_rw` pool every use case runs on, with RLS.
pub async fn seed(
    owner: &PgPool,
    app: &PgPool,
    cipher: &FieldCipher,
    options: &DemoOptions<'_>,
) -> anyhow::Result<DemoReport> {
    let slug = options.tenant_slug;
    let tenant = TenantId::from_uuid(
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM app.tenant WHERE slug = $1")
            .bind(slug)
            .fetch_optional(owner)
            .await?
            .with_context(|| format!("no spa with slug {slug:?}"))?,
    );
    let run = next_run(owner, tenant, options.force_again).await?;
    let mut report = DemoReport {
        run,
        ..DemoReport::default()
    };

    let owner_actor = member_actor(app, owner_of(owner, tenant).await?, tenant).await?;
    report.catalog_seeded = ensure_catalog(owner, tenant, slug).await?;
    let catalog = Catalog::load(app, &owner_actor).await?;
    report.plans_created = ensure_plans(app, &owner_actor, &catalog).await?;
    let catalog = Catalog::load(app, &owner_actor).await?;

    let manager_user = identity::provisioning::add_user(
        owner,
        identity::provisioning::NewUser {
            tenant_slug: slug,
            phone: data::MANAGER_PHONE,
            full_name: data::MANAGER_NAME,
            password: options.manager_password,
            role: identity::SystemRole::Manager,
        },
    )
    .await
    .map_err(fail("add the demo manager"))?;
    let manager = member_actor(app, manager_user, tenant).await?;
    let (staff_added, staff) = ensure_staff(owner, app, &manager, slug).await?;
    report.staff_added = staff_added;

    let tz = {
        let mut tx = TenantTx::begin(app, tenant).await.map_err(fail("open"))?;
        let profile = kernel::tenant::profile(&mut tx)
            .await
            .map_err(fail("read the spa"))?;
        tx.commit().await.map_err(fail("commit"))?;
        profile.timezone
    };
    let mut spa = Spa {
        app,
        cipher,
        owner: owner_actor,
        manager,
        catalog,
        staff,
        tz,
        today: kernel::BusinessDate::of_instant(Utc::now(), tz).as_naive(),
        rng: Rng::new(SEED ^ u64::from(run)),
        customers: Vec::new(),
        packages: Vec::new(),
        report,
    };
    spa.create_customers(run).await?;
    spa.import_paper_books(run).await?;
    spa.back_fill_history().await?;
    spa.run_today().await?;
    spa.cancel_one_with_refund().await?;
    Ok(spa.report)
}

/// Maps a use-case error to a readable failure naming the step.
fn fail(step: &'static str) -> impl FnOnce(AppError) -> anyhow::Error {
    move |e| anyhow!("{step}: {} ({e:?})", e.code())
}

fn idempotency() -> (Uuid, [u8; 32]) {
    let key = Uuid::now_v7();
    (key, hash_request(key.as_bytes()))
}

/// 0 for the first demo batch; refuses a second unless `force_again`.
async fn next_run(owner: &PgPool, tenant: TenantId, force_again: bool) -> anyhow::Result<u32> {
    let latest: Option<String> = sqlx::query_scalar(
        "SELECT max(substring(phone_e164 FROM 7 FOR 2)) FROM app.customer
         WHERE tenant_id = $1 AND phone_e164 LIKE $2",
    )
    .bind(tenant.as_uuid())
    .bind(format!("{DEMO_PHONE_PREFIX}%"))
    .fetch_one(owner)
    .await?;
    match latest {
        None => Ok(0),
        Some(_) if !force_again => bail!(
            "this spa already has demo data (customers with phones 0399…); \
             nothing was changed. Pass --force-again to add another batch."
        ),
        Some(run) => {
            let next = run.parse::<u32>().context("demo phone block")? + 1;
            anyhow::ensure!(next < 100, "the demo phone block is full (100 runs)");
            Ok(next)
        }
    }
}

/// `0399 RR CCCC` in national form: run `RR`, customer `CCCC` (1-based).
fn demo_phone(run: u32, customer: usize) -> String {
    format!("0399{run:02}{:04}", customer + 1)
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
         (refunds and the paper-book import act as the owner)",
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

struct Catalog {
    services: HashMap<String, (Uuid, catalog::ServicePrices)>,
    plans: HashMap<String, Uuid>,
}

impl Catalog {
    async fn load(app: &PgPool, actor: &Actor) -> anyhow::Result<Self> {
        let services = catalog::service::services(app, actor, true)
            .await
            .map_err(fail("list services"))?
            .into_iter()
            .map(|s| (s.name, (s.id, s.prices)))
            .collect();
        let plans = catalog::service::package_plans(app, actor, true)
            .await
            .map_err(fail("list package plans"))?
            .into_iter()
            .map(|p| (p.name, p.id))
            .collect();
        Ok(Self { services, plans })
    }

    fn service(&self, name: &str) -> anyhow::Result<Uuid> {
        self.services.get(name).map(|(id, _)| *id).with_context(|| {
            format!("service {name:?} is not on sale: the demo expects the printed menu")
        })
    }

    fn plan(&self, name: &str) -> anyhow::Result<Uuid> {
        self.plans
            .get(name)
            .copied()
            .with_context(|| format!("package plan {name:?} missing"))
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

async fn ensure_plans(app: &PgPool, owner: &Actor, catalog: &Catalog) -> anyhow::Result<usize> {
    let mut created = 0;
    for plan in PLANS {
        if catalog.plans.contains_key(plan.name) {
            continue;
        }
        let items = plan
            .items
            .iter()
            .map(|i| {
                Ok(catalog::commands::PlanItemInput {
                    service_id: catalog.service(i.service)?,
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

/// Active staff by name, adding the demo technicians and the manager's staff entry when missing.
async fn ensure_staff(
    owner: &PgPool,
    app: &PgPool,
    manager: &Actor,
    slug: &str,
) -> anyhow::Result<(usize, Staff)> {
    let active = |list: Vec<org::service::Staff>| -> HashMap<String, Uuid> {
        list.into_iter()
            .map(|s| (s.full_name, s.id))
            .collect::<HashMap<_, _>>()
    };
    let existing = active(
        org::service::list(app, manager, Some(org::StaffStatus::Active))
            .await
            .map_err(fail("list staff"))?,
    );
    let wanted = data::TECHNICIANS
        .iter()
        .map(|n| (*n, data::TECHNICIAN_POSITION))
        .chain([(data::MANAGER_NAME, data::MANAGER_POSITION)]);
    let mut added = 0;
    for (name, position) in wanted {
        if !existing.contains_key(name) {
            org::provisioning::add_staff(owner, slug, name, position)
                .await
                .map_err(fail("add staff"))?;
            added += 1;
        }
    }
    let all = active(
        org::service::list(app, manager, Some(org::StaffStatus::Active))
            .await
            .map_err(fail("list staff"))?,
    );
    let technicians = data::TECHNICIANS
        .iter()
        .map(|n| all.get(*n).copied().context("technician vanished"))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let closer = all
        .get(data::MANAGER_NAME)
        .copied()
        .context("manager staff vanished")?;
    Ok((
        added,
        Staff {
            technicians,
            closer,
        },
    ))
}

struct Staff {
    technicians: Vec<Uuid>,
    /// The manager's staff entry, who closes most sales (D9).
    closer: Uuid,
}

struct Customer {
    id: Uuid,
    sex: Sex,
    gender_recorded: bool,
}

struct Item {
    id: Uuid,
    service_id: Uuid,
    remaining: u32,
}

/// What the seeder knows about a package, mirrored from the use-case responses.
struct Package {
    id: Uuid,
    customer: usize,
    items: Vec<Item>,
    expires_on: Option<NaiveDate>,
    sold_on: NaiveDate,
    sold_price: i64,
    sessions_total: u32,
    from_sale: bool,
    cancelled: bool,
    /// Sessions the random visits leave untouched (the "expiring soon" package keeps some).
    keep: u32,
}

impl Package {
    fn remaining(&self) -> u32 {
        self.items.iter().map(|i| i.remaining).sum()
    }

    fn usable_on(&self, day: NaiveDate) -> bool {
        !self.cancelled
            && self.sold_on <= day
            && self.expires_on.is_none_or(|e| e >= day)
            && self.remaining() > self.keep
    }
}

#[derive(Deserialize)]
struct PackageJson {
    id: Uuid,
    sold_price: i64,
    expires_on: Option<NaiveDate>,
    starts_on: NaiveDate,
    sale_id: Option<Uuid>,
    items: Vec<ItemJson>,
}

#[derive(Deserialize)]
struct ItemJson {
    id: Uuid,
    service_id: Uuid,
    sessions_total: i32,
    sessions_remaining: i32,
}

fn packages_in(body: &Value, customer: usize) -> anyhow::Result<Vec<Package>> {
    let list: Vec<PackageJson> = serde_json::from_value(body["customer_packages"].clone())
        .context("customer_packages in the response")?;
    Ok(list
        .into_iter()
        .map(|p| {
            let as_u32 = |v: i32| u32::try_from(v).unwrap_or(0);
            Package {
                id: p.id,
                customer,
                sessions_total: p.items.iter().map(|i| as_u32(i.sessions_total)).sum(),
                items: p
                    .items
                    .iter()
                    .map(|i| Item {
                        id: i.id,
                        service_id: i.service_id,
                        remaining: as_u32(i.sessions_remaining),
                    })
                    .collect(),
                expires_on: p.expires_on,
                sold_on: p.starts_on,
                sold_price: p.sold_price,
                from_sale: p.sale_id.is_some(),
                cancelled: false,
                keep: 0,
            }
        })
        .collect())
}

/// One line of a visit the seeder is about to record.
struct Line {
    service_id: Uuid,
    /// `Some` = consume this package item; `None` = one-off at the listed price.
    item: Option<(usize, Uuid)>,
    unit_price: i64,
    override_reason: Option<&'static str>,
}

enum TodayEvent {
    Sale(usize),
    Visit,
    WrongSaleVoided(usize),
    WrongVisitVoided,
    ExpiredOverride,
}

struct Spa<'a> {
    app: &'a PgPool,
    cipher: &'a FieldCipher,
    owner: Actor,
    manager: Actor,
    catalog: Catalog,
    staff: Staff,
    tz: Tz,
    today: NaiveDate,
    rng: Rng,
    customers: Vec<Customer>,
    packages: Vec<Package>,
    report: DemoReport,
}

/// Customers bought a package on these days (index into `CUSTOMERS`, days before today).
const PAST_SALES: &[(usize, i64)] = &[
    (2, 20),
    (3, 19),
    (4, 18),
    (5, 18),
    (7, 17),
    (8, 15),
    (9, 15),
    (10, 14),
    (11, 12),
    (12, 11),
    (13, 10),
    (14, 9),
    (15, 8),
    (16, 6),
    (17, 5),
    (18, 4),
    (19, 3),
    (20, 2),
];

impl Spa<'_> {
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

    fn technician(&mut self) -> Uuid {
        let i = self.rng.below(self.staff.technicians.len());
        self.staff.technicians[i]
    }

    async fn create_customers(&mut self, run: u32) -> anyhow::Result<()> {
        for (i, c) in CUSTOMERS.iter().enumerate() {
            let (sex, recorded) = match c.gender {
                Gender::Recorded(s) => (s, true),
                Gender::Unrecorded(s) => (s, false),
            };
            // The first two arrive through the paper-book import (`import_paper_books`).
            let id = if i < 2 {
                Uuid::nil()
            } else {
                let view = crm::service::create(
                    self.app,
                    self.cipher,
                    &self.manager,
                    crm::service::ProfileInput {
                        full_name: c.full_name.to_owned(),
                        phone: demo_phone(run, i),
                        gender: recorded.then_some(crm_gender(sex)),
                        birth_year: c.birth_year,
                        address: c.address.map(str::to_owned),
                        source: c.source.map(str::to_owned),
                        note: None,
                    },
                    c.health_note,
                )
                .await
                .map_err(fail("create a customer"))?;
                self.report.customers += 1;
                self.report.health_notes += usize::from(c.health_note.is_some());
                view.row.id
            };
            self.customers.push(Customer {
                id,
                sex,
                gender_recorded: recorded,
            });
        }
        Ok(())
    }

    /// Three customers carried over from the paper books on go-live day: two new profiles created
    /// by the import itself, one existing. Includes a package expiring in five days and one that
    /// expired three days ago (used once today with the manager's override).
    async fn import_paper_books(&mut self, run: u32) -> anyhow::Result<()> {
        let today = self.today;
        let day = |offset: i64| (today + Duration::days(offset)).to_string();
        let item = |service: &str, purchased: u32, bonus: u32, remaining: u32| {
            Ok::<_, anyhow::Error>(json!({
                "service_id": self.catalog.service(service)?, "sessions_purchased": purchased,
                "sessions_bonus": bonus, "sessions_remaining": remaining,
            }))
        };
        let profile = |i: usize| {
            let c = &CUSTOMERS[i];
            let gender = match c.gender {
                Gender::Recorded(Sex::Female) => Some("female"),
                Gender::Recorded(Sex::Male) => Some("male"),
                Gender::Unrecorded(_) => None,
            };
            json!({
                "full_name": c.full_name, "phone": demo_phone(run, i), "gender": gender,
                "birth_year": c.birth_year, "address": c.address, "source": c.source,
                "health_note": c.health_note,
            })
        };
        let imports = [
            (
                0,
                json!({"customer": profile(0), "packages": [
                    {"package_plan_id": self.catalog.plan(PLANS[0].name)?, "sold_price": 790_000,
                     "starts_on": day(-176), "expires_on": day(5), "gift_note": "Đã nhận dầu gội",
                     "items": [item("Thư Giãn", 10, 2, 6)?]},
                    {"name": "Xông chân 10 buổi", "sold_price": 1_200_000,
                     "starts_on": day(-200), "expires_on": day(-3),
                     "items": [item("Xông chân thảo dược + Massage chân", 10, 0, 3)?]},
                ]}),
            ),
            (
                1,
                json!({"customer": profile(1), "packages": [
                    {"package_plan_id": self.catalog.plan(PLANS[2].name)?, "sold_price": 2_500_000,
                     "starts_on": day(-60), "expires_on": day(305),
                     "items": [item("Thân Tâm An Định", 10, 2, 9)?]},
                ]}),
            ),
            (
                6,
                json!({"customer_id": self.customers[6].id, "packages": [
                    {"name": "Cổ vai gáy 10 buổi", "sold_price": 2_000_000,
                     "starts_on": day(-90),
                     "items": [item("An Kiên Lạc", 10, 0, 6)?]},
                ]}),
            ),
        ];
        for (customer, body) in imports {
            let input: packages::opening_balance::OpeningBalanceCreate =
                serde_json::from_value(body).context("opening balance body")?;
            let (key, hash) = idempotency();
            let (_, reply) = packages::opening_balance::import(
                self.app,
                self.cipher,
                &self.owner,
                key,
                hash,
                &input,
            )
            .await
            .map_err(fail("import a paper-book package"))?;
            if customer < 2 {
                self.customers[customer].id = serde_json::from_value(reply["customer_id"].clone())
                    .context("imported customer id")?;
                self.report.customers += 1;
                self.report.health_notes += usize::from(CUSTOMERS[customer].health_note.is_some());
            }
            let mut imported = packages_in(&reply, customer)?;
            for p in &mut imported {
                // Imported on go-live day, three weeks ago: usable from then on.
                p.sold_on = today - Duration::days(HISTORY_DAYS);
                if p.expires_on.is_some_and(|e| e <= today + Duration::days(7)) {
                    p.keep = 2; // stays "expiring soon" with sessions left
                }
            }
            self.packages.extend(imported);
            self.report.opening_balances += 1;
        }
        Ok(())
    }

    async fn sell(
        &mut self,
        customer: usize,
        plan: usize,
        at: DateTime<Utc>,
    ) -> anyhow::Result<Uuid> {
        let plan = &PLANS[plan % PLANS.len()];
        let discount = if self.rng.chance(30) {
            let pct = if self.rng.chance(50) { 5 } else { 10 };
            let reason = *self.rng.pick(data::DISCOUNT_REASONS).context("reasons")?;
            Some((plan.price * pct / 100 / 1000 * 1000, reason))
        } else {
            None
        };
        let total = plan.price - discount.map_or(0, |(amount, _)| amount);
        let closer = if self.rng.chance(70) {
            self.staff.closer
        } else {
            self.technician()
        };
        let input = SaleCreate {
            customer_id: self.customers[customer].id,
            closer_staff_id: closer,
            lines: vec![SaleLineCreate {
                package_plan_id: self.catalog.plan(plan.name)?,
                gift_note: None,
            }],
            discount: discount.map(|(amount, reason)| DiscountCreate {
                amount,
                reason: Some(reason.to_owned()),
            }),
            payments: self.payments(total),
        };
        let (key, hash) = idempotency();
        let (_, body) =
            sales::service::create_sale_as_of(self.app, &self.manager, key, hash, &input, at)
                .await
                .map_err(fail("sell a package"))?;
        self.packages.extend(packages_in(&body, customer)?);
        self.report.package_sales += 1;
        self.report.discounted_sales += usize::from(discount.is_some());
        serde_json::from_value(body["id"].clone()).context("sale id")
    }

    /// Cash, transfer, or both (about 55 / 37 / 8), always the exact amount due (R1 collects in full).
    fn payments(&mut self, total: i64) -> Vec<PaymentCreate> {
        let pay = |method, amount| PaymentCreate {
            method,
            amount,
            reference: None,
        };
        if total <= 0 {
            return Vec::new();
        }
        match self.rng.below(100) {
            0..55 => vec![pay(PaymentMethodDto::Cash, total)],
            55..92 => vec![pay(PaymentMethodDto::BankTransfer, total)],
            _ if total >= 100_000 => {
                let cash = total / 2 / 10_000 * 10_000;
                vec![
                    pay(PaymentMethodDto::Cash, cash),
                    pay(PaymentMethodDto::BankTransfer, total - cash),
                ]
            }
            _ => vec![pay(PaymentMethodDto::Cash, total)],
        }
    }

    /// A package line when the customer has a usable package, else a one-off service; sometimes
    /// an extra paid on the spot.
    fn plan_visit(&mut self, customer: usize, day: NaiveDate) -> anyhow::Result<Vec<Line>> {
        let sex = self.customers[customer].sex;
        let usable: Vec<usize> = (0..self.packages.len())
            .filter(|&p| self.packages[p].customer == customer && self.packages[p].usable_on(day))
            .collect();
        let mut lines = Vec::new();
        if let Some(&p) = self.rng.pick(&usable) {
            let items: Vec<(Uuid, Uuid)> = self.packages[p]
                .items
                .iter()
                .filter(|i| i.remaining > 0)
                .map(|i| (i.id, i.service_id))
                .collect();
            let &(item, service_id) = self.rng.pick(&items).context("usable item")?;
            lines.push(Line {
                service_id,
                item: Some((p, item)),
                unit_price: 0,
                override_reason: None,
            });
        } else {
            let name = *self.rng.pick(data::ONE_OFF_SERVICES).context("services")?;
            let service_id = self.catalog.service(name)?;
            lines.push(Line {
                service_id,
                item: None,
                unit_price: self.catalog.price(service_id, sex)?,
                override_reason: None,
            });
        }
        if self.rng.chance(25) {
            let name = *self.rng.pick(data::ADD_ONS).context("add-ons")?;
            let service_id = self.catalog.service(name)?;
            lines.push(Line {
                service_id,
                item: None,
                unit_price: self.catalog.price(service_id, sex)?,
                override_reason: None,
            });
        }
        Ok(lines)
    }

    /// Records a visit as the manager at `at` (its business day is the day of `at`) and mirrors
    /// the sessions it took. Returns the visit id.
    async fn visit(
        &mut self,
        customer: usize,
        lines: Vec<Line>,
        at: DateTime<Utc>,
    ) -> anyhow::Result<Uuid> {
        let (customer_id, sex, recorded) = {
            let c = &self.customers[customer];
            (c.id, c.sex, c.gender_recorded)
        };
        let variant = (!recorded).then_some(match sex {
            Sex::Female => catalog::PriceVariant::Female,
            Sex::Male => catalog::PriceVariant::Male,
        });
        let total: i64 = lines.iter().map(|l| l.unit_price).sum();
        let mut wire = Vec::with_capacity(lines.len());
        for l in &lines {
            wire.push(VisitLineCreate {
                service_id: l.service_id,
                source: if l.item.is_some() {
                    VisitLineSourceDto::Package
                } else {
                    VisitLineSourceDto::OneOff
                },
                customer_package_item_id: l.item.map(|(_, item)| item),
                staff_id: Some(self.technician()),
                qty: None,
                price_variant: if l.item.is_some() { None } else { variant },
                expiry_override_reason: l.override_reason.map(str::to_owned),
            });
        }
        let note = if self.rng.chance(15) {
            self.rng.pick(data::VISIT_NOTES).map(|n| (*n).to_owned())
        } else {
            None
        };
        let input = VisitCreate {
            customer_id,
            check_in_at: None,
            discount: None,
            note,
            lines: wire,
            payments: self.payments(total),
        };
        let (key, hash) = idempotency();
        let (_, body) = operations::service::record_as_of(
            self.app,
            self.cipher,
            &self.manager,
            key,
            hash,
            &input,
            at,
        )
        .await
        .map_err(fail("record a visit"))?;
        for (p, item) in lines.iter().filter_map(|l| l.item) {
            if let Some(i) = self.packages[p].items.iter_mut().find(|i| i.id == item) {
                i.remaining = i.remaining.saturating_sub(1);
            }
        }
        self.report.visits += 1;
        if kernel::BusinessDate::of_instant(at, self.tz).as_naive() == self.today {
            self.report.visits_today += 1;
        }
        serde_json::from_value(body["id"].clone()).context("visit id")
    }

    /// Customers for one day's visits: those with a usable package come three times as often.
    fn customers_for(&mut self, day: NaiveDate, count: usize) -> Vec<usize> {
        let mut pool = Vec::new();
        for c in 0..self.customers.len() {
            let has_package = self
                .packages
                .iter()
                .any(|p| p.customer == c && p.usable_on(day));
            for _ in 0..if has_package { 3 } else { 1 } {
                pool.push(c);
            }
        }
        let mut chosen = Vec::new();
        let mut seen = HashSet::new();
        while chosen.len() < count && seen.len() < self.customers.len() {
            let c = pool[self.rng.below(pool.len())];
            if seen.insert(c) {
                chosen.push(c);
            }
        }
        chosen
    }

    async fn back_fill_history(&mut self) -> anyhow::Result<()> {
        for offset in (1..=HISTORY_DAYS).rev() {
            let day = self.today - Duration::days(offset);
            let sales: Vec<usize> = PAST_SALES
                .iter()
                .filter(|(_, days_ago)| *days_ago == offset)
                .map(|(c, _)| *c)
                .collect();
            for (k, customer) in sales.into_iter().enumerate() {
                let at = self.at(day, 8 * 60 + 15 + u32::try_from(k)?)?;
                self.sell(customer, customer, at).await?;
            }
            let count = 4 + self.rng.below(4);
            let mut minutes: Vec<u32> = (0..count)
                .map(|_| 8 * 60 + 30 + u32::try_from(self.rng.below(12 * 60)).unwrap_or(0))
                .collect();
            minutes.sort_unstable();
            for (customer, minute) in self.customers_for(day, count).into_iter().zip(minutes) {
                let lines = self.plan_visit(customer, day)?;
                let at = self.at(day, minute)?;
                self.visit(customer, lines, at).await?;
            }
        }
        Ok(())
    }

    /// Today: a busy front desk up to now — visits, two sales, a sale voided and sold again, a
    /// visit voided, and an expired package used with the manager's override.
    async fn run_today(&mut self) -> anyhow::Result<()> {
        use TodayEvent::{ExpiredOverride, Sale, Visit, WrongSaleVoided, WrongVisitVoided};
        let events = [
            Sale(21),
            Visit,
            Visit,
            ExpiredOverride,
            Visit,
            WrongSaleVoided(23),
            Visit,
            Visit,
            Sale(22),
            Visit,
            WrongVisitVoided,
            Visit,
            Visit,
            Visit,
            Visit,
        ];
        let times = self.times_today(events.len())?;
        let visitors = self.customers_for(self.today, 11);
        let mut visitors = visitors.into_iter().filter(|c| *c != 0);
        for (event, at) in events.into_iter().zip(times) {
            match event {
                Sale(customer) => {
                    self.sell(customer, customer, at).await?;
                }
                Visit => {
                    if let Some(customer) = visitors.next() {
                        let lines = self.plan_visit(customer, self.today)?;
                        self.visit(customer, lines, at).await?;
                    }
                }
                ExpiredOverride => self.expired_override(at).await?,
                WrongSaleVoided(customer) => self.wrong_sale_voided(customer, at).await?,
                WrongVisitVoided => self.wrong_visit_voided(at).await?,
            }
        }
        Ok(())
    }

    /// `n` increasing instants from 08:00 (or earlier, when it is early now) up to a minute ago,
    /// roughly evenly spaced: all on today's business day and never in the future.
    fn times_today(&mut self, n: usize) -> anyhow::Result<Vec<DateTime<Utc>>> {
        let midnight = self.at(self.today, 0)?;
        let end = (Utc::now() - Duration::minutes(1)).max(midnight);
        let mut start = self.at(self.today, 8 * 60)?;
        if end - start < Duration::minutes(30) {
            start = (end - Duration::minutes(30)).max(midnight);
        }
        let step = (end - start).num_seconds() / i64::try_from(n.max(1))?;
        let mut times = Vec::with_capacity(n);
        for i in 1..=i64::try_from(n)? {
            // Each instant falls somewhere in the second half of its own slot.
            let jitter = i64::try_from(self.rng.below(usize::try_from(step / 2).unwrap_or(0)))?;
            times.push(start + Duration::seconds(step * i - jitter));
        }
        Ok(times)
    }

    async fn expired_override(&mut self, at: DateTime<Utc>) -> anyhow::Result<()> {
        let today = self.today;
        let Some((p, item, service_id)) = self.packages.iter().enumerate().find_map(|(p, pkg)| {
            let expired = pkg.expires_on.is_some_and(|e| e < today) && !pkg.cancelled;
            let item = pkg.items.iter().find(|i| i.remaining > 0)?;
            expired.then_some((p, item.id, item.service_id))
        }) else {
            return Ok(());
        };
        let customer = self.packages[p].customer;
        let lines = vec![Line {
            service_id,
            item: Some((p, item)),
            unit_price: 0,
            override_reason: Some("Khách đi công tác nên quá hạn, chủ spa đồng ý cho dùng tiếp"),
        }];
        self.visit(customer, lines, at).await?;
        Ok(())
    }

    async fn wrong_sale_voided(
        &mut self,
        customer: usize,
        at: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        let first_package = self.packages.len();
        let sale = self.sell(customer, 0, at).await?;
        let (key, hash) = idempotency();
        sales::voids::void_sale(
            self.app,
            &self.manager,
            key,
            hash,
            sale,
            Some("Chọn nhầm gói, khách lấy gói khác"),
        )
        .await
        .map_err(fail("void a sale"))?;
        for p in &mut self.packages[first_package..] {
            p.cancelled = true;
        }
        self.report.voided_sales += 1;
        self.sell(customer, 2, at).await?;
        Ok(())
    }

    async fn wrong_visit_voided(&mut self, at: DateTime<Utc>) -> anyhow::Result<()> {
        let today = self.today;
        let Some(customer) = (2..self.customers.len()).find(|c| {
            self.packages
                .iter()
                .any(|p| p.customer == *c && p.usable_on(today))
        }) else {
            return Ok(());
        };
        let lines = self.plan_visit(customer, today)?;
        let consumed: Vec<(usize, Uuid)> = lines.iter().filter_map(|l| l.item).collect();
        let visit = self.visit(customer, lines, at).await?;
        let (key, hash) = idempotency();
        operations::service::void(
            self.app,
            self.cipher,
            &self.manager,
            key,
            hash,
            visit,
            Some("Ghi nhầm khách, người làm là khách khác"),
        )
        .await
        .map_err(fail("void a visit"))?;
        for (p, item) in consumed {
            if let Some(i) = self.packages[p].items.iter_mut().find(|i| i.id == item) {
                i.remaining += 1;
            }
        }
        self.report.voided_visits += 1;
        Ok(())
    }

    /// The owner cancels a part-used package sold earlier and refunds the unused share in cash.
    async fn cancel_one_with_refund(&mut self) -> anyhow::Result<()> {
        let today = self.today;
        let Some(p) = self.packages.iter().position(|p| {
            p.from_sale
                && !p.cancelled
                && p.sold_on < today
                && p.remaining() >= 2
                && p.remaining() < p.sessions_total
        }) else {
            return Ok(());
        };
        let package = &self.packages[p];
        let unused = i64::from(package.remaining());
        let total = i64::from(package.sessions_total.max(1));
        let refund = package.sold_price * unused / total / 10_000 * 10_000;
        let input: packages::lifecycle::PackageCancel = serde_json::from_value(json!({
            "reason": "Khách chuyển công tác vào TP.HCM, hoàn tiền các buổi chưa dùng",
            "refund_amount": refund, "refund_method": "cash",
        }))
        .context("cancel body")?;
        let (key, hash) = idempotency();
        packages::lifecycle::cancel(self.app, &self.owner, key, hash, package.id, &input)
            .await
            .map_err(fail("cancel a package with refund"))?;
        self.packages[p].cancelled = true;
        self.report.cancelled_with_refund += 1;
        Ok(())
    }
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
    fn demo_phones_are_valid_distinct_and_in_the_reserved_block() {
        let mut seen = HashSet::new();
        for run in [0, 1, 99] {
            for i in 0..CUSTOMERS.len() {
                let phone = kernel::PhoneE164::parse(&demo_phone(run, i));
                let e164 = phone.map(|p| p.as_str().to_owned()).unwrap_or_default();
                assert!(e164.starts_with(DEMO_PHONE_PREFIX), "{e164}");
                assert_eq!(&e164[6..8], format!("{run:02}"));
                assert!(seen.insert(e164));
            }
        }
    }

    #[test]
    fn every_past_sale_is_inside_the_history_and_unique_per_customer() {
        let mut customers = HashSet::new();
        for (c, days_ago) in PAST_SALES {
            assert!((1..=HISTORY_DAYS).contains(days_ago));
            assert!(*c >= 2 && *c < CUSTOMERS.len() && *c != 6);
            assert!(customers.insert(*c));
        }
    }
}
