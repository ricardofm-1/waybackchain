//! XRP Ledger via public full-history servers (no key).
//!
//! 1. Find the last ledger that closed at or before T. There is no
//!    date-to-ledger API, so search `ledger` by close time with
//!    `last_at_or_before` (interpolation; ledgers close every ~4 s).
//! 2. `account_info` at that ledger for XRP (drops), `account_lines` (paged)
//!    for issued tokens: a positive balance is a holding.
//!
//! rippled reports errors inside a 200 response (`"status": "error"`), and a
//! rate-limited server says `slowDown`; those move on to the next server.

use super::{warn, RawHoldings, RawToken};
use crate::money::parse_decimal;
use crate::upstream::{JsonSource, UpstreamError};
#[cfg(test)]
use alloy_primitives::U256;
use serde_json::{json, Value};

pub const DECIMALS: u8 = 6;
pub const SYMBOL: &str = "XRP";
/// Seconds between the Unix epoch and the Ripple epoch (2000-01-01).
const RIPPLE_EPOCH: i64 = 946_684_800;
/// Recorded history starts here (2013-01-01T03:21:10Z).
const FIRST_LEDGER: u64 = 32_570;
const FIRST_CLOSE: i64 = 1_357_010_470;
/// Above this many XRP the address is probably shared (an exchange).
pub const EXCHANGE_THRESHOLD_DROPS: u128 = 10_000_000 * 1_000_000;
const MAX_SEARCH_STEPS: usize = 40;

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Trust lines per `account_lines` page (rippled allows 10–400).
    pub lines_page: usize,
    /// Pages before giving up on the full token list.
    pub max_pages: usize,
}

impl Limits {
    pub const DEFAULT: Limits = Limits { lines_page: 400, max_pages: 5 };
}

enum Reply {
    Ok(Value),
    /// `actNotFound`: not funded yet at that ledger (or deleted).
    NotFound,
}

/// Errors that mean "this server can't answer right now": try the next one.
fn server_side(code: &str) -> bool {
    matches!(code, "slowDown" | "tooBusy" | "noNetwork" | "noCurrent" | "noClosed" | "lgrNotFound" | "internal")
}

async fn call<S: JsonSource>(nodes: &[S], method: &str, params: Value) -> Result<Reply, UpstreamError> {
    let body = json!({ "method": method, "params": [params] });
    let mut last = UpstreamError::Network;
    for node in nodes {
        let resp: Value = match node.post("", &body).await {
            Ok(v) => v,
            Err(e) if e.is_our_side() => {
                last = e;
                continue;
            }
            Err(e) => return Err(e),
        };
        let result = &resp["result"];
        match (result["status"].as_str(), result["error"].as_str()) {
            (Some("success"), _) => return Ok(Reply::Ok(result.clone())),
            (_, Some("actNotFound")) => return Ok(Reply::NotFound),
            (_, Some(code)) if server_side(code) => {
                last = if code == "slowDown" { UpstreamError::Status(429) } else { UpstreamError::Status(503) };
            }
            _ => return Err(UpstreamError::Status(502)),
        }
    }
    Err(last)
}

async fn ok<S: JsonSource>(nodes: &[S], method: &str, params: Value) -> Result<Value, UpstreamError> {
    match call(nodes, method, params).await? {
        Reply::Ok(v) => Ok(v),
        Reply::NotFound => Err(UpstreamError::Status(502)),
    }
}

/// (ledger index, close time as Unix seconds)
async fn ledger<S: JsonSource>(nodes: &[S], index: Value) -> Result<(u64, i64), UpstreamError> {
    let r = ok(nodes, "ledger", json!({ "ledger_index": index })).await?;
    let idx = r["ledger_index"]
        .as_u64()
        .or_else(|| r["ledger"]["ledger_index"].as_str().and_then(|s| s.parse().ok()))
        .ok_or(UpstreamError::Decode)?;
    let close = r["ledger"]["close_time"].as_i64().ok_or(UpstreamError::Decode)?;
    Ok((idx, close + RIPPLE_EPOCH))
}

