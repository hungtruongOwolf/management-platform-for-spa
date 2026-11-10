//! The appointment book: about two bookings a week over the history (most completed into a
//! visit, some no-shows, some cancelled with a reason), today's bookings (one overdue, one
//! arrived, later ones), the next two weeks, and a family booked together. All through the
//! appointments use cases; history uses their `_as_of` forms (booked a few days before, completed
//! on the day). Days are independent of each other, so they run side by side.

use super::data::{self, ONE_OFF_SERVICES, Sex};
use super::people::{PAPER_CUSTOMERS, Person, Segment};
use super::rng::Rng;
use super::run::{each, retry};
use super::{Ctx, fail, idempotency};
use anyhow::Context;
use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use kernel::AppError;
use operations::appointments::dto::{
    AppointmentArrive, AppointmentComplete, AppointmentCreate, AppointmentGroupCreate,
    GroupPersonCreate, GuestCreate,
};
use operations::appointments::service::Outcome;
use operations::dto::{VisitLineCreate, VisitLineSourceDto};
use sales::dto::{PaymentCreate, PaymentMethodDto};
use uuid::Uuid;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CalendarReport {
    pub completed: usize,
    pub no_shows: usize,
    pub cancelled: usize,
    pub today_booked: usize,
    pub today_overdue: usize,
    pub today_arrived: usize,
    pub today_completed: usize,
    pub upcoming: usize,
    pub group_members: usize,
    /// Bookings the rules refused (a technician or customer already busy, outside opening hours).
    pub skipped: usize,
}

