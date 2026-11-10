//! Who the demo spa's customers are and when they come: a fixed cast of hand-written profiles
//! followed by generated ones, each with a segment (regular, package holder, lapsed, walk-in…)
//! that decides the first and last day they visit and how often.

use super::Window;
use super::data::{self, CUSTOMERS, Gender, Sex};
use super::rng::Rng;
use chrono::{Datelike, Duration, NaiveDate};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Segment {
    /// Comes every week or two from early in the history.
    Regular,
    /// Buys a package and comes back to use it.
    PackageHolder,
    Normal,
    Occasional,
    /// Stopped coming 60–180 days ago ("Khách lâu chưa quay lại").
    Lapsed,
    /// One visit, no name and no phone (ADR-0011).
    WalkIn,
    /// Registered at the desk today (gender not asked yet), no visit so far: a visit with a
    /// one-off service would record the gender, so only these keep a profile without one.
    Prospect,
}

impl Segment {
    /// Relative chance of being picked for a visit on a day they are around.
    pub const fn weight(self) -> u32 {
        match self {
            Self::Regular => 9,
            Self::PackageHolder => 5,
            Self::Lapsed => 3,
            Self::Normal => 2,
            Self::Occasional => 1,
            Self::WalkIn | Self::Prospect => 0,
        }
    }
}

/// A customer of the demo spa: profile plus behaviour.
#[derive(Debug, Clone)]
pub struct Person {
    pub full_name: Option<String>,
    pub phone: Option<String>,
    pub sex: Sex,
    pub gender_recorded: bool,
    pub birth_year: Option<i16>,
    pub birth_date: Option<NaiveDate>,
    pub address: Option<&'static str>,
    pub source: Option<&'static str>,
    pub health_note: Option<&'static str>,
    pub segment: Segment,
    /// First and last day this customer visits (both visits are on the schedule).
    pub first_day: NaiveDate,
    pub last_day: NaiveDate,
    /// Buys a package when they have none left.
    pub buys_packages: bool,
}

/// Paper-book customers (index 0–3, see `import_paper_books`) and the customer whose wrong sale
/// is voided and sold again today.
pub const PAPER_CUSTOMERS: usize = 4;
pub const RESALE_CUSTOMER: usize = 5;

/// `0399 00 CCCC` in national form: customer `CCCC` (1-based) of the demo block.
pub fn demo_phone(customer: usize) -> String {
    format!("039900{:04}", customer + 1)
}

/// `count` customers for `window`, the same for the same seed and window.
pub fn people(window: Window, count: usize, rng: &mut Rng) -> Vec<Person> {
    (0..count).map(|i| person(window, i, rng)).collect()
}

fn person(window: Window, i: usize, rng: &mut Rng) -> Person {
    let mut segment = match i {
        0 | 1 => Segment::PackageHolder,
        2 | RESALE_CUSTOMER => Segment::Regular,
        3 => Segment::Normal,
        _ => match rng.below(100) {
            0..8 if i >= CUSTOMERS.len() => Segment::WalkIn,
            8..11 if i >= CUSTOMERS.len() => Segment::Prospect,
            0..11 => Segment::Normal,
            11..23 => Segment::Lapsed,
            23..35 => Segment::Regular,
            35..60 => Segment::PackageHolder,
            60..85 => Segment::Normal,
            _ => Segment::Occasional,
        },
    };
    let span = window.days();
    let at = |offset: i64| window.start + Duration::days(offset.clamp(0, span));
    let mut last_day = window.today;
    // The hand-written customers are the spa's existing clientele: back in the first two weeks.
    let existing = i < CUSTOMERS.len() && segment != Segment::Lapsed;
    let first_day = match segment {
        _ if existing => at(rng.days(13)),
        Segment::Regular => at(rng.days(span * 3 / 10)),
        Segment::PackageHolder => at(rng.days(span - 20)),
        Segment::Normal | Segment::Occasional => at(rng.days(span - 7)),
        Segment::Prospect => window.today,
        Segment::WalkIn => {
            let day = at(rng.days(span));
            last_day = day;
            day
        }
        Segment::Lapsed => {
            let lapse = 60 + rng.days(120);
            let last = window.today - Duration::days(lapse);
            if last < window.start + Duration::days(7) {
                // The history is too short for this customer to have stopped coming.
                segment = Segment::Occasional;
                at(rng.days(span - 7))
            } else {
                last_day = last;
                at(rng.days((last - window.start).num_days() * 2 / 3))
            }
        }
    };
    let buys_packages = match i {
        0 | 1 | 3 => false,
        2 => true,
        _ => match segment {
            Segment::PackageHolder => true,
            Segment::Regular => rng.chance(50),
            Segment::Lapsed => rng.chance(40),
            Segment::Normal => rng.chance(10),
            Segment::Occasional | Segment::WalkIn | Segment::Prospect => false,
        },
    };
    let mut p = if let Some(c) = CUSTOMERS.get(i) {
        let (sex, recorded) = match c.gender {
            Gender::Recorded(s) => (s, true),
            Gender::Unrecorded(s) => (s, false),
        };
        Person {
            full_name: Some(c.full_name.to_owned()),
            phone: Some(demo_phone(i)),
            sex,
            gender_recorded: recorded,
            birth_year: c.birth_year,
            birth_date: None,
            address: c.address,
            source: c.source,
            health_note: c.health_note,
            segment,
            first_day,
            last_day,
            buys_packages,
        }
    } else {
        generated(window, i, segment, first_day, last_day, buys_packages, rng)
    };
    // A few hand-written customers have a birthday this month too (the birthday list).
    if i >= PAPER_CUSTOMERS && i < CUSTOMERS.len() && i.is_multiple_of(9) {
        let year = p.birth_year.unwrap_or(1985);
        p.birth_date = birthday_this_month(window.today, year, rng);
    }
    p
}

