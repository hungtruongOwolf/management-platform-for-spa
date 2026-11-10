//! One customer's year at the front desk, replayed in time order through the desk's use cases:
//! visits (package sessions, one-off services, add-ons, discounts, session notes), package sales
//! at the desk or inside a visit, the odd cancelled or corrected visit, and calls to a customer
//! who stopped coming. A customer's records depend only on their own earlier ones (their
//! packages), so different customers run side by side; each has its own random stream, so the
//! result does not depend on the interleaving.

use super::data::{self, PLANS, Sex};
use super::people::{Person, Segment};
use super::plan::{Script, Visit};
use super::rng::Rng;
use super::run::retry;
use super::{Ctx, fail, idempotency};
use anyhow::{Context, anyhow};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use crm::follow_up::{Channel, FollowUpInput, Outcome};
use operations::dto::{
    VisitCreate, VisitLineCreate, VisitLineSourceDto, VisitLineUpdate, VisitPackageSaleCreate,
    VisitUpdate,
};
use sales::dto::{
    DiscountCreate, PaymentCreate, PaymentMethodDto, SaleCreate, SaleLineCreate,
    ServiceDiscountCreate,
};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Item {
    pub id: Uuid,
    pub service_id: Uuid,
    pub remaining: u32,
}

/// What the seeder knows about a package, mirrored from the use-case responses.
#[derive(Debug, Clone)]
pub struct Package {
    pub id: Uuid,
    pub customer: usize,
    pub items: Vec<Item>,
    pub starts_on: NaiveDate,
    pub expires_on: Option<NaiveDate>,
    pub sold_price: i64,
    pub sessions_total: u32,
    pub from_sale: bool,
    pub cancelled: bool,
    /// Sessions the history leaves untouched (a paper-book package that stays "expiring soon").
    pub keep: u32,
}

impl Package {
    pub fn remaining(&self) -> u32 {
        self.items.iter().map(|i| i.remaining).sum()
    }

    pub fn usable_on(&self, day: NaiveDate) -> bool {
        !self.cancelled
            && self.starts_on <= day
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

/// The packages of a sale or opening-balance reply (`customer_packages`).
pub fn packages_in(body: &Value, customer: usize) -> anyhow::Result<Vec<Package>> {
    let list: Vec<PackageJson> = serde_json::from_value(body["customer_packages"].clone())
        .context("customer_packages in the response")?;
    let as_u32 = |v: i32| u32::try_from(v).unwrap_or(0);
    Ok(list
        .into_iter()
        .map(|p| Package {
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
            starts_on: p.starts_on,
            sold_price: p.sold_price,
            from_sale: p.sale_id.is_some(),
            cancelled: false,
            keep: 0,
        })
        .collect())
}

/// What one customer's chain did (summed over all customers for the report).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Tally {
    pub visits: usize,
    pub visits_today: usize,
    pub package_lines: usize,
    pub one_off_lines: usize,
    pub desk_package_sales: usize,
    pub upsells_in_visit: usize,
    pub discounted_package_sales: usize,
    pub discounted_visits: usize,
    pub session_notes: usize,
    pub companions: usize,
    pub referrals: usize,
    pub voided_visits: usize,
    pub corrected_visits: usize,
    pub edited_visits: usize,
    pub voided_sales: usize,
    pub expired_overrides: usize,
    pub follow_ups: usize,
    pub statements_in_visits: u64,
}

impl Tally {
    pub fn add(&mut self, o: &Self) {
        self.visits += o.visits;
        self.visits_today += o.visits_today;
        self.package_lines += o.package_lines;
        self.one_off_lines += o.one_off_lines;
        self.desk_package_sales += o.desk_package_sales;
        self.upsells_in_visit += o.upsells_in_visit;
        self.discounted_package_sales += o.discounted_package_sales;
        self.discounted_visits += o.discounted_visits;
        self.session_notes += o.session_notes;
        self.companions += o.companions;
        self.referrals += o.referrals;
        self.voided_visits += o.voided_visits;
        self.corrected_visits += o.corrected_visits;
        self.edited_visits += o.edited_visits;
        self.voided_sales += o.voided_sales;
        self.expired_overrides += o.expired_overrides;
        self.follow_ups += o.follow_ups;
        self.statements_in_visits += o.statements_in_visits;
    }
}

#[derive(Clone)]
enum Source {
    Item {
        package: usize,
        item: Uuid,
    },
    OneOff,
    /// Today's session of the package sold in this visit under `UPSELL_REF`.
    NewPackage,
}

#[derive(Clone)]
struct Line {
    service_id: Uuid,
    source: Source,
    unit_price: i64,
    staff: Uuid,
    referrer: Option<Uuid>,
    override_reason: Option<&'static str>,
}

const UPSELL_REF: &str = "goi-moi";

/// One customer's chain of desk records.
pub struct Chain<'a> {
    ctx: &'a Ctx<'a>,
    c: usize,
    customer: Uuid,
    person: &'a Person,
    rng: Rng,
    pub packages: Vec<Package>,
    pub tally: Tally,
}

