//! Pure booking rules (spec appointments.md §2, §10): the spa's calendar settings, how long a
//! booking lasts, whether it fits the opening hours and slot grid, and where each booking or visit
//! stands on the desk's timeline (waiting / in service / done).

use chrono::{DateTime, Duration, NaiveDate, NaiveTime, Timelike, Utc};
use chrono_tz::Tz;
use serde_json::Value;

pub const DEFAULT_OPEN: (u32, u32) = (9, 0);
pub const DEFAULT_CLOSE: (u32, u32) = (21, 0);
pub const DEFAULT_SLOT_MINUTES: u32 = 15;
/// A service without a duration ("theo tư vấn") books one hour.
pub const DEFAULT_SERVICE_MINUTES: u32 = 60;
pub const MAX_DURATION_MINUTES: u32 = 480;
/// Shortest booking or visit block. Lengths are exact (EM 2026-11-07, decision A), not slots.
pub const MIN_DURATION_MINUTES: u32 = 5;
pub const MAX_SERVICES: usize = 5;
/// How far ahead the desk may book.
pub const MAX_DAYS_AHEAD: i64 = 90;
/// Widest `from..to` of one calendar read.
pub const MAX_RANGE_DAYS: i64 = 14;
/// People in one group (family) booking.
pub const MIN_GROUP: usize = 2;
pub const MAX_GROUP: usize = 6;
/// Upper bound of the owner's advisory "max customers per slot" (DB CHECK too).
pub const MAX_PER_SLOT: u32 = 200;

/// Where a booking or a visit stands on the desk's timeline (spec §10.1). Derived on every read
/// from columns that already exist (`arrived_at`, `visit.check_in_at` + service durations): no
/// stored state to keep in sync with the visit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Booked, the customer has not arrived.
    Waiting,
    /// Arrived, or a recorded visit still inside its service time.
    InService,
    /// A recorded visit past its service time.
    Done,
    NoShow,
    Cancelled,
    /// The visit was voided since.
    Voided,
}

/// A recorded visit (walk-in or from a booking): in service until `ends_at`, then done.
pub fn served_phase(voided: bool, ends_at: DateTime<Utc>, now: DateTime<Utc>) -> Phase {
    if voided {
        Phase::Voided
    } else if now < ends_at {
        Phase::InService
    } else {
        Phase::Done
    }
}

/// An appointment's phase. `visit` = (its visit's service start, voided) once done; the service
/// lasts as long as the booking.
pub fn appointment_phase(
    status: Status,
    visit: Option<(DateTime<Utc>, bool)>,
    booked_minutes: i64,
    now: DateTime<Utc>,
) -> Phase {
    match (status, visit) {
        (Status::Booked, _) => Phase::Waiting,
        (Status::Arrived, _) => Phase::InService,
        (Status::NoShow, _) => Phase::NoShow,
        (Status::Cancelled, _) => Phase::Cancelled,
        (Status::Done, Some((start, voided))) => {
            served_phase(voided, start + Duration::minutes(booked_minutes), now)
        }
        (Status::Done, None) => Phase::Done,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Booked,
    Arrived,
    Done,
    NoShow,
    Cancelled,
}

impl Status {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Booked => "booked",
            Self::Arrived => "arrived",
            Self::Done => "done",
            Self::NoShow => "no_show",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [
            Self::Booked,
            Self::Arrived,
            Self::Done,
            Self::NoShow,
            Self::Cancelled,
        ]
        .into_iter()
        .find(|v| v.as_str() == s)
    }

    /// Still holds its slot and may change; closed ones are final (DB trigger too).
    pub const fn is_open(self) -> bool {
        matches!(self, Self::Booked | Self::Arrived)
    }
}

/// Opening hours and slot grid of a spa (`tenant.settings`), with safe defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CalendarSettings {
    pub open: NaiveTime,
    pub close: NaiveTime,
    pub slot_minutes: u32,
}

fn hm(h: u32, m: u32) -> NaiveTime {
    NaiveTime::from_hms_opt(h, m, 0).unwrap_or(NaiveTime::MIN)
}