fn generated(
    window: Window,
    i: usize,
    segment: Segment,
    first_day: NaiveDate,
    last_day: NaiveDate,
    buys_packages: bool,
    rng: &mut Rng,
) -> Person {
    let sex = if rng.chance(78) {
        Sex::Female
    } else {
        Sex::Male
    };
    if segment == Segment::WalkIn {
        return Person {
            full_name: None,
            phone: None,
            sex,
            gender_recorded: false,
            birth_year: None,
            birth_date: None,
            address: None,
            source: None,
            health_note: None,
            segment,
            first_day,
            last_day,
            buys_packages,
        };
    }
    let pick = |rng: &mut Rng, list: &[&'static str]| *rng.pick(list).unwrap_or(&"");
    let (middle, given) = match sex {
        Sex::Female => (data::FEMALE_MIDDLE, data::FEMALE_GIVEN),
        Sex::Male => (data::MALE_MIDDLE, data::MALE_GIVEN),
    };
    let full_name = format!(
        "{} {} {}",
        pick(rng, data::FAMILY_NAMES),
        pick(rng, middle),
        pick(rng, given)
    );
    let year = 1960 + i16::try_from(rng.below(44)).unwrap_or(0);
    let (birth_year, birth_date) = match rng.below(100) {
        0..12 => (None, birthday_this_month(window.today, year, rng)),
        12..65 => (Some(year), None),
        _ => (None, None),
    };
    Person {
        full_name: Some(full_name),
        // ADR-0011: a name without a phone is a valid profile.
        phone: (!rng.chance(5)).then(|| demo_phone(i)),
        sex,
        gender_recorded: segment != Segment::Prospect && !rng.chance(7),
        birth_year,
        birth_date,
        address: rng.chance(60).then(|| pick(rng, data::ADDRESSES)),
        source: rng.chance(70).then(|| pick(rng, data::SOURCES)),
        health_note: rng.chance(25).then(|| pick(rng, data::HEALTH_NOTES)),
        segment,
        first_day,
        last_day,
        buys_packages,
    }
}

fn birthday_this_month(today: NaiveDate, year: i16, rng: &mut Rng) -> Option<NaiveDate> {
    let day = 1 + u32::try_from(rng.below(28)).unwrap_or(0);
    NaiveDate::from_ymd_opt(i32::from(year), today.month(), day)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(months: u32) -> Window {
        Window::new(
            NaiveDate::from_ymd_opt(2026, 11, 9).unwrap_or_default(),
            months,
        )
    }

    #[test]
    fn every_segment_and_profile_variant_shows_up_in_a_year() {
        let w = window(12);
        let people = people(w, 220, &mut Rng::new(1));
        for segment in [
            Segment::Regular,
            Segment::PackageHolder,
            Segment::Normal,
            Segment::Occasional,
            Segment::Lapsed,
            Segment::WalkIn,
            Segment::Prospect,
        ] {
            assert!(people.iter().any(|p| p.segment == segment), "{segment:?}");
        }
        assert!(
            people
                .iter()
                .any(|p| p.full_name.is_some() && p.phone.is_none())
        );
        assert!(
            people
                .iter()
                .any(|p| !p.gender_recorded && p.full_name.is_some())
        );
        assert!(people.iter().any(|p| p.health_note.is_some()));
        assert!(
            people
                .iter()
                .any(|p| p.birth_date.is_some_and(|d| d.month() == w.today.month()))
        );
        for p in &people {
            assert!(w.start <= p.first_day && p.first_day <= p.last_day);
            assert!(p.last_day <= w.today);
            if p.segment == Segment::Lapsed {
                let away = (w.today - p.last_day).num_days();
                assert!((60..=180).contains(&away), "{away}");
            }
            if p.segment == Segment::WalkIn {
                assert!(p.full_name.is_none() && p.phone.is_none());
            }
        }
    }

    #[test]
    fn a_short_history_has_no_impossible_lapse() {
        let w = window(2);
        for p in people(w, 70, &mut Rng::new(9)) {
            assert!(w.start <= p.first_day && p.last_day <= w.today);
        }
    }

    #[test]
    fn demo_phones_are_valid_distinct_and_in_the_demo_block() {
        let mut seen = std::collections::HashSet::new();
        for i in 0..400 {
            let e164 = kernel::PhoneE164::parse(&demo_phone(i))
                .map(|p| p.as_str().to_owned())
                .unwrap_or_default();
            assert!(e164.starts_with("+8439900"), "{e164}");
            assert!(seen.insert(e164));
        }
    }
}
