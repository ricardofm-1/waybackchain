//! Solana via Helius RPC (public RPC as fallback for standard methods).
//!
//! For the wallet and each of its token accounts: find the newest signature
//! with `blockTime <= T`, load that transaction, and read the account's
//! post-transaction balance (`postBalances` for SOL, `postTokenBalances` for
//! SPL). Every step is a JSON-RPC batch, so 50 token accounts cost a handful
//! of subrequests rather than hundreds.
//!
//! Token accounts come from Helius DAS `getTokenAccounts` (paged), never
//! `getTokenAccountsByOwner`: for some owners that returns millions of
//! accounts in one response.

use super::{warn, RawHoldings, RawToken};
use crate::upstream::{JsonSource, UpstreamError};
use alloy_primitives::U256;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;

pub const DECIMALS: u8 = 9;
pub const SYMBOL: &str = "SOL";
/// Mainnet-beta genesis: 2020-03-16 14:29:00 UTC.
const GENESIS_TIME: i64 = 1_584_368_940;

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Signatures per `getSignaturesForAddress` page (RPC maximum 1,000).
    pub sig_page: usize,
    /// Paging rounds before giving up on reaching T.
    pub max_rounds: usize,
    /// Token accounts checked per wallet (spec: 50).
    pub max_token_accounts: usize,
}

impl Limits {
    pub const DEFAULT: Limits = Limits { sig_page: 1_000, max_rounds: 3, max_token_accounts: 50 };
}

/// Accounts per `getSignaturesForAddress` batch, and transactions per
/// `getTransaction` batch. Helius counts every batch item against its rate
/// limit (10 req/s on the free plan), so keep these at 10.
const SIG_BATCH: usize = 10;
const TX_BATCH: usize = 10;

#[derive(Deserialize)]
struct RpcItem {
    #[serde(default)]
    id: Value,
    result: Option<Value>,
    error: Option<Value>,
}

fn rpc(method: &str, params: Value, id: usize) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

/// Sends a batch and returns results in request order (`None` per failed item).
async fn batch<S: JsonSource>(src: &S, calls: Vec<Value>) -> Result<Vec<Option<Value>>, UpstreamError> {
    let n = calls.len();
    let items: Vec<RpcItem> = src.post("", &Value::Array(calls)).await?;
    let mut out = vec![None; n];
    for item in items {
        let Some(i) = item.id.as_u64().map(|i| i as usize).filter(|&i| i < n) else { continue };
        if item.error.is_none() {
            out[i] = item.result;
        }
    }
    Ok(out)
}

#[derive(Deserialize)]
struct SigInfo {
    signature: String,
    #[serde(rename = "blockTime")]
    block_time: Option<i64>,
}

#[derive(Clone, Debug, PartialEq)]
enum Search {
    /// Keep paging from this signature (None = start at newest).
    Pending(Option<String>),
    /// Newest signature at or before T.
    Found(String),
    /// No activity at or before T: balance 0.
    NoneBefore,
}

/// For each account, the newest signature with blockTime <= cutoff.
async fn last_signatures<S: JsonSource>(
    src: &S,
    accounts: &[String],
    cutoff: i64,
    limits: Limits,
) -> Result<Vec<Search>, UpstreamError> {
    let mut state = vec![Search::Pending(None); accounts.len()];
    for _ in 0..limits.max_rounds {
        let pending: Vec<usize> = (0..accounts.len()).filter(|&i| matches!(state[i], Search::Pending(_))).collect();
        if pending.is_empty() {
            break;
        }
        for chunk in pending.chunks(SIG_BATCH) {
            let calls = chunk
                .iter()
                .enumerate()
                .map(|(j, &i)| {
                    let mut opts = json!({ "limit": limits.sig_page, "commitment": "finalized" });
                    if let Search::Pending(Some(before)) = &state[i] {
                        opts["before"] = json!(before);
                    }
                    rpc("getSignaturesForAddress", json!([accounts[i], opts]), j)
                })
                .collect();
            let results = match batch(src, calls).await {
                Ok(r) => r,
                Err(UpstreamError::Budget) => return Ok(state),
                Err(e) => return Err(e),
            };
            for (&i, result) in chunk.iter().zip(results) {
                let Some(result) = result else { return Err(UpstreamError::Status(502)) };
                let sigs: Vec<SigInfo> = serde_json::from_value(result).map_err(|_| UpstreamError::Decode)?;
                state[i] = match sigs.iter().find(|s| s.block_time.is_some_and(|t| t <= cutoff)) {
                    Some(s) => Search::Found(s.signature.clone()),
                    None if sigs.len() < limits.sig_page => Search::NoneBefore,
                    None => Search::Pending(sigs.last().map(|s| s.signature.clone())),
                };
            }
        }
    }
    Ok(state)
}

#[derive(Deserialize)]
struct Tx {
    transaction: TxBody,
    meta: TxMeta,
}

#[derive(Deserialize)]
struct TxBody {
    message: Message,
}

