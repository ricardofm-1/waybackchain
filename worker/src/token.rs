//! `lookupToken`: proves a paid lookup for a fixed (visitor, date, address
//! list) for 10 minutes, so per-address calls and retries cost no quota but
//! can't be stretched to other addresses.
//!
//! The signed claims carry the sorted canonical address list itself, so the
//! Worker can check membership statelessly and never stores addresses.
//!
//! Format: `<base64url(JSON claims)>.<base64url(HMAC-SHA256("lt:" + claims))>`.

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const LIFETIME_MS: u64 = 10 * 60 * 1000;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claims {
    pub sid: String,
    /// UTC day the quota was spent (for refunds).
    pub day: String,
    /// Lookup date; the cutoff is 23:59:59 UTC on it.
    pub date: String,
    /// Sorted, de-duplicated canonical addresses.
    pub a: Vec<String>,
    /// Expiry, Unix milliseconds.
    pub exp: u64,
}

impl Claims {
    /// Position of `canonical` in the signed list, if it's part of the lookup.
    pub fn index_of(&self, canonical: &str) -> Option<usize> {
        self.a.binary_search_by(|x| x.as_str().cmp(canonical)).ok()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum TokenError {
    Malformed,
    BadSignature,
    Expired,
    WrongVisitor,
}

pub fn sorted_addresses(mut canonical: Vec<String>) -> Vec<String> {
    canonical.sort_unstable();
    canonical.dedup();
    canonical
}

fn mac(secret: &[u8], payload: &str) -> HmacSha256 {
    let mut m = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
    m.update(b"lt:");
    m.update(payload.as_bytes());
    m
}

pub fn sign(secret: &[u8], claims: &Claims) -> String {
    let payload = B64.encode(serde_json::to_vec(claims).expect("claims serialize"));
    let sig = B64.encode(mac(secret, &payload).finalize().into_bytes());
    format!("{payload}.{sig}")
}

/// Verified claims plus a short lookup ID (for refund bookkeeping).
pub fn verify(secret: &[u8], token: &str, sid: &str, now_ms: u64) -> Result<(Claims, String), TokenError> {
    let (payload, sig) = token.split_once('.').ok_or(TokenError::Malformed)?;
    let sig = B64.decode(sig).map_err(|_| TokenError::Malformed)?;
    mac(secret, payload).verify_slice(&sig).map_err(|_| TokenError::BadSignature)?;
    let bytes = B64.decode(payload).map_err(|_| TokenError::Malformed)?;
    let claims: Claims = serde_json::from_slice(&bytes).map_err(|_| TokenError::Malformed)?;
    if now_ms >= claims.exp {
        return Err(TokenError::Expired);
    }
    if claims.sid != sid {
        return Err(TokenError::WrongVisitor);
    }
    let id = hex::encode(&Sha256::digest(payload.as_bytes())[..8]);
    Ok((claims, id))
}

#[cfg(test)]
mod tests {
    use super::*;

    const K: &[u8] = b"k";

    fn claims() -> Claims {
        Claims {
            sid: "s1".into(),
            day: "2026-11-03".into(),
            date: "2024-01-01".into(),
            a: sorted_addresses(vec!["b".into(), "a".into(), "b".into()]),
            exp: 1_000,
        }
    }

    #[test]
    fn round_trip() {
        let t = sign(K, &claims());
        let (c, id) = verify(K, &t, "s1", 999).unwrap();
        assert_eq!(c, claims());
        assert_eq!(id.len(), 16);
    }

    #[test]
    fn rejects_expired_foreign_and_tampered() {
        let t = sign(K, &claims());
        assert_eq!(verify(K, &t, "s1", 1_000), Err(TokenError::Expired));
        assert_eq!(verify(K, &t, "s2", 0), Err(TokenError::WrongVisitor));
        assert_eq!(verify(b"x", &t, "s1", 0), Err(TokenError::BadSignature));
        let mut c = claims();
        c.a.push("c".into());
        let forged = format!("{}.{}", sign(K, &c).split_once('.').unwrap().0, t.split_once('.').unwrap().1);
        assert_eq!(verify(K, &forged, "s1", 0), Err(TokenError::BadSignature));
    }

    #[test]
    fn membership() {
        let c = claims();
        assert_eq!(c.a, vec!["a", "b"]);
        assert_eq!(c.index_of("b"), Some(1));
        assert_eq!(c.index_of("c"), None);
    }
}
