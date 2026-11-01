//! Ethereum: 0x + 40 hex. Any single case is accepted; mixed case must pass
//! the EIP-55 checksum.

use crate::{AddressError, Chain};
use sha3::{Digest, Keccak256};

fn bad(reason: &'static str) -> AddressError {
    AddressError::Malformed { chain: Chain::Eth, reason }
}

pub fn validate(s: &str) -> Result<String, AddressError> {
    let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) else {
        return Err(AddressError::Unrecognised);
    };
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(AddressError::Unrecognised);
    }
    if hex.len() != 40 {
        return Err(bad("must be 40 hex characters after 0x"));
    }

    let has_upper = hex.bytes().any(|b| b.is_ascii_uppercase());
    let has_lower = hex.bytes().any(|b| b.is_ascii_lowercase());
    if has_upper && has_lower && checksum_encode(hex) != hex {
        return Err(bad("EIP-55 checksum mismatch"));
    }
    Ok(format!("0x{}", hex.to_ascii_lowercase()))
}

/// Returns the EIP-55 mixed-case form of a 40-char hex string (no 0x).
pub fn checksum_encode(hex: &str) -> String {
    let lower = hex.to_ascii_lowercase();
    let hash = Keccak256::digest(lower.as_bytes());
    lower
        .chars()
        .enumerate()
        .map(|(i, c)| {
            let nibble = (hash[i / 2] >> if i % 2 == 0 { 4 } else { 0 }) & 0x0f;
            if c.is_ascii_alphabetic() && nibble >= 8 {
                c.to_ascii_uppercase()
            } else {
                c
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eip55_vectors() {
        for a in [
            "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed",
            "0xfB6916095ca1df60bB79Ce92cE3Ea74c37c5d359",
            "0xdbF03B407c01E7cD3CBea99509d93f8DDDC8C6FB",
            "0xD1220A0cf47c7B9Be7A2E6BA89F429762e7b9aDb",
        ] {
            assert_eq!(validate(a).unwrap(), a.to_ascii_lowercase());
            assert_eq!(checksum_encode(&a[2..]), &a[2..]);
        }
    }

    #[test]
    fn single_case_ok_bad_mixed_rejected() {
        assert!(validate("0x5aaeb6053f3e94c9b9a09f33669435e7ef1beaed").is_ok());
        assert!(validate("0x5AAEB6053F3E94C9B9A09F33669435E7EF1BEAED").is_ok());
        assert!(validate("0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAeD").is_err());
        assert!(validate("0x5aaeb6053f3e94c9b9a09f33669435e7ef1bea").is_err());
    }
}
