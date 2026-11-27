//! The draw's rules, without I/O (spec lucky-draw §3): who may win a prize, what is left of it, when
//! a draw is over, the limits of what is entered, and the uniform pick of a winner from a random
//! source the caller hands in (the OS CSPRNG in production, a scripted one in tests).

use kernel::FieldError;
use unicode_normalization::UnicodeNormalization;

/// Longest draw name, in characters.
pub const DRAW_NAME_MAX: usize = 80;
/// Longest participant name.
pub const PARTICIPANT_NAME_MAX: usize = 120;
/// Longest prize name.
pub const PRIZE_NAME_MAX: usize = 140;
/// Longest prize description.
pub const DESCRIPTION_MAX: usize = 250;
/// Slots of one prize.
pub const QUANTITY_RANGE: std::ops::RangeInclusive<i64> = 1..=1000;
/// People added by one request (the web sends a longer paste in batches).
pub const PARTICIPANTS_PER_REQUEST: usize = 500;
/// People in one draw.
pub const PARTICIPANTS_MAX: usize = 2000;
/// Prizes in one draw.
pub const PRIZES_MAX: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Gender {
    Female,
    Male,
}

impl Gender {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Female => "female",
            Self::Male => "male",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "female" => Some(Self::Female),
            "male" => Some(Self::Male),
            _ => None,
        }
    }
}

/// Who a prize is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Eligible {
    All,
    Female,
    Male,
}

impl Eligible {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Female => "female",
            Self::Male => "male",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "all" => Some(Self::All),
            "female" => Some(Self::Female),
            "male" => Some(Self::Male),
            _ => None,
        }
    }

    pub const fn admits(self, gender: Gender) -> bool {
        matches!(
            (self, gender),
            (Self::All, _) | (Self::Female, Gender::Female) | (Self::Male, Gender::Male)
        )
    }

    /// People still in the draw who may win this prize.
    pub const fn eligible_count(self, undrawn: Undrawn) -> u32 {
        match self {
            Self::All => undrawn.female + undrawn.male,
            Self::Female => undrawn.female,
            Self::Male => undrawn.male,
        }
    }
}

/// The people of a draw without a result yet (a forfeited result still counts as one: whoever won
/// once, present or not, is out of every later spin, spec L3).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Undrawn {
    pub female: u32,
    pub male: u32,
}

impl Undrawn {
    pub fn of(genders: impl IntoIterator<Item = Gender>) -> Self {
        genders.into_iter().fold(Self::default(), |mut u, g| {
            match g {
                Gender::Female => u.female += 1,
                Gender::Male => u.male += 1,
            }
            u
        })
    }
}

/// A prize as the draw sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrizeTally {
    pub quantity: u32,
    /// Results kept (not forfeited).
    pub awarded: u32,
    pub eligible_count: u32,
}

impl PrizeTally {
    /// Slots left. Never negative: a quantity below what was awarded is refused on edit.
    pub const fn remaining(self) -> u32 {
        self.quantity.saturating_sub(self.awarded)
    }

    /// A spin of this prize can produce a winner.
    pub const fn drawable(self) -> bool {
        self.remaining() > 0 && self.eligible_count > 0
    }
}

/// The derived state of a draw (never stored).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// No result yet.
    NotStarted,
    /// Results, and some prize can still be drawn.
    InProgress,
    /// Results, and no prize can be drawn any more (out of slots or of people).
    Finished,
}

impl Status {
    /// The one definition of the draw's state (contract §3): finished = at least one result and
    /// no prize with both a slot and an eligible person left.
    pub fn of(results: usize, prizes: impl IntoIterator<Item = PrizeTally>) -> Self {
        if results == 0 {
            Self::NotStarted
        } else if prizes.into_iter().any(PrizeTally::drawable) {
            Self::InProgress
        } else {
            Self::Finished
        }
    }

    pub const fn finished(self) -> bool {
        matches!(self, Self::Finished)
    }
}

