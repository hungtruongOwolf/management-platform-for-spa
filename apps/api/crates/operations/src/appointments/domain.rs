//! Pure booking rules (spec appointments.md §2): the spa's calendar settings, how long a booking
//! lasts, whether it fits the opening hours and slot grid, and how many bookings overlap.

use chrono::{DateTime, Duration, NaiveDate, NaiveTime, Timelike, Utc};
use chrono_tz::Tz;
use serde_json::Value;

pub const DEFAULT_OPEN: (u32, u32) = (9, 0);
pub const DEFAULT_CLOSE: (u32, u32) = (21, 0);
pub const DEFAULT_SLOT_MINUTES: u32 = 15;
/// A service without a duration ("theo tư vấn") books one hour.
pub const DEFAULT_SERVICE_MINUTES: u32 = 60;
pub const MAX_DURATION_MINUTES: u32 = 480;
pub const MAX_SERVICES: usize = 5;
/// How far ahead the desk may book.
pub const MAX_DAYS_AHEAD: i64 = 90;
/// Widest `from..to` of one calendar read.
pub const MAX_RANGE_DAYS: i64 = 14;

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
        let on_grid = |t: NaiveTime| (t.hour() * 60 + t.minute()) % slot == 0;
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
    /// Not a multiple of the slot, or outside 1 slot..8 h.
    BadDuration,
    /// Start not on the slot grid (`starts_at not_on_slot`).
    NotOnSlot,
    /// Starts before opening or ends after closing (`422 appointment.outside_opening_hours`).
    OutsideHours,
}

/// Minutes the booking lasts: the desk's choice, or the services' durations (empty = 1 hour)
/// rounded up to the slot grid.
pub fn duration_minutes(
    service_minutes: &[Option<i32>],
    requested: Option<u32>,
    slot: u32,
) -> Result<u32, SlotError> {
    let slot = slot.max(1);
    let minutes = match requested {
        Some(m) => {
            if m % slot != 0 {
                return Err(SlotError::BadDuration);
            }
            m
        }
        None => {
            let sum: u32 = service_minutes
                .iter()
                .map(|m| {
                    m.and_then(|v| u32::try_from(v).ok())
                        .filter(|v| *v > 0)
                        .unwrap_or(DEFAULT_SERVICE_MINUTES)
                })
                .sum();
            sum.div_ceil(slot) * slot
        }
    };
    if minutes < slot || minutes > MAX_DURATION_MINUTES {
        return Err(SlotError::BadDuration);
    }
    Ok(minutes)
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
        || (start.hour() * 60 + start.minute()) % settings.slot_minutes.max(1) != 0
    {
        return Err(SlotError::NotOnSlot);
    }
    let ends_at = starts_at + Duration::minutes(i64::from(minutes));
    let end_local = ends_at.with_timezone(&tz);
    let same_day = end_local.date_naive() == local.date_naive()
        || (end_local.date_naive() == local.date_naive().succ_opt().unwrap_or(NaiveDate::MAX)
            && end_local.time() == NaiveTime::MIN
            && settings.close == NaiveTime::MIN);
    if start < settings.open || !same_day || end_local.time() > settings.close {
        return Err(SlotError::OutsideHours);
    }
    Ok((ends_at, local.date_naive()))
}

/// Largest number of `others` running at the same instant inside `[from, to)`. Intervals are
/// half-open, so a booking ending at 11:00 and one starting at 11:00 do not overlap.
pub fn peak_overlap(others: &[(DateTime<Utc>, DateTime<Utc>)], from: DateTime<Utc>, to: DateTime<Utc>) -> usize {
    // The count only rises at a start, so checking every start inside the window (and the window
    // start itself) finds the peak.
    let mut probes: Vec<DateTime<Utc>> = vec![from];
    probes.extend(
        others
            .iter()
            .map(|(s, _)| *s)
            .filter(|s| *s > from && *s < to),
    );
    probes
        .into_iter()
        .map(|t| others.iter().filter(|(s, e)| *s <= t && t < *e).count())
        .max()
        .unwrap_or(0)
}

/// Who counts as a technician for capacity and calendar columns: position mentions "kỹ thuật"
/// or "KTV". A spa that names no one so counts every active staff member.
pub fn is_technician(position: &str) -> bool {
    let p = position.to_lowercase();
    p.contains("kỹ thuật") || p.contains("ky thuat") || p.contains("ktv")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use serde_json::json;

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
        assert_eq!((s.open, s.close, s.slot_minutes), (hm(8, 30), hm(22, 0), 30));
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
    fn duration_comes_from_services_rounded_to_the_slot() {
        assert_eq!(duration_minutes(&[Some(60)], None, 15), Ok(60));
        assert_eq!(duration_minutes(&[Some(50), None], None, 15), Ok(120));
        assert_eq!(duration_minutes(&[Some(40)], None, 15), Ok(45));
        assert_eq!(duration_minutes(&[Some(60)], Some(90), 15), Ok(90));
        assert_eq!(
            duration_minutes(&[Some(60)], Some(50), 15),
            Err(SlotError::BadDuration)
        );
        assert_eq!(
            duration_minutes(&[Some(60)], Some(495), 15),
            Err(SlotError::BadDuration)
        );
        assert_eq!(
            duration_minutes(&[Some(300), Some(300)], None, 15),
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
    }

    #[test]
    fn peak_overlap_counts_simultaneous_bookings_half_open() {
        let others = [(at(10, 0), at(11, 0)), (at(10, 30), at(11, 30)), (at(11, 0), at(12, 0))];
        assert_eq!(peak_overlap(&others, at(10, 0), at(12, 0)), 2);
        assert_eq!(peak_overlap(&others, at(9, 0), at(10, 0)), 0);
        assert_eq!(peak_overlap(&others, at(11, 30), at(12, 0)), 1);
        assert_eq!(peak_overlap(&[], at(9, 0), at(10, 0)), 0);
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
