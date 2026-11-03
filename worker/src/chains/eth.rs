//! Ethereum, Alchemy only: ETH from an archive `eth_getBalance`, ERC-20
//! tokens discovered from `alchemy_getAssetTransfers` and valued with
//! `balanceOf` at the block at T (batched through Multicall3, or a JSON-RPC
//! batch before it existed).
//!
//! - Block at T: there's no date-to-block method, so `last_at_or_before`
//!   searches `eth_getBlockByNumber` timestamps (~12 s blocks: few probes).
//! - Only *incoming* transfers are needed for discovery: a token can't have a
//!   positive balance without one (mints are transfers from 0x0).
//! - Symbol and decimals come from the transfer records, saving two calls
//!   per token.

use super::{last_at_or_before, warn, RawHoldings, RawToken};
use crate::upstream::{JsonSource, UpstreamError};
use alloy_primitives::{address, Address, U256};
use alloy_sol_types::{sol, SolCall};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::str::FromStr;

pub const DECIMALS: u8 = 18;
pub const SYMBOL: &str = "ETH";
/// Block 1 (the genesis block's own timestamp is 0): 2015-07-30 15:26:28 UTC.
const BLOCK1_TIME: i64 = 1_438_269_988;
pub const MULTICALL3: Address = address!("cA11bde05977b3631167028862bE2a173976CA11");
pub const MULTICALL3_FROM_BLOCK: u64 = 14_353_601;
const MAX_SEARCH_PROBES: usize = 40;
/// `balanceOf` calls per Multicall3 / JSON-RPC batch.
const CHUNK: usize = 150;

/// Token-history limits. Tests shrink them to exercise paging cheaply.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Transfers per `alchemy_getAssetTransfers` page (maximum 1,000).
    pub transfers_page: usize,
    /// Pages before giving up on full token discovery.
    pub max_pages: usize,
    /// Distinct token contracts checked per address.
    pub max_tokens: usize,
}

impl Limits {
    pub const DEFAULT: Limits = Limits { transfers_page: 1_000, max_pages: 3, max_tokens: 300 };
}

sol! {
    function balanceOf(address owner) external view returns (uint256);

    struct Call3 { address target; bool allowFailure; bytes callData; }
    struct Result3 { bool success; bytes returnData; }
    function aggregate3(Call3[] calls) external payable returns (Result3[] returnData);
}

#[derive(Deserialize)]
struct RpcResponse<T> {
    #[serde(default)]
    id: Value,
    result: Option<T>,
    error: Option<Value>,
}

impl<T> RpcResponse<T> {
    fn into_result(self) -> Result<T, UpstreamError> {
        match (self.result, self.error) {
            (Some(r), None) => Ok(r),
            _ => Err(UpstreamError::Status(502)),
        }
    }
}

fn parse_hex_u256(s: &str) -> Result<U256, UpstreamError> {
    U256::from_str_radix(s.trim_start_matches("0x"), 16).map_err(|_| UpstreamError::Decode)
}

fn hex_u64(s: &str) -> Result<u64, UpstreamError> {
    u64::from_str_radix(s.trim_start_matches("0x"), 16).map_err(|_| UpstreamError::Decode)
}

fn hex_bytes(s: &str) -> Result<Vec<u8>, UpstreamError> {
    hex::decode(s.trim_start_matches("0x")).map_err(|_| UpstreamError::Decode)
}

fn rpc(method: &str, params: Value, id: usize) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

#[derive(Deserialize)]
struct BlockHeader {
    number: String,
    timestamp: String,
}

/// (number, unix time) of a block; `which` is a hex number or "latest".
async fn block<R: JsonSource>(node: &R, which: &str) -> Result<(u64, i64), UpstreamError> {
    let r: RpcResponse<BlockHeader> = node.post("", &rpc("eth_getBlockByNumber", json!([which, false]), 1)).await?;
    let b = r.into_result()?;
    Ok((hex_u64(&b.number)?, hex_u64(&b.timestamp)? as i64))
}

async fn block_at<R: JsonSource>(node: &R, cutoff: i64) -> Result<u64, UpstreamError> {
    let latest = block(node, "latest").await?;
    if latest.1 <= cutoff {
        return Ok(latest.0);
    }
    last_at_or_before((1, BLOCK1_TIME), latest, cutoff, MAX_SEARCH_PROBES, |n| async move {
        block(node, &format!("0x{n:x}")).await.map(|(_, t)| t)
    })
    .await
}

#[derive(Deserialize)]
struct Transfers {
    transfers: Vec<Transfer>,
    #[serde(rename = "pageKey")]
    page_key: Option<String>,
}

#[derive(Deserialize)]
struct Transfer {
    asset: Option<String>,
    #[serde(rename = "rawContract")]
    raw_contract: RawContract,
}

