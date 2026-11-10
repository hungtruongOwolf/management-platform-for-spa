//! A tiny deterministic generator (SplitMix64): the same seed gives the same demo spa every run.
//! Not for anything secret.

pub struct Rng(u64);

impl Rng {
    pub const fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n` (`n` > 0; 0 when `n` is 0).
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        let n64 = u64::try_from(n).unwrap_or(u64::MAX);
        usize::try_from(self.next_u64() % n64).unwrap_or(0)
    }

    /// True with probability `percent` / 100.
    pub fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }

    /// Uniform whole days in `0..=max` (0 when `max` is negative).
    pub fn days(&mut self, max: i64) -> i64 {
        let n = usize::try_from(max.max(0)).unwrap_or(0).saturating_add(1);
        i64::try_from(self.below(n)).unwrap_or(0)
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        items.get(self.below(items.len()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence_and_bounded() {
        let (mut a, mut b) = (Rng::new(7), Rng::new(7));
        for _ in 0..1000 {
            let (x, y) = (a.below(13), b.below(13));
            assert_eq!(x, y);
            assert!(x < 13);
        }
        assert_eq!(Rng::new(1).below(0), 0);
    }
}
