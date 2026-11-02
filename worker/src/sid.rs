//! `hl_sid`: random 128-bit visitor ID + HMAC signature, owned by the Worker.
//! Format: `<32 hex id>.<64 hex HMAC-SHA256("sid:" + id)>`.

use hmac::{Hmac, Mac};
use sha2::Sha256;

pub const COOKIE_NAME: &str = "hl_sid";
const MAX_AGE_SECS: u32 = 365 * 24 * 60 * 60;

type HmacSha256 = Hmac<Sha256>;

fn mac(secret: &[u8], id: &str) -> HmacSha256 {
    let mut m = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
    m.update(b"sid:");
    m.update(id.as_bytes());
    m
}

/// A fresh signed cookie value and its ID.
pub fn issue(secret: &[u8]) -> (String, String) {
    let mut raw = [0u8; 16];
    getrandom::getrandom(&mut raw).expect("crypto.getRandomValues is available in Workers");
    let id = hex::encode(raw);
    let sig = hex::encode(mac(secret, &id).finalize().into_bytes());
    (id.clone(), format!("{id}.{sig}"))
}

/// The ID if the value is well-formed and correctly signed.
pub fn verify(secret: &[u8], value: &str) -> Option<String> {
    let (id, sig) = value.split_once('.')?;
    if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) {
        return None;
    }
    let sig = hex::decode(sig).ok()?;
    mac(secret, id).verify_slice(&sig).ok()?; // constant-time compare
    Some(id.to_string())
}

/// Finds `hl_sid` in a `Cookie` header.
pub fn from_cookie_header(header: &str) -> Option<&str> {
    header
        .split(';')
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == COOKIE_NAME)
        .map(|(_, v)| v)
}

pub fn set_cookie(value: &str) -> String {
    format!("{COOKIE_NAME}={value}; Max-Age={MAX_AGE_SECS}; Path=/api; HttpOnly; Secure; SameSite=Strict")
}

#[cfg(test)]
mod tests {
    use super::*;

    const K: &[u8] = b"test-secret";

    #[test]
    fn issue_then_verify() {
        let (id, value) = issue(K);
        assert_eq!(verify(K, &value), Some(id));
    }

    #[test]
    fn forged_or_mangled_is_rejected() {
        let (_, value) = issue(K);
        assert_eq!(verify(b"other-secret", &value), None);
        let mut tampered = value.clone();
        tampered.replace_range(0..1, if value.starts_with('0') { "1" } else { "0" });
        assert_eq!(verify(K, &tampered), None);
        assert_eq!(verify(K, "nope"), None);
        assert_eq!(verify(K, &format!("{}.", "a".repeat(32))), None);
    }

    #[test]
    fn reads_cookie_header() {
        assert_eq!(from_cookie_header("a=1; hl_sid=xyz; hl_addr=q"), Some("xyz"));
        assert_eq!(from_cookie_header("hl_addr=q"), None);
    }
}
