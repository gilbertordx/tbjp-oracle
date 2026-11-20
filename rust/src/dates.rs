//! Date parsing helpers.
//!
//! Python equivalents: `datetime.fromisoformat(...)` and
//! `datetime.strptime(..., "%B %d, %Y at %I:%M %p")`, which were repeated in
//! `ingest.py`, `scraper.py` and `vector_store.py`. In Rust they live in one
//! place (DRY, which your AGENTS.md asks for).
//!
//! We use `NaiveDateTime` (a date and time with no timezone) because the
//! forum dates carry no timezone either.

use chrono::{DateTime, NaiveDateTime};

/// Parse an ISO-8601 timestamp such as `2026-03-02T15:12:00`, with or without
/// fractional seconds or a UTC offset. An offset is dropped, not converted.
pub fn parse_iso(raw: &str) -> Option<NaiveDateTime> {
    let raw = raw.trim();
    NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S%.f")
        .ok()
        .or_else(|| DateTime::parse_from_rfc3339(raw).ok().map(|d| d.naive_local()))
}

/// Parse the scraped forum date, e.g. `March 2, 2026 at 3:12 pm`.
/// Runs of whitespace are collapsed first, like the Python
/// `" ".join(raw.split())`.
pub fn parse_forum_date(raw: &str) -> Option<NaiveDateTime> {
    let normalized = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    NaiveDateTime::parse_from_str(&normalized, "%B %d, %Y at %I:%M %p").ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_iso() {
        let d = parse_iso("2026-03-02T15:12:00").unwrap();
        assert_eq!(d.to_string(), "2026-03-02 15:12:00");
    }

    #[test]
    fn parses_iso_with_offset_and_fraction() {
        assert!(parse_iso("2026-03-02T15:12:00.123456").is_some());
        assert!(parse_iso("2026-03-02T15:12:00+00:00").is_some());
        assert!(parse_iso("not a date").is_none());
    }

    #[test]
    fn parses_forum_dates_in_any_case() {
        let pm = parse_forum_date("March 2, 2026 at 3:12 pm").unwrap();
        assert_eq!(pm.to_string(), "2026-03-02 15:12:00");
        let am = parse_forum_date("March  10, 2026   at 10:22 AM").unwrap();
        assert_eq!(am.to_string(), "2026-03-10 10:22:00");
        assert!(parse_forum_date("yesterday").is_none());
    }
}