#[derive(Deserialize)]
struct Message {
    #[serde(rename = "accountKeys")]
    account_keys: Vec<String>,
}

#[derive(Deserialize)]
struct TxMeta {
    #[serde(rename = "postBalances")]
    post_balances: Vec<u64>,
    #[serde(rename = "postTokenBalances", default)]
    post_token_balances: Option<Vec<TokenBalance>>,
    #[serde(rename = "loadedAddresses", default)]
    loaded_addresses: Option<Loaded>,
}

#[derive(Deserialize, Default)]
struct Loaded {
    #[serde(default)]
    writable: Vec<String>,
    #[serde(default)]
    readonly: Vec<String>,
}

#[derive(Deserialize)]
struct TokenBalance {
    #[serde(rename = "accountIndex")]
    account_index: usize,
    mint: String,
    #[serde(rename = "uiTokenAmount")]
    ui_token_amount: UiAmount,
}

#[derive(Deserialize)]
struct UiAmount {
    amount: String,
    decimals: u8,
}

impl Tx {
    /// Static keys, then v0 lookup-table writable, then readonly.
    fn index_of(&self, account: &str) -> Option<usize> {
        let loaded = self.meta.loaded_addresses.as_ref();
        self.transaction
            .message
            .account_keys
            .iter()
            .chain(loaded.map(|l| l.writable.iter()).into_iter().flatten())
            .chain(loaded.map(|l| l.readonly.iter()).into_iter().flatten())
            .position(|k| k == account)
    }
}

async fn transactions<S: JsonSource>(src: &S, sigs: &[String]) -> Result<HashMap<String, Tx>, UpstreamError> {
    let mut out = HashMap::new();
    for chunk in sigs.chunks(TX_BATCH) {
        let calls = chunk
            .iter()
            .enumerate()
            .map(|(j, s)| {
                rpc(
                    "getTransaction",
                    json!([s, { "encoding": "json", "maxSupportedTransactionVersion": 0, "commitment": "finalized" }]),
                    j,
                )
            })
            .collect();
        for (sig, result) in chunk.iter().zip(batch(src, calls).await?) {
            let tx: Tx = result
                .and_then(|v| serde_json::from_value(v).ok())
                .ok_or(UpstreamError::Decode)?;
            out.insert(sig.clone(), tx);
        }
    }
    Ok(out)
}

#[derive(Deserialize)]
struct DasTokenAccounts {
    token_accounts: Vec<DasTokenAccount>,
}

#[derive(Deserialize)]
struct DasTokenAccount {
    address: String,
}

/// Token symbols for mints via Helius DAS; missing ones are left out.
async fn symbols<S: JsonSource>(helius: &S, mints: &[String]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    if mints.is_empty() {
        return out;
    }
    let body = rpc("getAssetBatch", json!({ "ids": mints }), 1);
    let Ok(resp) = helius.post::<RpcItem>("", &body).await else { return out };
    let Some(Value::Array(assets)) = resp.result else { return out };
    for a in assets {
        let id = a["id"].as_str().unwrap_or_default().to_string();
        let sym = a["token_info"]["symbol"]
            .as_str()
            .filter(|s| !s.is_empty())
            .or_else(|| a["content"]["metadata"]["symbol"].as_str())
            .unwrap_or_default();
        let sym: String = sym.chars().filter(|c| !c.is_control()).take(20).collect();
        if !id.is_empty() && !sym.trim().is_empty() {
            out.insert(id, sym.trim().to_string());
        }
    }
    out
}

fn short_mint(mint: &str) -> String {
    format!("{}…{}", &mint[..4.min(mint.len())], &mint[mint.len().saturating_sub(4)..])
}

