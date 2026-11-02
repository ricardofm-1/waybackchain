//! `lookupToken`: proves a paid lookup for a fixed (visitor, date, address
//! list) for 10 minutes, so per-address calls cost no quota but can't be
//! stretched to other addresses.
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
    pub date: String,
    /// hex sha256 of the sorted canonical addresses, newline-joined.
    pub h: String,
    /// Expiry, Unix milliseconds.
    pub exp: u64,
}

#[derive(Debug, PartialEq, Eq)]
#[allow(dead_code)] // verified by /api/lookup/address (step 3)
pub enum TokenError {
    Malformed,
    BadSignature,
    Expired,
    WrongVisitor,
}

pub fn address_set_hash<S: AsRef<str>>(canonical: &[S]) -> String {
    let mut v: Vec<&str> = canonical.iter().map(AsRef::as_ref).collect();
    v.sort_unstable();
    v.dedup();
    hex::encode(Sha256::digest(v.join("\n").as_bytes()))
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

#[allow(dead_code)]
pub fn verify(secret: &[u8], token: &str, sid: &str, now_ms: u64) -> Result<Claims, TokenError> {
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
    Ok(claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    const K: &[u8] = b"k";

    fn claims() -> Claims {
        Claims { sid: "s1".into(), date: "2024-01-01".into(), h: address_set_hash(&["b", "a"]), exp: 1_000 }
    }

    #[test]
    fn round_trip() {
        let t = sign(K, &claims());
        assert_eq!(verify(K, &t, "s1", 999), Ok(claims()));
    }

    #[test]
    fn rejects_expired_foreign_and_tampered() {
        let t = sign(K, &claims());
        assert_eq!(verify(K, &t, "s1", 1_000), Err(TokenError::Expired));
        assert_eq!(verify(K, &t, "s2", 0), Err(TokenError::WrongVisitor));
        assert_eq!(verify(b"x", &t, "s1", 0), Err(TokenError::BadSignature));
        let mut c = claims();
        c.date = "2020-01-01".into();
        let forged = format!("{}.{}", sign(K, &c).split_once('.').unwrap().0, t.split_once('.').unwrap().1);
        assert_eq!(verify(K, &forged, "s1", 0), Err(TokenError::BadSignature));
    }

    #[test]
    fn hash_ignores_order_and_duplicates() {
        assert_eq!(address_set_hash(&["a", "b"]), address_set_hash(&["b", "a", "a"]));
        assert_ne!(address_set_hash(&["a"]), address_set_hash(&["a", "b"]));
    }
}
