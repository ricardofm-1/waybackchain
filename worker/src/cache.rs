//! Result cache: a (chain, address, date) result for a date at least two
//! days old is final, so it's kept in KV for 30 days.
//!
//! Privacy: the key is an HMAC of (chain, address, date), so KV never holds
//! an address in readable form, and the stored value has the address blanked.
//! Results limited by our own budgets (too_many_txs, tokens_incomplete…)
//! aren't cached: a later call might do better.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use wayback_core::api::Holdings;
use worker::kv::KvStore;

pub const TTL_SECS: u64 = 30 * 24 * 60 * 60;

/// Warnings that mean "this result is partial because of our limits".
const NOT_CACHEABLE: &[&str] = &["too_many_txs", "tokens_incomplete", "token_cap_reached"];

pub fn is_final(cutoff: i64, now: i64) -> bool {
    cutoff + 2 * 86_400 < now
}

pub fn key(secret: &[u8], chain: &str, canonical: &str, date: &str) -> String {
    let mut m = Hmac::<Sha256>::new_from_slice(secret).expect("any key length");
    m.update(format!("res:{chain}:{canonical}:{date}").as_bytes());
    format!("res:{}", &hex::encode(m.finalize().into_bytes())[..32])
}

pub async fn get(kv: &KvStore, key: &str) -> Option<Holdings> {
    kv.get(key).json::<Holdings>().await.ok().flatten()
}

pub async fn put(kv: &KvStore, key: &str, h: &Holdings) {
    if h.warnings.iter().any(|w| NOT_CACHEABLE.contains(&w.as_str())) {
        return;
    }
    let mut stored = h.clone();
    stored.address = String::new();
    stored.native.iter_mut().for_each(|n| n.usd = None);
    stored.tokens.iter_mut().for_each(|t| t.usd = None);
    if let Ok(json) = serde_json::to_string(&stored) {
        if let Ok(put) = kv.put(key, json) {
            let _ = put.expiration_ttl(TTL_SECS).execute().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_hide_the_address() {
        let k = key(b"s", "btc", "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa", "2024-01-01");
        assert!(k.starts_with("res:") && k.len() == 36);
        assert!(!k.contains("1A1z"));
        assert_ne!(k, key(b"s", "btc", "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa", "2024-01-02"));
        assert_ne!(k, key(b"other", "btc", "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa", "2024-01-01"));
    }

    #[test]
    fn finality() {
        let now = 1_800_000_000;
        assert!(is_final(now - 3 * 86_400, now));
        assert!(!is_final(now - 86_400, now));
    }
}
