//! Tiny ISO 8601 / RFC 3339 formatter for UTC second-precision timestamps.
//!
//! Avoids pulling `chrono` into `graphify-report`. Used to stamp
//! `generated_at` on serialized reports (`analysis.json`,
//! `consolidation-candidates.json`, …) so consumers can compute artifact age
//! without consulting filesystem mtimes — which on POSIX systems do not
//! update when an existing file is overwritten in place inside a directory
//! (the bug this module was extracted for: GH issue #15 / BUG-028).

/// Current UTC time formatted as `YYYY-MM-DDTHH:MM:SSZ`.
pub fn now_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_epoch_seconds_utc(secs)
}

/// Format a Unix epoch (seconds) as an ISO 8601 UTC timestamp.
///
/// Uses Howard Hinnant's civil-date algorithm. Valid for any non-negative
/// Unix timestamp (years 1970+).
pub fn format_epoch_seconds_utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let hour = rem / 3600;
    let minute = (rem % 3600) / 60;
    let second = rem % 60;

    // Shift so that year-0 (= 0000-03-01) is the zero of the era.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y };

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, m, d, hour, minute, second
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_epoch_zero_is_unix_epoch() {
        assert_eq!(format_epoch_seconds_utc(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn format_epoch_known_value() {
        // 1700000000 → 2023-11-14T22:13:20Z (carried over from the
        // pre-extraction test in consolidation.rs).
        assert_eq!(
            format_epoch_seconds_utc(1_700_000_000),
            "2023-11-14T22:13:20Z"
        );
    }

    #[test]
    fn now_iso8601_has_correct_shape() {
        let s = now_iso8601();
        assert_eq!(s.len(), 20, "expected fixed-width YYYY-MM-DDTHH:MM:SSZ");
        assert!(s.ends_with('Z'));
        assert_eq!(s.chars().nth(4), Some('-'));
        assert_eq!(s.chars().nth(10), Some('T'));
    }
}
