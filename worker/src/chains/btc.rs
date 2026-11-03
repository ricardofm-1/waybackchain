//! Bitcoin via Esplora (mempool.space, Blockstream fallback).
//!
//! Balance at T = current confirmed balance minus the net effect of every
//! confirmed transaction with `block_time > T`, walking history newest-first.

use super::{warn, RawHoldings};
use crate::upstream::{JsonSource, UpstreamError};
use futures_util::future::join;
use serde::Deserialize;

pub const DECIMALS: u8 = 8;
pub const SYMBOL: &str = "BTC";
/// Esplora pages confirmed history 25 transactions at a time.
const PAGE_SIZE: usize = 25;
/// Spec: above this many transactions after T, warn instead of returning data.
pub const MAX_TXS_AFTER_T: usize = 2_000;

pub const PROVIDERS: &[&str] = &["https://mempool.space/api", "https://blockstream.info/api"];

#[derive(Deserialize)]
struct AddressInfo {
    chain_stats: Stats,
}

#[derive(Deserialize)]
struct Stats {
    funded_txo_sum: u64,
    spent_txo_sum: u64,
    tx_count: u64,
}

#[derive(Deserialize)]
struct Tx {
    txid: String,
    status: TxStatus,
    vin: Vec<Vin>,
    vout: Vec<Vout>,
}

#[derive(Deserialize)]
struct TxStatus {
    confirmed: bool,
    block_time: Option<i64>,
}

#[derive(Deserialize)]
struct Vin {
    prevout: Option<Vout>,
}

#[derive(Deserialize)]
struct Vout {
    scriptpubkey_address: Option<String>,
    value: u64,
}

#[derive(Deserialize)]
struct BlockAtTime {
    height: u64,
}

/// Net satoshi change this transaction caused for `addr`.
fn net_effect(tx: &Tx, addr: &str) -> i128 {
    let received: u64 = tx
        .vout
        .iter()
        .filter(|o| o.scriptpubkey_address.as_deref() == Some(addr))
        .map(|o| o.value)
        .sum();
    let sent: u64 = tx
        .vin
        .iter()
        .filter_map(|i| i.prevout.as_ref())
        .filter(|o| o.scriptpubkey_address.as_deref() == Some(addr))
        .map(|o| o.value)
        .sum();
    i128::from(received) - i128::from(sent)
}

/// `addr` must be the canonical form (bech32 lowercased).
pub async fn get_holdings<S: JsonSource>(
    src: &S,
    addr: &str,
    cutoff: i64,
    max_txs: usize,
) -> Result<RawHoldings, UpstreamError> {
    // The block at T is nice to have (only mempool.space offers the
    // endpoint); fetch it alongside the balance so a slow answer costs nothing.
    let block_path = format!("/v1/mining/blocks/timestamp/{cutoff}");
    let info_path = format!("/address/{addr}");
    let (block, info) = join(src.get::<BlockAtTime>(&block_path), src.get::<AddressInfo>(&info_path)).await;
    let as_of_block = block.ok().map(|b| b.height);
    let info = info?;
    let stats = info.chain_stats;
    let mut balance = i128::from(stats.funded_txo_sum) - i128::from(stats.spent_txo_sum);
    let mut out = RawHoldings { as_of_block, ..Default::default() };

    let too_many = |mut out: RawHoldings| {
        out.warnings.push(warn::TOO_MANY_TXS);
        Ok(out)
    };

    if stats.tx_count > 0 {
        let mut after_t = 0usize;
        let mut last_txid: Option<String> = None;
        'pages: loop {
            let path = match &last_txid {
                None => format!("/address/{addr}/txs/chain"),
                Some(id) => format!("/address/{addr}/txs/chain/{id}"),
            };
            let page: Vec<Tx> = match src.get(&path).await {
                Ok(p) => p,
                // Ran out of subrequests mid-walk: same outcome as too many txs.
                Err(UpstreamError::Budget) => return too_many(out),
                Err(e) => return Err(e),
            };
            for tx in &page {
                let Some(block_time) = tx.status.block_time.filter(|_| tx.status.confirmed) else {
                    continue;
                };
                if block_time <= cutoff {
                    break 'pages;
                }
                after_t += 1;
                if after_t > max_txs {
                    return too_many(out);
                }
                balance -= net_effect(tx, addr);
            }
            if page.len() < PAGE_SIZE {
                break;
            }
            last_txid = page.last().map(|t| t.txid.clone());
        }
    }

    // A negative result means the provider's data is inconsistent.
    let balance = u128::try_from(balance).map_err(|_| UpstreamError::Decode)?;
    out.native = Some(balance);
    Ok(out)
}

#[cfg(test)]
mod tests {
    //! Fixtures recorded from mempool.space by `test/record_btc.py`.
    //! Expected balances cross-checked by summing history forward.
    use super::*;
    use crate::time::cutoff_unix;
    use crate::upstream::fixtures::Fixtures;
    use futures::executor::block_on;

    const EMPTY: &str = "1JG56Dftg8Ygy4XZR7eZ4e2fZxMCsaHEFd";
    const LIGHT: &str = "14taMumBrvWCVw214fExgvX1FdmiK2FFy6";
    const BUSY: &str = "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa";

    fn run(addr: &str, date: &str, max_txs: usize) -> RawHoldings {
        block_on(get_holdings(&Fixtures::new("btc"), addr, cutoff_unix(date).unwrap(), max_txs)).unwrap()
    }

    #[test]
    fn empty_address_is_zero_at_any_date() {
        for date in ["2015-06-01", "2024-01-01"] {
            let h = run(EMPTY, date, MAX_TXS_AFTER_T);
            assert_eq!(h.native, Some(0), "{date}");
            assert!(h.warnings.is_empty());
        }
    }

    #[test]
    fn light_address_replays_history() {
        // Funded 2024-01-01 23:40:50 with 341,550 sats.
        let h = run(LIGHT, "2024-01-01", MAX_TXS_AFTER_T);
        assert_eq!(h.native, Some(341_550));
        assert_eq!(h.as_of_block, Some(823_940));
        // Before its first transaction.
        assert_eq!(run(LIGHT, "2023-12-31", MAX_TXS_AFTER_T).native, Some(0));
        // After its last transaction: the current balance.
        assert_eq!(run(LIGHT, "2024-01-02", MAX_TXS_AFTER_T).native, Some(9_697));
    }

    #[test]
    fn busy_address_warns_instead_of_partial_data() {
        let h = run(BUSY, "2015-06-01", 30);
        assert_eq!(h.native, None);
        assert_eq!(h.warnings, vec![warn::TOO_MANY_TXS]);
    }

    #[test]
    fn busy_address_recent_date() {
        let h = run(BUSY, super::tests_busy::RECENT_DATE, MAX_TXS_AFTER_T);
        assert_eq!(h.native, Some(super::tests_busy::RECENT_BALANCE));
        assert!(h.warnings.is_empty());
    }
}

#[cfg(test)]
#[path = "../../test/fixtures/btc/expected.rs"]
mod tests_busy;
