//! Pure package rules: derived status and the value of a session (spec R1-1 D4).

use chrono::NaiveDate;
use kernel::Money;

/// Never stored, so it can never drift (data.md): precedence cancelled → exhausted → expired → active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageStatus {
    Active,
    Expired,
    Exhausted,
    Cancelled,
}

impl PackageStatus {
    pub fn derive(
        cancelled: bool,
        sessions_remaining: i64,
        expires_on: Option<NaiveDate>,
        today: NaiveDate,
    ) -> Self {
        if cancelled {
            Self::Cancelled
        } else if sessions_remaining <= 0 {
            Self::Exhausted
        } else if expires_on.is_some_and(|d| d < today) {
            Self::Expired
        } else {
            Self::Active
        }
    }
}

/// How a refund is paid back (the same two ways money comes in).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefundMethod {
    Cash,
    BankTransfer,
}

impl RefundMethod {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cash => "cash",
            Self::BankTransfer => "bank_transfer",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "cash" => Some(Self::Cash),
            "bank_transfer" => Some(Self::BankTransfer),
            _ => None,
        }
    }
}

/// Value of one purchased session: price after discount ÷ purchased sessions, rounded down.
/// Bonus sessions are worth 0, so they do not dilute it (D4). A package with no purchased
/// sessions (e.g. an all-bonus opening balance) is worth 0 per session.
pub fn per_session_value(sold_price: Money, sessions_purchased: u32) -> Money {
    sold_price
        .per_unit(sessions_purchased)
        .unwrap_or(Money::ZERO)
}

/// Value of `qty` sessions taken when `paid_used_before` purchased sessions were already used.
/// Purchased sessions are consumed first, then bonus ones, so commission (R2) is earned on paid
/// sessions before free ones.
pub fn consumed_value(
    per_session: Money,
    sessions_purchased: u32,
    paid_used_before: u32,
    qty: u32,
) -> Money {
    let paid_left = sessions_purchased.saturating_sub(paid_used_before);
    per_session
        .checked_mul(qty.min(paid_left))
        .unwrap_or(Money::ZERO)
}

/// Purchased sessions already used, from the item's lines on completed visits: their paid
/// sessions (`line_paid`) plus every session used outside those lines (`used_before −
/// line_sessions`: an opening balance's past use, or earlier lines of the same checkout), counted
/// paid first.
///
/// `used_before` alone (`total − remaining`) is not enough once a void has given sessions back:
/// a void restores the sessions of its lines whatever they were, so after "10 paid + 1 bonus used,
/// void a paid visit" the count says 10 used, of which 9 are paid; the next session is paid.
pub fn paid_used_from_lines(
    sessions_purchased: u32,
    used_before: u32,
    line_sessions: u32,
    line_paid: u32,
) -> u32 {
    used_before
        .saturating_sub(line_sessions)
        .saturating_add(line_paid)
        .min(sessions_purchased)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vnd(v: i64) -> Money {
        Money::vnd(v).unwrap_or_default()
    }

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, d).unwrap_or_default()
    }

    #[test]
    fn ten_plus_two_bonus_at_1_5m_is_150k_per_paid_session() {
        assert_eq!(per_session_value(vnd(1_500_000), 10), vnd(150_000));
        assert_eq!(per_session_value(vnd(1_000_000), 3), vnd(333_333));
        assert_eq!(per_session_value(vnd(500_000), 0), Money::ZERO);
    }

    #[test]
    fn paid_sessions_are_used_before_bonus_ones() {
        let v = vnd(150_000);
        assert_eq!(consumed_value(v, 10, 0, 1), v);
        assert_eq!(consumed_value(v, 10, 9, 1), v);
        assert_eq!(consumed_value(v, 10, 10, 1), Money::ZERO, "11th = bonus");
        assert_eq!(consumed_value(v, 10, 9, 2), v, "one paid + one bonus");
        assert_eq!(consumed_value(v, 10, 11, 1), Money::ZERO);
    }

    #[test]
    fn a_paid_session_given_back_by_a_void_is_paid_again() {
        // 10 paid + 2 bonus: 10 paid lines and 1 bonus line, then the first paid visit is voided.
        // 10 used, the lines left hold 9 paid sessions: the next one is the 10th paid one.
        assert_eq!(paid_used_from_lines(10, 10, 10, 9), 9);
        // Without a void the lines agree with the plain count.
        assert_eq!(paid_used_from_lines(10, 11, 11, 10), 10);
        // Opening balance "4 left of 10 + 2": 8 used before the books, no line yet → 8 paid.
        assert_eq!(paid_used_from_lines(10, 8, 0, 0), 8);
        // Earlier lines of the same checkout are not stored yet: counted paid first.
        assert_eq!(paid_used_from_lines(10, 10, 9, 8), 9);
        assert_eq!(
            paid_used_from_lines(10, 12, 0, 0),
            10,
            "never more than purchased"
        );
    }

    #[test]
    fn status_precedence() {
        let today = day(10);
        assert_eq!(
            PackageStatus::derive(false, 3, None, today),
            PackageStatus::Active
        );
        assert_eq!(
            PackageStatus::derive(false, 3, Some(day(10)), today),
            PackageStatus::Active,
            "valid through its last day"
        );
        assert_eq!(
            PackageStatus::derive(false, 3, Some(day(9)), today),
            PackageStatus::Expired
        );
        assert_eq!(
            PackageStatus::derive(false, 0, Some(day(9)), today),
            PackageStatus::Exhausted
        );
        assert_eq!(
            PackageStatus::derive(true, 0, Some(day(9)), today),
            PackageStatus::Cancelled
        );
    }
}
