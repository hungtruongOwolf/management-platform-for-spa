//! The spa's business day, computed on the server in the tenant's time zone.
//! A visit at 23:30 UTC on 4 Nov belongs to business day 5 Nov in Asia/Ho_Chi_Minh.

use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BusinessDate(Date);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown time zone: {0}")]
pub struct UnknownTimeZone(pub String);

impl BusinessDate {
    /// The calendar day `instant` falls on in `tz`.
    pub fn of_instant(instant: Timestamp, tz: &TimeZone) -> Self {
        Self(tz.to_datetime(instant).date())
    }

    pub const fn as_date(self) -> Date {
        self.0
    }

    /// Parses an IANA zone name such as the tenant's `timezone` column, from the zone database
    /// compiled into the binary.
    pub fn parse_tz(name: &str) -> Result<TimeZone, UnknownTimeZone> {
        TimeZone::get(name).map_err(|_| UnknownTimeZone(name.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    fn vietnam() -> TimeZone {
        BusinessDate::parse_tz("Asia/Ho_Chi_Minh").unwrap_or(TimeZone::UTC)
    }

    fn utc(y: i16, m: i8, d: i8, h: i8, min: i8, s: i8) -> Timestamp {
        date(y, m, d)
            .at(h, min, s, 0)
            .to_zoned_datetime(TimeZone::UTC)
            .map(|z| z.timestamp())
            .unwrap_or_default()
    }

    #[test]
    fn late_utc_evening_is_next_day_in_vietnam() {
        let instant = utc(2026, 11, 4, 17, 30, 0);
        assert_eq!(
            BusinessDate::of_instant(instant, &vietnam()).as_date(),
            date(2026, 11, 5)
        );
    }

    #[test]
    fn early_utc_is_same_day_in_vietnam() {
        let instant = utc(2026, 11, 4, 16, 59, 59);
        assert_eq!(
            BusinessDate::of_instant(instant, &vietnam()).as_date(),
            date(2026, 11, 4)
        );
    }

    #[test]
    fn the_zone_database_is_compiled_in() {
        // Vietnam has kept UTC+7 since 1975; the database answers without the host's zoneinfo.
        let tz = vietnam();
        assert_eq!(tz.iana_name(), Some("Asia/Ho_Chi_Minh"));
        assert_eq!(tz.to_offset(utc(2026, 6, 1, 0, 0, 0)).seconds(), 7 * 3600);
    }

    #[test]
    fn rejects_unknown_zone() {
        assert_eq!(
            BusinessDate::parse_tz("Mars/Olympus"),
            Err(UnknownTimeZone("Mars/Olympus".to_owned()))
        );
    }
}
