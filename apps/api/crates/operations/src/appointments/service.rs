//! Appointment use cases (spec appointments.md). Every change runs in one `TenantTx`; bookings of
//! one business day are serialized by an advisory lock so the technician-overlap check holds
//! under concurrency (the `EXCLUDE` constraint is the second wall). The spa-wide "max per slot"
//! is advisory since round 5 (§10.4): shown and warned about on the calendar, never refused here.
//! Completing an appointment records its visit through [`crate::service::record_in`]: the same
//! pricing, session, payment and discount rules as `POST /visits`, in the same transaction.

use super::domain::{self, CalendarSettings, Phase, SlotError, Status};
use super::dto::{
    AppointmentArrive, AppointmentComplete, AppointmentCompleted, AppointmentCreate,
    AppointmentDto, AppointmentGroupCreate, AppointmentGroupDto, AppointmentGroupMove,
    AppointmentGroupMoved, AppointmentServiceDto, AppointmentSettingsDto,
    AppointmentSettingsUpdate, AppointmentStatusDto, AppointmentUpdate, CalendarDto, CalendarQuery,
    CalendarTechnician, CalendarVisitDto, GuestCreate, MyCalendar, MyCalendarData, MyCalendarEntry,
    MyEntryKind, MyEntryService, PaymentStateDto, PhaseDto,
};
use super::repo::{self, AppointmentInsert, AppointmentRow, Reschedule};
use crate::dto::VisitCreate;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use kernel::audit::{self, AuditEntry};
use kernel::crypto::FieldCipher;
use kernel::idempotency::{self, Claim};
use kernel::{Actor, AppError, FieldError, Permission, PhoneE164, TenantProfile, TenantTx};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

const MAX_NOTE: usize = 500;
const MAX_NAME: usize = 200;

/// Which way an open appointment is closed without a visit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    NoShow,
    Cancelled,
}

impl Outcome {
    const fn status(self) -> Status {
        match self {
            Self::NoShow => Status::NoShow,
            Self::Cancelled => Status::Cancelled,
        }
    }
}

/// What every use case needs about the spa inside its transaction.
struct Spa {
    profile: TenantProfile,
    settings: CalendarSettings,
    technicians: Vec<CalendarTechnician>,
}

impl Spa {
    fn today(&self) -> NaiveDate {
        self.profile.business_date.as_naive()
    }
}

async fn spa(tx: &mut TenantTx) -> Result<Spa, AppError> {
    let profile = kernel::tenant::profile(tx).await?;
    let (hours, slot) = repo::calendar_settings(tx).await?;
    let staff = org::service::active_in(tx).await?;
    let named: Vec<_> = staff
        .iter()
        .filter(|s| domain::is_technician(&s.position))
        .collect();
    let columns = if named.is_empty() {
        staff.iter().collect()
    } else {
        named
    };
    Ok(Spa {
        profile,
        settings: CalendarSettings::from_settings(hours.as_ref(), slot.as_ref()),
        technicians: columns
            .into_iter()
            .map(|s| CalendarTechnician {
                id: s.id,
                full_name: s.full_name.clone(),
            })
            .collect(),
    })
}

fn invalid(field: impl Into<String>, code: &'static str) -> AppError {
    AppError::Validation(vec![FieldError::new(field, code)])
}

fn clean_note(note: Option<&String>) -> Result<Option<String>, AppError> {
    let note = note.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty());
    if note.as_ref().is_some_and(|n| n.chars().count() > MAX_NOTE) {
        return Err(invalid("note", "too_long"));
    }
    Ok(note)
}

fn slot_error(e: SlotError) -> AppError {
    match e {
        SlotError::BadDuration => invalid("duration_min", "out_of_range"),
        SlotError::NotOnSlot => invalid("starts_at", "not_on_slot"),
        SlotError::OutsideHours => AppError::Unprocessable {
            code: "appointment.outside_opening_hours",
        },
    }
}

/// Where a booking sits once its time and technician are decided.
struct Placement {
    business_date: NaiveDate,
    starts_at: DateTime<Utc>,
    ends_at: DateTime<Utc>,
    staff_id: Option<Uuid>,
}

/// Day rules of a new booking or of a move to another day: today up to 90 days ahead. A past day
/// is refused for everyone, the owner too (EM 2026-11-07): a service already done is a back-dated
/// visit ("nhập bù" at Quầy), not an appointment. Appointments already on a past day still follow
/// the closed-day rule for every change (`lock_open`). An earlier time of today: [`check_time`].
fn check_day(spa: &Spa, day: NaiveDate) -> Result<(), AppError> {
    if day < spa.today() {
        return Err(AppError::Unprocessable {
            code: "appointment.in_the_past",
        });
    }
    if day > spa.today() + Duration::days(domain::MAX_DAYS_AHEAD) {
        return Err(invalid("starts_at", "too_far"));
    }
    Ok(())
}

/// A new booking, or a booked one moved to another start, never starts in a slot that is already
/// over (EM 2026-11-08, F12): `422 appointment.in_the_past`, the owner too. The slot under way is
/// still bookable (19:00 at 19:08 with 30-minute slots: the customer is arriving). Before this rule
/// only the day was checked, so a 10:00 booking was created at 19:08 the same day. A service
/// already done is a visit recorded at Quầy ("Đổi giờ / nhập bù").
fn check_time(spa: &Spa, starts_at: DateTime<Utc>, now: DateTime<Utc>) -> Result<(), AppError> {
    if domain::slot_is_over(starts_at, now, spa.settings.slot_minutes) {
        return Err(AppError::Unprocessable {
            code: "appointment.in_the_past",
        });
    }
    Ok(())
}

fn place(
    spa: &Spa,
    starts_at: DateTime<Utc>,
    minutes: u32,
    staff_id: Option<Uuid>,
) -> Result<Placement, AppError> {
    let (ends_at, business_date) =
        domain::place(starts_at, minutes, spa.profile.timezone, spa.settings)
            .map_err(slot_error)?;
    Ok(Placement {
        business_date,
        starts_at,
        ends_at,
        staff_id,
    })
}

/// The person a booking is for, to find their other bookings at the same time.
#[derive(Clone, Copy)]
struct Holder<'a> {
    customer_id: Option<Uuid>,
    guest_phone: Option<&'a str>,
    group_id: Option<Uuid>,
}