/// Text as stored: NFC (a decomposed "Hà" typed on another keyboard is the same name), trimmed,
/// inner whitespace (tabs, line breaks from a paste) collapsed to single spaces.
pub fn clean_text(input: &str) -> String {
    input
        .nfc()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// A required name of at most `max` characters, cleaned; field errors are pushed to `errors`.
pub fn required_text(
    input: &str,
    field: impl Into<String>,
    max: usize,
    errors: &mut Vec<FieldError>,
) -> String {
    let text = clean_text(input);
    let len = text.chars().count();
    if len == 0 {
        errors.push(FieldError::new(field, "required"));
    } else if len > max {
        errors.push(FieldError::new(field, "too_long"));
    }
    text
}

/// An optional text: blank = none.
pub fn optional_text(
    input: Option<&str>,
    field: impl Into<String>,
    max: usize,
    errors: &mut Vec<FieldError>,
) -> Option<String> {
    let text = clean_text(input?);
    if text.is_empty() {
        return None;
    }
    if text.chars().count() > max {
        errors.push(FieldError::new(field, "too_long"));
    }
    Some(text)
}

// ---------------------------------------------------------------- the pick

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the operating system's random source is unavailable")]
pub struct RandomUnavailable;

/// Uniform 64-bit words. The winner is never chosen by the client (spec §7): the server draws it
/// from this source. `Send`: a spin holds it across the awaits of its transaction.
pub trait RandomSource: Send {
    fn next_u64(&mut self) -> Result<u64, RandomUnavailable>;
}

/// The operating system's CSPRNG (`getrandom`).
#[derive(Debug, Clone, Copy, Default)]
pub struct OsRandom;

impl RandomSource for OsRandom {
    fn next_u64(&mut self) -> Result<u64, RandomUnavailable> {
        let mut bytes = [0u8; 8];
        getrandom::fill(&mut bytes).map_err(|_| RandomUnavailable)?;
        Ok(u64::from_le_bytes(bytes))
    }
}

/// An index in `0..n`, each equally likely: words at or above the largest multiple of `n` that
/// fits in a `u64` are drawn again (rejection sampling), so `word % n` has no modulo bias. At most
/// `n` of the 2^64 words are rejected, so a second draw is needed with probability below
/// `n / 2^64`. `None` when `n` is 0 (nobody to pick).
pub fn uniform_index(
    n: usize,
    rng: &mut dyn RandomSource,
) -> Result<Option<usize>, RandomUnavailable> {
    let Ok(n64) = u64::try_from(n) else {
        return Ok(None);
    };
    if n64 == 0 {
        return Ok(None);
    }
    let zone = u64::MAX - u64::MAX % n64;
    loop {
        let word = rng.next_u64()?;
        if word < zone {
            // `word % n64 < n64 = n`, so it fits in a usize.
            return Ok(usize::try_from(word % n64).ok());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hands out the scripted words in order, then fails.
    struct Script<'a>(std::slice::Iter<'a, u64>);

    impl<'a> Script<'a> {
        fn new(words: &'a [u64]) -> Self {
            Self(words.iter())
        }
    }

    impl RandomSource for Script<'_> {
        fn next_u64(&mut self) -> Result<u64, RandomUnavailable> {
            self.0.next().copied().ok_or(RandomUnavailable)
        }
    }

    #[test]
    fn eligibility_follows_the_prize_gender() {
        assert!(Eligible::All.admits(Gender::Female) && Eligible::All.admits(Gender::Male));
        assert!(Eligible::Female.admits(Gender::Female) && !Eligible::Female.admits(Gender::Male));
        assert!(Eligible::Male.admits(Gender::Male) && !Eligible::Male.admits(Gender::Female));
        let undrawn = Undrawn::of([Gender::Female, Gender::Female, Gender::Male]);
        assert_eq!(undrawn, Undrawn { female: 2, male: 1 });
        assert_eq!(Eligible::All.eligible_count(undrawn), 3);
        assert_eq!(Eligible::Female.eligible_count(undrawn), 2);
        assert_eq!(Eligible::Male.eligible_count(undrawn), 1);
    }

    #[test]
    fn codes_round_trip() {
        for g in [Gender::Female, Gender::Male] {
            assert_eq!(Gender::parse(g.as_str()), Some(g));
        }
        for e in [Eligible::All, Eligible::Female, Eligible::Male] {
            assert_eq!(Eligible::parse(e.as_str()), Some(e));
        }
        assert_eq!(Gender::parse("unknown"), None);
        assert_eq!(Gender::parse("Female"), None, "codes, not labels");
        assert_eq!(Eligible::parse("both"), None);
    }

    fn tally(quantity: u32, awarded: u32, eligible_count: u32) -> PrizeTally {
        PrizeTally {
            quantity,
            awarded,
            eligible_count,
        }
    }

    #[test]
    fn a_draw_is_finished_when_no_prize_can_be_drawn() {
        assert_eq!(Status::of(0, [tally(3, 0, 5)]), Status::NotStarted);
        assert_eq!(Status::of(0, []), Status::NotStarted, "nothing entered yet");
        assert_eq!(Status::of(1, [tally(3, 1, 4)]), Status::InProgress);
        // Out of slots.
        assert_eq!(Status::of(3, [tally(3, 3, 4)]), Status::Finished);
        // Slots left but nobody eligible (a men-only prize once every man has won).
        assert_eq!(
            Status::of(2, [tally(2, 2, 0), tally(2, 0, 0)]),
            Status::Finished
        );
        // One drawable prize keeps the draw going.
        assert_eq!(
            Status::of(2, [tally(2, 2, 3), tally(1, 0, 3)]),
            Status::InProgress
        );
        // A forfeited result frees a slot again.
        assert_eq!(Status::of(3, [tally(3, 2, 1)]), Status::InProgress);
        assert!(Status::Finished.finished() && !Status::InProgress.finished());
        assert_eq!(tally(2, 5, 1).remaining(), 0, "never negative");
    }

    #[test]
    fn the_pick_stays_in_bounds() -> Result<(), RandomUnavailable> {
        let mut os = OsRandom;
        for n in 1..=64 {
            for _ in 0..50 {
                let i = uniform_index(n, &mut os)?.unwrap_or(usize::MAX);
                assert!(i < n, "{i} out of 0..{n}");
            }
        }
        assert_eq!(uniform_index(0, &mut os)?, None, "nobody to pick");
        assert_eq!(
            uniform_index(1, &mut Script::new(&[u64::MAX - 1]))?,
            Some(0)
        );
        Ok(())
    }

    #[test]
    fn words_beyond_the_last_whole_multiple_are_drawn_again() -> Result<(), RandomUnavailable> {
        // 2^64 ≡ 2 (mod 7): the two highest words would favour 0 and 1, so they are rejected.
        assert_eq!(u64::MAX % 7, 1);
        let picked = uniform_index(7, &mut Script::new(&[u64::MAX, u64::MAX - 1, 10]))?;
        assert_eq!(picked, Some(3), "10 % 7, after two rejections");
        // The highest accepted word.
        assert_eq!(
            uniform_index(7, &mut Script::new(&[u64::MAX - 2]))?,
            Some(usize::try_from((u64::MAX - 2) % 7).unwrap_or(usize::MAX))
        );
        // A source that fails is an error, never a default pick.
        assert_eq!(
            uniform_index(7, &mut Script::new(&[u64::MAX])),
            Err(RandomUnavailable)
        );
        Ok(())
    }

    #[test]
    fn every_index_is_equally_likely_over_a_whole_cycle() -> Result<(), RandomUnavailable> {
        // Words 0..(n × k) hit each index exactly k times: no index is favoured.
        let n = 6;
        let words: Vec<u64> = (0..60).collect();
        let mut rng = Script::new(&words);
        let mut hits = [0u32; 6];
        for _ in 0..60 {
            if let Some(i) = uniform_index(n, &mut rng)? {
                hits[i] += 1;
            }
        }
        assert_eq!(hits, [10; 6]);
        Ok(())
    }

    #[test]
    fn text_is_normalised_and_limited() {
        let decomposed = "Ha\u{0300}  Thi\u{0323}\tLan\n";
        assert_eq!(clean_text(decomposed), "Hà Thị Lan");
        let mut errors = Vec::new();
        assert_eq!(required_text("  ", "name", 5, &mut errors), "");
        required_text("abcdef", "participants[2].name", 5, &mut errors);
        // 5 characters, 9 bytes: the limit counts characters.
        assert_eq!(required_text("Đặngà", "name", 5, &mut errors), "Đặngà");
        assert_eq!(
            errors,
            [
                FieldError::new("name", "required"),
                FieldError::new("participants[2].name", "too_long"),
            ]
        );
        let mut errors = Vec::new();
        assert_eq!(
            optional_text(Some("   "), "description", 3, &mut errors),
            None
        );
        assert_eq!(optional_text(None, "description", 3, &mut errors), None);
        assert_eq!(
            optional_text(Some("abcd"), "description", 3, &mut errors).as_deref(),
            Some("abcd")
        );
        assert_eq!(errors, [FieldError::new("description", "too_long")]);
    }
}
