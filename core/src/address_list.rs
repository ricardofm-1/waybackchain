//! Parsing the textarea (one address per line) and the `hl_addr` cookie.

use crate::{detect, AddressError, Detected, MAX_ADDRESSES};
use std::collections::HashMap;

/// Separator inside the `hl_addr` cookie. None of the supported address
/// alphabets contain it.
pub const COOKIE_SEPARATOR: char = ',';
/// Keep name + value under the 4096-byte browser limit with some headroom.
pub const COOKIE_MAX_VALUE_BYTES: usize = 4000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineState {
    Valid(Detected),
    Invalid(AddressError),
    /// Same canonical address as an earlier line (1-based line number).
    Duplicate { of_line: usize, detected: Detected },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// 1-based line number in the textarea.
    pub number: usize,
    pub text: String,
    pub state: LineState,
}

/// Classifies every non-blank line.
pub fn parse_lines(text: &str) -> Vec<Line> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    text.lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .map(|(i, l)| {
            let number = i + 1;
            let state = match detect(l) {
                Ok(d) => match seen.get(&d.canonical) {
                    Some(&of_line) => LineState::Duplicate { of_line, detected: d },
                    None => {
                        seen.insert(d.canonical.clone(), number);
                        LineState::Valid(d)
                    }
                },
                Err(e) => LineState::Invalid(e),
            };
            Line { number, text: l.trim().to_string(), state }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubmitError {
    Empty,
    /// Line numbers that failed validation.
    InvalidLines(Vec<usize>),
    /// Unique address count after removing duplicates.
    TooMany(usize),
}

impl std::fmt::Display for SubmitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SubmitError::Empty => f.write_str("Enter at least one address."),
            SubmitError::InvalidLines(lines) => {
                let list: Vec<String> = lines.iter().map(|n| n.to_string()).collect();
                let noun = if lines.len() == 1 { "line" } else { "lines" };
                write!(f, "Fix the invalid {noun}: {}.", list.join(", "))
            }
            SubmitError::TooMany(n) => {
                write!(f, "{n} addresses entered; the limit is {MAX_ADDRESSES} per lookup.")
            }
        }
    }
}

/// Unique, valid addresses ready to submit, or why the form can't be sent.
pub fn submittable(lines: &[Line]) -> Result<Vec<Detected>, SubmitError> {
    let invalid: Vec<usize> = lines
        .iter()
        .filter(|l| matches!(l.state, LineState::Invalid(_)))
        .map(|l| l.number)
        .collect();
    if !invalid.is_empty() {
        return Err(SubmitError::InvalidLines(invalid));
    }
    let unique: Vec<Detected> = lines
        .iter()
        .filter_map(|l| match &l.state {
            LineState::Valid(d) => Some(d.clone()),
            _ => None,
        })
        .collect();
    match unique.len() {
        0 => Err(SubmitError::Empty),
        n if n > MAX_ADDRESSES => Err(SubmitError::TooMany(n)),
        _ => Ok(unique),
    }
}

/// Cookie value for a list of addresses (as typed). Addresses that would push
/// the value past the size limit are left out.
pub fn encode_cookie(addresses: &[Detected]) -> String {
    let mut out = String::new();
    for d in addresses.iter().take(MAX_ADDRESSES) {
        let extra = d.input.len() + usize::from(!out.is_empty());
        if out.len() + extra > COOKIE_MAX_VALUE_BYTES {
            break;
        }
        if !out.is_empty() {
            out.push(COOKIE_SEPARATOR);
        }
        out.push_str(&d.input);
    }
    out
}

