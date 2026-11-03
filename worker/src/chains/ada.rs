//! Cardano via Blockfrost: the Bitcoin replay pattern, multi-asset.
//!
//! Holdings at T = current holdings (lovelace + every native asset) minus the
//! net effect of each transaction with `block_time > T`, walking the address's
//! history newest-first. Native tokens travel in the same outputs as ADA, so
//! one `/txs/{hash}/utxos` call per transaction covers every asset.
//!
//! Plutus transactions: a failed script consumes the collateral inputs (and
//! creates the collateral return) instead of the normal inputs and outputs.
//! Only when our address supplied collateral do we pay one extra call to
//! `/txs/{hash}` for `valid_contract`.

use super::{warn, RawHoldings, RawToken};
use crate::upstream::{JsonSource, UpstreamError};
use alloy_primitives::U256;
use futures_util::future::join_all;
use serde::Deserialize;
use std::collections::BTreeMap;

pub const DECIMALS: u8 = 6;
pub const SYMBOL: &str = "ADA";
pub const BASE: &str = "https://cardano-mainnet.blockfrost.io/api/v0";
/// Byron mainnet genesis: 2017-09-23 21:44:51 UTC.
const GENESIS_TIME: i64 = 1_506_203_091;
/// Requests in flight at once (Blockfrost free: 10/s, bursts of 500).
const PARALLEL: usize = 10;

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Transactions per history page (Blockfrost maximum 100).
    pub tx_page: usize,
    /// Spec: above this many transactions after T, warn instead of data.
    pub max_txs: usize,
    /// Native assets whose metadata (ticker, decimals) we look up.
    pub max_assets: usize,
}

impl Limits {
    pub const DEFAULT: Limits = Limits { tx_page: 100, max_txs: 1_000, max_assets: 50 };
}

#[derive(Deserialize)]
struct Quantity {
    unit: String,
    quantity: String,
}

#[derive(Deserialize)]
struct AddressInfo {
    amount: Vec<Quantity>,
}

#[derive(Deserialize)]
struct AddressTx {
    tx_hash: String,
    block_time: i64,
}

#[derive(Deserialize)]
struct Utxos {
    inputs: Vec<Utxo>,
    outputs: Vec<Utxo>,
}

#[derive(Deserialize)]
struct Utxo {
    address: String,
    amount: Vec<Quantity>,
    #[serde(default)]
    collateral: bool,
    #[serde(default)]
    reference: bool,
}

#[derive(Deserialize)]
struct TxInfo {
    valid_contract: bool,
}

#[derive(Deserialize)]
struct AssetInfo {
    #[serde(default)]
    asset_name: Option<String>,
    #[serde(default)]
    metadata: Option<AssetMetadata>,
}

#[derive(Deserialize)]
struct AssetMetadata {
    #[serde(default)]
    ticker: Option<String>,
    #[serde(default)]
    decimals: Option<u8>,
}

type Holdings = BTreeMap<String, i128>;

fn add(map: &mut Holdings, amounts: &[Quantity], sign: i128) -> Result<(), UpstreamError> {
    for a in amounts {
        let q: i128 = a.quantity.parse().map_err(|_| UpstreamError::Decode)?;
        *map.entry(a.unit.clone()).or_default() += sign * q;
    }
    Ok(())
}

/// Net change per asset this transaction caused for `addr`.
async fn net_effect<S: JsonSource>(src: &S, hash: &str, addr: &str) -> Result<Holdings, UpstreamError> {
    let u: Utxos = src.get(&format!("/txs/{hash}/utxos")).await?;
    let ours = |x: &&Utxo| x.address == addr;
    let posted_collateral = u.inputs.iter().filter(ours).any(|i| i.collateral);
    let valid = if posted_collateral {
        src.get::<TxInfo>(&format!("/txs/{hash}")).await?.valid_contract
    } else {
        true
    };
    let mut delta = Holdings::new();
    for o in u.outputs.iter().filter(ours).filter(|o| o.collateral != valid) {
        add(&mut delta, &o.amount, 1)?;
    }
    // Reference inputs are only read, never spent.
    for i in u.inputs.iter().filter(ours).filter(|i| !i.reference && i.collateral != valid) {
        add(&mut delta, &i.amount, -1)?;
    }
    Ok(delta)
}

