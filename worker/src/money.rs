//! Exact decimal arithmetic for amounts and USD values. Never floats.

use alloy_primitives::U256;

/// Positive decimal string, plain or scientific (`"12.5"`, `"1.85e-07"`,
/// `"2e3"`) → (integer mantissa, decimals). `None` for zero, negative,
/// malformed, or more than 77 decimals.
pub fn parse_decimal(s: &str) -> Option<(U256, u8)> {
    let s = s.trim();
    if s.starts_with('-') {
        return None;
    }
    let (num, exp) = match s.split_once(['e', 'E']) {
        Some((n, e)) => (n, e.trim_start_matches('+').parse::<i32>().ok()?),
        None => (s, 0),
    };
    let (int, frac) = num.split_once('.').unwrap_or((num, ""));
    if int.is_empty() && frac.is_empty()
        || !(int.bytes().all(|b| b.is_ascii_digit()) && frac.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    let mut digits = format!("{int}{frac}");
    let mut decimals = frac.len() as i32 - exp;
    if decimals < 0 {
        digits.push_str(&"0".repeat((-decimals) as usize));
        decimals = 0;
    }
    if decimals > 77 {
        return None;
    }
    let digits = digits.trim_start_matches('0');
    if digits.is_empty() {
        return None;
    }
    Some((U256::from_str_radix(digits, 10).ok()?, decimals as u8))
}

/// USD value of `amount` (a decimal string) at `price` (a decimal string),
/// as a 2-decimal string rounded half up. `None` if either can't be parsed
/// or the product doesn't fit in 256 bits (absurd spam supplies).
pub fn usd_value(amount: &str, price: &str) -> Option<String> {
    let (a, ad) = parse_decimal(amount).or_else(|| (amount.trim() == "0").then_some((U256::ZERO, 0)))?;
    let (p, pd) = parse_decimal(price)?;
    let num = a.checked_mul(p)?.checked_mul(U256::from(100u8))?;
    let div = U256::from(10u8).checked_pow(U256::from(u32::from(ad) + u32::from(pd)))?;
    let cents = (num + div / U256::from(2u8)) / div;
    let s = format!("{cents:0>3}");
    let (int, frac) = s.split_at(s.len() - 2);
    Some(format!("{int}.{frac}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimals_plain_and_scientific() {
        assert_eq!(parse_decimal("10.5"), Some((U256::from(105u64), 1)));
        assert_eq!(parse_decimal("1.23e-5"), Some((U256::from(123u64), 7)));
        assert_eq!(parse_decimal("1.85502e-07"), Some((U256::from(185_502u64), 12)));
        assert_eq!(parse_decimal("2e3"), Some((U256::from(2000u64), 0)));
        assert_eq!(parse_decimal("44147.0"), Some((U256::from(441_470u64), 1)));
        assert_eq!(parse_decimal("0"), None);
        assert_eq!(parse_decimal("-4.2"), None);
        assert_eq!(parse_decimal("1e-90"), None);
        assert_eq!(parse_decimal("abc"), None);
        assert_eq!(parse_decimal("."), None);
    }

    #[test]
    fn usd() {
        // 0.0034155 BTC at $44,147 = $150.7840785 → rounds down
        assert_eq!(usd_value("0.0034155", "44147").as_deref(), Some("150.78"));
        // 659,674,430.87 ELON at $1.85502e-7 = $122.37
        assert_eq!(usd_value("659674430.870154627440676081", "1.85502e-07").as_deref(), Some("122.37"));
        assert_eq!(usd_value("1", "0.004").as_deref(), Some("0.00"));
        assert_eq!(usd_value("1", "0.005").as_deref(), Some("0.01"));
        assert_eq!(usd_value("0", "44147").as_deref(), Some("0.00"));
        assert_eq!(usd_value("12", "nope"), None);
    }
}
