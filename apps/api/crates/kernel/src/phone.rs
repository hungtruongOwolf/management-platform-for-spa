//! Vietnamese mobile numbers normalised to E.164 (`+84` + 9 digits) at the boundary.
//! `0905 123 456`, `0905.123.456`, `+84 905 123 456` and `84905123456` are the same number.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[serde(try_from = "String", into = "String")]
#[sqlx(transparent)]
pub struct PhoneE164(String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid Vietnamese phone number")]
pub struct InvalidPhone;

impl PhoneE164 {
    pub fn parse(input: &str) -> Result<Self, InvalidPhone> {
        let allowed_separators = |c: char| c.is_whitespace() || matches!(c, '.' | '-' | '(' | ')');
        if input
            .chars()
            .any(|c| !(c.is_ascii_digit() || c == '+' || allowed_separators(c)))
        {
            return Err(InvalidPhone);
        }
        let has_plus = input.trim_start().starts_with('+');
        if input.matches('+').count() > usize::from(has_plus) {
            return Err(InvalidPhone);
        }
        let digits: String = input.chars().filter(char::is_ascii_digit).collect();

        let national = if let Some(rest) = digits.strip_prefix("84").filter(|r| r.len() == 9) {
            rest
        } else if !has_plus && digits.len() == 10 && digits.starts_with('0') {
            &digits[1..]
        } else {
            return Err(InvalidPhone);
        };
        // Vietnamese mobile prefixes after the trunk zero: 3, 5, 7, 8, 9.
        if !national.starts_with(['3', '5', '7', '8', '9']) {
            return Err(InvalidPhone);
        }
        Ok(Self(format!("+84{national}")))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// National display form, e.g. `0905 123 456`.
    pub fn display_national(&self) -> String {
        let national = &self.0[3..];
        format!("0{} {} {}", &national[..3], &national[3..6], &national[6..])
    }
}

impl TryFrom<String> for PhoneE164 {
    type Error = InvalidPhone;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<PhoneE164> for String {
    fn from(value: PhoneE164) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equivalent_spellings_normalise_to_the_same_number() {
        let expected = Ok(PhoneE164("+84905123456".to_owned()));
        for input in [
            "0905123456",
            "0905 123 456",
            "0905.123.456",
            "+84 905 123 456",
            "84905123456",
            "(090) 512-3456",
        ] {
            assert_eq!(PhoneE164::parse(input), expected, "{input}");
        }
    }

    #[test]
    fn rejects_malformed_numbers() {
        for input in [
            "",
            "12345",
            "090512345",
            "09051234567",
            "1905123456",
            "0205123456",
            "+1 415 555 0100",
            "0905abc456",
            "++84905123456",
            "+0905123456",
        ] {
            assert_eq!(PhoneE164::parse(input), Err(InvalidPhone), "{input}");
        }
    }

    #[test]
    fn displays_in_national_format() {
        assert_eq!(
            PhoneE164::parse("+84905123456").map(|p| p.display_national()),
            Ok("0905 123 456".to_owned())
        );
    }
}
