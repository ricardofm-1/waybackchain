//! One validator per chain. Each returns the canonical address string, or
//! `AddressError::Unrecognised` when the input doesn't look like that chain at
//! all, or `Malformed` when it looks like it but fails a check.

pub mod ada;
pub mod btc;
pub mod eth;
pub mod sol;
pub mod xrp;
