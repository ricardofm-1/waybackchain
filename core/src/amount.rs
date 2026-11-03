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

/// Exact sum of two non-negative plain decimal strings ("1.5" + "0.25" =
/// "1.75"), any length. Returns `None` if either isn't a plain decimal.
pub fn add_decimal_str(a: &str, b: &str) -> Option<String> {
    fn split(s: &str) -> Option<(&str, &str)> {
        let (int, frac) = s.split_once('.').unwrap_or((s, ""));
        let ok = !int.is_empty() && int.bytes().all(|c| c.is_ascii_digit()) && frac.bytes().all(|c| c.is_ascii_digit());
        ok.then_some((int, frac))
    }
    let (ai, af) = split(a.trim())?;
    let (bi, bf) = split(b.trim())?;
    let flen = af.len().max(bf.len());
    let ilen = ai.len().max(bi.len());
    // Align both as fixed-width digit strings, then add right to left.
    let x = format!("{ai:0>ilen$}{af:0<flen$}");
    let y = format!("{bi:0>ilen$}{bf:0<flen$}");
    let mut out = Vec::with_capacity(x.len() + 1);
    let mut carry = 0u8;
    for (p, q) in x.bytes().rev().zip(y.bytes().rev()) {
        let d = (p - b'0') + (q - b'0') + carry;
        out.push(b'0' + d % 10);
        carry = d / 10;
    }
    if carry > 0 {
        out.push(b'0' + carry);
    }
    out.reverse();
    let digits = String::from_utf8(out).ok()?;
    let digits = digits.trim_start_matches('0');
    Some(format_units_str(if digits.is_empty() { "0" } else { digits }, u8::try_from(flen).ok()?))
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

    #[test]
    fn adds_exactly() {
        assert_eq!(add_decimal_str("1.5", "0.25").as_deref(), Some("1.75"));
        assert_eq!(add_decimal_str("0.0034155", "0.00009697").as_deref(), Some("0.00351247"));
        assert_eq!(add_decimal_str("999", "1").as_deref(), Some("1000"));
        assert_eq!(add_decimal_str("0.5", "0.5").as_deref(), Some("1"));
        assert_eq!(add_decimal_str("0", "0").as_deref(), Some("0"));
        // Far beyond u128.
        let big = "115792089237316195423570985008687907853269984665640564039457.584007913129639935";
        assert_eq!(
            add_decimal_str(big, "0.000000000000000001").as_deref(),
            Some("115792089237316195423570985008687907853269984665640564039457.584007913129639936")
        );
        assert_eq!(add_decimal_str("1e5", "1"), None);
        assert_eq!(add_decimal_str("", "1"), None);
    }
}
