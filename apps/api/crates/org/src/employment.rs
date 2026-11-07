//! Employment periods (ADR-0012): a staff member works at the spa during one or more periods —
//! hired, possibly ended (nghỉ việc), possibly rehired. Every "was this person working on day D /
//! in month M" question and the tenure ("làm bao lâu") are answered here, from the periods, so a
//! rehired person's gap is neither unrecorded attendance nor employed time.

use chrono::{Datelike, Duration, NaiveDate};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Period {
    pub id: Uuid,
    /// `None` = before the books (a profile created without a start date).
    pub started_on: Option<NaiveDate>,
    /// `None` = still working.
    pub ended_on: Option<NaiveDate>,
    pub end_reason: Option<String>,
}

impl Period {
    pub fn covers(&self, day: NaiveDate) -> bool {
        self.started_on.is_none_or(|s| day >= s) && self.ended_on.is_none_or(|e| day <= e)
    }

    fn overlaps(&self, first: NaiveDate, last: NaiveDate) -> bool {
        self.started_on.is_none_or(|s| s <= last) && self.ended_on.is_none_or(|e| e >= first)
    }
}

/// How long someone has worked here, summed over their periods up to a day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tenure {
    /// Calendar days worked (both ends included).
    pub days: i64,
    /// Completed calendar months, per period, summed.
    pub months: i64,
}

/// One person's periods, oldest first. Never empty for a stored profile (0025 trigger); an empty
/// one means "never employed".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Employment(Vec<Period>);

impl Employment {
    pub fn new(mut periods: Vec<Period>) -> Self {
        periods.sort_by_key(|p| p.started_on);
        Self(periods)
    }

    /// Always employed (a profile with no recorded dates), for tests and pure calculators.
    pub fn always() -> Self {
        Self(vec![Period {
            id: Uuid::nil(),
            started_on: None,
            ended_on: None,
            end_reason: None,
        }])
    }

    /// A single period, for tests and pure calculators.
    pub fn between(started_on: Option<NaiveDate>, ended_on: Option<NaiveDate>) -> Self {
        Self(vec![Period {
            id: Uuid::nil(),
            started_on,
            ended_on,
            end_reason: None,
        }])
    }

    pub fn periods(&self) -> &[Period] {
        &self.0
    }

    pub fn latest(&self) -> Option<&Period> {
        self.0.last()
    }

    /// The open period, if the person is still working.
    pub fn current(&self) -> Option<&Period> {
        self.0.last().filter(|p| p.ended_on.is_none())
    }

    /// Start of the first period (`None` = unknown or never employed).
    pub fn first_started_on(&self) -> Option<NaiveDate> {
        self.0.first().and_then(|p| p.started_on)
    }

    pub fn on(&self, day: NaiveDate) -> bool {
        self.0.iter().any(|p| p.covers(day))
    }

    /// Worked at least one day of `first..=last`.
    pub fn overlaps(&self, first: NaiveDate, last: NaiveDate) -> bool {
        self.0.iter().any(|p| p.overlaps(first, last))
    }

    /// Days of `first..=last` this person worked.
    pub fn days_in(&self, first: NaiveDate, last: NaiveDate) -> u32 {
        let mut n = 0;
        let mut d = first;
        while d <= last {
            if self.on(d) {
                n += 1;
            }
            d += Duration::days(1);
        }
        n
    }

    /// Tenure up to `today` (included). `None` when a start date is unknown: the UI says so
    /// instead of guessing. Periods starting after `today` count nothing yet.
    pub fn tenure(&self, today: NaiveDate) -> Option<Tenure> {
        if self.0.is_empty() {
            return Some(Tenure { days: 0, months: 0 });
        }
        let mut t = Tenure { days: 0, months: 0 };
        for p in &self.0 {
            let start = p.started_on?;
            let end = p.ended_on.map_or(today, |e| e.min(today));
            if end < start {
                continue;
            }
            t.days += (end - start).num_days() + 1;
            t.months += completed_months(start, end + Duration::days(1));
        }
        Some(t)
    }
}

/// Whole calendar months from `start` to `end_exclusive` (5 Jan → 5 Feb = 1).
fn completed_months(start: NaiveDate, end_exclusive: NaiveDate) -> i64 {
    let months = i64::from(end_exclusive.year() - start.year()) * 12
        + i64::from(end_exclusive.month()) - i64::from(start.month());
    if end_exclusive.day() < start.day() {
        months - 1
    } else {
        months
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap_or_default()
    }

    fn period(start: Option<&str>, end: Option<&str>) -> Period {
        Period {
            id: Uuid::nil(),
            started_on: start.map(d),
            ended_on: end.map(d),
            end_reason: None,
        }
    }

    #[test]
    fn a_rehired_persons_gap_is_not_employment() {
        let e = Employment::new(vec![
            period(Some("2026-05-10"), None),
            period(Some("2025-01-06"), Some("2025-12-31")),
        ]);
        assert!(e.on(d("2025-01-06")));
        assert!(e.on(d("2025-12-31")));
        assert!(!e.on(d("2026-03-01")), "gap");
        assert!(e.on(d("2026-05-10")));
        assert!(!e.overlaps(d("2026-02-01"), d("2026-02-28")));
        assert!(e.overlaps(d("2026-05-01"), d("2026-05-31")));
        assert_eq!(e.days_in(d("2026-05-01"), d("2026-05-31")), 22);
        assert_eq!(e.first_started_on(), Some(d("2025-01-06")));
        assert_eq!(
            e.current().and_then(|p| p.started_on),
            Some(d("2026-05-10"))
        );
    }

    #[test]
    fn tenure_sums_the_periods_up_to_today() {
        let e = Employment::new(vec![
            period(Some("2025-01-06"), Some("2025-12-31")),
            period(Some("2026-05-10"), None),
        ]);
        let t = e.tenure(d("2026-11-07"));
        // 2025: 360 days, 11 full months (6 Jan → 6 Dec) ; 2026: 182 days, 5 months (10 May → 10 Oct).
        assert_eq!(t, Some(Tenure { days: 360 + 182, months: 11 + 5 }));
        assert_eq!(
            Employment::between(Some(d("2026-01-05")), None).tenure(d("2026-02-04")),
            Some(Tenure { days: 31, months: 1 }),
            "5 Jan → 4 Feb inclusive is one month"
        );
        assert_eq!(
            Employment::between(Some(d("2026-12-01")), None).tenure(d("2026-11-07")),
            Some(Tenure { days: 0, months: 0 }),
            "starts in the future"
        );
        assert_eq!(Employment::always().tenure(d("2026-11-07")), None);
    }

    #[test]
    fn whole_months_handle_month_ends() {
        assert_eq!(completed_months(d("2026-01-31"), d("2026-03-01")), 1);
        assert_eq!(completed_months(d("2026-01-31"), d("2026-02-28")), 0);
        assert_eq!(completed_months(d("2024-02-29"), d("2026-03-01")), 24);
    }
}