impl Default for CalendarSettings {
    fn default() -> Self {
        Self {
            open: hm(DEFAULT_OPEN.0, DEFAULT_OPEN.1),
            close: hm(DEFAULT_CLOSE.0, DEFAULT_CLOSE.1),
            slot_minutes: DEFAULT_SLOT_MINUTES,
        }
    }
}

impl CalendarSettings {
    /// `opening_hours = {"open":"HH:MM","close":"HH:MM"}`, `slot_minutes ∈ {5,10,15,20,30,60}`.
    /// Anything malformed falls back to the default instead of failing every booking.
    pub fn from_settings(opening_hours: Option<&Value>, slot_minutes: Option<&Value>) -> Self {
        let d = Self::default();
        let time = |key: &str| {
            opening_hours
                .and_then(|o| o.get(key))
                .and_then(Value::as_str)
                .and_then(|s| NaiveTime::parse_from_str(s, "%H:%M").ok())
        };
        let (open, close) = match (time("open"), time("close")) {
            (Some(o), Some(c)) if o < c => (o, c),
            _ => (d.open, d.close),
        };
        let slot = slot_minutes
            .and_then(Value::as_u64)
            .and_then(|v| u32::try_from(v).ok())
            .filter(|v| [5, 10, 15, 20, 30, 60].contains(v))
            .unwrap_or(d.slot_minutes);
        let on_grid = |t: NaiveTime| (t.hour() * 60 + t.minute()).is_multiple_of(slot);
        if on_grid(open) && on_grid(close) {
            Self {
                open,
                close,
                slot_minutes: slot,
            }
        } else {
            d
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotError {
    /// Outside 5 minutes..8 h.
    BadDuration,
    /// Start not on the slot grid (`starts_at not_on_slot`).
    NotOnSlot,
    /// Starts before opening or ends after closing (`422 appointment.outside_opening_hours`).
    OutsideHours,
}

/// Minutes the booking lasts: the desk's choice, or the services' exact durations (EM 2026-11-07,
/// decision A: a 70-minute service is 09:00–10:10, no longer rounded up to 10:15 by the 15-minute
/// grid). Only the start sits on the grid; the end falls where the service ends.
pub fn duration_minutes(
    service_minutes: &[Option<i32>],
    requested: Option<u32>,
) -> Result<u32, SlotError> {
    let minutes = requested.unwrap_or_else(|| services_minutes(service_minutes));
    if !(MIN_DURATION_MINUTES..=MAX_DURATION_MINUTES).contains(&minutes) {
        return Err(SlotError::BadDuration);
    }
    Ok(minutes)
}

/// The services' durations summed, exactly. A service without a duration ("theo tư vấn") books
/// one hour when nothing else has a duration; next to a timed service it is an add-on done within
/// that time ("Mặt nạ thuốc bắc" with a 30-minute "Gội đầu") and adds nothing. Counting every
/// add-on as an hour drew a 30-minute visit 2.5 hours tall (round 6).
fn services_minutes(service_minutes: &[Option<i32>]) -> u32 {
    let timed: Vec<u32> = service_minutes
        .iter()
        .filter_map(|m| m.and_then(|v| u32::try_from(v).ok()).filter(|v| *v > 0))
        .collect();
    if timed.is_empty() {
        u32::try_from(service_minutes.len())
            .unwrap_or(u32::MAX)
            .saturating_mul(DEFAULT_SERVICE_MINUTES)
    } else {
        timed.iter().sum()
    }
}

/// How long a recorded visit occupies the timeline: its services' exact durations like a booking,
/// clamped to 5 minutes..8 h (a visit is never refused for being long, it is already done).
pub fn visit_minutes(service_minutes: &[Option<i32>]) -> u32 {
    services_minutes(service_minutes).clamp(MIN_DURATION_MINUTES, MAX_DURATION_MINUTES)
}

/// The booking's end, its business day, and whether it fits the day's grid and opening hours.
pub fn place(
    starts_at: DateTime<Utc>,
    minutes: u32,
    tz: Tz,
    settings: CalendarSettings,
) -> Result<(DateTime<Utc>, NaiveDate), SlotError> {
    let local = starts_at.with_timezone(&tz);
    let start = local.time();
    if start.second() != 0
        || start.nanosecond() != 0
        || !(start.hour() * 60 + start.minute()).is_multiple_of(settings.slot_minutes.max(1))
    {
        return Err(SlotError::NotOnSlot);
    }
    let ends_at = starts_at + Duration::minutes(i64::from(minutes));
    let end_local = ends_at.with_timezone(&tz);
    // `close` is always after `open` (`from_settings`), so it is never 00:00 of the next day: a
    // booking ends on the day it starts.
    let same_day = end_local.date_naive() == local.date_naive();
    if start < settings.open || !same_day || end_local.time() > settings.close {
        return Err(SlotError::OutsideHours);
    }
    Ok((ends_at, local.date_naive()))
}

/// Who counts as a technician for the calendar columns: position mentions "kỹ thuật"
/// or "KTV". A spa that names no one so counts every active staff member.
pub fn is_technician(position: &str) -> bool {
    let p = position.to_lowercase();
    p.contains("kỹ thuật") || p.contains("ky thuat") || p.contains("ktv")
}

/// A start whose whole first slot is already over (F12, EM 2026-11-08): it cannot be booked. The
/// slot under way can (19:00 at 19:08 with 15- or 30-minute slots: the customer is arriving).
pub fn slot_is_over(starts_at: DateTime<Utc>, now: DateTime<Utc>, slot_minutes: u32) -> bool {
    starts_at + Duration::minutes(i64::from(slot_minutes)) <= now
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use serde_json::json;

    #[test]
    fn a_slot_already_over_is_past_the_one_under_way_is_not() {
        let now = at(19, 8);
        assert!(!slot_is_over(at(19, 0), now, 15), "the slot under way");
        assert!(!slot_is_over(at(19, 0), now, 30));
        assert!(!slot_is_over(at(19, 15), now, 15), "a later slot");
        assert!(slot_is_over(at(18, 45), now, 15), "ended 19:00");
        assert!(
            slot_is_over(at(10, 0), now, 30),
            "the 10:00 booked at 19:08 (live test)"
        );
        assert!(slot_is_over(at(18, 53), at(19, 8), 15), "ends exactly now");
    }

    fn vn() -> Tz {
        kernel::BusinessDate::parse_tz("Asia/Ho_Chi_Minh").unwrap_or(chrono_tz::UTC)
    }

    fn at(h: u32, m: u32) -> DateTime<Utc> {
        vn().with_ymd_and_hms(2026, 11, 6, h, m, 0)
            .single()
            .map(|t| t.with_timezone(&Utc))
            .unwrap_or_default()
    }

    #[test]
    fn settings_default_and_parse() {
        let d = CalendarSettings::default();
        assert_eq!((d.open, d.close, d.slot_minutes), (hm(9, 0), hm(21, 0), 15));
        let s = CalendarSettings::from_settings(
            Some(&json!({"open": "08:30", "close": "22:00"})),
            Some(&json!(30)),
        );
        assert_eq!(
            (s.open, s.close, s.slot_minutes),
            (hm(8, 30), hm(22, 0), 30)
        );
        // Malformed or off-grid values fall back.
        for (o, sl) in [
            (json!({"open": "x", "close": "22:00"}), json!(15)),
            (json!({"open": "22:00", "close": "08:00"}), json!(15)),
            (json!({"open": "08:10", "close": "22:00"}), json!(15)),
        ] {
            assert_eq!(CalendarSettings::from_settings(Some(&o), Some(&sl)), d);
        }
        assert_eq!(
            CalendarSettings::from_settings(None, Some(&json!(7))).slot_minutes,
            15
        );
    }

    #[test]
    fn duration_is_the_services_exact_time() {
        assert_eq!(duration_minutes(&[Some(60)], None), Ok(60));
        // Decision A (EM 2026-11-07): 70 minutes stay 70, not the 75 of the 15-minute grid.
        assert_eq!(duration_minutes(&[Some(70)], None), Ok(70));
        assert_eq!(duration_minutes(&[Some(40)], None), Ok(40));
        // An untimed service is an hour on its own, an add-on next to a timed one.
        assert_eq!(duration_minutes(&[Some(50), None], None), Ok(50));
        assert_eq!(duration_minutes(&[Some(30), None, None], None), Ok(30));
        assert_eq!(duration_minutes(&[None], None), Ok(60));
        assert_eq!(duration_minutes(&[None, None], None), Ok(120));
        // The desk's own length is exact too, 5 minutes..8 hours.
        assert_eq!(duration_minutes(&[Some(60)], Some(90)), Ok(90));
        assert_eq!(duration_minutes(&[Some(60)], Some(50)), Ok(50));
        for bad in [4, 481] {
            assert_eq!(
                duration_minutes(&[Some(60)], Some(bad)),
                Err(SlotError::BadDuration)
            );
        }
        assert_eq!(
            duration_minutes(&[Some(300), Some(300)], None),
            Err(SlotError::BadDuration)
        );
    }

    #[test]
    fn a_booking_must_sit_on_the_grid_inside_opening_hours() {
        let s = CalendarSettings::default();
        let day = NaiveDate::from_ymd_opt(2026, 11, 6).unwrap_or_default();
        assert_eq!(place(at(10, 0), 60, vn(), s), Ok((at(11, 0), day)));
        assert_eq!(place(at(20, 0), 60, vn(), s), Ok((at(21, 0), day)));
        assert_eq!(place(at(10, 7), 60, vn(), s), Err(SlotError::NotOnSlot));
        assert_eq!(place(at(8, 45), 60, vn(), s), Err(SlotError::OutsideHours));
        assert_eq!(place(at(20, 30), 60, vn(), s), Err(SlotError::OutsideHours));
        // Only the start is on the grid: 09:00 + 70 minutes ends at 10:10.
        assert_eq!(place(at(9, 0), 70, vn(), s), Ok((at(10, 10), day)));
        assert_eq!(place(at(19, 45), 70, vn(), s), Ok((at(20, 55), day)));
        assert_eq!(place(at(20, 0), 70, vn(), s), Err(SlotError::OutsideHours));
    }

    #[test]
    fn a_visit_occupies_its_services_exact_time() {
        assert_eq!(visit_minutes(&[Some(90)]), 90);
        assert_eq!(visit_minutes(&[Some(70)]), 70);
        assert_eq!(visit_minutes(&[Some(40), None]), 40);
        assert_eq!(visit_minutes(&[Some(90), None]), 90);
        assert_eq!(visit_minutes(&[None]), 60);
        assert_eq!(visit_minutes(&[Some(2)]), 5);
        assert_eq!(visit_minutes(&[]), 5);
        assert_eq!(visit_minutes(&[Some(300), Some(300)]), 480);
    }

    #[test]
    fn phases_follow_status_and_service_time() {
        let (ten, half_past, noon) = (at(10, 0), at(10, 30), at(12, 0));
        assert_eq!(
            appointment_phase(Status::Booked, None, 60, noon),
            Phase::Waiting
        );
        assert_eq!(
            appointment_phase(Status::Arrived, None, 60, noon),
            Phase::InService
        );
        // Done at 10:00 for 60 minutes: in service at 10:30, done by noon; voided wins.
        assert_eq!(
            appointment_phase(Status::Done, Some((ten, false)), 60, half_past),
            Phase::InService
        );
        assert_eq!(
            appointment_phase(Status::Done, Some((ten, false)), 60, noon),
            Phase::Done
        );
        assert_eq!(
            appointment_phase(Status::Done, Some((ten, true)), 60, half_past),
            Phase::Voided
        );
        assert_eq!(
            appointment_phase(Status::NoShow, None, 60, noon),
            Phase::NoShow
        );
        assert_eq!(
            appointment_phase(Status::Cancelled, None, 60, noon),
            Phase::Cancelled
        );
        // The end is exclusive, like every interval on the grid.
        assert_eq!(served_phase(false, at(11, 0), at(11, 0)), Phase::Done);
    }

    #[test]
    fn technicians_are_recognised_by_position() {
        assert!(is_technician("Kỹ thuật viên"));
        assert!(is_technician("KTV chính"));
        assert!(!is_technician("Quản lý"));
    }

    #[test]
    fn statuses_round_trip() {
        for s in ["booked", "arrived", "done", "no_show", "cancelled"] {
            assert_eq!(Status::parse(s).map(Status::as_str), Some(s));
        }
        assert!(Status::Booked.is_open() && Status::Arrived.is_open());
        assert!(!Status::Done.is_open());
    }
}
