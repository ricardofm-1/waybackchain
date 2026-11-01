//! Shared code for the Wayback Wallet frontend and Worker.
//!
//! Address validation and chain detection live here so the browser and the
//! Worker run exactly the same checks.

pub mod address_list;
pub mod api;
pub mod chains;

use serde::{Deserialize, Serialize};
use std::fmt;

/// Maximum number of addresses in one lookup.
pub const MAX_ADDRESSES: usize = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Chain {
    Btc,
    Eth,
    Sol,
    Xrp,
    Ada,
}

impl Chain {
    pub const ALL: [Chain; 5] = [Chain::Btc, Chain::Eth, Chain::Sol, Chain::Xrp, Chain::Ada];

    /// Short ticker shown in the chain badge.
    pub fn ticker(self) -> &'static str {
        match self {
            Chain::Btc => "BTC",
            Chain::Eth => "ETH",
            Chain::Sol => "SOL",
            Chain::Xrp => "XRP",
            Chain::Ada => "ADA",
        }
    }

    fn validate(self, s: &str) -> Result<String, AddressError> {
        match self {
            Chain::Btc => chains::btc::validate(s),
            Chain::Eth => chains::eth::validate(s),
            Chain::Sol => chains::sol::validate(s),
            Chain::Xrp => chains::xrp::validate(s),
            Chain::Ada => chains::ada::validate(s),
        }
    }
}

impl fmt::Display for Chain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.ticker())
    }
}

/// A validated address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detected {
    pub chain: Chain,
    /// The address exactly as the user typed it (trimmed).
    pub input: String,
    /// Canonical form used for de-duplication and API calls: ETH lowercased,
    /// XRP X-addresses converted to the classic address, others unchanged.
    pub canonical: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddressError {
    Empty,
    /// Recognised format that v1 deliberately does not support.
    Unsupported(&'static str),
    /// Looks like an address of this chain but fails a check (checksum,
    /// length, network). Used to give a more useful message.
    Malformed { chain: Chain, reason: &'static str },
    /// Passed the validators of more than one chain.
    Ambiguous,
    Unrecognised,
}

impl fmt::Display for AddressError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AddressError::Empty => f.write_str("empty line"),
            AddressError::Unsupported(msg) => f.write_str(msg),
            AddressError::Malformed { chain, reason } => write!(f, "invalid {chain} address: {reason}"),
            AddressError::Ambiguous => f.write_str("matches more than one chain"),
            AddressError::Unrecognised => f.write_str("not a BTC, ETH, SOL, XRP or ADA address"),
        }
    }
}

impl std::error::Error for AddressError {}

/// Runs every chain validator; exactly one must pass.
pub fn detect(input: &str) -> Result<Detected, AddressError> {
    let s = input.trim();
    if s.is_empty() {
        return Err(AddressError::Empty);
    }

    let mut hit: Option<(Chain, String)> = None;
    // The most specific failure, reported when nothing passes.
    let mut best_err = AddressError::Unrecognised;

    for chain in Chain::ALL {
        match chain.validate(s) {
            Ok(canonical) => {
                if hit.is_some() {
                    return Err(AddressError::Ambiguous);
                }
                hit = Some((chain, canonical));
            }
            Err(e @ AddressError::Unsupported(_)) => best_err = e,
            Err(e @ AddressError::Malformed { .. }) => {
                if !matches!(best_err, AddressError::Unsupported(_)) {
                    best_err = e;
                }
            }
            Err(_) => {}
        }
    }

    match hit {
        Some((chain, canonical)) => Ok(Detected { chain, input: s.to_string(), canonical }),
        None => Err(best_err),
    }
}

/// Re-validates an address against the chain the client claimed.
/// The Worker uses this; it never trusts the client's detection.
pub fn verify(chain: Chain, input: &str) -> Result<Detected, AddressError> {
    let d = detect(input)?;
    if d.chain == chain {
        Ok(d)
    } else {
        Err(AddressError::Malformed { chain, reason: "address belongs to a different chain" })
    }
}