/// `helius` serves the Helius-only DAS methods; `node` the standard RPC
/// (Helius with public-RPC fallback in production).
pub async fn get_holdings<H: JsonSource, N: JsonSource>(
    helius: &H,
    node: &N,
    addr: &str,
    cutoff: i64,
    limits: Limits,
) -> Result<RawHoldings, UpstreamError> {
    let mut out = RawHoldings::default();
    if cutoff < GENESIS_TIME {
        out.native = Some(0);
        return Ok(out);
    }
    // Spec: token accounts closed before today can't be found.
    out.warnings.push(warn::SOL_CLOSED_ACCOUNTS);

    // Token accounts that exist today (including empty ones: they may have
    // held tokens on the date). One more than the cap tells us it was hit.
    let das = rpc(
        "getTokenAccounts",
        json!({ "owner": addr, "limit": limits.max_token_accounts + 1, "options": { "showZeroBalance": true } }),
        1,
    );
    let mut token_accounts: Vec<String> = match helius.post::<RpcItem>("", &das).await {
        Ok(RpcItem { result: Some(r), .. }) => serde_json::from_value::<DasTokenAccounts>(r)
            .map_err(|_| UpstreamError::Decode)?
            .token_accounts
            .into_iter()
            .map(|t| t.address)
            .collect(),
        Ok(_) | Err(UpstreamError::Budget) => {
            out.warnings.push(warn::TOKENS_INCOMPLETE);
            Vec::new()
        }
        Err(e) if e.is_our_side() => {
            out.warnings.push(warn::TOKENS_INCOMPLETE);
            Vec::new()
        }
        Err(e) => return Err(e),
    };
    if token_accounts.len() > limits.max_token_accounts {
        token_accounts.truncate(limits.max_token_accounts);
        out.warnings.push(warn::TOKEN_CAP_REACHED);
    }

    let mut accounts = vec![addr.to_string()];
    accounts.extend(token_accounts);
    let search = last_signatures(node, &accounts, cutoff, limits).await?;

    let mut sigs: Vec<String> = search
        .iter()
        .filter_map(|s| match s {
            Search::Found(sig) => Some(sig.clone()),
            _ => None,
        })
        .collect();
    sigs.sort();
    sigs.dedup(); // one transaction often touches several of these accounts
    let txs = match transactions(node, &sigs).await {
        Ok(t) => t,
        Err(UpstreamError::Budget) => return Ok(too_many(out)),
        Err(e) => return Err(e),
    };

    // Wallet: SOL balance.
    out.native = match &search[0] {
        Search::NoneBefore => Some(0),
        Search::Found(sig) => {
            let tx = &txs[sig];
            let i = tx.index_of(addr).ok_or(UpstreamError::Decode)?;
            Some(u128::from(*tx.meta.post_balances.get(i).ok_or(UpstreamError::Decode)?))
        }
        Search::Pending(_) => return Ok(too_many(out)),
    };

    // Token accounts: SPL balances.
    let mut raw: Vec<(String, u8, U256)> = Vec::new();
    for (account, state) in accounts.iter().zip(&search).skip(1) {
        match state {
            Search::NoneBefore => {}
            Search::Pending(_) => {
                if !out.warnings.contains(&warn::TOKENS_INCOMPLETE) {
                    out.warnings.push(warn::TOKENS_INCOMPLETE);
                }
            }
            Search::Found(sig) => {
                let tx = &txs[sig];
                let Some(i) = tx.index_of(account) else { continue };
                // No entry after the transaction: the account was closed (0).
                let Some(b) = tx.meta.post_token_balances.iter().flatten().find(|b| b.account_index == i) else {
                    continue;
                };
                let amount = U256::from_str_radix(&b.ui_token_amount.amount, 10).map_err(|_| UpstreamError::Decode)?;
                if !amount.is_zero() {
                    raw.push((b.mint.clone(), b.ui_token_amount.decimals, amount));
                }
            }
        }
    }

    // Two token accounts can hold the same mint: add them up.
    let mut by_mint: Vec<(String, u8, U256)> = Vec::new();
    for (mint, decimals, amount) in raw {
        match by_mint.iter_mut().find(|(m, _, _)| *m == mint) {
            Some(entry) => entry.2 += amount,
            None => by_mint.push((mint, decimals, amount)),
        }
    }
    let mints: Vec<String> = by_mint.iter().map(|t| t.0.clone()).collect();
    let names = symbols(helius, &mints).await;
    out.tokens = by_mint
        .into_iter()
        .map(|(mint, decimals, amount)| RawToken {
            symbol: names.get(&mint).cloned().unwrap_or_else(|| short_mint(&mint)),
            contract: mint,
            decimals,
            amount,
        })
        .collect();
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

    #[test]
    fn index_includes_lookup_table_addresses() {
        let tx: Tx = serde_json::from_value(json!({
            "transaction": { "message": { "accountKeys": ["A", "B"] } },
            "meta": { "postBalances": [1, 2, 3, 4], "loadedAddresses": { "writable": ["C"], "readonly": ["D"] } }
        }))
        .unwrap();
        assert_eq!(tx.index_of("A"), Some(0));
        assert_eq!(tx.index_of("C"), Some(2));
        assert_eq!(tx.index_of("D"), Some(3));
        assert_eq!(tx.index_of("E"), None);
    }

    #[test]
    fn short_mint_label() {
        assert_eq!(short_mint("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"), "EPjF…Dt1v");
    }
}

#[cfg(test)]
mod recorded {
    //! Real Helius responses under `test/fixtures/sol/`. Re-record with:
    //!   set -a; source .dev.vars; set +a; RECORD=1 cargo test -p wayback-worker sol::recorded
    //! Expected values cross-checked by `test/check_sol.py`, which sums every
    //! balance change from the account's first transaction instead.
    use super::*;
    use crate::time::cutoff_unix;
    use crate::upstream::fixtures::Recorded;
    use futures::executor::block_on;

    fn run(addr: &str, date: &str, limits: Limits) -> RawHoldings {
        let src = || Recorded::new("sol", "HELIUS_KEY", |k| (format!("https://mainnet.helius-rpc.com/?api-key={k}"), String::new()));
        block_on(get_holdings(&src(), &src(), addr, cutoff_unix(date).unwrap(), limits)).unwrap()
    }

    include!("../../test/fixtures/sol/expected.rs");
}
