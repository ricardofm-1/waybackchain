//! Integer base units → decimal strings, at the edge only. Never floats.

/// `format_units(341_550, 8)` → `"0.0034155"`. Trailing zeros are trimmed.
pub fn format_units(value: u128, decimals: u8) -> String {
    format_units_str(&value.to_string(), decimals)
}

/// Same, from a string of decimal digits (for 256-bit token balances).
pub fn format_units_str(digits: &str, decimals: u8) -> String {
    debug_assert!(digits.bytes().all(|b| b.is_ascii_digit()));
    let digits = digits.trim_start_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    let d = usize::from(decimals);
    if d == 0 {
        return digits.to_string();
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
        // 2^256 - 1 with 18 decimals
        let max = "115792089237316195423570985008687907853269984665640564039457584007913129639935";
        assert_eq!(
            format_units_str(max, 18),
            "115792089237316195423570985008687907853269984665640564039457.584007913129639935"
        );
        assert_eq!(format_units_str("000120", 2), "1.2");
    }
}