/// The technician (if any) is free over the whole booking (AC9.2) and, when `holder` is given, the
/// same person has no other open booking overlapping it (EM 2026-11-07: "Lan Anh 09:00 ×2").
/// Takes the day lock first, so the answers stay true until the caller commits.
async fn check_fits(
    tx: &mut TenantTx,
    p: &Placement,
    except: Option<Uuid>,
    holder: Option<Holder<'_>>,
) -> Result<(), AppError> {
    if let Some(staff) = p.staff_id {
        org::service::require_active(tx, staff).await?;
    }
    repo::lock_day(tx, p.business_date).await?;
    if let Some(staff) = p.staff_id
        && let Some(busy) = repo::staff_overlap(tx, staff, p.starts_at, p.ends_at, except).await?
    {
        return Err(AppError::ConflictWith {
            code: "appointment.staff_busy",
            existing_id: busy,
        });
    }
    if let Some(h) = holder
        && (h.customer_id.is_some() || h.guest_phone.is_some())
        && let Some(busy) = repo::customer_overlap(
            tx,
            h.customer_id,
            h.guest_phone,
            h.group_id,
            p.starts_at,
            p.ends_at,
            except,
        )
        .await?
    {
        return Err(AppError::ConflictWith {
            code: "appointment.customer_busy",
            existing_id: busy,
        });
    }
    Ok(())
}

/// Booked services, resolved against the catalog in order: `(id, name, duration)`.
async fn resolve_services(
    tx: &mut TenantTx,
    ids: &[Uuid],
) -> Result<Vec<(Uuid, String, Option<i32>)>, AppError> {
    if ids.is_empty() || ids.len() > domain::MAX_SERVICES {
        return Err(invalid("service_ids", "count_out_of_range"));
    }
    let found = repo::services(tx, ids).await?;
    ids.iter()
        .enumerate()
        .map(|(i, id)| {
            let s = found
                .iter()
                .find(|s| s.id == *id)
                .ok_or(AppError::NotFound)?;
            if !s.active {
                return Err(invalid(format!("service_ids[{i}]"), "inactive"));
            }
            Ok((s.id, s.name.clone(), s.duration_min))
        })
        .collect()
}

fn status_dto(s: Status) -> AppointmentStatusDto {
    match s {
        Status::Booked => AppointmentStatusDto::Booked,
        Status::Arrived => AppointmentStatusDto::Arrived,
        Status::Done => AppointmentStatusDto::Done,
        Status::NoShow => AppointmentStatusDto::NoShow,
        Status::Cancelled => AppointmentStatusDto::Cancelled,
    }
}

fn parse_status(s: &str) -> Result<Status, AppError> {
    Status::parse(s).ok_or_else(|| AppError::Internal(format!("appointment status {s}")))
}

fn phase_dto(p: Phase) -> PhaseDto {
    match p {
        Phase::Waiting => PhaseDto::Waiting,
        Phase::InService => PhaseDto::InService,
        Phase::Done => PhaseDto::Done,
        Phase::NoShow => PhaseDto::NoShow,
        Phase::Cancelled => PhaseDto::Cancelled,
        Phase::Voided => PhaseDto::Voided,
    }
}

fn dto(
    r: AppointmentRow,
    services: Vec<AppointmentServiceDto>,
    now: DateTime<Utc>,
) -> Result<AppointmentDto, AppError> {
    let status = parse_status(&r.status)?;
    let duration_min = (r.ends_at - r.starts_at).num_minutes();
    let visit = r
        .visit_check_in_at
        .map(|at| (at, r.visit_status.as_deref() == Some("voided")));
    let phase = domain::appointment_phase(status, visit, duration_min, now);
    let paid = r.sale_total.unwrap_or(0);
    let payment_state = match status {
        Status::Booked | Status::Arrived => PaymentStateDto::Unpaid,
        Status::NoShow | Status::Cancelled => PaymentStateDto::None,
        Status::Done if r.visit_status.as_deref() == Some("voided") => PaymentStateDto::Voided,
        Status::Done if paid > 0 => PaymentStateDto::Paid,
        Status::Done => PaymentStateDto::NoCharge,
    };
    Ok(AppointmentDto {
        status: status_dto(status),
        phase: phase_dto(phase),
        group_id: r.group_id,
        companions: r.companions,
        duration_min,
        is_guest: r.customer_id.is_none(),
        customer_gender: crm::http::gender_dto(r.customer_gender.as_deref()),
        paid_amount: if payment_state == PaymentStateDto::Paid {
            paid
        } else {
            0
        },
        payment_state,
        id: r.id,
        business_date: r.business_date,
        starts_at: r.starts_at,
        ends_at: r.ends_at,
        customer_id: r.customer_id,
        customer_name: r.customer_name,
        customer_phone: r.customer_phone,
        staff_id: r.staff_id,
        staff_name: r.staff_name,
        services,
        note: r.note,
        status_reason: r.status_reason,
        arrived_at: r.arrived_at,
        closed_at: r.closed_at,
        visit_id: r.visit_id,
        version: r.version,
        created_at: r.created_at,
    })
}

/// Appointments with their services in two queries (no N+1).
async fn load(
    tx: &mut TenantTx,
    id: Option<Uuid>,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
) -> Result<Vec<AppointmentDto>, AppError> {
    let rows = repo::appointments(tx, id, from, to).await?;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let mut services: HashMap<Uuid, Vec<AppointmentServiceDto>> = HashMap::new();
    for s in repo::services_of(tx, &ids).await? {
        services
            .entry(s.appointment_id)
            .or_default()
            .push(AppointmentServiceDto {
                service_id: s.service_id,
                service_name: s.service_name,
                category_name: s.category_name,
                duration_min: s.duration_min,
            });
    }
    let now = Utc::now();
    rows.into_iter()
        .map(|r| {
            let s = services.remove(&r.id).unwrap_or_default();
            dto(r, s, now)
        })
        .collect()
}

/// Walk-in visits of `from..=to` on the timeline, with their lines in one more query.
async fn walk_ins(
    tx: &mut TenantTx,
    spa: &Spa,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<CalendarVisitDto>, AppError> {
    let rows = repo::walk_ins(tx, spa.profile.branch.as_uuid(), from, to).await?;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let mut lines: HashMap<Uuid, Vec<repo::VisitLineRow>> = HashMap::new();
    for l in repo::visit_lines(tx, &ids).await? {
        lines.entry(l.visit_id).or_default().push(l);
    }
    let now = Utc::now();
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        // Every visit has a line; one without would have no column to sit in.
        let Some(ls) = lines.remove(&r.id) else {
            continue;
        };
        let Some((staff_id, staff_name)) = ls.first().map(|l| (l.staff_id, l.staff_name.clone()))
        else {
            continue;
        };
        let durations: Vec<Option<i32>> = ls.iter().map(|l| l.duration_min).collect();
        let minutes = domain::visit_minutes(&durations);
        let ends_at = r.check_in_at + Duration::minutes(i64::from(minutes));
        let voided = r.status == "voided";
        let paid = r.sale_total.unwrap_or(0);
        let payment_state = if voided {
            PaymentStateDto::Voided
        } else if paid > 0 {
            PaymentStateDto::Paid
        } else {
            PaymentStateDto::NoCharge
        };
        out.push(CalendarVisitDto {
            id: r.id,
            phase: phase_dto(domain::served_phase(voided, ends_at, now)),
            business_date: r.business_date,
            starts_at: r.check_in_at,
            ends_at,
            duration_min: i64::from(minutes),
            customer_id: r.customer_id,
            customer_name: r.customer_name,
            customer_gender: crm::http::gender_dto(r.customer_gender.as_deref()),
            staff_id,
            staff_name,
            services: ls
                .into_iter()
                .map(|l| AppointmentServiceDto {
                    service_id: l.service_id,
                    service_name: l.service_name,
                    category_name: l.category_name,
                    duration_min: l.duration_min,
                })
                .collect(),
            paid_amount: if payment_state == PaymentStateDto::Paid {
                paid
            } else {
                0
            },
            payment_state,
        });
    }
    Ok(out)
}

