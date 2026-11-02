//! Cloudflare Turnstile server-side verification.

use serde::Deserialize;
use worker::*;

const SITEVERIFY: &str = "https://challenges.cloudflare.com/turnstile/v0/siteverify";

#[derive(Deserialize)]
struct SiteVerify {
    success: bool,
}

/// `Ok(false)` when Turnstile rejects the token; `Err` when the call itself fails.
pub async fn verify(secret: &str, token: &str, ip: Option<&str>) -> Result<bool> {
    if token.is_empty() || token.len() > 2048 {
        return Ok(false);
    }
    let body = serde_json::json!({ "secret": secret, "response": token, "remoteip": ip });
    let headers = Headers::new();
    headers.set("Content-Type", "application/json")?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(body.to_string().into()));
    let mut resp = Fetch::Request(Request::new_with_init(SITEVERIFY, &init)?).send().await?;
    if resp.status_code() != 200 {
        return Err(Error::RustError(format!("siteverify status {}", resp.status_code())));
    }
    Ok(resp.json::<SiteVerify>().await?.success)
}
