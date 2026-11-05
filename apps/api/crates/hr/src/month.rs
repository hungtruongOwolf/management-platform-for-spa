//! A calendar month (`YYYY-MM`): the unit of attendance sheets, evaluations and payroll.

use chrono::{Datelike, Months, NaiveDate};
use kernel::{AppError, FieldError};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Month(NaiveDate);

impl Month {
    /// Earliest month the API accepts (well before any spa on the platform).
    const MIN_YEAR: i32 = 2020;
    const MAX_YEAR: i32 = 2100;

    pub fn of(day: NaiveDate) -> Self {
        Self(day.with_day(1).unwrap_or(day))
    }

    pub fn from_first_day(day: NaiveDate) -> Option<Self> {
        (day.day() == 1).then_some(Self(day))
    }

    /// Parses `YYYY-MM`; `field` names the input in the validation error.
    pub fn parse(field: &str, raw: &str) -> Result<Self, AppError> {
        let invalid = || AppError::Validation(vec![FieldError::new(field, "invalid_month")]);
        let (y, m) = raw.trim().split_once('-').ok_or_else(invalid)?;
        if y.len() != 4 || m.len() != 2 {
            return Err(invalid());
        }
        let year: i32 = y.parse().map_err(|_| invalid())?;
        let month: u32 = m.parse().map_err(|_| invalid())?;
        if !(Self::MIN_YEAR..=Self::MAX_YEAR).contains(&year) {
            return Err(invalid());
        }
        NaiveDate::from_ymd_opt(year, month, 1)
            .map(Self)
            .ok_or_else(invalid)
    }

    pub const fn first_day(self) -> NaiveDate {
        self.0
    }

    pub fn last_day(self) -> NaiveDate {
        self.next()
            .0
            .pred_opt()
            .unwrap_or(self.0)
    }

    pub fn next(self) -> Self {
        Self(self.0.checked_add_months(Months::new(1)).unwrap_or(self.0))
    }

    pub fn days(self) -> u32 {
        self.last_day().day()
    }

    pub fn contains(self, day: NaiveDate) -> bool {
        Self::of(day) == self
    }

    /// Every day of the month, in order.
    pub fn iter_days(self) -> impl Iterator<Item = NaiveDate> {
        let last = self.last_day();
        self.0.iter_days().take_while(move |d| *d <= last)
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
        assert_eq!(Month::parse("month", "2026-12")?.next().to_string(), "2027-01");
        assert_eq!(Month::parse("month", "2026-11")?.iter_days().count(), 30);
        for bad in ["2026-13", "2026-1", "26-01", "2019-12", "abc", "2026/01"] {
            assert!(Month::parse("month", bad).is_err(), "{bad}");
        }
        Ok(())
    }
}
