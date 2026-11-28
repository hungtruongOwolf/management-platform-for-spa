//! A calendar month (`YYYY-MM`): the unit of attendance sheets, evaluations and payroll.

use jiff::ToSpan;
use jiff::civil::Date;
use kernel::{AppError, FieldError};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Month(Date);

impl Month {
    /// Earliest month the API accepts (well before any spa on the platform).
    const MIN_YEAR: i16 = 2020;
    const MAX_YEAR: i16 = 2100;

    pub fn of(day: Date) -> Self {
        Self(day.first_of_month())
    }

    /// Parses `YYYY-MM`; `field` names the input in the validation error.
    pub fn parse(field: &str, raw: &str) -> Result<Self, AppError> {
        let invalid = || AppError::Validation(vec![FieldError::new(field, "invalid_month")]);
        let (y, m) = raw.trim().split_once('-').ok_or_else(invalid)?;
        if y.len() != 4 || m.len() != 2 {
            return Err(invalid());
        }
        let year: i16 = y.parse().map_err(|_| invalid())?;
        let month: i8 = m.parse().map_err(|_| invalid())?;
        if !(Self::MIN_YEAR..=Self::MAX_YEAR).contains(&year) {
            return Err(invalid());
        }
        Date::new(year, month, 1).map(Self).map_err(|_| invalid())
    }

    pub const fn first_day(self) -> Date {
        self.0
    }

    pub fn last_day(self) -> Date {
        self.0.last_of_month()
    }

    pub fn next(self) -> Self {
        Self(self.0.checked_add(1.month()).unwrap_or(self.0))
    }

    pub fn days(self) -> u32 {
        u32::from(self.0.days_in_month().unsigned_abs())
    }

    pub fn contains(self, day: Date) -> bool {
        Self::of(day) == self
    }

    /// Every day of the month, in order.
    pub fn iter_days(self) -> impl Iterator<Item = Date> {
        let last = self.last_day();
        self.0.series(1.day()).take_while(move |d| *d <= last)
    }
}

impl fmt::Display for Month {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}", self.0.year(), self.0.month())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_bounds_months() -> Result<(), AppError> {
        let feb = Month::parse("month", "2028-02")?;
        assert_eq!(feb.days(), 29, "leap year");
        assert_eq!(feb.to_string(), "2028-02");
        assert_eq!(feb.next().to_string(), "2028-03");
        assert_eq!(
            Month::parse("month", "2026-12")?.next().to_string(),
            "2027-01"
        );
        assert_eq!(Month::parse("month", "2026-11")?.iter_days().count(), 30);
        for bad in ["2026-13", "2026-1", "26-01", "2019-12", "abc", "2026/01"] {
            assert!(Month::parse("month", bad).is_err(), "{bad}");
        }
        Ok(())
    }
}
