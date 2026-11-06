//! Pure visit rules: when a check-in may be recorded, and which gender prices a one-off line.

use catalog::{Gender as PriceGender, PriceVariant};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use chrono_tz::Tz;
use crm::service::Gender;
use kernel::BusinessDate;

/// Clock skew tolerated between the counter's device and the server.
pub const FUTURE_TOLERANCE_MINUTES: i64 = 5;

/// How far back the owner may enter a visit (spec appointments.md §3).
pub const MAX_BACKDATE_DAYS: i64 = 366;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckInError {
    /// More than the tolerance ahead of the server clock (`422 visit.check_in_in_future`).
    InFuture,
    /// On an earlier business day, without `visit.backdate` (`422 visit.check_in_not_today`).
    NotToday,
    /// Earlier than [`MAX_BACKDATE_DAYS`] before today (`422 visit.check_in_too_old`).
    TooOld,
}

/// When the service happened (`visit.check_in_at`) and the business day it belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceTime {
    pub at: DateTime<Utc>,
    pub business_date: NaiveDate,
    /// On a day before today: entered late by the owner.
    pub backdated: bool,
}

/// The service instant to record: `requested`, or `now` when the desk sent none. Today only,
/// unless the caller may back-date (`visit.backdate`, the owner), then up to a year back.
pub fn service_time(
    requested: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    tz: Tz,
    today: NaiveDate,
    may_backdate: bool,
) -> Result<ServiceTime, CheckInError> {
    let at = requested.unwrap_or(now);
    if at > now + Duration::minutes(FUTURE_TOLERANCE_MINUTES) {
        return Err(CheckInError::InFuture);
    }
    let day = BusinessDate::of_instant(at, tz).as_naive();
    // A few minutes of skew past midnight still belongs to today.
    let business_date = day.min(today);
    if business_date < today {
        if !may_backdate {
            return Err(CheckInError::NotToday);
        }
        if (today - business_date).num_days() > MAX_BACKDATE_DAYS {
            return Err(CheckInError::TooOld);
        }
    }
    Ok(ServiceTime {
        at,
        business_date,
        backdated: business_date < today,
    })
}

impl CheckInError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::InFuture => "visit.check_in_in_future",
            Self::NotToday => "visit.check_in_not_today",
            Self::TooOld => "visit.check_in_too_old",
        }
    }
}

/// Why a recorded visit's service time cannot move to the requested instant ("Sửa lượt", direct
/// edit): the sale and payments belong to the visit's business day, so the time stays on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveError {
    InFuture,
    OtherDay,
}

/// May a visit of business day `day` get the service time `at`, at server time `now`?
pub fn moved_check_in(
    at: DateTime<Utc>,
    now: DateTime<Utc>,
    tz: Tz,
    day: NaiveDate,
) -> Result<(), MoveError> {
    if at > now + Duration::minutes(FUTURE_TOLERANCE_MINUTES) {
        return Err(MoveError::InFuture);
    }
    if BusinessDate::of_instant(at, tz).as_naive() != day {
        return Err(MoveError::OtherDay);
    }
    Ok(())
}

/// The requested `price_variant` contradicts the customer's recorded gender.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VariantConflict;

