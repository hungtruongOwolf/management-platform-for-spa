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
    fn deserializing_negative_json_fails() {
        assert!(serde_json::from_str::<Money>("-5").is_err());
        assert_eq!(
            serde_json::from_str::<Money>("49000").ok(),
            Money::vnd(49_000).ok()
        );
    }
}
