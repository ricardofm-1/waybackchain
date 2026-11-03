//! Ethereum: ETH from an archive `eth_getBalance`, ERC-20 tokens discovered
//! from Etherscan's `tokentx` history and valued with `balanceOf` at the block
//! at T (batched through Multicall3, or a JSON-RPC batch before it existed).
//!
//! Token symbol and decimals come from the `tokentx` rows themselves, which
//! saves two calls per token.

use super::{warn, RawHoldings, RawToken};
use crate::upstream::{JsonSource, UpstreamError};
use alloy_primitives::{address, Address, U256};
use alloy_sol_types::{sol, SolCall};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::str::FromStr;

pub const DECIMALS: u8 = 18;
pub const SYMBOL: &str = "ETH";
/// Unix time of the genesis block (2015-07-30 15:26:13 UTC).
const GENESIS_TIME: i64 = 1_438_269_973;
pub const MULTICALL3: Address = address!("cA11bde05977b3631167028862bE2a173976CA11");
pub const MULTICALL3_FROM_BLOCK: u64 = 14_353_601;

/// Token-history limits. Tests shrink them to exercise paging cheaply.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Rows per `tokentx` query (Etherscan's maximum is 10,000).
    pub tokentx_page: usize,
    /// Block-range splits before giving up on full token discovery.
    pub max_pages: usize,
    /// Distinct token contracts checked per address.
    pub max_tokens: usize,
}

impl Limits {
    pub const DEFAULT: Limits = Limits { tokentx_page: 10_000, max_pages: 3, max_tokens: 300 };
}
/// `balanceOf` calls per Multicall3 / JSON-RPC batch.
const CHUNK: usize = 150;

sol! {
    function balanceOf(address owner) external view returns (uint256);

    struct Call3 { address target; bool allowFailure; bytes callData; }
    struct Result3 { bool success; bytes returnData; }
    function aggregate3(Call3[] calls) external payable returns (Result3[] returnData);
}

#[derive(Deserialize)]
struct Envelope {
    status: String,
    message: String,
    result: Value,
}