/// Gender used to price a one-off line (AC4.2b): the customer's recorded gender; for a customer
/// without one, the `price_variant` the desk chose for that line. `None` = unknown, which only a
/// flat-priced service accepts.
pub fn pricing_gender(
    customer: Option<Gender>,
    requested: Option<PriceVariant>,
) -> Result<Option<PriceGender>, VariantConflict> {
    let requested = match requested {
        Some(PriceVariant::Female) => Some(PriceGender::Female),
        Some(PriceVariant::Male) => Some(PriceGender::Male),
        Some(PriceVariant::All) | None => None,
    };
    match customer {
        None => Ok(requested),
        Some(g) => {
            let recorded = match g {
                Gender::Female => PriceGender::Female,
                Gender::Male => PriceGender::Male,
            };
            if requested.is_some_and(|r| r != recorded) {
                Err(VariantConflict)
            } else {
                Ok(Some(recorded))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn vn() -> Tz {
        BusinessDate::parse_tz("Asia/Ho_Chi_Minh").unwrap_or(chrono_tz::UTC)
    }

    fn at(h: u32, m: u32) -> DateTime<Utc> {
        // 2026-11-05 h:m in Vietnam (UTC+7).
        vn().with_ymd_and_hms(2026, 11, 5, h, m, 0)
            .single()
            .map(|t| t.with_timezone(&Utc))
            .unwrap_or_default()
    }

    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 5).unwrap_or_default()
    }

    #[test]
    fn a_recorded_visit_moves_only_within_its_day_and_not_ahead_of_now() {
        let now = at(15, 0);
        assert_eq!(moved_check_in(at(9, 30), now, vn(), day()), Ok(()));
        assert_eq!(moved_check_in(at(15, 4), now, vn(), day()), Ok(()));
        assert_eq!(
            moved_check_in(at(15, 6), now, vn(), day()),
            Err(MoveError::InFuture)
        );
        // 2026-11-04 23:50 in Vietnam is the day before, though the same UTC date as 05 06:00.
        let yesterday = vn_at(2026, 11, 4, 23, 50);
        assert_eq!(
            moved_check_in(yesterday, now, vn(), day()),
            Err(MoveError::OtherDay)
        );
        assert_eq!(moved_check_in(at(0, 0), now, vn(), day()), Ok(()));
    }

    fn check_in_at(
        requested: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
        may_backdate: bool,
    ) -> Result<DateTime<Utc>, CheckInError> {
        service_time(requested, now, vn(), day(), may_backdate).map(|t| t.at)
    }

    fn vn_at(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Utc> {
        vn().with_ymd_and_hms(y, mo, d, h, mi, 0)
            .single()
            .map(|t| t.with_timezone(&Utc))
            .unwrap_or_default()
    }

    #[test]
    fn check_in_defaults_to_now_and_tolerates_small_skew() {
        let now = at(9, 0);
        assert_eq!(check_in_at(None, now, false), Ok(now));
        assert_eq!(check_in_at(Some(at(9, 5)), now, false), Ok(at(9, 5)));
        assert_eq!(check_in_at(Some(at(8, 10)), now, false), Ok(at(8, 10)));
    }

    #[test]
    fn check_in_in_the_future_or_on_another_day_is_rejected() {
        let now = at(9, 0);
        assert_eq!(
            check_in_at(Some(at(9, 6)), now, false),
            Err(CheckInError::InFuture)
        );
        let yesterday_evening = vn_at(2026, 11, 4, 23, 59);
        assert_eq!(
            check_in_at(Some(yesterday_evening), now, false),
            Err(CheckInError::NotToday)
        );
    }

    #[test]
    fn the_owner_back_dates_up_to_a_year_onto_the_day_of_the_service() {
        let now = at(9, 0);
        let t = service_time(Some(vn_at(2026, 11, 4, 18, 30)), now, vn(), day(), true);
        assert_eq!(
            t,
            Ok(ServiceTime {
                at: vn_at(2026, 11, 4, 18, 30),
                business_date: NaiveDate::from_ymd_opt(2026, 11, 4).unwrap_or_default(),
                backdated: true,
            })
        );
        assert!(service_time(Some(vn_at(2025, 11, 5, 9, 0)), now, vn(), day(), true).is_ok());
        assert_eq!(
            check_in_at(Some(vn_at(2025, 11, 3, 9, 0)), now, true),
            Err(CheckInError::TooOld)
        );
        // Back-dating never opens the future.
        assert_eq!(
            check_in_at(Some(at(10, 0)), now, true),
            Err(CheckInError::InFuture)
        );
    }

    #[test]
    fn skew_past_midnight_stays_on_today() {
        let now = vn_at(2026, 11, 5, 23, 58);
        let t = service_time(Some(vn_at(2026, 11, 6, 0, 2)), now, vn(), day(), false);
        assert_eq!(
            t.map(|t| (t.business_date, t.backdated)),
            Ok((day(), false))
        );
    }

    #[test]
    fn recorded_gender_prices_the_line() {
        assert_eq!(
            pricing_gender(Some(Gender::Male), None),
            Ok(Some(PriceGender::Male))
        );
        assert_eq!(
            pricing_gender(Some(Gender::Female), Some(PriceVariant::Female)),
            Ok(Some(PriceGender::Female))
        );
        assert_eq!(
            pricing_gender(Some(Gender::Male), Some(PriceVariant::All)),
            Ok(Some(PriceGender::Male))
        );
        assert_eq!(
            pricing_gender(Some(Gender::Male), Some(PriceVariant::Female)),
            Err(VariantConflict)
        );
    }

    #[test]
    fn unknown_gender_uses_the_desks_choice() {
        assert_eq!(pricing_gender(None, None), Ok(None));
        assert_eq!(
            pricing_gender(None, Some(PriceVariant::Male)),
            Ok(Some(PriceGender::Male))
        );
        assert_eq!(pricing_gender(None, Some(PriceVariant::All)), Ok(None));
    }
}
