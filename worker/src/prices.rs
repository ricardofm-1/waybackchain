//! Indicative USD prices from DefiLlama's free coins API (no key).
//!
//! One request prices many assets at a timestamp: native coins by their
//! CoinGecko id, ETH and SOL tokens by contract / mint. XRPL and Cardano
//! tokens have no ids here and stay unpriced (the spec expects that).
//!
//! Historical prices never change, so for final dates (at least two days
//! old) each date's prices live in one KV document: one read and at most one
//! write per address call, whatever the number of tokens. Unknown prices are
//! cached too (as null) so spam tokens aren't asked for again.

use crate::upstream::Http;
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use wayback_core::Chain;
use worker::kv::KvStore;

const BASE: &str = "https://coins.llama.fi";
/// Coins per request, to keep URLs well under length limits.
const CHUNK: usize = 60;

/// DefiLlama coin id for a native asset (`token == None`) or a token.
pub fn coin_id(chain: Chain, token: Option<&str>) -> Option<String> {
    match (chain, token) {
        (Chain::Btc, None) => Some("coingecko:bitcoin".into()),
        (Chain::Eth, None) => Some("coingecko:ethereum".into()),
        (Chain::Sol, None) => Some("coingecko:solana".into()),
        (Chain::Xrp, None) => Some("coingecko:ripple".into()),
        (Chain::Ada, None) => Some("coingecko:cardano".into()),
        (Chain::Eth, Some(contract)) => Some(format!("ethereum:{}", contract.to_ascii_lowercase())),
        (Chain::Sol, Some(mint)) => Some(format!("solana:{mint}")),
        _ => None,
    }
}

#[derive(Deserialize)]
struct Response {
    coins: HashMap<String, Coin>,
}

#[derive(Deserialize)]
struct Coin {
    price: serde_json::Number,
}

/// Prices (as decimal strings) for `ids` at `cutoff`. Best effort: any
/// failure just leaves assets unpriced.
pub async fn lookup(http: &Http, kv: Option<&KvStore>, ids: &[String], date: &str, cutoff: i64, now: i64) -> HashMap<String, String> {
    let is_final = cutoff + 2 * 86_400 < now;
    let doc_key = format!("px:{date}");
    let mut known: BTreeMap<String, Option<String>> = BTreeMap::new();
    if let (true, Some(kv)) = (is_final, kv) {
        if let Ok(Some(doc)) = kv.get(&doc_key).json::<BTreeMap<String, Option<String>>>().await {
            known = doc;
        }
    }

    let missing: Vec<&String> = ids.iter().filter(|id| !known.contains_key(*id)).collect();
    let mut fetched = false;
    for chunk in missing.chunks(CHUNK) {
        let list = chunk.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(",");
        let url = if cutoff >= now {
            format!("{BASE}/prices/current/{list}")
        } else {
            format!("{BASE}/prices/historical/{cutoff}/{list}?searchWidth=12h")
        };
        let Ok(resp) = http.get_json::<Response>(&url, &[]).await else { break };
        for id in chunk {
            known.insert((*id).clone(), resp.coins.get(*id).map(|c| c.price.to_string()));
        }
        fetched = true;
    }

    if let (true, true, Some(kv)) = (is_final, fetched, kv) {
        if let Ok(json) = serde_json::to_string(&known) {
            if let Ok(put) = kv.put(&doc_key, json) {
                let _ = put.execute().await;
            }
        }
    }
    known.into_iter().filter_map(|(k, v)| v.map(|p| (k, p))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids() {
        assert_eq!(coin_id(Chain::Btc, None).as_deref(), Some("coingecko:bitcoin"));
        assert_eq!(
            coin_id(Chain::Eth, Some("0x4a220E6096B25EADb88358cb44068A3248254675")).as_deref(),
            Some("ethereum:0x4a220e6096b25eadb88358cb44068a3248254675")
        );
        assert_eq!(coin_id(Chain::Sol, Some("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v")).as_deref(), Some("solana:EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"));
        assert_eq!(coin_id(Chain::Xrp, Some("USD.rvYAfWj5gh67oV6fW32ZzP3Aw4Eubs59B")), None);
        assert_eq!(coin_id(Chain::Ada, Some("f43a62fd")), None);
    }
}