#[derive(Deserialize)]
struct RawContract {
    address: Option<String>,
    decimal: Option<String>,
}

struct Discovery {
    tokens: Vec<(Address, String, u8)>,
    complete: bool,
}

/// Every ERC-20 contract that ever sent tokens to `addr` up to `block`.
async fn discover_tokens<R: JsonSource>(node: &R, addr: &str, block: u64, limits: Limits) -> Result<Discovery, UpstreamError> {
    let mut seen = HashSet::new();
    let mut tokens = Vec::new();
    let mut page_key: Option<String> = None;
    for _ in 0..limits.max_pages {
        let mut q = json!({
            "fromBlock": "0x0",
            "toBlock": format!("0x{block:x}"),
            "toAddress": addr,
            "category": ["erc20"],
            "withMetadata": false,
            "excludeZeroValue": false,
            "maxCount": format!("0x{:x}", limits.transfers_page),
        });
        if let Some(k) = &page_key {
            q["pageKey"] = json!(k);
        }
        let r: RpcResponse<Transfers> = match node.post("", &rpc("alchemy_getAssetTransfers", json!([q]), 1)).await {
            Ok(r) => r,
            Err(UpstreamError::Budget) => return Ok(Discovery { tokens, complete: false }),
            Err(e) => return Err(e),
        };
        let page = r.into_result()?;
        for t in &page.transfers {
            let Some(contract) = t.raw_contract.address.as_deref().and_then(|a| Address::from_str(a).ok()) else {
                continue;
            };
            if seen.insert(contract) {
                let symbol: String =
                    t.asset.as_deref().unwrap_or_default().chars().filter(|c| !c.is_control()).take(20).collect();
                let decimals =
                    t.raw_contract.decimal.as_deref().and_then(|d| hex_u64(d).ok()).unwrap_or(0).min(77) as u8;
                tokens.push((contract, symbol, decimals));
            }
        }
        match page.page_key {
            Some(k) => page_key = Some(k),
            None => return Ok(Discovery { tokens, complete: true }),
        }
    }
    Ok(Discovery { tokens, complete: false })
}

fn balance_of_data(owner: Address) -> Vec<u8> {
    balanceOfCall { owner }.abi_encode()
}

/// `balanceOf(owner)` for each contract at `block`; `None` where the call failed.
async fn token_balances<R: JsonSource>(
    node: &R,
    owner: Address,
    contracts: &[Address],
    block: u64,
) -> Result<Vec<Option<U256>>, UpstreamError> {
    let block_hex = format!("0x{block:x}");
    let mut out = Vec::with_capacity(contracts.len());
    for chunk in contracts.chunks(CHUNK) {
        if block >= MULTICALL3_FROM_BLOCK {
            let calls = chunk
                .iter()
                .map(|&target| Call3 { target, allowFailure: true, callData: balance_of_data(owner).into() })
                .collect();
            let data = aggregate3Call { calls }.abi_encode();
            let params = json!([{ "to": MULTICALL3.to_string(), "data": format!("0x{}", hex::encode(data)) }, block_hex]);
            let r: RpcResponse<String> = node.post("", &rpc("eth_call", params, 1)).await?;
            let bytes = hex_bytes(&r.into_result()?)?;
            let results = aggregate3Call::abi_decode_returns(&bytes).map_err(|_| UpstreamError::Decode)?;
            if results.len() != chunk.len() {
                return Err(UpstreamError::Decode);
            }
            out.extend(results.into_iter().map(|r| {
                (r.success && r.returnData.len() >= 32).then(|| U256::from_be_slice(&r.returnData[..32]))
            }));
        } else {
            // Before Multicall3: one JSON-RPC batch of eth_calls.
            let data = format!("0x{}", hex::encode(balance_of_data(owner)));
            let batch: Vec<Value> = chunk
                .iter()
                .enumerate()
                .map(|(i, c)| rpc("eth_call", json!([{ "to": c.to_string(), "data": data }, block_hex]), i))
                .collect();
            let responses: Vec<RpcResponse<String>> = node.post("", &Value::Array(batch)).await?;
            let mut by_id = vec![None; chunk.len()];
            for r in responses {
                let Some(i) = r.id.as_u64().map(|i| i as usize).filter(|&i| i < chunk.len()) else { continue };
                by_id[i] = r
                    .into_result()
                    .ok()
                    .and_then(|h| hex_bytes(&h).ok())
                    .filter(|b| b.len() >= 32)
                    .map(|b| U256::from_be_slice(&b[..32]));
            }
            out.extend(by_id);
        }
    }
    Ok(out)
}

