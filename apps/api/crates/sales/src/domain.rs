//! Pure sale rules: discount authority (D10), discount allocation across lines, full payment (AC3.3).

use kernel::Money;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaymentMethod {
    Cash,
    BankTransfer,
}

impl PaymentMethod {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cash => "cash",
            Self::BankTransfer => "bank_transfer",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "cash" => Some(Self::Cash),
            "bank_transfer" => Some(Self::BankTransfer),
            _ => None,
        }
    }
}

/// What the acting user may grant, from their permissions and the tenant's cap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscountAuthority {
    None,
    UpToPercent(u32),
    Unlimited,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscountError {
    /// No `sale.discount` at all.
    NotAllowed,
    /// Above the manager cap without `sale.discount_unlimited` (403 `discount.limit_exceeded`).
    LimitExceeded,
}

/// A zero discount needs no authority; anything else must fit the caller's authority.
pub fn check_discount(
    subtotal: Money,
    discount: Money,
    authority: DiscountAuthority,
) -> Result<(), DiscountError> {
    if discount == Money::ZERO {
        return Ok(());
    }
    match authority {
        DiscountAuthority::Unlimited => Ok(()),
        DiscountAuthority::UpToPercent(pct) if discount.within_percent_of(subtotal, pct) => Ok(()),
        DiscountAuthority::UpToPercent(_) => Err(DiscountError::LimitExceeded),
        DiscountAuthority::None => Err(DiscountError::NotAllowed),
    }
}

/// Splits a sale-level discount across lines in proportion to their totals (rounded down), so
/// per-line shares always add up to the discount exactly. The rounding remainder goes to the last
/// line, spilling to earlier lines once a line's share reaches its own total: no line is ever
/// discounted below zero. Requires `discount <= sum(line_totals)`.
pub fn allocate_discount(line_totals: &[Money], discount: Money) -> Vec<Money> {
    let subtotal: i128 = line_totals.iter().map(|m| i128::from(m.as_i64())).sum();
    if subtotal == 0 || discount == Money::ZERO {
        return vec![Money::ZERO; line_totals.len()];
    }
    let d = i128::from(discount.as_i64());
    let mut shares: Vec<i128> = line_totals
        .iter()
        .map(|m| d * i128::from(m.as_i64()) / subtotal)
        .collect();
    // Floors leave a remainder smaller than the line count; total headroom (subtotal minus the
    // floors) is at least that remainder because discount <= subtotal, so it always fits.
    let mut remainder = d - shares.iter().sum::<i128>();
    for (share, total) in shares.iter_mut().zip(line_totals).rev() {
        let headroom = i128::from(total.as_i64()) - *share;
        let given = remainder.min(headroom.max(0));
        *share += given;
        remainder -= given;
    }
    shares
        .into_iter()
        .map(|s| Money::vnd(i64::try_from(s).unwrap_or(0)).unwrap_or(Money::ZERO))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vnd(v: i64) -> Money {
        Money::vnd(v).unwrap_or_default()
    }

    #[test]
    fn manager_may_discount_up_to_the_cap_inclusive() {
        let sub = vnd(1_500_000);
        let manager = DiscountAuthority::UpToPercent(10);
        assert_eq!(check_discount(sub, vnd(150_000), manager), Ok(()));
        assert_eq!(
            check_discount(sub, vnd(150_001), manager),
            Err(DiscountError::LimitExceeded)
        );
        assert_eq!(
            check_discount(sub, vnd(600_000), DiscountAuthority::Unlimited),
            Ok(())
        );
        assert_eq!(
            check_discount(sub, vnd(1), DiscountAuthority::None),
            Err(DiscountError::NotAllowed)
        );
        assert_eq!(
            check_discount(sub, Money::ZERO, DiscountAuthority::None),
            Ok(())
        );
    }

    #[test]
    fn discount_shares_add_up_exactly() {
        let lines = [vnd(1_000_000), vnd(500_000), vnd(500_000)];
        let shares = allocate_discount(&lines, vnd(100_001));
        assert_eq!(shares, vec![vnd(50_000), vnd(25_000), vnd(25_001)]);
        assert_eq!(Money::sum(shares), Ok(vnd(100_001)));
        assert_eq!(
            allocate_discount(&[vnd(1_500_000)], vnd(150_000)),
            vec![vnd(150_000)]
        );
        assert_eq!(
            allocate_discount(&[vnd(0), vnd(0)], Money::ZERO),
            vec![Money::ZERO, Money::ZERO]
        );
    }

    #[test]
    fn the_rounding_remainder_never_pushes_a_line_past_its_price() {
        // Security review INFO: [1, 1, 1] with a discount of 2 used to give the last line 2.
        assert_eq!(
            allocate_discount(&[vnd(1), vnd(1), vnd(1)], vnd(2)),
            vec![vnd(0), vnd(1), vnd(1)]
        );
        assert_eq!(
            allocate_discount(&[vnd(5), vnd(0)], vnd(5)),
            vec![vnd(5), vnd(0)]
        );
    }

    /// Property-style check over many generated sales (deterministic xorshift, no extra crate):
    /// shares add up to the discount and each lies in `0..=line total`.
    #[test]
    fn shares_always_sum_to_the_discount_and_fit_their_lines() {
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = |bound: u64| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state % bound
        };
        for _ in 0..20_000 {
            let n = 1 + next(6) as usize;
            let scale = [10, 1_000, 2_000_000][next(3) as usize];
            let lines: Vec<Money> = (0..n)
                .map(|_| vnd(i64::try_from(next(scale)).unwrap_or(0)))
                .collect();
            let subtotal = lines.iter().map(|m| m.as_i64()).sum::<i64>();
            let discount =
                vnd(i64::try_from(next(u64::try_from(subtotal).unwrap_or(0) + 1)).unwrap_or(0));
            let shares = allocate_discount(&lines, discount);
            assert_eq!(shares.len(), lines.len());
            assert_eq!(
                Money::sum(shares.iter().copied()),
                Ok(discount),
                "{lines:?} {discount:?}"
            );
            for (share, line) in shares.iter().zip(&lines) {
                assert!(
                    *share >= Money::ZERO && share <= line,
                    "{lines:?} {discount:?} {shares:?}"
                );
            }
        }
    }

    #[test]
    fn methods_round_trip() {
        for m in [PaymentMethod::Cash, PaymentMethod::BankTransfer] {
            assert_eq!(PaymentMethod::parse(m.as_str()), Some(m));
        }
        assert_eq!(PaymentMethod::parse("card"), None);
    }
}
