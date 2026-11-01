//! Cardano mainnet: Shelley payment addresses (bech32 `addr1…`) and Byron
//! addresses (base58 `Ae2…` / `DdzFF…`). Stake addresses are recognised but
//! rejected as not supported in v1.
//!
//! Shelley addresses exceed the 90-char limit of BIP-173 decoders, so this
//! uses bech32's generic checksum check (code length 1023), not `segwit`.

use crate::{AddressError, Chain};
use bech32::primitives::decode::CheckedHrpstring;
use bech32::Bech32;

fn bad(reason: &'static str) -> AddressError {
    AddressError::Malformed { chain: Chain::Ada, reason }
}

pub fn validate(s: &str) -> Result<String, AddressError> {
    let lower = s.to_ascii_lowercase();
    if lower.starts_with("stake1") || lower.starts_with("stake_test1") {
        return Err(AddressError::Unsupported("stake addresses are not supported yet; enter a payment address (addr1…)"));
    }
    if lower.starts_with("addr_test1") {
        return Err(bad("testnet address"));
    }
    if lower.starts_with("addr1") {
        return validate_shelley(s);
    }
    if s.starts_with("Ae2") || s.starts_with("DdzFF") {
        return validate_byron(s);
    }
    Err(AddressError::Unrecognised)
}

fn validate_shelley(s: &str) -> Result<String, AddressError> {
    let checked = CheckedHrpstring::new::<Bech32>(s).map_err(|_| bad("bad bech32 checksum"))?;
    if checked.hrp().as_str() != "addr" {
        return Err(AddressError::Unrecognised);
    }
    let bytes: Vec<u8> = checked.byte_iter().collect();
    let Some(&header) = bytes.first() else {
        return Err(bad("empty payload"));
    };
    let kind = header >> 4;
    let network = header & 0x0f;
    if network != 1 {
        return Err(bad("not a mainnet address"));
    }
    let len_ok = match kind {
        0..=3 => bytes.len() == 57,       // base: header + payment + stake (28 + 28)
        4 | 5 => bytes.len() > 29,        // pointer: header + payment + variable pointer
        6 | 7 => bytes.len() == 29,       // enterprise: header + payment
        14 | 15 => return Err(AddressError::Unsupported("stake addresses are not supported yet")),
        _ => return Err(bad("unknown address type")),
    };
    if !len_ok {
        return Err(bad("wrong payload length"));
    }
    Ok(s.to_ascii_lowercase())
}

fn validate_byron(s: &str) -> Result<String, AddressError> {
    let raw = bs58::decode(s).into_vec().map_err(|_| AddressError::Unrecognised)?;
    let inner = parse_byron_envelope(&raw).ok_or(bad("malformed Byron address"))?;
    if byron_has_network_magic(inner).ok_or(bad("malformed Byron address"))? {
        return Err(bad("testnet Byron address"));
    }
    Ok(s.to_string())
}

/// Minimal CBOR reader: enough to walk a Byron address.
struct Cbor<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Cbor<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn byte(&mut self) -> Option<u8> {
        let b = *self.buf.get(self.pos)?;
        self.pos += 1;
        Some(b)
    }

    /// Reads an item head; returns (major type, argument).
    fn head(&mut self) -> Option<(u8, u64)> {
        let b = self.byte()?;
        let major = b >> 5;
        let info = b & 0x1f;
        let n = match info {
            0..=23 => return Some((major, info as u64)),
            24 => 1,
            25 => 2,
            26 => 4,
            27 => 8,
            _ => return None, // indefinite lengths aren't used in addresses
        };
        let mut v = 0u64;
        for _ in 0..n {
            v = (v << 8) | self.byte()? as u64;
        }
        Some((major, v))
    }

    fn bytes(&mut self) -> Option<&'a [u8]> {
        let (major, len) = self.head()?;
        if major != 2 {
            return None;
        }
        let end = self.pos.checked_add(usize::try_from(len).ok()?)?;
        let out = self.buf.get(self.pos..end)?;
        self.pos = end;
        Some(out)
    }

    fn done(&self) -> bool {
        self.pos == self.buf.len()
    }
}

/// `[ #6.24(bytes .cbor payload), crc32 ]` → payload, if the CRC matches.
fn parse_byron_envelope(raw: &[u8]) -> Option<&[u8]> {
    let mut c = Cbor::new(raw);
    if c.head()? != (4, 2) || c.head()? != (6, 24) {
        return None;
    }
    let payload = c.bytes()?;
    let (major, crc) = c.head()?;
    if major != 0 || !c.done() || crc != crc32fast::hash(payload) as u64 {
        return None;
    }
    Some(payload)
}

/// Payload is `[ root (28 bytes), attributes map, type ]`. Attribute key 2
/// (protocol magic) is present only on non-mainnet addresses.
fn byron_has_network_magic(payload: &[u8]) -> Option<bool> {
    let mut c = Cbor::new(payload);
    if c.head()? != (4, 3) || c.bytes()?.len() != 28 {
        return None;
    }
    let (major, entries) = c.head()?;
    if major != 5 {
        return None;
    }
    let mut magic = false;
    for _ in 0..entries {
        let (kmaj, key) = c.head()?;
        if kmaj != 0 {
            return None;
        }
        c.bytes()?;
        magic |= key == 2;
    }
    let (tmaj, _) = c.head()?;
    if tmaj != 0 || !c.done() {
        return None;
    }
    Some(magic)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Vectors from CIP-19.
    const BASE: &str = "addr1qx2fxv2umyhttkxyxp8x0dlpdt3k6cwng5pxj3jhsydzer3n0d3vllmyqwsx5wktcd8cc3sq835lu7drv2xwl2wywfgse35a3x";
    const ENTERPRISE: &str = "addr1vx2fxv2umyhttkxyxp8x0dlpdt3k6cwng5pxj3jhsydzers66hrl8";
    const POINTER: &str = "addr1gx2fxv2umyhttkxyxp8x0dlpdt3k6cwng5pxj3jhsydzer5pnz75xxcrzqf96k";
    const STAKE: &str = "stake1uyehkck0lajq8gr28t9uxnuvgcqrc6070x3k9r8048z8y5gh6ffgw";
    const BYRON_ICARUS: &str = "Ae2tdPwUPEZFRbyhz3cpfC2CumGzNkFBN2L42rcUc2yjQpEkxDbkPodpMAi";
    const BYRON_DAEDALUS: &str = "DdzFFzCqrht4PWfBGtmrQz4x1GkZHYLVGbK7aaBkjWxujxzz3L5GxCgPiTsks5RjUr3yX9KvwKjNJBt7ZzPCmS3fUQrGeRvo9Y1YBQKQ";

    #[test]
    fn shelley() {
        for a in [BASE, ENTERPRISE, POINTER] {
            assert!(validate(a).is_ok(), "{a}");
        }
        let mut broken = BASE.to_string();
        broken.pop();
        broken.push('y');
        assert!(validate(&broken).is_err());
    }

    #[test]
    fn stake_is_unsupported() {
        assert!(matches!(validate(STAKE), Err(AddressError::Unsupported(_))));
    }

    #[test]
    fn byron() {
        assert!(validate(BYRON_ICARUS).is_ok());
        assert!(validate(BYRON_DAEDALUS).is_ok());
        assert!(validate("Ae2tdPwUPEZFRbyhz3cpfC2CumGzNkFBN2L42rcUc2yjQpEkxDbkPodpMAj").is_err());
    }
}
