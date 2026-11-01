//! XRP Ledger: classic address (r…) or X-address (X…). X-addresses are
//! decoded to the classic address; the destination tag is ignored.

use crate::{AddressError, Chain};

fn bad(reason: &'static str) -> AddressError {
    AddressError::Malformed { chain: Chain::Xrp, reason }
}

fn decode_check(s: &str) -> Result<Vec<u8>, AddressError> {
    bs58::decode(s)
        .with_alphabet(bs58::Alphabet::RIPPLE)
        .with_check(None)
        .into_vec()
        .map_err(|e| match e {
            bs58::decode::Error::InvalidChecksum { .. } => bad("checksum mismatch"),
            _ => AddressError::Unrecognised,
        })
}

fn encode_classic(account_id: &[u8]) -> String {
    let mut payload = Vec::with_capacity(21);
    payload.push(0x00);
    payload.extend_from_slice(account_id);
    bs58::encode(payload)
        .with_alphabet(bs58::Alphabet::RIPPLE)
        .with_check()
        .into_string()
}

pub fn validate(s: &str) -> Result<String, AddressError> {
    match s.as_bytes().first() {
        Some(b'r') => validate_classic(s),
        Some(b'X') => validate_x_address(s),
        _ => Err(AddressError::Unrecognised),
    }
}

fn validate_classic(s: &str) -> Result<String, AddressError> {
    if !(25..=35).contains(&s.len()) {
        return Err(AddressError::Unrecognised);
    }
    let bytes = decode_check(s)?;
    if bytes.len() != 21 || bytes[0] != 0x00 {
        return Err(AddressError::Unrecognised);
    }
    Ok(s.to_string())
}

fn validate_x_address(s: &str) -> Result<String, AddressError> {
    if s.len() != 47 {
        return Err(AddressError::Unrecognised);
    }
    let bytes = decode_check(s)?;
    // 2-byte prefix + 20-byte account ID + 1 flag byte + 8 tag bytes
    if bytes.len() != 31 {
        return Err(AddressError::Unrecognised);
    }
    match bytes[..2] {
        [0x05, 0x44] => {}
        [0x04, 0x93] => return Err(bad("testnet X-address")),
        _ => return Err(AddressError::Unrecognised),
    }
    let flag = bytes[22];
    let tag = &bytes[23..31];
    let tag_ok = match flag {
        0 => tag.iter().all(|&b| b == 0),
        1 => tag[4..].iter().all(|&b| b == 0),
        _ => false,
    };
    if !tag_ok {
        return Err(bad("malformed X-address tag"));
    }
    Ok(encode_classic(&bytes[2..22]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic() {
        assert!(validate("rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh").is_ok()); // genesis
        assert!(validate("rrrrrrrrrrrrrrrrrrrrrhoLvTp").is_ok()); // ACCOUNT_ZERO
        assert!(validate("rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTi").is_err());
    }

    fn make_x(prefix: [u8; 2], classic: &str, tag: Option<u32>) -> String {
        let id = decode_check(classic).unwrap();
        let mut p = prefix.to_vec();
        p.extend_from_slice(&id[1..]);
        p.push(u8::from(tag.is_some()));
        p.extend_from_slice(&tag.unwrap_or(0).to_le_bytes());
        p.extend_from_slice(&[0; 4]);
        bs58::encode(p).with_alphabet(bs58::Alphabet::RIPPLE).with_check().into_string()
    }

    #[test]
    fn x_address_decodes_to_classic() {
        let classic = "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh";
        for tag in [None, Some(0), Some(12345)] {
            let x = make_x([0x05, 0x44], classic, tag);
            assert!(x.starts_with('X') && x.len() == 47, "{x}");
            assert_eq!(validate(&x).unwrap(), classic);
        }
    }

    #[test]
    fn testnet_x_address_rejected() {
        let t = make_x([0x04, 0x93], "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh", None);
        assert!(t.starts_with('T'));
        assert!(validate(&t).is_err());
    }
}