/// Ticker and decimals from asset metadata, else the decoded asset name.
fn label(unit: &str, info: Option<&AssetInfo>) -> (String, u8) {
    let meta = info.and_then(|i| i.metadata.as_ref());
    let decimals = meta.and_then(|m| m.decimals).unwrap_or(0).min(77);
    let ticker = meta.and_then(|m| m.ticker.clone()).filter(|t| !t.trim().is_empty());
    let name_hex = info
        .and_then(|i| i.asset_name.clone())
        .unwrap_or_else(|| unit.get(56..).unwrap_or_default().to_string());
    let from_name = hex::decode(&name_hex)
        .ok()
        .filter(|b| !b.is_empty() && b.iter().all(|c| c.is_ascii_graphic() || *c == b' '))
        .map(|b| String::from_utf8_lossy(&b).trim().to_string());
    let symbol = ticker
        .or(from_name)
        .unwrap_or_else(|| format!("{}…", &unit[..8.min(unit.len())]));
    (symbol.chars().filter(|c| !c.is_control()).take(20).collect(), decimals)
}

pub async fn get_holdings<S: JsonSource>(src: &S, addr: &str, cutoff: i64, limits: Limits) -> Result<RawHoldings, UpstreamError> {
    let mut out = RawHoldings::default();
    if cutoff < GENESIS_TIME {
        out.native = Some(0);
        return Ok(out);
    }
    // Payment address only: other addresses of the same wallet and
    // unwithdrawn staking rewards aren't included.
    out.warnings.push(warn::ADA_PAYMENT_ADDRESS_ONLY);

    // An address Blockfrost has never seen is a 404: nothing, ever.
    let mut holdings = Holdings::new();
    match src.get::<AddressInfo>(&format!("/addresses/{addr}")).await {
        Ok(info) => add(&mut holdings, &info.amount, 1)?,
        Err(UpstreamError::Status(404)) => {
            out.native = Some(0);
            return Ok(out);
        }
        Err(e) => return Err(e),
    }

    let mut after_t = 0usize;
    'pages: for page in 1.. {
        let path = format!("/addresses/{addr}/transactions?order=desc&count={}&page={page}", limits.tx_page);
        let txs: Vec<AddressTx> = match src.get(&path).await {
            Ok(t) => t,
            Err(UpstreamError::Budget) => return Ok(too_many(out)),
            Err(e) => return Err(e),
        };
        let later: Vec<&AddressTx> = txs.iter().take_while(|t| t.block_time > cutoff).collect();
        after_t += later.len();
        if after_t > limits.max_txs {
            return Ok(too_many(out));
        }
        for chunk in later.chunks(PARALLEL) {
            let deltas = join_all(chunk.iter().map(|t| net_effect(src, &t.tx_hash, addr))).await;
            for d in deltas {
                let d = match d {
                    Ok(d) => d,
                    Err(UpstreamError::Budget) => return Ok(too_many(out)),
                    Err(e) => return Err(e),
                };
                for (unit, q) in d {
                    *holdings.entry(unit).or_default() -= q;
                }
            }
        }
        if later.len() < txs.len() || txs.len() < limits.tx_page {
            break 'pages;
        }
    }

    // A negative result means the provider's data is inconsistent.
    let mut assets = Vec::new();
    for (unit, q) in holdings {
        let q = u128::try_from(q).map_err(|_| UpstreamError::Decode)?;
        if unit == "lovelace" {
            out.native = Some(q);
        } else if q > 0 {
            assets.push((unit, q));
        }
    }
    out.native.get_or_insert(0);

    if assets.len() > limits.max_assets {
        out.warnings.push(warn::TOKEN_CAP_REACHED);
    }
    let lookups: Vec<&str> = assets.iter().take(limits.max_assets).map(|(u, _)| u.as_str()).collect();
    let mut infos: Vec<Option<AssetInfo>> = Vec::new();
    for chunk in lookups.chunks(PARALLEL) {
        // Metadata is cosmetic: on any failure fall back to the asset name.
        let paths: Vec<String> = chunk.iter().map(|u| format!("/assets/{u}")).collect();
        infos.extend(join_all(paths.iter().map(|p| src.get::<AssetInfo>(p))).await.into_iter().map(Result::ok));
    }
    infos.resize_with(assets.len(), || None);
    for ((unit, q), info) in assets.into_iter().zip(infos) {
        let (symbol, decimals) = label(&unit, info.as_ref());
        out.tokens.push(RawToken { contract: unit, symbol, decimals, amount: U256::from(q) });
    }
    Ok(out)
}

