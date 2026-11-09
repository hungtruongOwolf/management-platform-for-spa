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

/// The gender to write on a profile that has none (F2, live test 2026-11-08): the one the desk
/// picked to price the visit's one-off lines (Nữ / Nam in the cart). Only when the profile has no
/// gender and every line that names one agrees; never an overwrite.
pub fn gender_to_record(
    customer: Option<Gender>,
    requested: impl IntoIterator<Item = Option<PriceVariant>>,
) -> Option<Gender> {
    if customer.is_some() {
        return None;
    }
    let mut picked = None;
    for variant in requested.into_iter().flatten() {
        let g = match variant {
            PriceVariant::Female => Gender::Female,
            PriceVariant::Male => Gender::Male,
            PriceVariant::All => continue,
        };
        match picked {
            None => picked = Some(g),
            Some(p) if p == g => {}
            Some(_) => return None,
        }
    }
    picked
}

/// Does a correction give a service of the old visit to another technician (ADR-0019 D8)? Lines
/// are `(service, technician)` pairs. A pair the old visit had that the new one has fewer of,
/// while that service is still done, is a technician change; a service removed or added is not.
pub fn technician_changed<T: PartialEq + Copy>(old: &[(T, T)], new: &[(T, T)]) -> bool {
    let count = |lines: &[(T, T)], pair: (T, T)| lines.iter().filter(|&&p| p == pair).count();
    old.iter().any(|&(service, staff)| {
        new.iter().any(|&(s, _)| s == service)
            && count(new, (service, staff)) < count(old, (service, staff))
    })
}

/// A one-off line's price on the visit a correction replaces (ADR-0019 D8: kept, never re-read
/// from today's price list).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeptPrice<Id> {
    pub service_id: Id,
    pub variant: PriceVariant,
    pub name: String,
    pub unit_price: kernel::Money,
}

/// The kept price of `service_id` for a line priced with `gender`: a flat price fits any gender, a
/// Nữ / Nam price only the same one.
pub fn kept_price<Id: PartialEq>(
    kept: &[KeptPrice<Id>],
    service_id: &Id,
    gender: Option<PriceGender>,
) -> Option<&KeptPrice<Id>> {
    kept.iter().find(|k| {
        &k.service_id == service_id
            && match k.variant {
                PriceVariant::All => true,
                PriceVariant::Female => gender == Some(PriceGender::Female),
                PriceVariant::Male => gender == Some(PriceGender::Male),
            }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn a_technician_change_is_a_service_moved_to_someone_else() {
        // (service, technician)
        let old = [(1, 10), (2, 20)];
        assert!(!technician_changed(&old, &old), "nothing changed");
        assert!(
            technician_changed(&old, &[(1, 11), (2, 20)]),
            "service 1 moved"
        );
        assert!(!technician_changed(&old, &[(1, 10)]), "service 2 removed");
        assert!(
            !technician_changed(&old, &[(1, 10), (2, 20), (3, 30)]),
            "service 3 added"
        );
        assert!(
            !technician_changed(&old, &[(2, 20), (1, 10)]),
            "order does not matter"
        );
        // Two sessions of one service: one of them moved.
        assert!(technician_changed(&[(1, 10), (1, 10)], &[(1, 10), (1, 11)]));
        assert!(
            !technician_changed(&[(1, 10)], &[(1, 10), (1, 11)]),
            "a session added"
        );
    }

    #[test]
    fn a_kept_price_matches_the_line_gender() {
        let price = |variant, amount| KeptPrice {
            service_id: 1,
            variant,
            name: "Gội".to_owned(),
            unit_price: kernel::Money::vnd(amount).unwrap_or(kernel::Money::ZERO),
        };
        let kept = [
            price(PriceVariant::Female, 79_000),
            price(PriceVariant::Male, 89_000),
        ];
        let female = kept_price(&kept, &1, Some(PriceGender::Female));
        assert_eq!(female.map(|k| k.unit_price.as_i64()), Some(79_000));
        let male = kept_price(&kept, &1, Some(PriceGender::Male));
        assert_eq!(male.map(|k| k.unit_price.as_i64()), Some(89_000));
        assert_eq!(
            kept_price(&kept, &1, None),
            None,
            "gendered price, no gender"
        );
        assert_eq!(kept_price(&kept, &2, Some(PriceGender::Female)), None);
        let flat = [price(PriceVariant::All, 49_000)];
        assert_eq!(
            kept_price(&flat, &1, Some(PriceGender::Male)).map(|k| k.unit_price.as_i64()),
            Some(49_000)
        );
        assert!(kept_price(&flat, &1, None).is_some());
    }

    #[test]
    fn the_price_gender_fills_an_empty_profile_only() {
        use PriceVariant::{All, Female, Male};
        assert_eq!(
            gender_to_record(None, [Some(Female), None, Some(All)]),
            Some(Gender::Female)
        );
        assert_eq!(
            gender_to_record(None, [Some(Male), Some(Male)]),
            Some(Gender::Male)
        );
        // Never an overwrite, never a guess.
        assert_eq!(gender_to_record(Some(Gender::Male), [Some(Female)]), None);
        assert_eq!(gender_to_record(None, [Some(Female), Some(Male)]), None);
        assert_eq!(gender_to_record(None, [None, Some(All)]), None);
    }

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