async fn load_one(tx: &mut TenantTx, id: Uuid) -> Result<AppointmentDto, AppError> {
    load(tx, Some(id), None, None)
        .await?
        .pop()
        .ok_or_else(|| AppError::Internal("appointment vanished".into()))
}

async fn record_audit(
    tx: &mut TenantTx,
    actor: &Actor,
    action: &'static str,
    id: Uuid,
    before: Option<Value>,
    after: Value,
    reason: Option<&str>,
) -> Result<(), AppError> {
    audit::record(
        tx,
        actor,
        AuditEntry {
            action,
            entity_type: "appointment",
            entity_id: id,
            before,
            after: Some(after),
            reason,
        },
    )
    .await
}

// ------------------------------------------------------------------------------------ reads

/// `GET /appointments?from=&to=` (`appointment.manage`): the grid and every appointment of the
/// days, earliest first. Default today; at most 14 days.
pub async fn calendar(
    pool: &PgPool,
    actor: &Actor,
    query: &CalendarQuery,
) -> Result<CalendarDto, AppError> {
    actor.require(Permission::AppointmentManage)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let spa = spa(&mut tx).await?;
    let from = query.from.unwrap_or(spa.today());
    let to = query.to.unwrap_or(from);
    if to < from {
        return Err(invalid("to", "before_from"));
    }
    if (to - from).num_days() >= domain::MAX_RANGE_DAYS {
        return Err(invalid("to", "range_too_long"));
    }
    let appointments = load(&mut tx, None, Some(from), Some(to)).await?;
    // Entitled data only: walk-in visits are visit records (spec §10.1).
    let visits = if actor.can(Permission::VisitRead) {
        walk_ins(&mut tx, &spa, from, to).await?
    } else {
        Vec::new()
    };
    let max_per_slot = repo::max_per_slot(&mut tx).await?;
    tx.commit().await?;
    Ok(CalendarDto {
        from,
        to,
        today: spa.today(),
        opens_at: spa.settings.open.format("%H:%M").to_string(),
        closes_at: spa.settings.close.format("%H:%M").to_string(),
        slot_minutes: spa.settings.slot_minutes,
        time_zone: spa.profile.timezone.name().to_owned(),
        technicians: spa.technicians,
        max_per_slot: max_per_slot.and_then(|m| u32::try_from(m).ok()),
        appointments,
        visits,
    })
}

/// `GET /me/appointments?from=&to=` (`staff.read_own`): the signed-in technician's own calendar
/// (EM 2026-11-07, "như Google Calendar"): bookings assigned to them and visits they served, as
/// blocks at their real length. Read-only. The person is the session's staff link, never a
/// request parameter (non-negotiable 3); entries carry no phone, booking or customer note, money
/// or ids of others — only the session note of a visit the person performed a line of.
pub async fn my_calendar(
    pool: &PgPool,
    cipher: &FieldCipher,
    actor: &Actor,
    query: &CalendarQuery,
) -> Result<MyCalendar, AppError> {
    actor.require(Permission::StaffReadOwn)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let Some(staff) = org::profile::own_staff_id(&mut tx, actor).await? else {
        tx.commit().await?;
        return Ok(MyCalendar {
            linked: false,
            data: None,
        });
    };
    let profile = kernel::tenant::profile(&mut tx).await?;
    let (hours, slot) = repo::calendar_settings(&mut tx).await?;
    let settings = CalendarSettings::from_settings(hours.as_ref(), slot.as_ref());
    let today = profile.business_date.as_naive();
    let from = query.from.unwrap_or(today);
    let to = query.to.unwrap_or(from);
    if to < from {
        return Err(invalid("to", "before_from"));
    }
    if (to - from).num_days() >= domain::MAX_RANGE_DAYS {
        return Err(invalid("to", "range_too_long"));
    }
    let booked = repo::my_appointments(&mut tx, staff, from, to).await?;
    let ids: Vec<Uuid> = booked.iter().map(|a| a.id).collect();
    let mut booked_services: HashMap<Uuid, Vec<MyEntryService>> = HashMap::new();
    for s in repo::services_of(&mut tx, &ids).await? {
        booked_services
            .entry(s.appointment_id)
            .or_default()
            .push(MyEntryService {
                service_name: s.service_name,
                category_name: s.category_name,
                from_package: false,
            });
    }
    let served = repo::my_visits(&mut tx, staff, from, to).await?;
    let ids: Vec<Uuid> = served.iter().map(|v| v.id).collect();
    let mut lines: HashMap<Uuid, Vec<repo::MyVisitLineRow>> = HashMap::new();
    for l in repo::my_visit_lines(&mut tx, staff, &ids).await? {
        lines.entry(l.visit_id).or_default().push(l);
    }
    let tenant = tx.tenant().as_uuid();
    tx.commit().await?;
    let note_of = |visit: Option<Uuid>, ct: Option<&[u8]>| match visit {
        Some(v) => kernel::crypto::open_visit_note(cipher, tenant, v, ct),
        None => Ok(None),
    };

    let now = Utc::now();
    let mut entries = Vec::with_capacity(booked.len() + served.len());
    for a in booked {
        let status = parse_status(&a.status)?;
        let duration_min = (a.ends_at - a.starts_at).num_minutes();
        let visit = a
            .visit_check_in_at
            .map(|at| (at, a.visit_status.as_deref() == Some("voided")));
        entries.push(MyCalendarEntry {
            phase: phase_dto(domain::appointment_phase(status, visit, duration_min, now)),
            services: booked_services.remove(&a.id).unwrap_or_default(),
            id: a.id,
            kind: MyEntryKind::Appointment,
            business_date: a.business_date,
            starts_at: a.starts_at,
            ends_at: a.ends_at,
            duration_min,
            note: note_of(a.visit_id, a.note_ct.as_deref())?,
            customer_name: a.customer_name,
            group_size: a.group_size,
            walk_in: false,
        });
    }
    for v in served {
        let ls = lines.remove(&v.id).unwrap_or_default();
        let durations: Vec<Option<i32>> = ls.iter().map(|l| l.duration_min).collect();
        let minutes = domain::visit_minutes(&durations);
        let ends_at = v.check_in_at + Duration::minutes(i64::from(minutes));
        entries.push(MyCalendarEntry {
            id: v.id,
            kind: MyEntryKind::Visit,
            business_date: v.business_date,
            starts_at: v.check_in_at,
            ends_at,
            duration_min: i64::from(minutes),
            phase: phase_dto(domain::served_phase(false, ends_at, now)),
            note: note_of(Some(v.id), v.note_ct.as_deref())?,
            customer_name: v.customer_name,
            services: ls
                .into_iter()
                .map(|l| MyEntryService {
                    from_package: l.source == "package",
                    service_name: l.service_name,
                    category_name: l.category_name,
                })
                .collect(),
            group_size: None,
            walk_in: v.walk_in,
        });
    }
    entries.sort_by(|x, y| x.starts_at.cmp(&y.starts_at).then(x.id.cmp(&y.id)));
    Ok(MyCalendar {
        linked: true,
        data: Some(MyCalendarData {
            from,
            to,
            today,
            opens_at: settings.open.format("%H:%M").to_string(),
            closes_at: settings.close.format("%H:%M").to_string(),
            slot_minutes: settings.slot_minutes,
            entries,
        }),
    })
}

