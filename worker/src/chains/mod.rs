//! One module per chain behind a common result shape:
//! `get_holdings(source, address, T) -> RawHoldings`.
//! Amounts stay as integer base units here; `lib.rs` formats them.

pub mod ada;
pub mod btc;
pub mod eth;
pub mod sol;
pub mod xrp;

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
    /// XRP Ledger: the date is before recorded history (January 2013).
    pub const NO_HISTORY: &str = "no_history";
    /// XRP balance includes the reserve the account can't spend.
    pub const XRP_INCLUDES_RESERVE: &str = "xrp_includes_reserve";
    /// Very large balance: probably an exchange address shared via tags.
    pub const LIKELY_EXCHANGE: &str = "likely_exchange";
    /// Cardano: one payment address only; the wallet's other addresses and
    /// unwithdrawn staking rewards aren't included.
    pub const ADA_PAYMENT_ADDRESS_ONLY: &str = "ada_payment_address_only";
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

/// Finds the last block (or ledger) whose time is at or before `cutoff`.
///
/// `lo` must be at or before the cutoff and `hi` after it, both as
/// (index, unix time). Chains with steady block times converge in a handful
/// of probes by interpolation; every third probe bisects to bound the worst
/// case. Returns `Budget` if `max_probes` runs out.
pub async fn last_at_or_before<F, Fut>(
    mut lo: (u64, i64),
    mut hi: (u64, i64),
    cutoff: i64,
    max_probes: usize,
    probe: F,
) -> Result<u64, crate::upstream::UpstreamError>
where
    F: Fn(u64) -> Fut,
    Fut: std::future::Future<Output = Result<i64, crate::upstream::UpstreamError>>,
{
    for step in 0..max_probes {
        if hi.0 - lo.0 <= 1 {
            return Ok(lo.0);
        }
        let guess = if step % 3 == 2 || hi.1 <= lo.1 {
            lo.0 + (hi.0 - lo.0) / 2
        } else {
            let frac = (cutoff - lo.1) as f64 / (hi.1 - lo.1) as f64;
            lo.0 + ((hi.0 - lo.0) as f64 * frac) as u64
        }
        .clamp(lo.0 + 1, hi.0 - 1);
        let t = probe(guess).await?;
        if t <= cutoff {
            lo = (guess, t);
        } else {
            hi = (guess, t);
        }
    }
    if hi.0 - lo.0 <= 1 {
        Ok(lo.0)
    } else {
        Err(crate::upstream::UpstreamError::Budget)
    }
}
