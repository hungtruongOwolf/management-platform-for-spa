//! Appointment use cases (spec appointments.md). Every change runs in one `TenantTx`; bookings of
//! one business day are serialized by an advisory lock so the technician-overlap check holds
//! under concurrency (the `EXCLUDE` constraint is the second wall). The spa-wide "max per slot"
//! is advisory since round 5 (§10.4): shown and warned about on the calendar, never refused here.
//! Completing an appointment records its visit through [`crate::service::record_in`]: the same
//! pricing, session, payment and discount rules as `POST /visits`, in the same transaction.

use super::domain::{self, CalendarSettings, Phase, SlotError, Status};
use super::dto::{
    AppointmentArrive, AppointmentComplete, AppointmentCompleted, AppointmentCreate,
    AppointmentDto, AppointmentGroupCreate, AppointmentGroupDto, AppointmentServiceDto,
    AppointmentSettingsDto, AppointmentSettingsUpdate, AppointmentStatusDto, AppointmentUpdate,
    CalendarDto, CalendarQuery, CalendarTechnician, CalendarVisitDto, GuestCreate, PaymentStateDto,
    PhaseDto,
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

/// Day rules: past days follow the closed-day rule (owner only), at most 90 days ahead.
fn check_day(actor: &Actor, spa: &Spa, day: NaiveDate) -> Result<(), AppError> {
    kernel::corrections::authorize_closed_day(actor, day, spa.today())?;
    if day > spa.today() + Duration::days(domain::MAX_DAYS_AHEAD) {
        return Err(invalid("starts_at", "too_far"));
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

/// The technician (if any) is free over the whole booking (AC9.2). Takes the day lock first, so
/// the answer stays true until the caller commits.
async fn check_fits(
    tx: &mut TenantTx,
    p: &Placement,
    except: Option<Uuid>,
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
        let minutes = domain::visit_minutes(&durations, spa.settings.slot_minutes);
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
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
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
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
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
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let found = load(&mut tx, Some(id), None, None).await?.pop();
    tx.commit().await?;
    found.ok_or(AppError::NotFound)
}

// ----------------------------------------------------------------------------------- writes

/// Who the booking is for: an existing customer or a newcomer (name + E.164 phone).
enum Who {
    Customer(Uuid),
    Guest { name: String, phone: PhoneE164 },
}

fn parse_who(customer_id: Option<Uuid>, guest: Option<&GuestCreate>) -> Result<Who, AppError> {
    match (customer_id, guest) {
        (Some(id), None) => Ok(Who::Customer(id)),
        (None, Some(g)) => {
            let name = g.full_name.trim();
            let mut errors = Vec::new();
            if name.is_empty() || name.chars().count() > MAX_NAME {
                errors.push(FieldError::new("guest.full_name", "required"));
            }
            let phone = PhoneE164::parse(&g.phone);
            if phone.is_err() {
                errors.push(FieldError::new("guest.phone", "invalid"));
            }
            match phone {
                Ok(phone) if errors.is_empty() => Ok(Who::Guest {
                    name: name.to_owned(),
                    phone,
                }),
                _ => Err(AppError::Validation(errors)),
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
        Who::Guest { phone, .. } => {
            if let Some((existing, _)) = crm::service::phone_holder_in(tx, phone).await? {
                return Err(AppError::ConflictWith {
                    code: "customer.phone_taken",
                    existing_id: existing,
                });
            }
            None
        }
    };
    let services = resolve_services(tx, b.service_ids).await?;
    let durations: Vec<Option<i32>> = services.iter().map(|s| s.2).collect();
    let minutes = domain::duration_minutes(&durations, b.duration_min, spa.settings.slot_minutes)
        .map_err(slot_error)?;
    let p = place(spa, b.starts_at, minutes, b.staff_id)?;
    check_day(actor, spa, p.business_date)?;
    check_fits(tx, &p, None).await?;

    let id = Uuid::now_v7();
    let (guest_name, guest_phone) = match &b.who {
        Who::Guest { name, phone } => (Some(name.as_str()), Some(phone.as_str())),
        Who::Customer(_) => (None, None),
    };
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
    let booking = Booking {
        who: parse_who(input.customer_id, input.guest.as_ref())?,
        service_ids: &input.service_ids,
        staff_id: input.staff_id,
        starts_at: input.starts_at,
        duration_min: input.duration_min,
    };
    let note = clean_note(input.note.as_ref())?;
    let companions = companions(&input.companions)?;

    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    if let Claim::Replay { status, body } =
        idempotency::claim(&mut tx, actor, key, request_hash).await?
    {
        tx.commit().await?;
        return Ok((status, body));
    }
    let spa = spa(&mut tx).await?;
    let (id, _) = insert_booking(
        &mut tx,
        actor,
        &spa,
        &booking,
        note.as_deref(),
        None,
        &companions,
    )
    .await?;
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
        let who =
            parse_who(person.customer_id, person.guest.as_ref()).map_err(|e| in_person(i, e))?;
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

    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
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

/// Locks an appointment that must still be open, applying the closed-day rule to its day.
async fn lock_open(
    tx: &mut TenantTx,
    actor: &Actor,
    spa: &Spa,
    id: Uuid,
) -> Result<repo::Locked, AppError> {
    let row = repo::lock(tx, id).await?.ok_or(AppError::NotFound)?;
    if !parse_status(&row.status)?.is_open() {
        return Err(AppError::Conflict {
            code: "appointment.closed",
        });
    }
    kernel::corrections::authorize_closed_day(actor, row.business_date, spa.today())?;
    Ok(row)
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

    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
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
            domain::duration_minutes(&d, requested, spa.settings.slot_minutes)
                .map_err(slot_error)?
        }
        (Some(m), None) => {
            domain::duration_minutes(&[], Some(m), spa.settings.slot_minutes).map_err(slot_error)?
        }
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
        check_day(actor, &spa, p.business_date)?;
    }
    check_fits(&mut tx, &p, Some(id)).await?;
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
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
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
    check_fits(&mut tx, &p, Some(id)).await?;
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
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
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

    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
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
