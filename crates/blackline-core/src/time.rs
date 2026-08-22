//! UTC timestamps without a datetime crate.
//!
//! Revision markup (`w:ins` / `w:del`) and core properties need ISO-8601
//! instants. Pulling in `chrono` for that is not worth a dependency.

use std::time::{SystemTime, UNIX_EPOCH};

/// Current UTC time as `YYYY-MM-DDTHH:MM:SSZ`.
pub fn utc_now_iso() -> String {
    format_unix(unix_secs())
}

/// Format a Unix timestamp (seconds) as `YYYY-MM-DDTHH:MM:SSZ`.
pub fn format_unix(total: u64) -> String {
    let mut days = total / 86400;
    let rem = total % 86400;
    let hour = rem / 3600;
    let min = (rem % 3600) / 60;
    let sec = rem % 60;

    let mut year = 1970u64;
    loop {
        let diy = if is_leap(year) { 366 } else { 365 };
        if days < diy {
            break;
        }
        days -= diy;
        year += 1;
    }

    const DIM: [u64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let leap = is_leap(year);
    let mut month = 1u64;
    for (i, &dim) in DIM.iter().enumerate() {
        let d = if i == 1 && leap { 29 } else { dim };
        if days < d {
            break;
        }
        days -= d;
        month += 1;
    }

    format!(
        "{year:04}-{month:02}-{:02}T{hour:02}:{min:02}:{sec:02}Z",
        days + 1
    )
}

fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn is_leap(year: u64) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_unix_epochs() {
        assert_eq!(format_unix(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_unix(86400), "1970-01-02T00:00:00Z");
        // 2000-01-01 — a leap-year century.
        assert_eq!(format_unix(946684800), "2000-01-01T00:00:00Z");
        // 2024-02-29 12:00:00 — leap day.
        assert_eq!(format_unix(1709208000), "2024-02-29T12:00:00Z");
    }

    #[test]
    fn now_is_plausible() {
        let s = utc_now_iso();
        assert!(s.starts_with("20"));
        assert!(s.ends_with('Z'));
        assert_eq!(s.len(), 20);
    }
}
