//! Pure visit rules: when a check-in may be recorded, and which gender prices a one-off line.

use catalog::{Gender as PriceGender, PriceVariant};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use chrono_tz::Tz;
use crm::service::Gender;
use kernel::BusinessDate;

/// Clock skew tolerated between the counter's device and the server.
pub const FUTURE_TOLERANCE_MINUTES: i64 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckInError {
    /// More than the tolerance ahead of the server clock (`422 visit.check_in_in_future`).
    InFuture,
    /// On another business day than today (`422 visit.check_in_not_today`): back-dating is a
    /// correction of a closed day (S8), not a visit.
    NotToday,
}

/// The check-in instant to record: `requested`, or `now` when the desk sent none.
pub fn check_in_at(
    requested: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    tz: Tz,
    today: NaiveDate,
) -> Result<DateTime<Utc>, CheckInError> {
    let at = requested.unwrap_or(now);
    if at > now + Duration::minutes(FUTURE_TOLERANCE_MINUTES) {
        return Err(CheckInError::InFuture);
    }
    if BusinessDate::of_instant(at, tz).as_naive() != today {
        return Err(CheckInError::NotToday);
    }
    Ok(at)
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
    fn check_in_defaults_to_now_and_tolerates_small_skew() {
        let now = at(9, 0);
        assert_eq!(check_in_at(None, now, vn(), day()), Ok(now));
        assert_eq!(
            check_in_at(Some(at(9, 5)), now, vn(), day()),
            Ok(at(9, 5))
        );
        assert_eq!(
            check_in_at(Some(at(8, 10)), now, vn(), day()),
            Ok(at(8, 10))
        );
    }

    #[test]
    fn check_in_in_the_future_or_on_another_day_is_rejected() {
        let now = at(9, 0);
        assert_eq!(
            check_in_at(Some(at(9, 6)), now, vn(), day()),
            Err(CheckInError::InFuture)
        );
        let yesterday_evening = vn()
            .with_ymd_and_hms(2026, 11, 4, 23, 59, 0)
            .single()
            .map(|t| t.with_timezone(&Utc))
            .unwrap_or_default();
        assert_eq!(
            check_in_at(Some(yesterday_evening), now, vn(), day()),
            Err(CheckInError::NotToday)
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
