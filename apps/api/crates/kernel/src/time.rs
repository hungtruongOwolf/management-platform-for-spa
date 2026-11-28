//! Instants on the wire (ADR-0026).
//!
//! The domain works in jiff: `jiff::Timestamp` for an instant (`timestamptz`), `jiff::civil::Date`
//! for a calendar day (`date`), `jiff::civil::Time` for a time of day on the wall clock (`time`),
//! `jiff::SignedDuration` for elapsed time and `jiff::tz::TimeZone` for the spa's IANA zone. SQL
//! binds and rows use the `jiff_sqlx` wrappers (each crate's `sqlx.toml` maps the column types to
//! them; `.to_sqlx()` / `.to_jiff()` or `From` convert).
//!
//! Instants keep the text the API always sent: RFC 3339 in UTC with `Z`, the fraction of a second
//! in groups of three digits (none, milliseconds, microseconds or nanoseconds, the shortest that
//! is exact). jiff's own serializer trims trailing zeros instead (`.12Z` where the API sent
//! `.120Z`), which would change strings that clients and stored JSON already hold, so every
//! serialized `Timestamp` field goes through [`rfc3339`].

use jiff::Timestamp;

/// Serde for a `Timestamp` field: `#[serde(with = "kernel::time::rfc3339")]`. Parsing is jiff's:
/// any RFC 3339 instant (any offset, any number of fraction digits), as before.
pub mod rfc3339 {
    use jiff::Timestamp;
    use serde::{Deserialize, Deserializer, Serializer};

    /// The instant as the API writes it, e.g. `2026-11-05T03:00:00Z`, `2026-11-05T03:00:00.120Z`,
    /// `2026-11-05T03:00:00.123456Z`.
    pub fn format(at: Timestamp) -> String {
        match at.subsec_nanosecond() {
            0 => format!("{at:.0}"),
            n if n % 1_000_000 == 0 => format!("{at:.3}"),
            n if n % 1_000 == 0 => format!("{at:.6}"),
            _ => format!("{at:.9}"),
        }
    }

    pub fn serialize<S: Serializer>(at: &Timestamp, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format(*at))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Timestamp, D::Error> {
        Timestamp::deserialize(deserializer)
    }

    /// The same for an `Option<Timestamp>` field:
    /// `#[serde(serialize_with = "kernel::time::rfc3339::serialize_option")]`. Parsing stays
    /// serde's own, so a missing key is still `None`.
    // serde's `serialize_with` hands the field by reference.
    #[allow(clippy::ref_option)]
    pub fn serialize_option<S: Serializer>(
        at: &Option<Timestamp>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match at {
            Some(at) => serializer.serialize_str(&format(*at)),
            None => serializer.serialize_none(),
        }
    }
}

/// An instant at the precision Postgres stores (microseconds), so a value compared with its own
/// round trip through a `timestamptz` column is equal.
pub fn to_micros(at: Timestamp) -> Timestamp {
    Timestamp::from_microsecond(at.as_microsecond()).unwrap_or(at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Wire {
        #[serde(with = "rfc3339")]
        at: Timestamp,
        #[serde(default, serialize_with = "rfc3339::serialize_option")]
        maybe: Option<Timestamp>,
    }

    fn at(nanos: i32) -> Timestamp {
        Timestamp::new(1_793_934_000, nanos).unwrap_or_default()
    }

    /// The exact strings chrono's serializer produced (`SecondsFormat::AutoSi`, `Z`).
    #[test]
    fn instants_keep_the_text_the_api_always_sent() {
        assert_eq!(rfc3339::format(at(0)), "2026-11-06T03:00:00Z");
        assert_eq!(rfc3339::format(at(120_000_000)), "2026-11-06T03:00:00.120Z");
        assert_eq!(
            rfc3339::format(at(123_450_000)),
            "2026-11-06T03:00:00.123450Z"
        );
        assert_eq!(
            rfc3339::format(at(123_456_000)),
            "2026-11-06T03:00:00.123456Z"
        );
        assert_eq!(rfc3339::format(at(100_000)), "2026-11-06T03:00:00.000100Z");
        assert_eq!(rfc3339::format(at(1)), "2026-11-06T03:00:00.000000001Z");
    }

    #[test]
    fn fields_round_trip_and_accept_any_offset() -> Result<(), serde_json::Error> {
        let wire = Wire {
            at: at(120_000_000),
            maybe: None,
        };
        let json = serde_json::to_string(&wire)?;
        assert_eq!(json, r#"{"at":"2026-11-06T03:00:00.120Z","maybe":null}"#);
        assert_eq!(serde_json::from_str::<Wire>(&json)?, wire);

        // What chrono wrote into stored JSON, and what clients send, still parses.
        let old: Wire = serde_json::from_str(
            r#"{"at":"2026-11-06T10:00:00.123450+07:00","maybe":"2026-11-06T03:00:00.000100Z"}"#,
        )?;
        assert_eq!(old.at, at(123_450_000));
        assert_eq!(old.maybe, Some(at(100_000)));
        Ok(())
    }

    #[test]
    fn an_instant_without_an_offset_is_rejected() {
        assert!(
            serde_json::from_str::<Wire>(r#"{"at":"2026-11-06T03:00:00","maybe":null}"#).is_err()
        );
    }

    #[test]
    fn micros_match_the_database_precision() {
        assert_eq!(to_micros(at(123_456_789)), at(123_456_000));
    }
}