/// Parses a cookie value. Returns `None` if anything is off (bad address,
/// too many entries); the caller then discards the cookie silently.
pub fn decode_cookie(value: &str) -> Option<Vec<String>> {
    if value.is_empty() {
        return Some(Vec::new());
    }
    let parts: Vec<&str> = value.split(COOKIE_SEPARATOR).collect();
    if parts.len() > MAX_ADDRESSES {
        return None;
    }
    parts
        .into_iter()
        .map(|p| detect(p).ok().filter(|d| d.input == p).map(|d| d.input))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Chain;

    const BTC: &str = "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa";
    const ETH: &str = "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed";
    const SOL: &str = "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM";
    const XRP: &str = "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh";
    const ADA: &str = "addr1vx2fxv2umyhttkxyxp8x0dlpdt3k6cwng5pxj3jhsydzers66hrl8";

    #[test]
    fn detects_each_chain_exactly_once() {
        for (a, chain) in [(BTC, Chain::Btc), (ETH, Chain::Eth), (SOL, Chain::Sol), (XRP, Chain::Xrp), (ADA, Chain::Ada)] {
            assert_eq!(detect(a).unwrap().chain, chain, "{a}");
        }
    }

    #[test]
    fn duplicates_by_canonical_form() {
        let text = format!("{ETH}\n\n  {}  \n{BTC}", ETH.to_ascii_lowercase());
        let lines = parse_lines(&text);
        assert_eq!(lines.len(), 3);
        assert!(matches!(lines[1].state, LineState::Duplicate { of_line: 1, .. }));
        assert_eq!(lines[1].number, 3);
        assert_eq!(submittable(&lines).unwrap().len(), 2);
    }

    #[test]
    fn submit_rejects_invalid_and_too_many() {
        let lines = parse_lines(&format!("{BTC}\nnope\n{XRP}"));
        assert_eq!(submittable(&lines), Err(SubmitError::InvalidLines(vec![2])));

        // 31 distinct valid ETH addresses
        let many: Vec<String> = (0..31).map(|i| format!("0x{:040x}", i + 1)).collect();
        let lines = parse_lines(&many.join("\n"));
        assert_eq!(submittable(&lines), Err(SubmitError::TooMany(31)));

        assert_eq!(submittable(&parse_lines("\n  \n")), Err(SubmitError::Empty));
    }

    #[test]
    fn cookie_round_trip() {
        let list = submittable(&parse_lines(&[BTC, ETH, SOL, XRP, ADA].join("\n"))).unwrap();
        let value = encode_cookie(&list);
        assert_eq!(decode_cookie(&value).unwrap(), vec![BTC, ETH, SOL, XRP, ADA]);
    }

    #[test]
    fn bad_cookie_is_discarded() {
        assert_eq!(decode_cookie(&format!("{BTC},garbage")), None);
        assert_eq!(decode_cookie(&format!("{BTC}%2C")), None);
        assert_eq!(decode_cookie(""), Some(vec![]));
    }

    #[test]
    fn thirty_long_cardano_addresses_fit() {
        let base = "addr1qx2fxv2umyhttkxyxp8x0dlpdt3k6cwng5pxj3jhsydzer3n0d3vllmyqwsx5wktcd8cc3sq835lu7drv2xwl2wywfgse35a3x";
        let d = detect(base).unwrap();
        let list = vec![d; 30];
        let value = encode_cookie(&list);
        assert_eq!(value.split(',').count(), 30);
        assert!(value.len() <= COOKIE_MAX_VALUE_BYTES);
    }
}

#[cfg(test)]
mod donation_addresses {
    use crate::{detect, Chain};

    /// The donation addresses shown in the site footer must be valid.
    #[test]
    fn all_valid() {
        for (a, chain) in [
            ("bc1qeywc96rfefjr4220zewj9zxmds4yz0ctl0h63z", Chain::Btc),
            ("0x189419C6A19d11263c28E5391a473741e44EF459", Chain::Eth),
            ("9zgzGhbMYSj7Sac19jgSwP1UoZ6kWqXURHKs6ARe3w8X", Chain::Sol),
            ("rKGTvURZpK4ug3TrHPikoafywyaBjuMaXh", Chain::Xrp),
            ("addr1q8qwc455erdfevgj8xv78r262fs8eu6xln9u3s7lps8vvygckhpjgqxgmkcxvr2zhjjrdd9ty9ggev4yahznlzymg8cs5n6pt6", Chain::Ada),
        ] {
            assert_eq!(detect(a).map(|d| d.chain), Ok(chain), "{a}");
        }
    }
}