/// Last validated ledger with close time <= cutoff; `None` before history.
async fn ledger_at<S: JsonSource>(nodes: &[S], cutoff: i64) -> Result<Option<u64>, UpstreamError> {
    if cutoff < FIRST_CLOSE {
        return Ok(None);
    }
    let latest = ledger(nodes, json!("validated")).await?;
    if latest.1 <= cutoff {
        return Ok(Some(latest.0));
    }
    let lo = (FIRST_LEDGER, FIRST_CLOSE);
    let index = super::last_at_or_before(lo, latest, cutoff, MAX_SEARCH_STEPS, |n| async move {
        ledger(nodes, json!(n)).await.map(|(_, t)| t)
    })
    .await?;
    Ok(Some(index))
}

/// Currency code as text: 3-char codes as-is; 40-hex codes decoded when
/// they hold printable ASCII, otherwise shortened hex (e.g. LP tokens).
pub fn currency_text(code: &str) -> String {
    if code.len() != 40 || !code.bytes().all(|b| b.is_ascii_hexdigit()) {
        return code.to_string();
    }
    let bytes = hex::decode(code).unwrap_or_default();
    let trimmed: Vec<u8> = bytes.iter().copied().skip_while(|&b| b == 0).collect::<Vec<_>>();
    let text: Vec<u8> = trimmed.iter().copied().take_while(|&b| b != 0).collect();
    if !text.is_empty() && text.len() + trimmed.iter().filter(|&&b| b == 0).count() == trimmed.len()
        && text.iter().all(|b| b.is_ascii_graphic() || *b == b' ')
    {
        String::from_utf8_lossy(&text).trim().to_string()
    } else {
        format!("{}…", &code[..8])
    }
}

