//! The visit calendar of the demo history, decided before anything is written: how many visits
//! each day (busier at the weekend), who comes (each customer's segment and active days), at what
//! time, and which few visits are later cancelled or corrected. Pure: the same seed, window and
//! clock give the same calendar.

use super::Window;
use super::people::{PAPER_CUSTOMERS, Person, RESALE_CUSTOMER, Segment};
use super::rng::Rng;
use chrono::{Datelike, Duration, NaiveDate, Weekday};

/// Something done to a visit right after it is recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Script {
    Plain,
    /// Cancelled ("Hủy lượt") with a reason: sessions back, payments reversed.
    Void,
    /// "Sửa lượt" that changes money: void + record again.
    Correct,
    /// "Sửa lượt" without money: another technician.
    Edit,
    /// Today: an expired paper-book package used with the manager's override.
    ExpiredOverride,
    /// Today: a package sold by mistake, voided, and the right one sold.
    WrongSaleVoided,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Visit {
    pub day: NaiveDate,
    /// Local minutes after midnight.
    pub minute: u32,
    pub script: Script,
}

/// Visits per customer, in time order.
pub type Schedule = Vec<Vec<Visit>>;

/// Opening of the desk for back-filled visits: 08:30–20:30.
const FIRST_MINUTE: u32 = 8 * 60 + 30;
const LAST_MINUTE: u32 = 20 * 60 + 30;

/// How many visits a past day has: 4–6 on weekdays, 6–8 at the weekend.
fn visits_on(day: NaiveDate, rng: &mut Rng) -> usize {
    match day.weekday() {
        Weekday::Sat | Weekday::Sun => 6 + rng.below(3),
        _ => 4 + rng.below(3),
    }
}

/// The calendar. `today_minutes` are the times of today's visits (up to now, chosen by the
/// caller from the clock); their count is today's number of visits.
pub fn schedule(
    window: Window,
    people: &[Person],
    today_minutes: &[u32],
    rng: &mut Rng,
) -> Schedule {
    let mut out: Schedule = vec![Vec::new(); people.len()];
    let mut day = window.start;
    while day <= window.today {
        let is_today = day == window.today;
        let mut chosen: Vec<usize> = Vec::new();
        // A handful a day: a linear scan beats a set.
        let take = |c: usize, chosen: &mut Vec<usize>| {
            if !chosen.contains(&c) {
                chosen.push(c);
            }
        };
        for (c, p) in people.iter().enumerate() {
            // Everyone's first visit, and the last visit of a customer who then stops coming.
            let first_visit = p.first_day == day && p.segment != Segment::Prospect;
            if first_visit || (p.segment == Segment::Lapsed && p.last_day == day) {
                take(c, &mut chosen);
            }
        }
        if is_today {
            take(0, &mut chosen);
            take(RESALE_CUSTOMER, &mut chosen);
        }
        let wanted = if is_today {
            today_minutes.len()
        } else {
            visits_on(day, rng)
        };
        while chosen.len() < wanted {
            let pool: Vec<(usize, u32)> = people
                .iter()
                .enumerate()
                .filter(|(c, p)| p.first_day <= day && day <= p.last_day && !chosen.contains(c))
                .map(|(c, p)| (c, p.segment.weight()))
                .filter(|(_, w)| *w > 0)
                .collect();
            let total: u32 = pool.iter().map(|(_, w)| w).sum();
            if total == 0 {
                break;
            }
            let mut roll =
                u32::try_from(rng.below(usize::try_from(total).unwrap_or(1))).unwrap_or(0);
            let Some(&(c, _)) = pool.iter().find(|(_, w)| {
                if roll < *w {
                    true
                } else {
                    roll -= w;
                    false
                }
            }) else {
                break;
            };
            take(c, &mut chosen);
        }
        let minutes = if is_today {
            spread_today(today_minutes, chosen.len())
        } else {
            let mut m: Vec<u32> = (0..chosen.len())
                .map(|_| {
                    FIRST_MINUTE
                        + u32::try_from(rng.below((LAST_MINUTE - FIRST_MINUTE) as usize))
                            .unwrap_or(0)
                })
                .collect();
            m.sort_unstable();
            m
        };
        // Who comes when: shuffle the day's customers over its times.
        for i in (1..chosen.len()).rev() {
            chosen.swap(i, rng.below(i + 1));
        }
        for (c, minute) in chosen.into_iter().zip(minutes) {
            out[c].push(Visit {
                day,
                minute,
                script: Script::Plain,
            });
        }
        day += Duration::days(1);
    }
    for visits in &mut out {
        visits.sort_by_key(|v| (v.day, v.minute));
    }
    script(window, people, &mut out, rng);
    out
}

/// Today's times when more customers are forced in than the caller planned: extra ones a few
/// minutes before the last.
fn spread_today(minutes: &[u32], n: usize) -> Vec<u32> {
    let last = minutes.last().copied().unwrap_or(8 * 60);
    let mut m: Vec<u32> = minutes.iter().copied().take(n).collect();
    while m.len() < n {
        let k = u32::try_from(m.len()).unwrap_or(0);
        m.push(last.saturating_sub(3 * k));
    }
    m.sort_unstable();
    m
}

