//! One module per chain behind a common result shape:
//! `get_holdings(source, address, T) -> RawHoldings`.
//! Amounts stay as integer base units here; `lib.rs` formats them.

pub mod btc;
pub mod eth;
pub mod sol;

/// Warning codes returned to the frontend.
pub mod warn {
    /// Too many transactions since the date to replay within our limits.
    pub const TOO_MANY_TXS: &str = "too_many_txs";
    /// Token discovery stopped early; the token list may be missing some.
    pub const TOKENS_INCOMPLETE: &str = "tokens_incomplete";
    /// More token contracts than we check; the rest were skipped.
    pub const TOKEN_CAP_REACHED: &str = "token_cap_reached";
    /// Solana: token accounts closed before today can't be found.
    pub const SOL_CLOSED_ACCOUNTS: &str = "sol_closed_accounts";
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawToken {
    pub contract: String,
    pub symbol: String,
    pub decimals: u8,
    /// Base units; 256-bit because ERC-20 balances can exceed u128.
    pub amount: alloy_primitives::U256,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RawHoldings {
    /// Base units; `None` when the balance couldn't be determined.
    pub native: Option<u128>,
    pub tokens: Vec<RawToken>,
    pub as_of_block: Option<u64>,
    pub warnings: Vec<&'static str>,
}