/// `nodes` are tried in order for every call (primary first).
pub async fn get_holdings<S: JsonSource>(
    nodes: &[S],
    addr: &str,
    cutoff: i64,
    limits: Limits,
) -> Result<RawHoldings, UpstreamError> {
    let mut out = RawHoldings::default();
    let Some(index) = ledger_at(nodes, cutoff).await? else {
        out.warnings.push(warn::NO_HISTORY);
        return Ok(out);
    };
    out.as_of_block = Some(index);

    let drops = match call(nodes, "account_info", json!({ "account": addr, "ledger_index": index })).await? {
        Reply::NotFound => {
            out.native = Some(0);
            return Ok(out);
        }
        Reply::Ok(r) => r["account_data"]["Balance"]
            .as_str()
            .and_then(|b| b.parse::<u128>().ok())
            .ok_or(UpstreamError::Decode)?,
    };
    out.native = Some(drops);
    out.warnings.push(warn::XRP_INCLUDES_RESERVE);
    if drops >= EXCHANGE_THRESHOLD_DROPS {
        out.warnings.push(warn::LIKELY_EXCHANGE);
    }

    let mut marker: Option<Value> = None;
    for page in 0..=limits.max_pages {
        if page == limits.max_pages {
            out.warnings.push(warn::TOKENS_INCOMPLETE);
            break;
        }
        let mut params = json!({ "account": addr, "ledger_index": index, "limit": limits.lines_page });
        if let Some(m) = &marker {
            params["marker"] = m.clone();
        }
        let r = match ok(nodes, "account_lines", params).await {
            Ok(r) => r,
            Err(UpstreamError::Budget) => {
                out.warnings.push(warn::TOKENS_INCOMPLETE);
                break;
            }
            Err(e) => return Err(e),
        };
        for line in r["lines"].as_array().into_iter().flatten() {
            let (Some(balance), Some(currency), Some(issuer)) =
                (line["balance"].as_str(), line["currency"].as_str(), line["account"].as_str())
            else {
                continue;
            };
            if let Some((amount, decimals)) = parse_decimal(balance) {
                out.tokens.push(RawToken {
                    contract: format!("{currency}.{issuer}"),
                    symbol: currency_text(currency),
                    decimals,
                    amount,
                });
            }
        }
        match r.get("marker") {
            Some(m) if !m.is_null() => marker = Some(m.clone()),
            _ => break,
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;
    use std::cell::RefCell;

    #[test]
    fn currency_codes() {
        assert_eq!(currency_text("USD"), "USD");
        // "SOLO" as a 160-bit code
        assert_eq!(currency_text("534F4C4F00000000000000000000000000000000"), "SOLO");
        // LP token (0x03 prefix, binary): shortened
        assert_eq!(currency_text("03B20F3A7D26D33C6DA3503E5CCE3E67B102D4D2"), "03B20F3A…");
    }

    /// A synthetic ledger: index n closes at FIRST_CLOSE + 4 s * (n - FIRST_LEDGER).
    struct Chain {
        latest: u64,
        calls: RefCell<usize>,
        slow_down_first: bool,
    }

    impl JsonSource for Chain {
        async fn get<T: serde::de::DeserializeOwned>(&self, _: &str) -> Result<T, UpstreamError> {
            unreachable!()
        }
        async fn post<T: serde::de::DeserializeOwned>(&self, _: &str, body: &Value) -> Result<T, UpstreamError> {
            *self.calls.borrow_mut() += 1;
            if self.slow_down_first {
                return Ok(serde_json::from_value(json!({ "result": { "status": "error", "error": "slowDown" } })).unwrap());
            }
            let p = &body["params"][0];
            let v = match body["method"].as_str().unwrap() {
                "ledger" => {
                    let n = p["ledger_index"].as_u64().unwrap_or(self.latest);
                    let close = FIRST_CLOSE + 4 * (n - FIRST_LEDGER) as i64 - RIPPLE_EPOCH;
                    json!({ "result": { "status": "success", "ledger_index": n, "ledger": { "close_time": close } } })
                }
                "account_info" => json!({ "result": { "status": "error", "error": "actNotFound" } }),
                m => panic!("{m}"),
            };
            Ok(serde_json::from_value(v).unwrap())
        }
    }

    fn chain(slow: bool) -> Chain {
        Chain { latest: 100_000_000, calls: RefCell::new(0), slow_down_first: slow }
    }

    #[test]
    fn ledger_search_is_exact_and_cheap() {
        let nodes = [chain(false)];
        for target in [40_000u64, 12_345_678, 99_999_998] {
            let cutoff = FIRST_CLOSE + 4 * (target - FIRST_LEDGER) as i64 + 2; // between ledgers
            *nodes[0].calls.borrow_mut() = 0;
            assert_eq!(block_on(ledger_at(&nodes, cutoff)).unwrap(), Some(target));
            assert!(*nodes[0].calls.borrow() <= 8, "{} calls", nodes[0].calls.borrow());
        }
    }

    #[test]
    fn before_history_and_unfunded() {
        let nodes = [chain(false)];
        let h = block_on(get_holdings(&nodes, "rX", FIRST_CLOSE - 1, Limits::DEFAULT)).unwrap();
        assert_eq!((h.native, h.warnings), (None, vec![warn::NO_HISTORY]));
        let h = block_on(get_holdings(&nodes, "rX", FIRST_CLOSE + 1000, Limits::DEFAULT)).unwrap();
        assert_eq!(h.native, Some(0));
    }

    #[test]
    fn slow_down_falls_through_to_next_server() {
        let nodes = [chain(true), chain(false)];
        let h = block_on(get_holdings(&nodes, "rX", FIRST_CLOSE + 1000, Limits::DEFAULT)).unwrap();
        assert_eq!(h.native, Some(0));
        let only_slow = [chain(true)];
        assert_eq!(
            block_on(get_holdings(&only_slow, "rX", FIRST_CLOSE + 1000, Limits::DEFAULT)).unwrap_err(),
            UpstreamError::Status(429)
        );
    }
}

#[cfg(test)]
mod recorded {
    //! Real s2.ripple.com responses under `test/fixtures/xrp/`. No key needed.
    //! Re-record with: RECORD=1 cargo test -p wayback-worker xrp::recorded
    //! Expected values cross-checked by `test/check_xrp.py` against the
    //! second server (xrplcluster.com).
    use super::*;
    use crate::time::cutoff_unix;
    use crate::upstream::fixtures::Recorded;
    use futures::executor::block_on;

    fn run(addr: &str, date: &str, limits: Limits) -> RawHoldings {
        let nodes = [Recorded::new("xrp", "", |_| ("https://s2.ripple.com/".into(), String::new()))];
        block_on(get_holdings(&nodes, addr, cutoff_unix(date).unwrap(), limits)).unwrap()
    }

    include!("../../test/fixtures/xrp/expected.rs");
}