/// Marks the few visits that are cancelled or corrected, and today's scripted ones.
fn script(window: Window, people: &[Person], out: &mut Schedule, rng: &mut Rng) {
    let eligible = |c: usize| c >= PAPER_CUSTOMERS && people[c].segment != Segment::WalkIn;
    let mut past: Vec<(usize, usize)> = Vec::new();
    let mut today: Vec<(usize, usize)> = Vec::new();
    for (c, visits) in out.iter().enumerate() {
        for (k, v) in visits.iter().enumerate() {
            if !eligible(c) || c == RESALE_CUSTOMER {
                continue;
            }
            if v.day < window.today {
                past.push((c, k));
            } else {
                today.push((c, k));
            }
        }
    }
    let total = past.len();
    let wanted = [
        (Script::Void, (total / 250).max(2)),
        (Script::Correct, (total / 400).max(1)),
        (Script::Edit, (total / 300).max(1)),
    ];
    for (kind, n) in wanted {
        for _ in 0..n {
            if past.is_empty() {
                break;
            }
            let (c, k) = past.swap_remove(rng.below(past.len()));
            out[c][k].script = kind;
        }
    }
    for kind in [Script::Void, Script::Correct, Script::Edit] {
        if !today.is_empty() {
            let (c, k) = today.swap_remove(rng.below(today.len()));
            out[c][k].script = kind;
        }
    }
    for (c, kind) in [
        (0, Script::ExpiredOverride),
        (RESALE_CUSTOMER, Script::WrongSaleVoided),
    ] {
        if let Some(v) = out[c].iter_mut().find(|v| v.day == window.today) {
            v.script = kind;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo::people::people;
    use std::collections::BTreeSet;

    fn plan(months: u32) -> (Window, Vec<Person>, Schedule) {
        let today = NaiveDate::from_ymd_opt(2026, 11, 9).unwrap_or_default();
        let w = Window::new(today, months);
        let mut rng = Rng::new(42);
        let people = people(w, 220, &mut rng);
        let today_minutes: Vec<u32> = (0..10).map(|i| 8 * 60 + 40 * i).collect();
        let s = schedule(w, &people, &today_minutes, &mut rng);
        (w, people, s)
    }

    #[test]
    fn a_year_has_four_to_eight_visits_a_day_busier_at_the_weekend() {
        let (w, _, s) = plan(12);
        let mut per_day = std::collections::BTreeMap::new();
        for v in s.iter().flatten() {
            *per_day.entry(v.day).or_insert(0_usize) += 1;
        }
        assert_eq!(
            per_day.len(),
            usize::try_from((w.today - w.start).num_days() + 1).unwrap_or(0)
        );
        let total: usize = per_day.values().sum();
        assert!((1_900..2_600).contains(&total), "{total}");
        let avg = |weekend: bool| {
            let days: Vec<usize> = per_day
                .iter()
                .filter(|(d, _)| {
                    **d < w.today && matches!(d.weekday(), Weekday::Sat | Weekday::Sun) == weekend
                })
                .map(|(_, n)| *n)
                .collect();
            days.iter().sum::<usize>() as f64 / days.len() as f64
        };
        assert!(avg(true) > avg(false) + 1.0);
    }

    #[test]
    fn each_customer_comes_at_most_once_a_day_inside_their_active_days() {
        let (w, people, s) = plan(12);
        for (c, visits) in s.iter().enumerate() {
            let days: BTreeSet<NaiveDate> = visits.iter().map(|v| v.day).collect();
            assert_eq!(days.len(), visits.len(), "customer {c}");
            for v in visits {
                assert!(people[c].first_day <= v.day && v.day <= people[c].last_day);
                assert!(v.day >= w.start && v.day <= w.today);
            }
            if people[c].segment == Segment::Lapsed {
                assert_eq!(visits.last().map(|v| v.day), Some(people[c].last_day));
            }
        }
    }

    #[test]
    fn scripted_visits_are_present_and_today_has_its_cases() {
        let (w, _, s) = plan(12);
        let all: Vec<&Visit> = s.iter().flatten().collect();
        for kind in [Script::Void, Script::Correct, Script::Edit] {
            assert!(
                all.iter().any(|v| v.script == kind && v.day < w.today),
                "{kind:?}"
            );
            assert!(
                all.iter().any(|v| v.script == kind && v.day == w.today),
                "{kind:?}"
            );
        }
        assert_eq!(s[0].last().map(|v| v.script), Some(Script::ExpiredOverride));
        assert_eq!(
            s[RESALE_CUSTOMER].last().map(|v| v.script),
            Some(Script::WrongSaleVoided)
        );
    }

    #[test]
    fn the_same_seed_gives_the_same_calendar() {
        assert_eq!(plan(3).2, plan(3).2);
    }
}
