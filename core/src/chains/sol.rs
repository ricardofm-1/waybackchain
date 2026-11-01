//! Solana: base58 (Bitcoin alphabet) that decodes to exactly 32 bytes.

use crate::{AddressError, Chain};

pub fn validate(s: &str) -> Result<String, AddressError> {
    if !(32..=44).contains(&s.len()) {
        return Err(AddressError::Unrecognised);
    }
    let bytes = bs58::decode(s).into_vec().map_err(|_| AddressError::Unrecognised)?;
    if bytes.len() != 32 {
        return Err(AddressError::Unrecognised);
    }
    // Reject non-canonical encodings (e.g. extra leading '1's).
    if bs58::encode(&bytes).into_string() != s {
        return Err(AddressError::Malformed { chain: Chain::Sol, reason: "non-canonical base58" });
    }
    Ok(s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid() {
        for a in [
            "11111111111111111111111111111111",            // system program
            "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA", // SPL Token program
            "So11111111111111111111111111111111111111112", // wrapped SOL mint
            "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM",
        ] {
            assert!(validate(a).is_ok(), "{a}");
        }
    }

    #[test]
    fn invalid() {
        assert!(validate("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa").is_err()); // 25 bytes
        assert!(validate("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ50A").is_err()); // '0' not base58
    }
}
