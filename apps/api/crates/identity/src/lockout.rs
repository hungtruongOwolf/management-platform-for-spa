//! Progressive lockout after repeated failed logins (per account; per-IP limits live in middleware).

use chrono::{DateTime, Duration, Utc};

pub const FREE_ATTEMPTS: i32 = 5;
const BASE_LOCK_MINUTES: i64 = 15;
const MAX_LOCK_HOURS: i64 = 24;

/// Given the failure count *after* the latest failure, when the account unlocks (None = not locked).
/// 5th failure → 15 min, 6th → 30 min, 7th → 60 min … capped at 24 h.
pub fn lock_until(failed_logins: i32, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    if failed_logins < FREE_ATTEMPTS {
        return None;
    }
    let doublings = u32::try_from(failed_logins - FREE_ATTEMPTS)
        .unwrap_or(0)
        .min(16);
    let minutes = BASE_LOCK_MINUTES.saturating_mul(1_i64 << doublings);
    Some(now + Duration::minutes(minutes).min(Duration::hours(MAX_LOCK_HOURS)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locks_progressively_and_caps_at_a_day() {
        let now = Utc::now();
        assert_eq!(lock_until(4, now), None);
        assert_eq!(lock_until(5, now), Some(now + Duration::minutes(15)));
        assert_eq!(lock_until(6, now), Some(now + Duration::minutes(30)));
        assert_eq!(lock_until(7, now), Some(now + Duration::minutes(60)));
        assert_eq!(lock_until(50, now), Some(now + Duration::hours(24)));
    }
}