/// `PUT /appointment-settings` (`appointment.settings`, owner): sets or clears the advisory
/// "max customers per slot" (1–200, `null` = no limit). Audited.
pub async fn update_settings(
    pool: &PgPool,
    actor: &Actor,
    input: &AppointmentSettingsUpdate,
) -> Result<AppointmentSettingsDto, AppError> {
    actor.require(Permission::AppointmentSettings)?;
    let value = input
        .max_per_slot
        .ok_or_else(|| invalid("max_per_slot", "required"))?;
    if value.is_some_and(|m| m == 0 || m > domain::MAX_PER_SLOT) {
        return Err(invalid("max_per_slot", "out_of_range"));
    }
    let stored = value
        .map(i32::try_from)
        .transpose()
        .map_err(|_| invalid("max_per_slot", "out_of_range"))?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let before = repo::set_max_per_slot(&mut tx, stored, actor.user_id.as_uuid()).await?;
    audit::record(
        &mut tx,
        actor,
        AuditEntry {
            action: "appointment.settings",
            entity_type: "tenant",
            entity_id: actor.tenant_id.as_uuid(),
            before: Some(json!({"max_per_slot": before})),
            after: Some(json!({"max_per_slot": stored})),
            reason: None,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(AppointmentSettingsDto {
        max_per_slot: value,
    })
}

/// `GET /appointments/{id}` (`appointment.manage`). Another spa's is `404`.
pub async fn get(pool: &PgPool, actor: &Actor, id: Uuid) -> Result<AppointmentDto, AppError> {
    actor.require(Permission::AppointmentManage)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let found = load(&mut tx, Some(id), None, None).await?.pop();
    tx.commit().await?;
    found.ok_or(AppError::NotFound)
}

// ----------------------------------------------------------------------------------- writes

/// Who the booking is for: an existing customer or a newcomer (name, and an E.164 phone unless
/// they are a served companion known only by name, spec §10.10).
enum Who {
    Customer(Uuid),
    Guest {
        name: String,
        phone: Option<PhoneE164>,
    },
}

/// Whether a newcomer must leave a phone: the person who books does, a served companion
/// (`people[1..]` of a group, `group_with`) may not.
#[derive(Clone, Copy, PartialEq, Eq)]
enum GuestPhone {
    Required,
    Optional,
}

fn parse_who(
    customer_id: Option<Uuid>,
    guest: Option<&GuestCreate>,
    phone_rule: GuestPhone,
) -> Result<Who, AppError> {
    match (customer_id, guest) {
        (Some(id), None) => Ok(Who::Customer(id)),
        (None, Some(g)) => {
            let name = g.full_name.trim();
            let mut errors = Vec::new();
            if name.is_empty() || name.chars().count() > MAX_NAME {
                errors.push(FieldError::new("guest.full_name", "required"));
            }
            let typed = g.phone.as_deref().map(str::trim).filter(|p| !p.is_empty());
            let phone = match typed {
                Some(p) => match PhoneE164::parse(p) {
                    Ok(phone) => Some(phone),
                    Err(_) => {
                        errors.push(FieldError::new("guest.phone", "invalid"));
                        None
                    }
                },
                None if phone_rule == GuestPhone::Required => {
                    errors.push(FieldError::new("guest.phone", "required"));
                    None
                }
                None => None,
            };
            if errors.is_empty() {
                Ok(Who::Guest {
                    name: name.to_owned(),
                    phone,
                })
            } else {
                Err(AppError::Validation(errors))
            }
        }
        _ => Err(invalid("customer_id", "customer_or_guest")),
    }
}

/// Companion notes of a booking (same rules as a visit's).
fn companions(input: &[String]) -> Result<Vec<String>, AppError> {
    let mut errors = Vec::new();
    let c = crate::service::parse_companions(input, &mut errors);
    if errors.is_empty() {
        Ok(c)
    } else {
        Err(AppError::Validation(errors))
    }
}

/// One person to book, parsed at the edge (single booking or one person of a group).
struct Booking<'a> {
    who: Who,
    service_ids: &'a [Uuid],
    staff_id: Option<Uuid>,
    starts_at: DateTime<Utc>,
    duration_min: Option<u32>,
}

/// Field errors of person `i` of a group point at `people[i].<field>`.
fn in_person(i: usize, e: AppError) -> AppError {
    match e {
        AppError::Validation(fields) => AppError::Validation(
            fields
                .into_iter()
                .map(|f| FieldError::new(format!("people[{i}].{}", f.field), f.code))
                .collect(),
        ),
        other => other,
    }
}

/// Checks and inserts one appointment inside the caller's transaction (day lock taken by
/// `check_fits`, so a later person of the same group sees this one). Returns its id and day.
async fn insert_booking(
    tx: &mut TenantTx,
    actor: &Actor,
    spa: &Spa,
    b: &Booking<'_>,
    note: Option<&str>,
    group_id: Option<Uuid>,
    companions: &[String],
) -> Result<(Uuid, NaiveDate), AppError> {
    let customer_id = match &b.who {
        Who::Customer(id) => Some(
            crm::service::brief_in(tx, *id)
                .await?
                .ok_or(AppError::NotFound)?
                .id,
        ),
        Who::Guest {
            phone: Some(phone), ..
        } => {
            if let Some((existing, _)) = crm::service::phone_holder_in(tx, phone).await? {
                return Err(AppError::ConflictWith {
                    code: "customer.phone_taken",
                    existing_id: existing,
                });
            }
            None
        }
        Who::Guest { phone: None, .. } => None,
    };
    let services = resolve_services(tx, b.service_ids).await?;
    let durations: Vec<Option<i32>> = services.iter().map(|s| s.2).collect();
    let minutes = domain::duration_minutes(&durations, b.duration_min).map_err(slot_error)?;
    // A start already over is refused before any other rule (opening hours, grid) of that time.
    check_time(spa, b.starts_at, Utc::now())?;
    let p = place(spa, b.starts_at, minutes, b.staff_id)?;
    check_day(spa, p.business_date)?;
    let (guest_name, guest_phone) = match &b.who {
        Who::Guest { name, phone } => (Some(name.as_str()), phone.as_ref().map(PhoneE164::as_str)),
        Who::Customer(_) => (None, None),
    };
    let holder = Holder {
        customer_id,
        guest_phone,
        group_id,
    };
    check_fits(tx, &p, None, Some(holder)).await?;

    let id = Uuid::now_v7();
    repo::insert(
        tx,
        &AppointmentInsert {
            id,
            branch_id: spa.profile.branch.as_uuid(),
            customer_id,
            guest_name,
            guest_phone,
            business_date: p.business_date,
            starts_at: p.starts_at,
            ends_at: p.ends_at,
            staff_id: p.staff_id,
            note,
            group_id,
            companions,
            created_by: actor.user_id.as_uuid(),
        },
    )
    .await?;
    let named: Vec<(Uuid, String)> = services.iter().map(|s| (s.0, s.1.clone())).collect();
    repo::set_services(tx, id, &named).await?;
    record_audit(
        tx,
        actor,
        "appointment.create",
        id,
        None,
        json!({
            "customer_id": customer_id, "guest": guest_name.is_some(),
            "starts_at": p.starts_at, "ends_at": p.ends_at, "staff_id": p.staff_id,
            "service_ids": b.service_ids, "has_note": note.is_some(), "group_id": group_id,
            "guest_phone": guest_phone.is_some(),
            "companions": companions.len(),
        }),
        None,
    )
    .await?;
    Ok((id, p.business_date))
}

/// `POST /appointments` (`appointment.manage`, Idempotency-Key): book a slot (AC9.1–9.4).
pub async fn book(
    pool: &PgPool,
    actor: &Actor,
    key: Uuid,
    request_hash: [u8; 32],
    input: &AppointmentCreate,
) -> Result<(u16, Value), AppError> {
    actor.require(Permission::AppointmentManage)?;
    let phone_rule = if input.group_with.is_some() {
        GuestPhone::Optional
    } else {
        GuestPhone::Required
    };
    let booking = Booking {
        who: parse_who(input.customer_id, input.guest.as_ref(), phone_rule)?,
        service_ids: &input.service_ids,
        staff_id: input.staff_id,
        starts_at: input.starts_at,
        duration_min: input.duration_min,
    };
    let note = clean_note(input.note.as_ref())?;
    let companions = companions(&input.companions)?;

    let mut tx = TenantTx::for_actor(pool, actor).await?;
    if let Claim::Replay { status, body } =
        idempotency::claim(&mut tx, actor, key, request_hash).await?
    {
        tx.commit().await?;
        return Ok((status, body));
    }
    let spa = spa(&mut tx).await?;
    let host = match input.group_with {
        Some(host) => Some(join_group(&mut tx, actor, &spa, host, &booking.who).await?),
        None => None,
    };
    let (id, day) = insert_booking(
        &mut tx,
        actor,
        &spa,
        &booking,
        note.as_deref(),
        host.map(|(group, _)| group),
        &companions,
    )
    .await?;
    if host.is_some_and(|(_, host_day)| host_day != day) {
        return Err(invalid("starts_at", "not_same_day"));
    }
    let body = serde_json::to_value(load_one(&mut tx, id).await?)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    idempotency::complete(&mut tx, actor, key, 201, &body).await?;
    tx.commit().await?;
    Ok((201, body))
}

/// `POST /appointments/group` (`appointment.manage`, Idempotency-Key): a family booked at once
/// (spec §10.3). Every person is checked like `POST /appointments` and the whole group is saved
/// in one transaction: one refusal saves nobody. Errors of person `i` name `people[i].…`.
pub async fn book_group(
    pool: &PgPool,
    actor: &Actor,
    key: Uuid,
    request_hash: [u8; 32],
    input: &AppointmentGroupCreate,
) -> Result<(u16, Value), AppError> {
    actor.require(Permission::AppointmentManage)?;
    let companions = companions(&input.companions)?;
    // A family where only one person is served is still a group: the others are notes (Q-U5).
    let min = if companions.is_empty() {
        domain::MIN_GROUP
    } else {
        1
    };
    if !(min..=domain::MAX_GROUP).contains(&input.people.len()) {
        return Err(invalid("people", "count_out_of_range"));
    }
    let note = clean_note(input.note.as_ref())?;
    let mut bookings = Vec::with_capacity(input.people.len());
    for (i, person) in input.people.iter().enumerate() {
        // The first person booked; the others are their served companions (spec §10.10).
        let phone_rule = if i == 0 {
            GuestPhone::Required
        } else {
            GuestPhone::Optional
        };
        let who = parse_who(person.customer_id, person.guest.as_ref(), phone_rule)
            .map_err(|e| in_person(i, e))?;
        if let Who::Customer(id) = who
            && input.people[..i].iter().any(|p| p.customer_id == Some(id))
        {
            return Err(invalid(format!("people[{i}].customer_id"), "duplicate"));
        }
        bookings.push(Booking {
            who,
            service_ids: &person.service_ids,
            staff_id: person.staff_id,
            starts_at: person.starts_at.unwrap_or(input.starts_at),
            duration_min: person.duration_min,
        });
    }

    let mut tx = TenantTx::for_actor(pool, actor).await?;
    if let Claim::Replay { status, body } =
        idempotency::claim(&mut tx, actor, key, request_hash).await?
    {
        tx.commit().await?;
        return Ok((status, body));
    }
    let spa = spa(&mut tx).await?;
    let group_id = Uuid::now_v7();
    let mut ids = Vec::with_capacity(bookings.len());
    let mut group_day = None;
    for (i, b) in bookings.iter().enumerate() {
        // The companions travel with the first person's booking.
        let mine: &[String] = if i == 0 { &companions } else { &[] };
        let (id, day) = insert_booking(
            &mut tx,
            actor,
            &spa,
            b,
            note.as_deref(),
            Some(group_id),
            mine,
        )
        .await
        .map_err(|e| in_person(i, e))?;
        if *group_day.get_or_insert(day) != day {
            return Err(invalid(format!("people[{i}].starts_at"), "not_same_day"));
        }
        ids.push(id);
    }
    let mut appointments = Vec::with_capacity(ids.len());
    for id in ids {
        appointments.push(load_one(&mut tx, id).await?);
    }
    let body = serde_json::to_value(AppointmentGroupDto {
        group_id,
        appointments,
    })
    .map_err(|e| AppError::Internal(e.to_string()))?;
    idempotency::complete(&mut tx, actor, key, 201, &body).await?;
    tx.commit().await?;
    Ok((201, body))
}

/// `group_with` of `POST /appointments`: locks the host appointment (open, closed-day rule),
/// opens a group on it when it had none, and refuses a customer already in that group.
/// Returns the group and the host's business day (the companion must share it).
async fn join_group(
    tx: &mut TenantTx,
    actor: &Actor,
    spa: &Spa,
    host: Uuid,
    who: &Who,
) -> Result<(Uuid, NaiveDate), AppError> {
    let row = lock_open(tx, actor, spa, host).await?;
    let group = match row.group_id {
        Some(g) => {
            if let Who::Customer(c) = who
                && repo::group_customers(tx, g).await?.contains(c)
            {
                return Err(invalid("customer_id", "duplicate"));
            }
            g
        }
        None => {
            if let Who::Customer(c) = who
                && row.customer_id == Some(*c)
            {
                return Err(invalid("customer_id", "duplicate"));
            }
            let g = Uuid::now_v7();
            repo::open_group(tx, host, g).await?;
            g
        }
    };
    Ok((group, row.business_date))
}

/// Locks an appointment that must still be open, applying the closed-day rule to its day.
async fn lock_open(
    tx: &mut TenantTx,
    actor: &Actor,
    spa: &Spa,
    id: Uuid,
) -> Result<repo::Locked, AppError> {
    let row = repo::lock(tx, id).await?.ok_or(AppError::NotFound)?;
    check_open(actor, spa, &row)?;
    Ok(row)
}

/// A locked appointment may still change: not closed, and its day open to the caller.
fn check_open(actor: &Actor, spa: &Spa, row: &repo::Locked) -> Result<(), AppError> {
    if !parse_status(&row.status)?.is_open() {
        return Err(AppError::Conflict {
            code: "appointment.closed",
        });
    }
    kernel::corrections::authorize_closed_day(actor, row.business_date, spa.today())
}

/// `PATCH /appointments/{id}` (`appointment.manage`, If-Match): reschedule, change technician,
/// services or note, or link a newcomer to their new customer record. Re-checks the grid,
/// technician and capacity like a new booking.
pub async fn update(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    version: i64,
    input: &AppointmentUpdate,
) -> Result<AppointmentDto, AppError> {
    actor.require(Permission::AppointmentManage)?;
    let note = input
        .note
        .as_ref()
        .map(|n| clean_note(n.as_ref()))
        .transpose()?;
    let new_companions = input.companions.as_deref().map(companions).transpose()?;

    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let spa = spa(&mut tx).await?;
    let current = lock_open(&mut tx, actor, &spa, id).await?;
    if current.version != version {
        return Err(AppError::StaleVersion);
    }
    let arrived = parse_status(&current.status)? == Status::Arrived;
    let staff_id = match input.staff_id {
        None => current.staff_id,
        Some(None) if arrived => return Err(invalid("staff_id", "required")),
        Some(s) => s,
    };
    if let Some(c) = input.customer_id {
        if current.customer_id.is_some_and(|x| x != c) {
            return Err(invalid("customer_id", "not_allowed"));
        }
        crm::service::brief_in(&mut tx, c)
            .await?
            .ok_or(AppError::NotFound)?;
        // A companion linked to a profile another member already has (a family sharing one
        // phone) would put one person in the group twice: the same rule as `group_with`.
        if let Some(g) = current.group_id
            && current.customer_id.is_none()
            && repo::group_customers(&mut tx, g).await?.contains(&c)
        {
            return Err(invalid("customer_id", "duplicate"));
        }
    }
    let services = match &input.service_ids {
        Some(ids) => Some(resolve_services(&mut tx, ids).await?),
        None => None,
    };
    let minutes = match (input.duration_min, &services) {
        (None, None) => u32::try_from((current.ends_at - current.starts_at).num_minutes())
            .map_err(|_| AppError::Internal("appointment duration".into()))?,
        (requested, Some(s)) => {
            let d: Vec<Option<i32>> = s.iter().map(|x| x.2).collect();
            domain::duration_minutes(&d, requested).map_err(slot_error)?
        }
        (Some(m), None) => domain::duration_minutes(&[], Some(m)).map_err(slot_error)?,
    };
    let p = place(
        &spa,
        input.starts_at.unwrap_or(current.starts_at),
        minutes,
        staff_id,
    )?;
    if p.business_date != current.business_date {
        if arrived {
            return Err(invalid("starts_at", "not_today"));
        }
        check_day(&spa, p.business_date)?;
    }
    // A customer already here keeps the time the desk sets; a booking still waiting cannot move
    // into a slot that is over.
    if !arrived && p.starts_at != current.starts_at {
        check_time(&spa, p.starts_at, Utc::now())?;
    }
    // The person's other bookings are checked only when this one moves or gets its customer: an
    // overlap saved before the rule existed must not stop a change of technician or note.
    let moved = p.starts_at != current.starts_at || p.ends_at != current.ends_at;
    let holder = (moved || input.customer_id.is_some()).then(|| Holder {
        customer_id: input.customer_id.or(current.customer_id),
        guest_phone: if input.customer_id.is_some() {
            None
        } else {
            current.guest_phone.as_deref()
        },
        group_id: current.group_id,
    });
    check_fits(&mut tx, &p, Some(id), holder).await?;
    let note_value = match &note {
        Some(n) => n.clone(),
        None => current.note.clone(),
    };
    if !repo::reschedule(
        &mut tx,
        id,
        version,
        &Reschedule {
            business_date: p.business_date,
            starts_at: p.starts_at,
            ends_at: p.ends_at,
            staff_id: p.staff_id,
            note: note_value.as_deref(),
            customer_id: input.customer_id,
            companions: new_companions.as_deref().unwrap_or(&current.companions),
        },
    )
    .await?
    {
        return Err(AppError::StaleVersion);
    }
    if let Some(s) = &services {
        let named: Vec<(Uuid, String)> = s.iter().map(|x| (x.0, x.1.clone())).collect();
        repo::set_services(&mut tx, id, &named).await?;
    }
    record_audit(
        &mut tx,
        actor,
        "appointment.update",
        id,
        Some(json!({
            "version": version, "starts_at": current.starts_at, "ends_at": current.ends_at,
            "staff_id": current.staff_id, "customer_id": current.customer_id,
        })),
        json!({
            "version": version + 1, "starts_at": p.starts_at, "ends_at": p.ends_at,
            "staff_id": p.staff_id, "customer_id": input.customer_id.or(current.customer_id),
            "service_ids": input.service_ids, "note_changed": note.is_some(),
            "companions": new_companions.as_ref().map(Vec::len),
        }),
        None,
    )
    .await?;
    let dto = load_one(&mut tx, id).await?;
    tx.commit().await?;
    Ok(dto)
}

/// `POST /appointments/{id}/arrive` (`appointment.manage`): the customer is here; the serving
/// technician is assigned (must be free over the booking). Today's appointments only.
pub async fn arrive(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    input: &AppointmentArrive,
) -> Result<AppointmentDto, AppError> {
    actor.require(Permission::AppointmentManage)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let spa = spa(&mut tx).await?;
    let current = lock_open(&mut tx, actor, &spa, id).await?;
    if parse_status(&current.status)? != Status::Booked {
        return Err(AppError::Conflict {
            code: "appointment.already_arrived",
        });
    }
    if current.business_date != spa.today() {
        return Err(AppError::Unprocessable {
            code: "appointment.not_today",
        });
    }
    let p = Placement {
        business_date: current.business_date,
        starts_at: current.starts_at,
        ends_at: current.ends_at,
        staff_id: Some(input.staff_id),
    };
    // Same time as booked: only the serving technician is new.
    check_fits(&mut tx, &p, Some(id), None).await?;
    if !repo::mark_arrived(&mut tx, id, input.staff_id).await? {
        return Err(AppError::Conflict {
            code: "appointment.already_arrived",
        });
    }
    record_audit(
        &mut tx,
        actor,
        "appointment.arrive",
        id,
        Some(json!({"status": "booked", "staff_id": current.staff_id})),
        json!({"status": "arrived", "staff_id": input.staff_id}),
        None,
    )
    .await?;
    let dto = load_one(&mut tx, id).await?;
    tx.commit().await?;
    Ok(dto)
}

/// `POST /appointments/{id}/undo-arrival` (`appointment.manage`, the same role that marks the
/// arrival): "Hoàn tác đã đến" for a mistaken "Chỉ báo đã đến". Only an arrived appointment of
/// today without a visit; once completed it is closed (`409 appointment.closed`), and a past day
/// follows the closed-day rule. Audited.
pub async fn undo_arrival(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
) -> Result<AppointmentDto, AppError> {
    actor.require(Permission::AppointmentManage)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let spa = spa(&mut tx).await?;
    let current = lock_open(&mut tx, actor, &spa, id).await?;
    if parse_status(&current.status)? != Status::Arrived {
        return Err(AppError::Conflict {
            code: "appointment.not_arrived",
        });
    }
    if current.business_date != spa.today() {
        return Err(AppError::Unprocessable {
            code: "appointment.not_today",
        });
    }
    if !repo::unmark_arrived(&mut tx, id).await? {
        return Err(AppError::Conflict {
            code: "appointment.not_arrived",
        });
    }
    record_audit(
        &mut tx,
        actor,
        "appointment.undo_arrival",
        id,
        Some(
            json!({"status": "arrived", "arrived_at": current.arrived_at,
                    "staff_id": current.staff_id}),
        ),
        json!({"status": "booked", "staff_id": current.staff_id}),
        None,
    )
    .await?;
    let dto = load_one(&mut tx, id).await?;
    tx.commit().await?;
    Ok(dto)
}

/// Errors of one member of a whole-group move name that member: `422 validation_failed` with
/// `errors[].field = "appointments.<id>"` (and the member's field, for a field error) and the
/// usual code (`appointment.staff_busy`, `appointment.customer_busy`, `appointment.in_the_past`,
/// `appointment.outside_opening_hours`, `staff.inactive`, `not_on_slot`, `too_far`…).
fn in_member(member: Uuid, e: AppError) -> AppError {
    let field = format!("appointments.{member}");
    match e {
        AppError::Validation(fields) => AppError::Validation(
            fields
                .into_iter()
                .map(|f| FieldError::new(format!("{field}.{}", f.field), f.code))
                .collect(),
        ),
        AppError::ConflictWith { code, .. }
        | AppError::Conflict { code }
        | AppError::Unprocessable { code } => {
            AppError::Validation(vec![FieldError::new(field, code)])
        }
        other => other,
    }
}

/// `POST /appointments/{id}/move-group` (`appointment.manage`, If-Match of `{id}`): "Đổi cả nhóm"
/// (EM 2026-11-07 item 5c). `{id}` moves to `starts_at` and every other member still booked
/// shifts by the same amount, each keeping its exact length and technician, in one transaction:
/// all move or none. Every member is checked like a single move (grid, opening hours, no past
/// day or slot, technician free, the person has no other booking then); a refusal names the
/// member.
/// Members already arrived or done stay where they are and come back in `not_moved`.
pub async fn move_group(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    version: i64,
    input: &AppointmentGroupMove,
) -> Result<AppointmentGroupMoved, AppError> {
    actor.require(Permission::AppointmentManage)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let spa = spa(&mut tx).await?;
    // Lock every member in id order, `{id}` among them. Locking `{id}` first and then the others
    // deadlocked two moves of one group started from different members (40P01 → 500).
    let mut members = Vec::new();
    if let Some(group) = repo::group_of(&mut tx, id)
        .await?
        .ok_or(AppError::NotFound)?
    {
        for member in repo::group_member_ids(&mut tx, group).await? {
            let row = repo::lock(&mut tx, member)
                .await?
                .ok_or_else(|| AppError::Internal("group member vanished".into()))?;
            members.push((member, row));
        }
    }
    let anchor = match members.iter().find(|(member, _)| *member == id) {
        Some((_, row)) => {
            check_open(actor, &spa, row)?;
            (row.version, row.status.clone(), row.group_id, row.starts_at)
        }
        None => {
            let row = lock_open(&mut tx, actor, &spa, id).await?;
            (row.version, row.status, row.group_id, row.starts_at)
        }
    };
    let (anchor_version, anchor_status, anchor_group, anchor_starts) = anchor;
    if anchor_version != version {
        return Err(AppError::StaleVersion);
    }
    if parse_status(&anchor_status)? != Status::Booked {
        return Err(AppError::Conflict {
            code: "appointment.already_arrived",
        });
    }
    let group = anchor_group.ok_or(AppError::Unprocessable {
        code: "appointment.not_in_group",
    })?;
    if !members.iter().any(|(member, _)| *member == id) {
        // Joined a group between the read and the lock: its version moved on.
        return Err(AppError::StaleVersion);
    }
    let delta = input.starts_at - anchor_starts;
    let now = Utc::now();
    let mut moving = Vec::new();
    let mut staying = Vec::new();
    for (member, row) in members {
        match parse_status(&row.status)? {
            Status::Booked => {
                kernel::corrections::authorize_closed_day(actor, row.business_date, spa.today())?;
                moving.push((member, row));
            }
            Status::Arrived | Status::Done => staying.push(member),
            Status::NoShow | Status::Cancelled => {}
        }
    }
    // Later bookings first when moving later (earlier first when moving earlier): a member never
    // lands on a technician's slot another member of the group still holds before it moves.
    moving.sort_by_key(|(member, row)| (row.starts_at, *member));
    if delta > Duration::zero() {
        moving.reverse();
    }
    let mut moved = Vec::with_capacity(moving.len());
    for (member, row) in &moving {
        let minutes = u32::try_from((row.ends_at - row.starts_at).num_minutes())
            .map_err(|_| AppError::Internal("appointment duration".into()))?;
        let p = place(&spa, row.starts_at + delta, minutes, row.staff_id)
            .map_err(|e| in_member(*member, e))?;
        if p.business_date != row.business_date {
            check_day(&spa, p.business_date).map_err(|e| in_member(*member, e))?;
        }
        check_time(&spa, p.starts_at, now).map_err(|e| in_member(*member, e))?;
        let holder = Holder {
            customer_id: row.customer_id,
            guest_phone: row.guest_phone.as_deref(),
            group_id: Some(group),
        };
        check_fits(&mut tx, &p, Some(*member), Some(holder))
            .await
            .map_err(|e| in_member(*member, e))?;
        if !repo::reschedule(
            &mut tx,
            *member,
            row.version,
            &Reschedule {
                business_date: p.business_date,
                starts_at: p.starts_at,
                ends_at: p.ends_at,
                staff_id: row.staff_id,
                note: row.note.as_deref(),
                customer_id: None,
                companions: &row.companions,
            },
        )
        .await?
        {
            return Err(AppError::StaleVersion);
        }
        record_audit(
            &mut tx,
            actor,
            "appointment.update",
            *member,
            Some(json!({
                "version": row.version, "starts_at": row.starts_at, "ends_at": row.ends_at,
            })),
            json!({
                "version": row.version + 1, "starts_at": p.starts_at, "ends_at": p.ends_at,
                "group_move": {"group_id": group, "from": id},
            }),
            None,
        )
        .await?;
        moved.push(*member);
    }
    let mut out = AppointmentGroupMoved {
        group_id: group,
        moved: Vec::with_capacity(moved.len()),
        not_moved: Vec::with_capacity(staying.len()),
    };
    for member in moved {
        out.moved.push(load_one(&mut tx, member).await?);
    }
    for member in staying {
        out.not_moved.push(load_one(&mut tx, member).await?);
    }
    out.moved.sort_by_key(|a| (a.starts_at, a.id));
    tx.commit().await?;
    Ok(out)
}

/// `POST /appointments/{id}/cancel` / `/no-show` (`appointment.manage`): closes the appointment
/// with a reason and frees its slot (AC9.7). "No-show" only once the booked start has passed.
pub async fn close(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    outcome: Outcome,
    reason: Option<&str>,
) -> Result<AppointmentDto, AppError> {
    actor.require(Permission::AppointmentManage)?;
    let reason = kernel::corrections::reason(reason)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let spa = spa(&mut tx).await?;
    let current = lock_open(&mut tx, actor, &spa, id).await?;
    if outcome == Outcome::NoShow {
        if parse_status(&current.status)? == Status::Arrived {
            return Err(AppError::Conflict {
                code: "appointment.already_arrived",
            });
        }
        if current.starts_at > Utc::now() {
            return Err(AppError::Unprocessable {
                code: "appointment.not_started",
            });
        }
    }
    let status = outcome.status();
    if !repo::close(
        &mut tx,
        id,
        status.as_str(),
        Some(&reason),
        None,
        actor.user_id.as_uuid(),
    )
    .await?
    {
        return Err(AppError::Conflict {
            code: "appointment.closed",
        });
    }
    record_audit(
        &mut tx,
        actor,
        match outcome {
            Outcome::NoShow => "appointment.no_show",
            Outcome::Cancelled => "appointment.cancel",
        },
        id,
        Some(json!({"status": current.status})),
        json!({"status": status.as_str()}),
        Some(&reason),
    )
    .await?;
    let dto = load_one(&mut tx, id).await?;
    tx.commit().await?;
    Ok(dto)
}

/// `POST /appointments/{id}/complete` (`appointment.manage` + `visit.record`, Idempotency-Key):
/// records the visit with the appointment's customer through the very same use case as
/// `POST /visits`, and marks the appointment done with it, in one transaction (AC9.5).
pub async fn complete(
    pool: &PgPool,
    cipher: &FieldCipher,
    actor: &Actor,
    key: Uuid,
    request_hash: [u8; 32],
    id: Uuid,
    input: AppointmentComplete,
) -> Result<(u16, Value), AppError> {
    actor.require(Permission::AppointmentManage)?;
    actor.require(Permission::VisitRecord)?;
    let mut visit = VisitCreate {
        // Replaced by the appointment's customer below; parse never looks at it.
        customer_id: Uuid::nil(),
        check_in_at: input.check_in_at,
        discount: input.discount,
        note: input.note,
        lines: input.lines,
        payments: input.payments,
        package_sales: input.package_sales,
        companions: input.companions,
    };
    let now = Utc::now();

    let mut tx = TenantTx::for_actor(pool, actor).await?;
    if let Claim::Replay { status, body } =
        idempotency::claim(&mut tx, actor, key, request_hash).await?
    {
        tx.commit().await?;
        return Ok((status, body));
    }
    let spa = spa(&mut tx).await?;
    let current = lock_open(&mut tx, actor, &spa, id).await?;
    if current.business_date > spa.today() {
        return Err(AppError::Unprocessable {
            code: "appointment.not_today",
        });
    }
    let customer_id = current.customer_id.ok_or(AppError::Unprocessable {
        code: "appointment.customer_required",
    })?;
    visit.customer_id = customer_id;
    if visit.companions.is_empty() {
        visit.companions.clone_from(&current.companions);
    }
    if visit.check_in_at.is_none() {
        visit.check_in_at = Some(current.arrived_at.unwrap_or(if current.starts_at <= now {
            current.starts_at
        } else {
            now
        }));
    }
    let order = crate::service::parse(actor, &visit)?;
    let visit_id = crate::service::record_in(&mut tx, cipher, actor, order, now).await?;
    if !repo::close(
        &mut tx,
        id,
        Status::Done.as_str(),
        None,
        Some(visit_id),
        actor.user_id.as_uuid(),
    )
    .await?
    {
        return Err(AppError::Conflict {
            code: "appointment.closed",
        });
    }
    record_audit(
        &mut tx,
        actor,
        "appointment.complete",
        id,
        Some(json!({"status": current.status})),
        json!({"status": "done", "visit_id": visit_id}),
        None,
    )
    .await?;
    let done = AppointmentCompleted {
        appointment: load_one(&mut tx, id).await?,
        // Like `POST /visits`, the stored response never carries the note.
        visit: crate::service::load_one(&mut tx, cipher, actor, visit_id, false).await?,
    };
    let body = serde_json::to_value(&done).map_err(|e| AppError::Internal(e.to_string()))?;
    idempotency::complete(&mut tx, actor, key, 200, &body).await?;
    tx.commit().await?;
    Ok((200, body))
}