/// Unwraps Etherscan's `{status, message, result}`; "no results" is an
/// empty list, rate limits map to 429, anything else (bad key…) to 502.
fn etherscan_result(env: Envelope) -> Result<Value, UpstreamError> {
    if env.status == "1" {
        return Ok(env.result);
    }
    if env.message.starts_with("No transactions found") || env.message.starts_with("No records found") {
        return Ok(Value::Array(Vec::new()));
    }
    let detail = env.result.as_str().unwrap_or_default().to_ascii_lowercase();
    if detail.contains("rate limit") {
        Err(UpstreamError::Status(429))
    } else {
        Err(UpstreamError::Status(502))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TokenTx {
    block_number: String,
    contract_address: String,
    #[serde(default)]
    token_symbol: String,
    #[serde(default)]
    token_decimal: String,
}

#[derive(Deserialize)]
struct RpcResponse {
    #[serde(default)]
    id: Value,
    result: Option<String>,
    error: Option<Value>,
}

impl RpcResponse {
    fn into_result(self) -> Result<String, UpstreamError> {
        match (self.result, self.error) {
            (Some(r), None) => Ok(r),
            _ => Err(UpstreamError::Status(502)),
        }
    }
}

fn parse_hex_u256(s: &str) -> Result<U256, UpstreamError> {
    U256::from_str_radix(s.trim_start_matches("0x"), 16).map_err(|_| UpstreamError::Decode)
}

fn hex_bytes(s: &str) -> Result<Vec<u8>, UpstreamError> {
    hex::decode(s.trim_start_matches("0x")).map_err(|_| UpstreamError::Decode)
}

fn rpc(method: &str, params: Value, id: usize) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

async fn block_at<E: JsonSource, R: JsonSource>(es: &E, node: &R, cutoff: i64, now: i64) -> Result<u64, UpstreamError> {
    if cutoff >= now {
        let r: RpcResponse = node.post("", &rpc("eth_blockNumber", json!([]), 1)).await?;
        return u64::try_from(parse_hex_u256(&r.into_result()?)?).map_err(|_| UpstreamError::Decode);
    }
    let env: Envelope = es
        .get(&format!("module=block&action=getblocknobytime&timestamp={cutoff}&closest=before"))
        .await?;
    etherscan_result(env)?
        .as_str()
        .and_then(|s| s.parse().ok())
        .ok_or(UpstreamError::Decode)
}

struct Discovery {
    tokens: Vec<(Address, String, u8)>,
    complete: bool,
}

/// Every token contract that ever moved in or out of `addr` up to `block`.
async fn discover_tokens<E: JsonSource>(es: &E, addr: &str, block: u64, limits: Limits) -> Result<Discovery, UpstreamError> {
    let mut seen = HashSet::new();
    let mut tokens = Vec::new();
    let mut start = 0u64;
    for _ in 0..limits.max_pages {
        let path = format!(
            "module=account&action=tokentx&address={addr}&startblock={start}&endblock={block}&page=1&offset={}&sort=asc",
            limits.tokentx_page
        );
        let env: Envelope = match es.get(&path).await {
            Ok(e) => e,
            Err(UpstreamError::Budget) => return Ok(Discovery { tokens, complete: false }),
            Err(e) => return Err(e),
        };
        let rows: Vec<TokenTx> = serde_json::from_value(etherscan_result(env)?).map_err(|_| UpstreamError::Decode)?;
        for row in &rows {
            let Ok(contract) = Address::from_str(&row.contract_address) else { continue };
            if seen.insert(contract) {
                let symbol: String = row.token_symbol.chars().filter(|c| !c.is_control()).take(20).collect();
                let decimals = row.token_decimal.parse::<u8>().unwrap_or(0).min(77);
                tokens.push((contract, symbol, decimals));
            }
        }
        if rows.len() < limits.tokentx_page {
            return Ok(Discovery { tokens, complete: true });
        }
        // Continue from the last block seen (overlap is harmless: we dedupe).
        let last: u64 = rows.last().and_then(|r| r.block_number.parse().ok()).unwrap_or(start);
        if last <= start {
            break; // more than 10,000 rows in one block: can't split further
        }
        start = last;
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
            let r: RpcResponse = node.post("", &rpc("eth_call", params, 1)).await?;
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
            let responses: Vec<RpcResponse> = node.post("", &Value::Array(batch)).await?;
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
pub async fn get_holdings<E: JsonSource, R: JsonSource>(
    es: &E,
    node: &R,
    addr: &str,
    cutoff: i64,
    now: i64,
    limits: Limits,
) -> Result<RawHoldings, UpstreamError> {
    if cutoff < GENESIS_TIME {
        return Ok(RawHoldings { native: Some(0), ..Default::default() });
    }
    let owner = Address::from_str(addr).map_err(|_| UpstreamError::Decode)?;
    let block = block_at(es, node, cutoff, now).await?;
    let mut out = RawHoldings { as_of_block: Some(block), ..Default::default() };

    let r: RpcResponse = node
        .post("", &rpc("eth_getBalance", json!([addr, format!("0x{block:x}")]), 1))
        .await?;
    let wei = parse_hex_u256(&r.into_result()?)?;
    out.native = Some(u128::try_from(wei).map_err(|_| UpstreamError::Decode)?);

    let mut found = discover_tokens(es, addr, block, limits).await?;
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
            out.warnings.push(warn::TOKENS_INCOMPLETE);
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

    /// In-memory source: GET answered from a list of (substring, response),
    /// POST by a function of the request body. Records every call.
    struct Mock {
        get: Vec<(&'static str, Value)>,
        post: Box<dyn Fn(&Value) -> Value>,
        calls: RefCell<Vec<String>>,
    }

    impl JsonSource for Mock {
        async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, UpstreamError> {
            self.calls.borrow_mut().push(path.to_string());
            let v = self
                .get
                .iter()
                .find(|(k, _)| path.contains(k))
                .map(|(_, v)| v.clone())
                .ok_or(UpstreamError::Status(404))?;
            serde_json::from_value(v).map_err(|_| UpstreamError::Decode)
        }

        async fn post<T: serde::de::DeserializeOwned>(&self, _: &str, body: &Value) -> Result<T, UpstreamError> {
            self.calls.borrow_mut().push(body.to_string());
            serde_json::from_value((self.post)(body)).map_err(|_| UpstreamError::Decode)
        }
    }

    const OWNER: &str = "0x5aaeb6053f3e94c9b9a09f33669435e7ef1beaed";
    const USDC: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
    const DEAD: &str = "0x000000000000000000000000000000000000dead";

    fn word(v: u64) -> String {
        format!("0x{:064x}", v)
    }

    fn etherscan(block: u64) -> Vec<(&'static str, Value)> {
        vec![
            ("getblocknobytime", json!({ "status": "1", "message": "OK", "result": block.to_string() })),
            (
                "tokentx",
                json!({ "status": "1", "message": "OK", "result": [
                    { "blockNumber": "100", "contractAddress": USDC, "tokenSymbol": "USDC", "tokenDecimal": "6" },
                    { "blockNumber": "101", "contractAddress": USDC, "tokenSymbol": "USDC", "tokenDecimal": "6" },
                    { "blockNumber": "102", "contractAddress": DEAD, "tokenSymbol": "SPAM", "tokenDecimal": "18" }
                ]}),
            ),
        ]
    }

    fn node(usdc: u64, dead: u64) -> Box<dyn Fn(&Value) -> Value> {
        Box::new(move |body: &Value| {
            if let Value::Array(batch) = body {
                // Pre-Multicall3 batch: answer each eth_call by its target.
                return Value::Array(
                    batch
                        .iter()
                        .map(|c| {
                            let to = c["params"][0]["to"].as_str().unwrap().to_ascii_lowercase();
                            let v = if to == USDC { usdc } else { dead };
                            json!({ "jsonrpc": "2.0", "id": c["id"], "result": word(v) })
                        })
                        .collect(),
                );
            }
            match body["method"].as_str().unwrap() {
                "eth_getBalance" => json!({ "jsonrpc": "2.0", "id": 1, "result": "0xde0b6b3a7640000" }), // 1 ETH
                "eth_blockNumber" => json!({ "jsonrpc": "2.0", "id": 1, "result": "0x1406f40" }),
                "eth_call" => {
                    let data = hex_bytes(body["params"][0]["data"].as_str().unwrap()).unwrap();
                    let call = aggregate3Call::abi_decode(&data).unwrap();
                    let results: Vec<Result3> = call
                        .calls
                        .iter()
                        .map(|c| {
                            let v = if c.target == Address::from_str(USDC).unwrap() { usdc } else { dead };
                            Result3 { success: true, returnData: U256::from(v).to_be_bytes::<32>().to_vec().into() }
                        })
                        .collect();
                    let ret = aggregate3Call::abi_encode_returns(&results);
                    json!({ "jsonrpc": "2.0", "id": 1, "result": format!("0x{}", hex::encode(ret)) })
                }
                m => panic!("unexpected {m}"),
            }
        })
    }

    fn mock(get: Vec<(&'static str, Value)>, post: Box<dyn Fn(&Value) -> Value>) -> Mock {
        Mock { get, post, calls: RefCell::new(Vec::new()) }
    }

    #[test]
    fn multicall_path_drops_zero_balances() {
        let es = mock(etherscan(20_000_000), Box::new(|_| Value::Null));
        let n = mock(vec![], node(1_500_000, 0));
        let h = block_on(get_holdings(&es, &n, OWNER, 1_700_000_000, 1_800_000_000, Limits::DEFAULT)).unwrap();
        assert_eq!(h.native, Some(1_000_000_000_000_000_000));
        assert_eq!(h.as_of_block, Some(20_000_000));
        assert_eq!(h.tokens.len(), 1);
        assert_eq!(h.tokens[0].symbol, "USDC");
        assert_eq!(h.tokens[0].decimals, 6);
        assert_eq!(h.tokens[0].amount, U256::from(1_500_000u64));
        assert!(h.warnings.is_empty());
        // One multicall for both tokens, not one call per token.
        assert_eq!(n.calls.borrow().iter().filter(|c| c.contains("eth_call")).count(), 1);
    }

    #[test]
    fn batch_path_before_multicall3() {
        let es = mock(etherscan(10_000_000), Box::new(|_| Value::Null));
        let n = mock(vec![], node(7, 9));
        let h = block_on(get_holdings(&es, &n, OWNER, 1_600_000_000, 1_800_000_000, Limits::DEFAULT)).unwrap();
        assert_eq!(h.tokens.len(), 2);
        assert!(n.calls.borrow().iter().all(|c| !c.contains(&MULTICALL3.to_string().to_ascii_lowercase())));
    }

    #[test]
    fn today_uses_latest_block() {
        let es = mock(etherscan(1), Box::new(|_| Value::Null));
        let n = mock(vec![], node(0, 0));
        let h = block_on(get_holdings(&es, &n, OWNER, 1_800_000_000, 1_799_999_000, Limits::DEFAULT)).unwrap();
        assert_eq!(h.as_of_block, Some(0x1406f40));
        assert!(es.calls.borrow().iter().all(|c| !c.contains("getblocknobytime")));
    }

    #[test]
    fn before_genesis_is_zero_without_calls() {
        let es = mock(vec![], Box::new(|_| Value::Null));
        let n = mock(vec![], Box::new(|_| Value::Null));
        let h = block_on(get_holdings(&es, &n, OWNER, 1_400_000_000, 1_800_000_000, Limits::DEFAULT)).unwrap();
        assert_eq!(h.native, Some(0));
        assert!(es.calls.borrow().is_empty() && n.calls.borrow().is_empty());
    }

    #[test]
    fn etherscan_envelopes() {
        let env = |status: &str, message: &str, result: Value| Envelope { status: status.into(), message: message.into(), result };
        assert_eq!(etherscan_result(env("0", "No transactions found", json!([]))), Ok(json!([])));
        assert_eq!(
            etherscan_result(env("0", "NOTOK", json!("Max rate limit reached"))),
            Err(UpstreamError::Status(429))
        );
        assert_eq!(etherscan_result(env("0", "NOTOK", json!("Invalid API Key"))), Err(UpstreamError::Status(502)));
    }

    #[test]
    fn rate_limit_is_our_side_bad_key_too() {
        assert!(UpstreamError::Status(429).is_our_side());
        assert!(UpstreamError::Status(502).is_our_side());
    }
}

#[cfg(test)]
mod recorded {
    //! Real Etherscan / Alchemy responses under `test/fixtures/eth/`.
    //! Re-record with:
    //!   set -a; source .dev.vars; set +a; RECORD=1 cargo test -p wayback-worker eth::recorded
    //! Expected values are cross-checked by `test/check_eth.py`, which sums
    //! each token's transfers independently of `balanceOf`.
    use super::*;
    use crate::time::cutoff_unix;
    use crate::upstream::fixtures::Recorded;
    use futures::executor::block_on;

    /// Fixed "now" (2026-11-03) so the today/latest-block branch is stable.
    const NOW: i64 = 1_794_182_400;

    fn run(addr: &str, date: &str, limits: Limits) -> RawHoldings {
        let es = Recorded::new("eth", "ETHERSCAN_KEY", |k| {
            ("https://api.etherscan.io/v2/api?chainid=1&".into(), format!("&apikey={k}"))
        });
        let node = Recorded::new("eth", "ALCHEMY_KEY", |k| (format!("https://eth-mainnet.g.alchemy.com/v2/{k}"), String::new()));
        block_on(get_holdings(&es, &node, addr, cutoff_unix(date).unwrap(), NOW, limits)).unwrap()
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