/// `addr` is the canonical lowercase `0x…` form.
pub async fn get_holdings<R: JsonSource>(node: &R, addr: &str, cutoff: i64, limits: Limits) -> Result<RawHoldings, UpstreamError> {
    if cutoff < BLOCK1_TIME {
        return Ok(RawHoldings { native: Some(0), ..Default::default() });
    }
    let owner = Address::from_str(addr).map_err(|_| UpstreamError::Decode)?;
    let block = block_at(node, cutoff).await?;
    let mut out = RawHoldings { as_of_block: Some(block), ..Default::default() };

    let r: RpcResponse<String> = node
        .post("", &rpc("eth_getBalance", json!([addr, format!("0x{block:x}")]), 1))
        .await?;
    let wei = parse_hex_u256(&r.into_result()?)?;
    out.native = Some(u128::try_from(wei).map_err(|_| UpstreamError::Decode)?);

    let mut found = discover_tokens(node, addr, block, limits).await?;
    if !found.complete {
        out.warnings.push(warn::TOKENS_INCOMPLETE);
    }
    if found.tokens.len() > limits.max_tokens {
        found.tokens.truncate(limits.max_tokens);
        out.warnings.push(warn::TOKEN_CAP_REACHED);
    }
    if found.tokens.is_empty() {
        return Ok(out);
    }

    let contracts: Vec<Address> = found.tokens.iter().map(|t| t.0).collect();
    let balances = match token_balances(node, owner, &contracts, block).await {
        Ok(b) => b,
        // Out of budget: return ETH alone, flagged.
        Err(UpstreamError::Budget) => {
            if !out.warnings.contains(&warn::TOKENS_INCOMPLETE) {
                out.warnings.push(warn::TOKENS_INCOMPLETE);
            }
            return Ok(out);
        }
        Err(e) => return Err(e),
    };
    for ((contract, symbol, decimals), balance) in found.tokens.into_iter().zip(balances) {
        if let Some(amount) = balance.filter(|b| !b.is_zero()) {
            out.tokens.push(RawToken { contract: contract.to_string(), symbol, decimals, amount });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;
    use std::cell::RefCell;

    /// In-memory node: a synthetic chain where block n is mined at
    /// BLOCK1_TIME + 12 s * (n - 1), two token contracts, and a fixed
    /// balance per contract. Records every request.
    struct Node {
        latest: u64,
        usdc: u64,
        dead: u64,
        calls: RefCell<Vec<Value>>,
    }

    const OWNER: &str = "0x5aaeb6053f3e94c9b9a09f33669435e7ef1beaed";
    const USDC: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
    const DEAD: &str = "0x000000000000000000000000000000000000dead";

    fn time_of(n: u64) -> i64 {
        BLOCK1_TIME + 12 * (n as i64 - 1)
    }

    fn word(v: u64) -> String {
        format!("0x{v:064x}")
    }

    impl Node {
        fn new(latest: u64, usdc: u64, dead: u64) -> Self {
            Self { latest, usdc, dead, calls: RefCell::new(Vec::new()) }
        }
        fn balance_for(&self, to: &str) -> u64 {
            if to.eq_ignore_ascii_case(USDC) { self.usdc } else { self.dead }
        }
        fn count(&self, method: &str) -> usize {
            self.calls.borrow().iter().filter(|c| c.to_string().contains(method)).count()
        }
    }

    impl JsonSource for Node {
        async fn get<T: serde::de::DeserializeOwned>(&self, _: &str) -> Result<T, UpstreamError> {
            unreachable!()
        }

        async fn post<T: serde::de::DeserializeOwned>(&self, _: &str, body: &Value) -> Result<T, UpstreamError> {
            self.calls.borrow_mut().push(body.clone());
            let v = if let Value::Array(batch) = body {
                // Pre-Multicall3 batch: answer each eth_call by its target.
                Value::Array(
                    batch
                        .iter()
                        .map(|c| {
                            let v = self.balance_for(c["params"][0]["to"].as_str().unwrap());
                            json!({ "jsonrpc": "2.0", "id": c["id"], "result": word(v) })
                        })
                        .collect(),
                )
            } else {
                let p = &body["params"];
                let result = match body["method"].as_str().unwrap() {
                    "eth_getBlockByNumber" => {
                        let n = match p[0].as_str().unwrap() {
                            "latest" => self.latest,
                            h => hex_u64(h).unwrap(),
                        };
                        json!({ "number": format!("0x{n:x}"), "timestamp": format!("0x{:x}", time_of(n)) })
                    }
                    "eth_getBalance" => json!("0xde0b6b3a7640000"), // 1 ETH
                    "alchemy_getAssetTransfers" => json!({ "transfers": [
                        { "asset": "USDC", "rawContract": { "address": USDC, "decimal": "0x6" } },
                        { "asset": "USDC", "rawContract": { "address": USDC, "decimal": "0x6" } },
                        { "asset": "SPAM", "rawContract": { "address": DEAD, "decimal": "0x12" } }
                    ]}),
                    "eth_call" => {
                        let data = hex_bytes(p[0]["data"].as_str().unwrap()).unwrap();
                        let call = aggregate3Call::abi_decode(&data).unwrap();
                        let results: Vec<Result3> = call
                            .calls
                            .iter()
                            .map(|c| Result3 {
                                success: true,
                                returnData: U256::from(self.balance_for(&c.target.to_string())).to_be_bytes::<32>().to_vec().into(),
                            })
                            .collect();
                        json!(format!("0x{}", hex::encode(aggregate3Call::abi_encode_returns(&results))))
                    }
                    m => panic!("unexpected {m}"),
                };
                json!({ "jsonrpc": "2.0", "id": 1, "result": result })
            };
            Ok(serde_json::from_value(v).unwrap())
        }
    }

    #[test]
    fn multicall_path_drops_zero_balances() {
        let n = Node::new(21_000_000, 1_500_000, 0);
        let target = 20_000_000;
        let h = block_on(get_holdings(&n, OWNER, time_of(target) + 5, Limits::DEFAULT)).unwrap();
        assert_eq!(h.as_of_block, Some(target));
        assert_eq!(h.native, Some(1_000_000_000_000_000_000));
        assert_eq!(h.tokens.len(), 1);
        assert_eq!((h.tokens[0].symbol.as_str(), h.tokens[0].decimals), ("USDC", 6));
        assert_eq!(h.tokens[0].amount, U256::from(1_500_000u64));
        assert!(h.warnings.is_empty());
        assert_eq!(n.count("\"eth_call\""), 1, "one multicall for both tokens");
        assert!(n.count("eth_getBlockByNumber") <= 10, "{} probes", n.count("eth_getBlockByNumber"));
    }

    #[test]
    fn batch_path_before_multicall3() {
        let n = Node::new(21_000_000, 7, 9);
        let h = block_on(get_holdings(&n, OWNER, time_of(10_000_000), Limits::DEFAULT)).unwrap();
        assert_eq!(h.as_of_block, Some(10_000_000));
        assert_eq!(h.tokens.len(), 2);
        let mc = MULTICALL3.to_string().to_ascii_lowercase();
        assert!(n.calls.borrow().iter().all(|c| !c.to_string().to_ascii_lowercase().contains(&mc)));
    }

    #[test]
    fn today_uses_latest_block() {
        let n = Node::new(21_000_000, 0, 0);
        let h = block_on(get_holdings(&n, OWNER, time_of(21_000_000) + 3600, Limits::DEFAULT)).unwrap();
        assert_eq!(h.as_of_block, Some(21_000_000));
        assert_eq!(n.count("eth_getBlockByNumber"), 1);
    }

    #[test]
    fn before_first_block_is_zero_without_calls() {
        let n = Node::new(21_000_000, 0, 0);
        let h = block_on(get_holdings(&n, OWNER, BLOCK1_TIME - 1, Limits::DEFAULT)).unwrap();
        assert_eq!(h.native, Some(0));
        assert!(n.calls.borrow().is_empty());
    }
}

#[cfg(test)]
mod recorded {
    //! Real Alchemy responses under `test/fixtures/eth/`. Re-record with:
    //!   set -a; source .dev.vars; set +a; RECORD=1 cargo test -p wayback-worker eth::recorded
    //! Token balances cross-checked by `test/check_eth.py`, which sums each
    //! token's transfers independently of `balanceOf`.
    use super::*;
    use crate::time::cutoff_unix;
    use crate::upstream::fixtures::Recorded;
    use futures::executor::block_on;

    fn run(addr: &str, date: &str, limits: Limits) -> RawHoldings {
        let node = Recorded::new("eth", "ALCHEMY_KEY", |k| (format!("https://eth-mainnet.g.alchemy.com/v2/{k}"), String::new()));
        block_on(get_holdings(&node, addr, cutoff_unix(date).unwrap(), limits)).unwrap()
    }

    fn token<'a>(h: &'a RawHoldings, symbol: &str) -> Option<&'a RawToken> {
        h.tokens.iter().find(|t| t.symbol == symbol)
    }

    /// sha256("wayback-wallet empty eth fixture")[..20]: never used on chain.
    const EMPTY: &str = "0x0136c1c53c6b9c55bce7e331a209c24912379d2f";

    #[test]
    fn empty_address() {
        for date in ["2019-06-01", "2024-01-01"] {
            let h = run(EMPTY, date, Limits::DEFAULT);
            assert_eq!(h.native, Some(0), "{date}");
            assert!(h.tokens.is_empty() && h.warnings.is_empty(), "{date}");
        }
    }

    include!("../../test/fixtures/eth/expected.rs");
}