impl CalendarReport {
    fn add(&mut self, o: &Self) {
        self.completed += o.completed;
        self.no_shows += o.no_shows;
        self.cancelled += o.cancelled;
        self.today_booked += o.today_booked;
        self.today_overdue += o.today_overdue;
        self.today_arrived += o.today_arrived;
        self.today_completed += o.today_completed;
        self.upcoming += o.upcoming;
        self.group_members += o.group_members;
        self.skipped += o.skipped;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fate {
    Complete,
    NoShow,
    Cancel,
    /// Today, start already passed, nobody marked anything.
    Overdue,
    Arrive,
    /// Booked for later (today or the coming days).
    Stay,
}

#[derive(Debug, Clone)]
struct Booking {
    day: NaiveDate,
    minute: u32,
    customer: Option<usize>,
    /// A newcomer's name and phone instead of a customer.
    guest: Option<(String, String)>,
    service: &'static str,
    with_staff: bool,
    note: Option<&'static str>,
    fate: Fate,
}

/// A slot of the calendar's 30-minute grid between 09:00 and 19:00.
fn slot(rng: &mut Rng) -> u32 {
    9 * 60 + 30 * u32::try_from(rng.below(21)).unwrap_or(0)
}

/// Writes the whole appointment book.
pub async fn write(
    ctx: &Ctx<'_>,
    people: &[Person],
    customer_ids: &[Uuid],
    rng: &mut Rng,
) -> anyhow::Result<CalendarReport> {
    let window = ctx.window;
    let bookable = |c: usize, day: NaiveDate| {
        let p = &people[c];
        c >= PAPER_CUSTOMERS
            && p.segment != Segment::WalkIn
            && p.full_name.is_some()
            && p.first_day <= day
            && day <= p.last_day
    };
    let pick_customer = |rng: &mut Rng, day: NaiveDate| -> Option<usize> {
        let pool: Vec<usize> = (0..people.len()).filter(|&c| bookable(c, day)).collect();
        rng.pick(&pool).copied()
    };
    let mut days: Vec<Vec<Booking>> = Vec::new();

    // History: about two bookings a week; of every seven, five completed, one no-show, one
    // cancelled (a fixed cycle, so a short history has each outcome too).
    let mut booked = 0_usize;
    let mut week = window.start;
    while week + Duration::days(1) < window.today {
        let mut list = Vec::new();
        for _ in 0..2 {
            let day = week + Duration::days(rng.days(6));
            if day >= window.today {
                continue;
            }
            let fate = match booked % 7 {
                3 => Fate::NoShow,
                5 => Fate::Cancel,
                _ => Fate::Complete,
            };
            booked += 1;
            if let Some(c) = pick_customer(rng, day) {
                list.push(Booking {
                    day,
                    minute: slot(rng),
                    customer: Some(c),
                    guest: None,
                    service: rng.pick(ONE_OFF_SERVICES).context("services")?,
                    with_staff: rng.chance(70),
                    note: rng
                        .chance(30)
                        .then(|| *rng.pick(data::APPOINTMENT_NOTES).unwrap_or(&"")),
                    fate,
                });
            }
        }
        list.sort_by_key(|b| (b.day, b.minute));
        days.push(list);
        week += Duration::days(7);
    }

    // Today, around the clock of the run.
    let now_minute = ctx.now_minute();
    let mut today = Vec::new();
    let grid = |m: u32| m / 15 * 15;
    let opening = 9 * 60;
    let closing_start = 19 * 60 + 30;
    let today_slots = [
        (now_minute.checked_sub(150).map(grid), Fate::Complete),
        (now_minute.checked_sub(60).map(grid), Fate::Overdue),
        (Some(grid(now_minute)), Fate::Arrive),
        (Some(grid(now_minute + 45)), Fate::Stay),
        (Some(grid(now_minute + 150)), Fate::Stay),
    ];
    for (minute, fate) in today_slots {
        let Some(minute) = minute.filter(|m| (opening..=closing_start).contains(m)) else {
            continue;
        };
        if let Some(c) = pick_customer(rng, window.today) {
            today.push(Booking {
                day: window.today,
                minute,
                customer: Some(c),
                guest: None,
                service: rng.pick(ONE_OFF_SERVICES).context("services")?,
                with_staff: fate != Fate::Stay || rng.chance(50),
                note: (fate == Fate::Overdue).then_some("Gọi nhắc trước 1 tiếng"),
                fate,
            });
        }
    }
    days.push(today);

    // The next two weeks.
    for ahead in 1..=14 {
        let day = window.today + Duration::days(ahead);
        let mut list = Vec::new();
        for k in 0..3 + rng.below(3) {
            let newcomer = rng.chance(20);
            let guest = newcomer.then(|| {
                (
                    format!("Khách mới {}", 1 + rng.below(90)),
                    format!("039999{:02}{:02}", ahead, k),
                )
            });
            list.push(Booking {
                day,
                minute: slot(rng),
                customer: if newcomer {
                    None
                } else {
                    pick_customer(rng, window.today)
                },
                guest,
                service: rng.pick(ONE_OFF_SERVICES).context("services")?,
                with_staff: rng.chance(60),
                note: rng
                    .chance(30)
                    .then(|| *rng.pick(data::APPOINTMENT_NOTES).unwrap_or(&"")),
                fate: Fate::Stay,
            });
        }
        list.sort_by_key(|b| b.minute);
        days.push(list);
    }

    let reports = each(days, ctx.workers, |list| async move {
        let mut report = CalendarReport::default();
        // A per-day stream so the technician picks do not depend on the interleaving.
        let mut rng = Rng::new(
            super::SEED
                ^ list
                    .first()
                    .map_or(0, |b| u64::try_from(b.day.num_days_from_ce()).unwrap_or(0)),
        );
        for b in list {
            day_booking(ctx, people, customer_ids, &b, &mut rng, &mut report).await?;
        }
        Ok(report)
    })
    .await?;
    let mut report = CalendarReport::default();
    for r in &reports {
        report.add(r);
    }
    report.group_members = family(ctx, people, customer_ids, rng).await?;
    Ok(report)
}

fn refused(e: &AppError) -> bool {
    matches!(
        e,
        AppError::Conflict { .. }
            | AppError::ConflictWith { .. }
            | AppError::Unprocessable { .. }
            | AppError::Validation(_)
    )
}

async fn day_booking(
    ctx: &Ctx<'_>,
    people: &[Person],
    customer_ids: &[Uuid],
    b: &Booking,
    rng: &mut Rng,
    report: &mut CalendarReport,
) -> anyhow::Result<()> {
    let past = b.day < ctx.window.today;
    let starts_at = ctx.at(b.day, b.minute)?;
    let technicians = ctx.technicians_on(b.day);
    let staff = if b.with_staff || b.fate == Fate::Arrive {
        rng.pick(&technicians).copied()
    } else {
        None
    };
    let service_id = ctx.catalog.service(b.service)?;
    let input = AppointmentCreate {
        customer_id: b.customer.map(|c| customer_ids[c]),
        guest: b.guest.as_ref().map(|(name, phone)| GuestCreate {
            full_name: name.clone(),
            phone: Some(phone.clone()),
        }),
        service_ids: vec![service_id],
        staff_id: staff,
        starts_at,
        duration_min: None,
        note: b.note.map(str::to_owned),
        companions: Vec::new(),
        group_with: None,
    };
    // History and today's earlier slots were booked a few days or hours before.
    let booked_at = if past {
        ctx.at(b.day - Duration::days(1 + rng.days(4)), 10 * 60)?
    } else if b.day == ctx.window.today && b.fate != Fate::Stay && b.fate != Fate::Arrive {
        ctx.at(b.day, 7 * 60)?
    } else {
        ctx.now
    };
    let (key, hash) = idempotency();
    let booked = retry(|| {
        operations::appointments::service::book_as_of(
            ctx.app,
            &ctx.manager,
            key,
            hash,
            &input,
            booked_at,
        )
    })
    .await;
    let body = match booked {
        Ok((_, body)) => body,
        Err(e) if refused(&e) => {
            report.skipped += 1;
            return Ok(());
        }
        Err(e) => return Err(fail("book an appointment")(e)),
    };
    let id: Uuid = serde_json::from_value(body["id"].clone()).context("appointment id")?;
    match b.fate {
        Fate::Complete => {
            let c = b.customer.context("a completed booking has a customer")?;
            let serving = staff.or_else(|| rng.pick(&technicians).copied());
            let done_at = if past {
                starts_at + Duration::minutes(70)
            } else {
                ctx.now
            };
            complete(ctx, &people[c], id, service_id, serving, done_at).await?;
            if past {
                report.completed += 1;
            } else {
                report.today_completed += 1;
            }
        }
        Fate::NoShow | Fate::Cancel => {
            let (outcome, reasons, at) = if b.fate == Fate::NoShow {
                (
                    Outcome::NoShow,
                    data::NO_SHOW_REASONS,
                    starts_at + Duration::minutes(40),
                )
            } else {
                (
                    Outcome::Cancelled,
                    data::CANCEL_REASONS,
                    booked_at + Duration::hours(5),
                )
            };
            let reason = *rng.pick(reasons).context("reasons")?;
            retry(|| {
                operations::appointments::service::close_as_of(
                    ctx.app,
                    &ctx.manager,
                    id,
                    outcome,
                    Some(reason),
                    at,
                )
            })
            .await
            .map_err(fail("close an appointment"))?;
            if b.fate == Fate::NoShow {
                report.no_shows += 1;
            } else {
                report.cancelled += 1;
            }
        }
        Fate::Arrive => {
            if let Some(staff) = staff {
                operations::appointments::service::arrive(
                    ctx.app,
                    &ctx.manager,
                    id,
                    &AppointmentArrive { staff_id: staff },
                )
                .await
                .map_err(fail("mark an arrival"))?;
                report.today_arrived += 1;
            }
        }
        Fate::Overdue => report.today_overdue += 1,
        Fate::Stay if b.day == ctx.window.today => report.today_booked += 1,
        Fate::Stay => report.upcoming += 1,
    }
    Ok(())
}

/// The booked service as a one-off visit, paid in cash.
async fn complete(
    ctx: &Ctx<'_>,
    person: &Person,
    id: Uuid,
    service_id: Uuid,
    staff: Option<Uuid>,
    at: DateTime<Utc>,
) -> anyhow::Result<()> {
    let price = ctx.catalog.price(service_id, person.sex)?;
    let variant = (!person.gender_recorded).then_some(match person.sex {
        Sex::Female => catalog::PriceVariant::Female,
        Sex::Male => catalog::PriceVariant::Male,
    });
    let input = || AppointmentComplete {
        check_in_at: None,
        note: None,
        lines: vec![VisitLineCreate {
            service_id,
            source: VisitLineSourceDto::OneOff,
            customer_package_item_id: None,
            staff_id: staff,
            qty: None,
            price_variant: variant,
            expiry_override_reason: None,
            package_ref: None,
            referrer_staff_id: None,
        }],
        payments: vec![PaymentCreate {
            method: PaymentMethodDto::Cash,
            amount: price,
            reference: None,
        }],
        discount: None,
        package_sales: Vec::new(),
        companions: Vec::new(),
    };
    let (key, hash) = idempotency();
    retry(|| {
        operations::appointments::service::complete_as_of(
            ctx.app,
            ctx.cipher,
            &ctx.manager,
            key,
            hash,
            id,
            input(),
            at,
        )
    })
    .await
    .map_err(fail("complete an appointment"))?;
    Ok(())
}

/// A mother, her husband and their daughter (no phone of her own) booked together the day after
/// tomorrow at 10:00.
async fn family(
    ctx: &Ctx<'_>,
    people: &[Person],
    customer_ids: &[Uuid],
    rng: &mut Rng,
) -> anyhow::Result<usize> {
    let day = ctx.window.today + Duration::days(2);
    let named: Vec<usize> = (PAPER_CUSTOMERS..people.len())
        .filter(|&c| people[c].segment == Segment::Regular && people[c].full_name.is_some())
        .collect();
    let (Some(&host), Some(&partner)) = (named.first(), named.get(1)) else {
        return Ok(0);
    };
    let technicians = ctx.active_technicians();
    let service = |name: &str| ctx.catalog.service(name);
    let input = AppointmentGroupCreate {
        starts_at: ctx.at(day, 10 * 60)?,
        note: Some("Gia đình đặt cùng giờ, xếp 3 giường cạnh nhau".to_owned()),
        people: vec![
            GroupPersonCreate {
                customer_id: Some(customer_ids[host]),
                guest: None,
                service_ids: vec![service("Thân Tâm An Định")?],
                staff_id: rng.pick(&technicians).copied(),
                starts_at: None,
                duration_min: None,
            },
            GroupPersonCreate {
                customer_id: Some(customer_ids[partner]),
                guest: None,
                service_ids: vec![service("An Kiên Lạc")?],
                staff_id: None,
                starts_at: None,
                duration_min: None,
            },
            GroupPersonCreate {
                customer_id: None,
                guest: Some(GuestCreate {
                    full_name: "Bé Na (con gái)".to_owned(),
                    phone: None,
                }),
                service_ids: vec![service("Thư Giãn")?],
                staff_id: None,
                starts_at: None,
                duration_min: None,
            },
        ],
        companions: Vec::new(),
    };
    let (key, hash) = idempotency();
    match retry(|| {
        operations::appointments::service::book_group(ctx.app, &ctx.manager, key, hash, &input)
    })
    .await
    {
        Ok(_) => Ok(input.people.len()),
        Err(e) if refused(&e) => Ok(0),
        Err(e) => Err(fail("book a family")(e)),
    }
}
