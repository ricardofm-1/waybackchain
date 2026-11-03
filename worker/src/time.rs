//! UTC calendar helpers (no chrono: keeps the Worker bundle small).

pub const MS_PER_DAY: u64 = 86_400_000;
/// First Bitcoin block.
pub const MIN_DATE: &str = "2009-01-03";

/// Days since 1970-01-01 → (year, month, day). Howard Hinnant's algorithm.
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// (year, month, day) → days since 1970-01-01.
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = i64::from((m + 9) % 12);
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn format_date(days: i64) -> String {
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn today(now_ms: u64) -> String {
    format_date((now_ms / MS_PER_DAY) as i64)
}

/// RFC 3339 timestamp of the next 00:00 UTC.
pub fn next_reset(now_ms: u64) -> String {
    format!("{}T00:00:00Z", format_date((now_ms / MS_PER_DAY) as i64 + 1))
}

/// Parses a strict `YYYY-MM-DD` that names a real calendar day.
pub fn parse_date(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let num = |r: std::ops::Range<usize>| -> Option<u32> {
        let part = &s[r];
        part.bytes().all(|c| c.is_ascii_digit()).then(|| part.parse().ok())?
    };
    let (y, m, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    if !(1..=12).contains(&m) || d == 0 {
        return None;
    }
    let days = days_from_civil(i64::from(y), m, d);
    // Round trip rejects 2023-02-30 and friends.
    (civil_from_days(days) == (i64::from(y), m, d)).then_some(days)
}

/// A lookup date is valid from the first Bitcoin block up to today (UTC).
pub fn is_valid_lookup_date(s: &str, now_ms: u64) -> bool {
    match (parse_date(s), parse_date(MIN_DATE)) {
        (Some(d), Some(min)) => d >= min && d <= (now_ms / MS_PER_DAY) as i64,
        _ => false,
    }
}

/// Cutoff `T`: 23:59:59 UTC on the date, as Unix seconds.
pub fn cutoff_unix(date: &str) -> Option<i64> {
    parse_date(date).map(|d| d * 86_400 + 86_399)
}

#[cfg(test)]
mod tests {
    use super::*;

    // 2026-11-01T23:45:00Z
    const NOW: u64 = 1_793_576_700_000;

    #[test]
    fn round_trips() {
        for days in [-1000, 0, 14_247, 20_393, 60_000] {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(days_from_civil(y, m, d), days);
        }
        assert_eq!(format_date(0), "1970-01-01");
        assert_eq!(parse_date("2009-01-03"), Some(14_247));
    }

    #[test]
    fn today_and_reset() {
        assert_eq!(today(NOW), "2026-11-01");
        assert_eq!(next_reset(NOW), "2026-11-02T00:00:00Z");
    }

    #[test]
    fn lookup_date_bounds() {
        assert!(is_valid_lookup_date("2009-01-03", NOW));
        assert!(is_valid_lookup_date("2026-11-01", NOW));
        assert!(!is_valid_lookup_date("2009-01-02", NOW));
        assert!(!is_valid_lookup_date("2026-11-02", NOW));
        assert!(!is_valid_lookup_date("2023-02-29", NOW));
        assert!(!is_valid_lookup_date("2024-2-01", NOW));
        assert!(!is_valid_lookup_date("+024-02-01", NOW));
    }

    #[test]
    fn cutoff() {
        // 2009-01-03T23:59:59Z
        assert_eq!(cutoff_unix("2009-01-03"), Some(1_231_027_199));
    }
}
