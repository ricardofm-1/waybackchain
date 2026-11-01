//! The `hl_addr` cookie: the browser-owned saved address list.

use wasm_bindgen::JsCast;
use wayback_core::address_list::{decode_cookie, encode_cookie};
use wayback_core::Detected;
use web_sys::HtmlDocument;

const NAME: &str = "hl_addr";
const MAX_AGE_SECS: u32 = 180 * 24 * 60 * 60;

fn document() -> Option<HtmlDocument> {
    web_sys::window()?.document()?.dyn_into::<HtmlDocument>().ok()
}

fn write(value: &str, max_age: u32) {
    if let Some(doc) = document() {
        let _ = doc.set_cookie(&format!(
            "{NAME}={value}; Max-Age={max_age}; Path=/; Secure; SameSite=Lax"
        ));
    }
}

/// Saved addresses, or an empty list. A cookie that fails to decode is
/// deleted silently.
pub fn load() -> Vec<String> {
    let Some(raw) = document().and_then(|d| d.cookie().ok()) else {
        return Vec::new();
    };
    let Some(value) = raw
        .split(';')
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == NAME)
        .map(|(_, v)| v)
    else {
        return Vec::new();
    };
    decode_cookie(value).unwrap_or_else(|| {
        clear();
        Vec::new()
    })
}

/// Saves (and refreshes the lifetime of) the address list. Returns how many
/// addresses fit in the cookie.
pub fn save(addresses: &[Detected]) -> usize {
    let value = encode_cookie(addresses);
    write(&value, MAX_AGE_SECS);
    if value.is_empty() { 0 } else { value.split(',').count() }
}

pub fn clear() {
    write("", 0);
}
