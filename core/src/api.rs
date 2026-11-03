//! Request/response bodies for the Worker API (spec: "Worker API contract").
//! Amounts are decimal strings, never floats.

use crate::Chain;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Quota {
    pub remaining: u32,
    /// RFC 3339 timestamp of the next 00:00 UTC.
    pub reset_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddressRef {
    pub address: String,
    pub chain: Chain,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LookupStartRequest {
    pub turnstile_token: String,
    /// `YYYY-MM-DD`; the cutoff is 23:59:59 UTC on this date.
    pub date: String,
    pub addresses: Vec<AddressRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LookupStartResponse {
    pub lookup_token: String,
    pub remaining: u32,
    pub reset_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LookupAddressRequest {
    pub lookup_token: String,
    pub address: String,
    /// Add indicative USD values (default on).
    #[serde(default = "yes")]
    pub with_usd: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Amount {
    pub symbol: String,
    pub amount: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usd: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenHolding {
    /// Contract / mint / issuer+currency / policy+asset, depending on chain.
    pub contract: String,
    pub symbol: String,
    pub amount: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usd: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Holdings {
    pub chain: Chain,
    pub address: String,
    /// Last block (or ledger / slot) at or before the cutoff, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub as_of_block: Option<String>,
    /// The cutoff: `YYYY-MM-DDT23:59:59Z`.
    pub as_of_time: String,
    /// `None` when the balance couldn't be determined (see `warnings`).
    pub native: Option<Amount>,
    pub tokens: Vec<TokenHolding>,
    /// Machine codes, e.g. `too_many_txs`. The frontend turns them into text.
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    /// Machine code, e.g. `captcha_failed`, `daily_limit`, `bad_token`.
    pub error: String,
    #[serde(default, rename = "resetAt", skip_serializing_if = "Option::is_none")]
    pub reset_at: Option<String>,
}
