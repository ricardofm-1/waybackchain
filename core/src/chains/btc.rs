//! Bitcoin mainnet: legacy P2PKH (1…), P2SH (3…), bech32 / bech32m (bc1…).

use crate::{AddressError, Chain};

fn bad(reason: &'static str) -> AddressError {
    AddressError::Malformed { chain: Chain::Btc, reason }
}

pub fn validate(s: &str) -> Result<String, AddressError> {
    let lower = s.to_ascii_lowercase();
    if lower.starts_with("bc1") {
        return validate_segwit(s);
    }
    if s.starts_with('1') || s.starts_with('3') {
        return validate_base58(s);
    }
    Err(AddressError::Unrecognised)
}

fn validate_base58(s: &str) -> Result<String, AddressError> {
    if !(25..=35).contains(&s.len()) {
        return Err(AddressError::Unrecognised);
    }
    let bytes = bs58::decode(s)
        .with_check(None)
        .into_vec()
        .map_err(|e| match e {
            bs58::decode::Error::InvalidChecksum { .. } => bad("checksum mismatch"),
            _ => AddressError::Unrecognised,
        })?;
    // version byte + 20-byte hash
    if bytes.len() != 21 {
        return Err(AddressError::Unrecognised);
    }
    match (bytes[0], s.as_bytes()[0]) {
        (0x00, b'1') | (0x05, b'3') => Ok(s.to_string()),
        _ => Err(AddressError::Unrecognised),
    }
}

fn validate_segwit(s: &str) -> Result<String, AddressError> {
    // `segwit::decode` checks the checksum variant (bech32 for v0, bech32m for
    // v1+), the witness version and the program length.
    let (hrp, _version, _program) =
        bech32::segwit::decode(s).map_err(|_| bad("bad bech32 checksum or witness program"))?;
    if hrp != bech32::hrp::BC {
        return Err(bad("not a mainnet address"));
    }
    // bech32 is case-insensitive but mixed case is rejected by the decoder;
    // canonical form is lowercase.
    Ok(s.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid() {
        for a in [
            "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa", // genesis coinbase
            "3J98t1WpEZ73CNmQviecrnyiWrnqRhWNLy",
            "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4", // BIP-173 vector
            "BC1QW508D6QEJXTDG4Y5R3ZARVARY0C5XW7KV8F3T4",
            "bc1p0xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vqzk5jj0", // BIP-350 vector
        ] {
            assert!(validate(a).is_ok(), "{a}");
        }
    }

    #[test]
    fn invalid() {
        for a in [
            "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNb",                            // checksum
            "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t5",                    // checksum
            "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx",                    // testnet
            "bc1pw508d6qejxtdg4y5r3zarvary0c5xw7kw508d6qejxtdg4y5r3zarvary0c5xw7k7grplx", // v1 with bech32 (not m)
        ] {
            assert!(validate(a).is_err(), "{a}");
        }
    }
}