fn too_many(mut out: RawHoldings) -> RawHoldings {
    out.native = None;
    out.tokens.clear();
    out.warnings.push(warn::TOO_MANY_TXS);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;
    use serde_json::{json, Value};
    use std::collections::HashMap;

    struct Mock(HashMap<String, Value>);

    impl JsonSource for Mock {
        async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, UpstreamError> {
            let v = self.0.get(path).cloned().ok_or(UpstreamError::Status(404))?;
            serde_json::from_value(v).map_err(|_| UpstreamError::Decode)
        }
    }

    const A: &str = "addr1me";
    const B: &str = "addr1other";

    fn utxo(addr: &str, lovelace: u64, collateral: bool) -> Value {
        json!({ "address": addr, "amount": [{ "unit": "lovelace", "quantity": lovelace.to_string() }], "collateral": collateral, "reference": false })
    }

    /// One failed Plutus tx after T where we posted 5 ADA collateral and got
    /// 3 back: we lose 2 ADA, and the normal inputs/outputs don't count.
    #[test]
    fn failed_script_spends_collateral_only() {
        let mut m = HashMap::new();
        m.insert(format!("/addresses/{A}"), json!({ "amount": [{ "unit": "lovelace", "quantity": "8000000" }] }));
        m.insert(
            format!("/addresses/{A}/transactions?order=desc&count=100&page=1"),
            json!([{ "tx_hash": "t1", "block_time": 2_000_000_000 }, { "tx_hash": "t0", "block_time": 1_600_000_000 }]),
        );
        m.insert(
            "/txs/t1/utxos".into(),
            json!({
                "inputs": [utxo(A, 50_000_000, false), utxo(A, 5_000_000, true)],
                "outputs": [utxo(B, 50_000_000, false), utxo(A, 3_000_000, true)]
            }),
        );
        m.insert("/txs/t1".into(), json!({ "valid_contract": false }));
        let h = block_on(get_holdings(&Mock(m), A, 1_700_000_000, Limits::DEFAULT)).unwrap();
        assert_eq!(h.native, Some(10_000_000)); // 8 now + 2 lost to collateral
    }

    #[test]
    fn unseen_address_is_zero() {
        let h = block_on(get_holdings(&Mock(HashMap::new()), A, 1_700_000_000, Limits::DEFAULT)).unwrap();
        assert_eq!(h.native, Some(0));
    }

    #[test]
    fn labels() {
        let unit = format!("{}{}", "f".repeat(56), hex::encode("HOSKY"));
        assert_eq!(label(&unit, None), ("HOSKY".into(), 0));
        let info = AssetInfo { asset_name: None, metadata: Some(AssetMetadata { ticker: Some("HSK".into()), decimals: Some(6) }) };
        assert_eq!(label(&unit, Some(&info)), ("HSK".into(), 6));
        let binary = format!("{}{}", "a".repeat(56), "00ff");
        assert_eq!(label(&binary, None), ("aaaaaaaa…".into(), 0));
    }
}

#[cfg(test)]
mod recorded {
    //! Real Blockfrost responses under `test/fixtures/ada/`. Re-record with:
    //!   set -a; source .dev.vars; set +a; RECORD=1 cargo test -p wayback-worker ada::recorded
    //! Expected values cross-checked by `test/check_ada.py`, which sums every
    //! transaction forward from the first instead of replaying backwards.
    use super::*;
    use crate::time::cutoff_unix;
    use crate::upstream::fixtures::Recorded;
    use futures::executor::block_on;

    fn run(addr: &str, date: &str, limits: Limits) -> RawHoldings {
        let src = Recorded::with_key_header("ada", "BLOCKFROST_KEY", BASE, "project_id");
        block_on(get_holdings(&src, addr, cutoff_unix(date).unwrap(), limits)).unwrap()
    }

    include!("../../test/fixtures/ada/expected.rs");
}
