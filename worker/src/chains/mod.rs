//! One module per chain behind a common result shape:
//! `get_holdings(source, address, T) -> RawHoldings`.
//! Amounts stay as integer base units here; `lib.rs` formats them.

pub mod btc;

/// Warning codes returned to the frontend.
pub mod warn {
    /// Too many transactions since the date to replay within our limits.
    pub const TOO_MANY_TXS: &str = "too_many_txs";
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawToken {
    pub contract: String,
    pub symbol: String,
    pub decimals: u8,
    pub amount: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RawHoldings {
    /// Base units; `None` when the balance couldn't be determined.
    pub native: Option<u128>,
    pub tokens: Vec<RawToken>,
    pub as_of_block: Option<u64>,
    pub warnings: Vec<&'static str>,
}