impl<'a> Chain<'a> {
    pub fn new(
        ctx: &'a Ctx<'a>,
        c: usize,
        customer: Uuid,
        person: &'a Person,
        packages: Vec<Package>,
    ) -> Self {
        Self {
            ctx,
            c,
            customer,
            person,
            rng: Rng::new(
                super::SEED ^ (u64::try_from(c).unwrap_or(0) + 1).wrapping_mul(0x9E37_79B9),
            ),
            packages,
            tally: Tally::default(),
        }
    }

    pub async fn run(mut self, visits: &[Visit]) -> anyhow::Result<(Vec<Package>, Tally)> {
        for v in visits {
            let (out, statements) = crate::server_timing::count_statements(self.visit(*v)).await;
            out?;
            self.tally.statements_in_visits += u64::from(statements);
        }
        if self.person.segment == Segment::Lapsed && self.person.phone.is_some() {
            self.follow_ups().await?;
        }
        Ok((self.packages, self.tally))
    }

    fn technician(&mut self, day: NaiveDate) -> Uuid {
        let on = self.ctx.attendance.technicians_on(&self.ctx.roster, day);
        let m = on[self.rng.below(on.len())];
        self.ctx.staff.ids[m]
    }

    /// Another technician at work that day than `than`, if any.
    fn other_technician(&mut self, day: NaiveDate, than: Uuid) -> Option<Uuid> {
        let others: Vec<Uuid> = self
            .ctx
            .attendance
            .technicians_on(&self.ctx.roster, day)
            .into_iter()
            .map(|m| self.ctx.staff.ids[m])
            .filter(|id| *id != than)
            .collect();
        self.rng.pick(&others).copied()
    }

    fn sex(&self) -> Sex {
        self.person.sex
    }

    fn one_off(&mut self, day: NaiveDate, names: &[&str]) -> anyhow::Result<Line> {
        let name = *self.rng.pick(names).context("services")?;
        let service_id = self.ctx.catalog.service(name)?;
        let staff = self.technician(day);
        let referrer = if self.rng.chance(8) {
            Some(self.ctx.staff.ids[self.rng.below(self.ctx.staff.ids.len())])
                .filter(|r| *r != staff && self.ctx.roster_works(*r, day))
        } else {
            None
        };
        Ok(Line {
            service_id,
            source: Source::OneOff,
            unit_price: self.ctx.catalog.price(service_id, self.sex())?,
            staff,
            referrer,
            override_reason: None,
        })
    }

    /// A session of the usable package that expires first.
    fn package_line(&mut self, day: NaiveDate) -> Option<Line> {
        let p = (0..self.packages.len())
            .filter(|&p| self.packages[p].usable_on(day))
            .min_by_key(|&p| (self.packages[p].expires_on.unwrap_or(NaiveDate::MAX), p))?;
        let items: Vec<(Uuid, Uuid)> = self.packages[p]
            .items
            .iter()
            .filter(|i| i.remaining > 0)
            .map(|i| (i.id, i.service_id))
            .collect();
        let &(item, service_id) = self.rng.pick(&items)?;
        Some(Line {
            service_id,
            source: Source::Item { package: p, item },
            unit_price: 0,
            staff: self.technician(day),
            referrer: None,
            override_reason: None,
        })
    }

