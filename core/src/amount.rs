//! Integer base units → decimal strings, at the edge only. Never floats.

/// `format_units(341_550, 8)` → `"0.0034155"`. Trailing zeros are trimmed.
pub fn format_units(value: u128, decimals: u8) -> String {
    let digits = value.to_string();
    let d = usize::from(decimals);
    if d == 0 {
        return digits;
    }
    let padded = format!("{digits:0>width$}", width = d + 1);
    let (int, frac) = padded.split_at(padded.len() - d);
    let frac = frac.trim_end_matches('0');
    if frac.is_empty() {
        int.to_string()
    } else {
        format!("{int}.{frac}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(format_units(0, 8), "0");
        assert_eq!(format_units(341_550, 8), "0.0034155");
        assert_eq!(format_units(100_000_000, 8), "1");
        assert_eq!(format_units(2_100_000_000_000_000, 8), "21000000");
        assert_eq!(format_units(1, 18), "0.000000000000000001");
        assert_eq!(format_units(u128::MAX, 18), "340282366920938463463.374607431768211455");
        assert_eq!(format_units(42, 0), "42");
    }
}
