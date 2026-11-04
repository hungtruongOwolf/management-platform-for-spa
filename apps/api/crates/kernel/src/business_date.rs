//! The spa's business day, computed on the server in the tenant's time zone.
//! A visit at 23:30 UTC on 4 Nov belongs to business day 5 Nov in Asia/Ho_Chi_Minh.

use chrono::{DateTime, NaiveDate, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, sqlx::Type,
)]
#[serde(transparent)]
#[sqlx(transparent)]
pub struct BusinessDate(NaiveDate);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown time zone: {0}")]
pub struct UnknownTimeZone(pub String);

impl BusinessDate {
    pub fn of_instant(instant: DateTime<Utc>, tz: Tz) -> Self {
        Self(instant.with_timezone(&tz).date_naive())
    }

    pub fn today(tz: Tz) -> Self {
        Self::of_instant(Utc::now(), tz)
    }

    pub const fn as_naive(self) -> NaiveDate {
        self.0
    }

    /// Parses an IANA zone name such as the tenant's `timezone` column.
    pub fn parse_tz(name: &str) -> Result<Tz, UnknownTimeZone> {
        name.parse().map_err(|_| UnknownTimeZone(name.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn late_utc_evening_is_next_day_in_vietnam() {
        let tz = BusinessDate::parse_tz("Asia/Ho_Chi_Minh").unwrap_or(chrono_tz::UTC);
        let instant = Utc.with_ymd_and_hms(2026, 11, 4, 17, 30, 0).single();
        let date = instant.map(|i| BusinessDate::of_instant(i, tz).as_naive());
        assert_eq!(date, NaiveDate::from_ymd_opt(2026, 11, 5));
    }

    #[test]
    fn early_utc_is_same_day_in_vietnam() {
        let tz = BusinessDate::parse_tz("Asia/Ho_Chi_Minh").unwrap_or(chrono_tz::UTC);
        let instant = Utc.with_ymd_and_hms(2026, 11, 4, 16, 59, 59).single();
        let date = instant.map(|i| BusinessDate::of_instant(i, tz).as_naive());
        assert_eq!(date, NaiveDate::from_ymd_opt(2026, 11, 4));
    }

    #[test]
    fn rejects_unknown_zone() {
        assert_eq!(
            BusinessDate::parse_tz("Mars/Olympus"),
            Err(UnknownTimeZone("Mars/Olympus".to_owned()))
        );
    }
}