    /// A package sold to this customer (the plan's catalog price, sometimes a discount).
    fn package_price(&mut self, plan: usize) -> (Uuid, i64, Option<(i64, &'static str)>) {
        let (id, price) = self.ctx.catalog.plans[plan];
        let discount = if self.rng.chance(30) {
            let pct = if self.rng.chance(50) { 5 } else { 10 };
            let reason = *self
                .rng
                .pick(data::DISCOUNT_REASONS)
                .unwrap_or(&"Khách quen");
            Some((price * pct / 100 / 1000 * 1000, reason))
        } else {
            None
        };
        (id, price, discount)
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

    fn plan_for(&mut self) -> usize {
        self.rng.below(PLANS.len())
    }

    /// Sells a package at the desk (`POST /sales`) at `at`; returns the sale id.
    async fn sell(&mut self, plan: usize, at: DateTime<Utc>, closer: Uuid) -> anyhow::Result<Uuid> {
        let (plan_id, price, discount) = self.package_price(plan);
        let total = price - discount.map_or(0, |(a, _)| a);
        let input = SaleCreate {
            customer_id: self.customer,
            beneficiary_customer_id: None,
            closer_staff_id: Some(closer),
            lines: vec![SaleLineCreate {
                package_plan_id: plan_id,
                gift_note: None,
            }],
            discount: discount.map(|(amount, reason)| DiscountCreate {
                amount,
                reason: Some(reason.to_owned()),
            }),
            payments: self.payments(total),
            note: None,
        };
        let (key, hash) = idempotency();
        let ctx = self.ctx;
        let (_, body) = retry(|| {
            sales::service::create_sale_as_of(
                ctx.app,
                ctx.cipher,
                &ctx.manager,
                key,
                hash,
                &input,
                at,
            )
        })
        .await
        .map_err(fail("sell a package"))?;
        self.packages.extend(packages_in(&body, self.c)?);
        self.tally.desk_package_sales += 1;
        self.tally.discounted_package_sales += usize::from(discount.is_some());
        serde_json::from_value(body["id"].clone()).context("sale id")
    }

    async fn visit(&mut self, v: Visit) -> anyhow::Result<()> {
        let ctx = self.ctx;
        let at = ctx.at(v.day, v.minute)?;
        let is_today = v.day == ctx.window.today;
        // The desk corrects a past day as the owner (closed-day rights); today as the manager.
        let corrector = if is_today { &ctx.manager } else { &ctx.owner };

        if v.script == Script::WrongSaleVoided {
            let first = self.packages.len();
            let sale = self
                .sell(0, at - Duration::minutes(4), ctx.manager_staff())
                .await?;
            let (key, hash) = idempotency();
            retry(|| {
                sales::voids::void_sale(
                    ctx.app,
                    &ctx.manager,
                    key,
                    hash,
                    sale,
                    Some("Chọn nhầm gói, khách lấy gói khác"),
                )
            })
            .await
            .map_err(fail("void a sale"))?;
            for p in &mut self.packages[first..] {
                p.cancelled = true;
            }
            self.tally.voided_sales += 1;
            self.sell(2, at - Duration::minutes(3), ctx.manager_staff())
                .await?;
        }

        let mut lines = Vec::new();
        let mut upsell = None;
        let has_package = self.packages.iter().any(|p| p.usable_on(v.day));
        if v.script == Script::ExpiredOverride
            && let Some(line) = self.expired_line(v.day)
        {
            lines.push(line);
            self.tally.expired_overrides += 1;
        } else if has_package {
            // Now and then a package holder takes a one-off service instead.
            if self.rng.chance(88)
                && let Some(line) = self.package_line(v.day)
            {
                lines.push(line);
            }
        } else if self.person.buys_packages
            && self.person.full_name.is_some()
            && self.rng.chance(65)
        {
            let plan = self.plan_for();
            let staff = self.technician(v.day);
            let service_id = self.ctx.catalog.plan_service(plan)?;
            if self.rng.chance(50) && v.script == Script::Plain {
                // Upsell inside the visit: today's session comes from the package just sold.
                upsell = Some(plan);
                lines.push(Line {
                    service_id,
                    source: Source::NewPackage,
                    unit_price: 0,
                    staff,
                    referrer: None,
                    override_reason: None,
                });
            } else {
                let closer = if self.rng.chance(70) {
                    ctx.manager_staff()
                } else {
                    staff
                };
                self.sell(plan, at - Duration::minutes(5), closer).await?;
                if let Some(line) = self.package_line(v.day) {
                    lines.push(line);
                }
            }
        }
        if lines.is_empty() {
            lines.push(self.one_off(v.day, data::ONE_OFF_SERVICES)?);
        }
        if self.rng.chance(20) {
            lines.push(self.one_off(v.day, data::ADD_ONS)?);
        }
        let subtotal: i64 = lines.iter().map(|l| l.unit_price).sum();
        let discount = (subtotal >= 100_000 && v.script == Script::Plain && self.rng.chance(5))
            .then(|| {
                (
                    subtotal / 10 / 1000 * 1000,
                    "Khách quen, giảm 10% dịch vụ lẻ",
                )
            });
        let mut sales = Vec::new();
        let mut due = subtotal - discount.map_or(0, |(a, _)| a);
        if let Some(plan) = upsell {
            let (plan_id, price, pkg_discount) = self.package_price(plan);
            due += price - pkg_discount.map_or(0, |(a, _)| a);
            sales.push(VisitPackageSaleCreate {
                package_ref: Some(UPSELL_REF.to_owned()),
                package_plan_id: plan_id,
                closer_staff_id: Some(lines[0].staff),
                gift_note: None,
                discount: pkg_discount.map(|(amount, reason)| DiscountCreate {
                    amount,
                    reason: Some(reason.to_owned()),
                }),
                beneficiary_customer_id: None,
                note: None,
            });
            self.tally.discounted_package_sales += usize::from(pkg_discount.is_some());
        }
        let note = self
            .rng
            .chance(15)
            .then(|| self.rng.pick(data::VISIT_NOTES).map(|n| (*n).to_owned()))
            .flatten();
        let companions: Vec<String> = if self.rng.chance(3) {
            self.rng
                .pick(data::COMPANIONS)
                .map(|c| vec![(*c).to_owned()])
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let input = VisitCreate {
            customer_id: self.customer,
            check_in_at: None,
            discount: discount.map(|(amount, reason)| ServiceDiscountCreate {
                percent: None,
                amount: Some(amount),
                reason: Some(reason.to_owned()),
            }),
            note: note.clone(),
            lines: self.wire(&lines),
            payments: self.payments(due),
            package_sales: sales,
            companions: companions.clone(),
        };
        let (key, hash) = idempotency();
        let (_, body) = retry(|| {
            operations::service::record_as_of(
                ctx.app,
                ctx.cipher,
                &ctx.manager,
                key,
                hash,
                &input,
                at,
            )
        })
        .await
        .map_err(fail("record a visit"))?;
        self.consume(&lines);
        self.tally.visits += 1;
        self.tally.visits_today += usize::from(is_today);
        self.tally.package_lines += lines
            .iter()
            .filter(|l| !matches!(l.source, Source::OneOff))
            .count();
        self.tally.one_off_lines += lines
            .iter()
            .filter(|l| matches!(l.source, Source::OneOff))
            .count();
        self.tally.referrals += lines.iter().filter(|l| l.referrer.is_some()).count();
        self.tally.discounted_visits += usize::from(discount.is_some());
        self.tally.session_notes += usize::from(note.is_some());
        self.tally.companions += usize::from(!companions.is_empty());
        if upsell.is_some() {
            self.tally.upsells_in_visit += 1;
            self.fetch_upsold(&body).await?;
        }

        let visit: Uuid = serde_json::from_value(body["id"].clone()).context("visit id")?;
        let version = body["version"].as_i64().context("visit version")?;
        match v.script {
            Script::Void if upsell.is_none() => {
                let reason = *self.rng.pick(data::VOID_REASONS).context("reasons")?;
                let (key, hash) = idempotency();
                retry(|| {
                    operations::service::void(
                        ctx.app,
                        ctx.cipher,
                        corrector,
                        key,
                        hash,
                        visit,
                        Some(reason),
                    )
                })
                .await
                .map_err(fail("void a visit"))?;
                self.restore(&lines);
                self.tally.voided_visits += 1;
            }
            Script::Correct if upsell.is_none() => {
                self.correct(v.day, visit, version, &lines, corrector)
                    .await?;
            }
            Script::Void | Script::Correct | Script::Edit => {
                self.edit(v.day, visit, version, &body, corrector).await?;
            }
            Script::Plain | Script::ExpiredOverride | Script::WrongSaleVoided => {}
        }
        Ok(())
    }

    fn wire(&self, lines: &[Line]) -> Vec<VisitLineCreate> {
        let variant = (!self.person.gender_recorded).then_some(match self.sex() {
            Sex::Female => catalog::PriceVariant::Female,
            Sex::Male => catalog::PriceVariant::Male,
        });
        lines
            .iter()
            .map(|l| VisitLineCreate {
                service_id: l.service_id,
                source: match l.source {
                    Source::OneOff => VisitLineSourceDto::OneOff,
                    Source::Item { .. } | Source::NewPackage => VisitLineSourceDto::Package,
                },
                customer_package_item_id: match l.source {
                    Source::Item { item, .. } => Some(item),
                    _ => None,
                },
                staff_id: Some(l.staff),
                qty: None,
                price_variant: match l.source {
                    Source::OneOff => variant,
                    _ => None,
                },
                expiry_override_reason: l.override_reason.map(str::to_owned),
                package_ref: matches!(l.source, Source::NewPackage).then(|| UPSELL_REF.to_owned()),
                referrer_staff_id: l.referrer,
            })
            .collect()
    }

    fn consume(&mut self, lines: &[Line]) {
        for l in lines {
            if let Source::Item { package, item } = l.source
                && let Some(i) = self.packages[package]
                    .items
                    .iter_mut()
                    .find(|i| i.id == item)
            {
                i.remaining = i.remaining.saturating_sub(1);
            }
        }
    }

    fn restore(&mut self, lines: &[Line]) {
        for l in lines {
            if let Source::Item { package, item } = l.source
                && let Some(i) = self.packages[package]
                    .items
                    .iter_mut()
                    .find(|i| i.id == item)
            {
                i.remaining += 1;
            }
        }
    }

    /// The packages sold inside a visit, read back with today's session already taken.
    async fn fetch_upsold(&mut self, body: &Value) -> anyhow::Result<()> {
        let ctx = self.ctx;
        let ids: Vec<Uuid> = body["package_sales"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|s| {
                s["customer_package_ids"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
            })
            .filter_map(|id| serde_json::from_value(id).ok())
            .collect();
        for id in ids {
            let p = retry(|| packages::service::get(ctx.app, &ctx.manager, id))
                .await
                .map_err(fail("read an upsold package"))?;
            self.packages.push(Package {
                id: p.id,
                customer: self.c,
                items: p
                    .items
                    .iter()
                    .map(|i| Item {
                        id: i.id,
                        service_id: i.service_id,
                        remaining: u32::try_from(i.sessions_remaining).unwrap_or(0),
                    })
                    .collect(),
                starts_on: p.starts_on,
                expires_on: p.expires_on,
                sold_price: p.sold_price.as_i64(),
                sessions_total: u32::try_from(p.sessions_total).unwrap_or(0),
                from_sale: p.sale_id.is_some(),
                cancelled: false,
                keep: 0,
            });
        }
        Ok(())
    }

    /// Today's session of an expired package, used with the manager's override.
    fn expired_line(&mut self, day: NaiveDate) -> Option<Line> {
        let (p, item, service_id) = self.packages.iter().enumerate().find_map(|(p, pkg)| {
            let expired = pkg.expires_on.is_some_and(|e| e < day) && !pkg.cancelled;
            let item = pkg.items.iter().find(|i| i.remaining > 0)?;
            expired.then_some((p, item.id, item.service_id))
        })?;
        Some(Line {
            service_id,
            source: Source::Item { package: p, item },
            unit_price: 0,
            staff: self.technician(day),
            referrer: None,
            override_reason: Some("Khách đi công tác nên quá hạn, chủ spa đồng ý cho dùng tiếp"),
        })
    }

    /// "Sửa lượt" with money (void + record): the first one-off service was another one, or, on a
    /// visit of package sessions only, an add-on the customer had was not typed in.
    async fn correct(
        &mut self,
        day: NaiveDate,
        visit: Uuid,
        version: i64,
        lines: &[Line],
        actor: &kernel::Actor,
    ) -> anyhow::Result<()> {
        let ctx = self.ctx;
        let mut fixed: Vec<Line> = lines.to_vec();
        match fixed
            .iter()
            .position(|l| matches!(l.source, Source::OneOff))
        {
            Some(k) => {
                let others: Vec<&str> = data::ONE_OFF_SERVICES
                    .iter()
                    .copied()
                    .filter(|n| ctx.catalog.service(n).ok() != Some(fixed[k].service_id))
                    .collect();
                let replacement = self.one_off(day, &others)?;
                fixed[k] = Line {
                    staff: fixed[k].staff,
                    referrer: None,
                    ..replacement
                };
            }
            None => fixed.push(self.one_off(day, data::ADD_ONS)?),
        }
        let total: i64 = fixed.iter().map(|l| l.unit_price).sum();
        let input = VisitCreate {
            customer_id: self.customer,
            check_in_at: None,
            discount: None,
            note: None,
            lines: self.wire(&fixed),
            payments: self.payments(total),
            package_sales: Vec::new(),
            companions: Vec::new(),
        };
        let reason = *self.rng.pick(data::CORRECTION_REASONS).context("reasons")?;
        let (key, hash) = idempotency();
        retry(|| {
            operations::service::correct(
                ctx.app,
                ctx.cipher,
                actor,
                key,
                hash,
                operations::service::Correction {
                    id: visit,
                    version,
                    reason: Some(reason),
                    note: None,
                    visit: &input,
                },
            )
        })
        .await
        .map_err(fail("correct a visit"))?;
        // The void gave the sessions back and the new visit took them again: no change.
        self.tally.corrected_visits += 1;
        Ok(())
    }

    /// "Sửa lượt" without money: the first line was done by another technician.
    async fn edit(
        &mut self,
        day: NaiveDate,
        visit: Uuid,
        version: i64,
        body: &Value,
        actor: &kernel::Actor,
    ) -> anyhow::Result<()> {
        let ctx = self.ctx;
        let line = &body["lines"][0];
        let (Some(line_id), Some(staff)) = (
            line["id"].as_str().and_then(|s| Uuid::parse_str(s).ok()),
            line["staff_id"]
                .as_str()
                .and_then(|s| Uuid::parse_str(s).ok()),
        ) else {
            return Err(anyhow!("visit reply without lines"));
        };
        let Some(other) = self.other_technician(day, staff) else {
            return Ok(());
        };
        let input = VisitUpdate {
            note: None,
            check_in_at: None,
            lines: vec![VisitLineUpdate {
                id: line_id,
                staff_id: other,
            }],
            companions: None,
        };
        retry(|| operations::service::update(ctx.app, ctx.cipher, actor, visit, version, &input))
            .await
            .map_err(fail("edit a visit"))?;
        self.tally.edited_visits += 1;
        Ok(())
    }

    /// The desk called a customer who stopped coming, once or twice in the last weeks, and noted
    /// when to call again (some of those dates are already due).
    async fn follow_ups(&mut self) -> anyhow::Result<()> {
        if !self.rng.chance(70) {
            return Ok(());
        }
        let ctx = self.ctx;
        let today = ctx.window.today;
        let mut contacted = today - Duration::days(18 + self.rng.days(10));
        for _ in 0..1 + self.rng.below(2) {
            let outcome = *self
                .rng
                .pick(&[
                    Outcome::NoAnswer,
                    Outcome::WillCome,
                    Outcome::NotInterested,
                    Outcome::Other,
                ])
                .context("outcomes")?;
            let next = contacted
                + Duration::days(match outcome {
                    Outcome::NoAnswer => 3,
                    Outcome::WillCome => 7,
                    Outcome::NotInterested => 90,
                    Outcome::Other => 14,
                });
            let input = FollowUpInput {
                contacted_on: Some(contacted),
                channel: if self.rng.chance(60) {
                    Channel::Phone
                } else {
                    Channel::Zalo
                },
                outcome,
                note: self
                    .rng
                    .pick(data::FOLLOW_UP_NOTES)
                    .map(|n| (*n).to_owned()),
                next_follow_up_on: Some(Some(next)),
            };
            let (key, hash) = idempotency();
            retry(|| {
                crm::follow_up::record(
                    ctx.app,
                    &ctx.manager,
                    key,
                    hash,
                    self.customer,
                    &input,
                    |f| Ok(serde_json::json!({ "id": f.id })),
                )
            })
            .await
            .map_err(fail("record a follow-up"))?;
            self.tally.follow_ups += 1;
            contacted = (contacted + Duration::days(4 + self.rng.days(6))).min(today);
        }
        Ok(())
    }
}
