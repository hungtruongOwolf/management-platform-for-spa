//! Amounts of Vietnamese đồng as non-negative integers. Floats never touch money.

use serde::{Deserialize, Serialize};

/// A non-negative amount of đồng (VND has no minor unit in practice).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(try_from = "i64", into = "i64")]
pub struct Money(i64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MoneyError {
    #[error("amount must not be negative")]
    Negative,
    #[error("amount overflow")]
    Overflow,
}

impl Money {
    pub const ZERO: Self = Self(0);
    /// Largest single amount a client may enter (a price, a sold price): ten billion dong, far
    /// above any real service or package, low enough to catch a run of mistyped zeros and to keep
    /// sums of a few lines far from `i64` overflow (G6: over the limit is a `422`).
    pub const MAX_INPUT: Self = Self(10_000_000_000);

    /// Whether this amount is one a client may enter (`<= MAX_INPUT`).
    pub fn is_enterable(self) -> bool {
        self <= Self::MAX_INPUT
    }

    pub fn vnd(amount: i64) -> Result<Self, MoneyError> {
        if amount < 0 {
            Err(MoneyError::Negative)
        } else {
            Ok(Self(amount))
        }
    }

    pub const fn as_i64(self) -> i64 {
        self.0
    }

    pub fn checked_add(self, other: Self) -> Result<Self, MoneyError> {
        self.0
            .checked_add(other.0)
            .map(Self)
            .ok_or(MoneyError::Overflow)
    }

    /// Subtraction that refuses to go below zero.
    pub fn checked_sub(self, other: Self) -> Result<Self, MoneyError> {
        match self.0.checked_sub(other.0) {
            Some(v) if v >= 0 => Ok(Self(v)),
            Some(_) => Err(MoneyError::Negative),
            None => Err(MoneyError::Overflow),
        }
    }

    pub fn checked_mul(self, qty: u32) -> Result<Self, MoneyError> {
        self.0
            .checked_mul(i64::from(qty))
            .map(Self)
            .ok_or(MoneyError::Overflow)
    }

    /// Share of one unit when split over `units`, rounded down (the remainder stays with the whole).
    /// `None` when `units` is zero.
    pub fn per_unit(self, units: u32) -> Option<Self> {
        (units > 0).then(|| Self(self.0 / i64::from(units)))
    }

    /// Whether `self` is at most `pct` percent of `whole` (exact integer arithmetic, no rounding).
    pub fn within_percent_of(self, whole: Self, pct: u32) -> bool {
        i128::from(self.0) * 100 <= i128::from(whole.0) * i128::from(pct)
    }

    /// Sum of amounts; `Overflow` instead of wrapping.
    pub fn sum<I: IntoIterator<Item = Self>>(amounts: I) -> Result<Self, MoneyError> {
        amounts
            .into_iter()
            .try_fold(Self::ZERO, |acc, m| acc.checked_add(m))
    }
}

impl TryFrom<i64> for Money {
    type Error = MoneyError;
    fn try_from(value: i64) -> Result<Self, Self::Error> {
        Self::vnd(value)
    }
}

impl From<Money> for i64 {
    fn from(value: Money) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enterable_amounts_stop_at_ten_billion() {
        assert!(Money::ZERO.is_enterable());
        assert!(Money::MAX_INPUT.is_enterable());
        assert!(!Money(10_000_000_001).is_enterable());
    }

    #[test]
    fn rejects_negative_amounts() {
        assert_eq!(Money::vnd(-1), Err(MoneyError::Negative));
        assert_eq!(Money::vnd(0), Ok(Money::ZERO));
    }

    #[test]
    fn arithmetic_is_checked() {
        let a = Money::vnd(1_500_000).unwrap_or_default();
        let b = Money::vnd(500_000).unwrap_or_default();
        assert_eq!(a.checked_add(b).map(Money::as_i64), Ok(2_000_000));
        assert_eq!(b.checked_sub(a), Err(MoneyError::Negative));
        assert_eq!(a.checked_mul(3).map(Money::as_i64), Ok(4_500_000));
        assert_eq!(
            Money::vnd(i64::MAX).unwrap_or_default().checked_add(b),
            Err(MoneyError::Overflow)
        );
    }

    #[test]
    fn per_unit_rounds_down_and_rejects_zero_units() {
        let price = Money::vnd(1_500_000).unwrap_or_default();
        assert_eq!(price.per_unit(10).map(Money::as_i64), Some(150_000));
        assert_eq!(
            Money::vnd(1_000_000)
                .unwrap_or_default()
                .per_unit(3)
                .map(Money::as_i64),
            Some(333_333)
        );
        assert_eq!(price.per_unit(0), None);
    }

    #[test]
    fn percent_bound_is_exact() {
        let whole = Money::vnd(1_500_000).unwrap_or_default();
        let ten_pct = Money::vnd(150_000).unwrap_or_default();
        let over = Money::vnd(150_001).unwrap_or_default();
        assert!(ten_pct.within_percent_of(whole, 10));
        assert!(!over.within_percent_of(whole, 10));
        assert!(
            Money::vnd(i64::MAX)
                .unwrap_or_default()
                .within_percent_of(Money::vnd(i64::MAX).unwrap_or_default(), 100)
        );
    }

    #[test]
    fn sum_is_checked() {
        let parts = [500_000, 1_000_000].map(|v| Money::vnd(v).unwrap_or_default());
        assert_eq!(Money::sum(parts).map(Money::as_i64), Ok(1_500_000));
        let huge = [i64::MAX, 1].map(|v| Money::vnd(v).unwrap_or_default());
        assert_eq!(Money::sum(huge), Err(MoneyError::Overflow));
    }

    #[test]
    fn deserializing_negative_json_fails() {
        assert!(serde_json::from_str::<Money>("-5").is_err());
        assert_eq!(
            serde_json::from_str::<Money>("49000").ok(),
            Money::vnd(49_000).ok()
        );
    }
}
