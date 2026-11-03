//! Wayback Wallet API Worker.
//!
//! Every request: resolve (or issue) the signed `hl_sid` cookie, route, then
//! add CORS and `Set-Cookie`. Logs carry route, status and duration only —
//! never addresses or IPs.

mod captcha;
mod chains;
mod ratelimit;
mod sid;
mod time;
mod token;
mod upstream;

use hmac::{Hmac, Mac};
use serde::Serialize;
use sha2::Sha256;
use std::collections::HashSet;
use wayback_core::amount::format_units;
use wayback_core::api::{
    Amount, Holdings, LookupAddressRequest, LookupStartRequest, LookupStartResponse, Quota, TokenHolding,
};
use wayback_core::{detect, verify as verify_address, Chain, MAX_ADDRESSES};
use worker::*;

pub use ratelimit::Limiter;

const MAX_BODY_BYTES: usize = 16 * 1024;
/// Outbound requests one address call may make to providers. Cloudflare's
/// free plan allows 50 per invocation (Durable Object calls count too); set
/// SUBREQUEST_BUDGET higher on Workers Paid.
const DEFAULT_SUBREQUEST_BUDGET: u32 = 45;
/// Wall-time cap for one address call; past it, history walks stop with a
/// `too_many_txs` warning.
const ADDRESS_TIME_LIMIT: std::time::Duration = std::time::Duration::from_secs(20);

struct Ctx {
    env: Env,
    secret: Vec<u8>,
    sid: String,
    ip: Option<String>,
    now_ms: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorBody<'a> {
    error: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    remaining: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reset_at: Option<String>,
}

fn error(status: u16, code: &str) -> Result<Response> {
    Ok(Response::from_json(&ErrorBody { error: code, remaining: None, reset_at: None })?.with_status(status))
}

#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let started = Date::now().as_millis();
    let route = format!("{} {}", req.method(), req.path());
    let origin = req.headers().get("Origin")?;
    let allowed_origin = env.var("ALLOWED_ORIGIN").map(|v| v.to_string()).ok();

    let mut resp = if req.method() == Method::Options {
        Response::empty().map(|r| r.with_status(204))
    } else {
        handle(req, env).await
    }
    .or_else(|e| {
        console_error!("{route} internal error: {e}");
        error(500, "internal")
    })?;

    if let (Some(o), Some(a)) = (origin, allowed_origin) {
        if o == a {
            let h = resp.headers_mut();
            h.set("Access-Control-Allow-Origin", &o)?;
            h.set("Access-Control-Allow-Credentials", "true")?;
            h.set("Access-Control-Allow-Methods", "GET, POST, OPTIONS")?;
            h.set("Access-Control-Allow-Headers", "Content-Type")?;
            h.set("Vary", "Origin")?;
        }
    }
    resp.headers_mut().set("Cache-Control", "no-store")?;
    console_log!("{route} -> {} in {}ms", resp.status_code(), Date::now().as_millis() - started);
    Ok(resp)
}

async fn handle(mut req: Request, env: Env) -> Result<Response> {
    let secret = env.secret("HMAC_SECRET")?.to_string().into_bytes();
    let existing = req
        .headers()
        .get("Cookie")?
        .as_deref()
        .and_then(sid::from_cookie_header)
        .and_then(|v| sid::verify(&secret, v));
    // A missing or badly signed hl_sid gets a fresh one (and a fresh counter;
    // the per-IP backstop covers cookie clearing).
    let (sid, new_cookie) = match existing {
        Some(id) => (id, None),
        None => {
            let (id, value) = sid::issue(&secret);
            (id, Some(value))
        }
    };
    let ctx = Ctx {
        ip: req.headers().get("CF-Connecting-IP")?,
        env,
        secret,
        sid,
        now_ms: Date::now().as_millis(),
    };

    let mut resp = match (req.method(), req.path().as_str()) {
        (Method::Get, "/api/quota") => quota(&ctx).await,
        (Method::Post, "/api/lookup/start") => {
            let body = req.text().await?;
            lookup_start(&ctx, &body).await
        }
        (Method::Post, "/api/lookup/address") => {
            let body = req.text().await?;
            lookup_address(&ctx, &body).await
        }
        _ => error(404, "not_found"),
    }?;

    if let Some(v) = new_cookie {
        resp.headers_mut().append("Set-Cookie", &sid::set_cookie(&v))?;
    }
    Ok(resp)
}

fn sid_key(ctx: &Ctx) -> String {
    format!("sid:{}", ctx.sid)
}

/// The IP is never stored in the clear: only an HMAC of it keys the counter.
fn ip_key(ctx: &Ctx) -> String {
    let ip = ctx.ip.as_deref().unwrap_or("unknown");
    let mut m = Hmac::<Sha256>::new_from_slice(&ctx.secret).expect("any key length");
    m.update(b"ip:");
    m.update(ip.as_bytes());
    format!("ip:{}", &hex::encode(m.finalize().into_bytes())[..32])
}

fn remaining(sid_count: u32, ip_count: u32) -> u32 {
    ratelimit::PER_VISITOR
        .saturating_sub(sid_count)
        .min(ratelimit::PER_IP.saturating_sub(ip_count))
}

async fn quota(ctx: &Ctx) -> Result<Response> {
    let date = time::today(ctx.now_ms);
    let s = ratelimit::call(&ctx.env, &sid_key(ctx), "peek", &date, 0).await?;
    let i = ratelimit::call(&ctx.env, &ip_key(ctx), "peek", &date, 0).await?;
    Response::from_json(&Quota { remaining: remaining(s.count, i.count), reset_at: time::next_reset(ctx.now_ms) })
}

async fn lookup_start(ctx: &Ctx, body: &str) -> Result<Response> {
    if body.len() > MAX_BODY_BYTES {
        return error(400, "invalid_input");
    }
    let Ok(input) = serde_json::from_str::<LookupStartRequest>(body) else {
        return error(400, "invalid_input");
    };

    // 1. Captcha before anything else.
    let secret = ctx.env.secret("TURNSTILE_SECRET")?.to_string();
    match captcha::verify(&secret, &input.turnstile_token, ctx.ip.as_deref()).await {
        Ok(true) => {}
        Ok(false) => return error(403, "captcha_failed"),
        Err(e) => {
            console_error!("turnstile unavailable: {e}");
            return error(503, "captcha_unavailable");
        }
    }

    // 2. Re-validate everything; never trust the client's chain detection.
    if !time::is_valid_lookup_date(&input.date, ctx.now_ms) {
        return error(400, "invalid_input");
    }
    let mut canonical = Vec::with_capacity(input.addresses.len());
    let mut seen = HashSet::new();
    for a in &input.addresses {
        let Ok(d) = verify_address(a.chain, &a.address) else {
            return error(400, "invalid_input");
        };
        if seen.insert(d.canonical.clone()) {
            canonical.push(d.canonical);
        }
    }
    if canonical.is_empty() || canonical.len() > MAX_ADDRESSES {
        return error(400, "invalid_input");
    }

    // 3. Spend quota: IP backstop first, then the visitor counter.
    let today = time::today(ctx.now_ms);
    let reset_at = time::next_reset(ctx.now_ms);
    let limited = |remaining: u32| -> Result<Response> {
        Ok(Response::from_json(&ErrorBody {
            error: "daily_limit",
            remaining: Some(remaining),
            reset_at: Some(reset_at.clone()),
        })?
        .with_status(429))
    };
    let ip = ratelimit::call(&ctx.env, &ip_key(ctx), "take", &today, ratelimit::PER_IP).await?;
    if !ip.allowed {
        return limited(0);
    }
    let s = ratelimit::call(&ctx.env, &sid_key(ctx), "take", &today, ratelimit::PER_VISITOR).await?;
    if !s.allowed {
        ratelimit::call(&ctx.env, &ip_key(ctx), "refund", &today, 0).await?;
        return limited(0);
    }

    // 4. Token binding visitor, date and the exact address set.
    let claims = token::Claims {
        sid: ctx.sid.clone(),
        day: today,
        date: input.date,
        a: token::sorted_addresses(canonical),
        exp: ctx.now_ms + token::LIFETIME_MS,
    };
    Response::from_json(&LookupStartResponse {
        lookup_token: token::sign(&ctx.secret, &claims),
        remaining: remaining(s.count, ip.count),
        reset_at,
    })
}

async fn lookup_address(ctx: &Ctx, body: &str) -> Result<Response> {
    if body.len() > MAX_BODY_BYTES {
        return error(400, "invalid_input");
    }
    let Ok(input) = serde_json::from_str::<LookupAddressRequest>(body) else {
        return error(400, "invalid_input");
    };
    let Ok((claims, lookup_id)) = token::verify(&ctx.secret, &input.lookup_token, &ctx.sid, ctx.now_ms) else {
        return error(401, "bad_token");
    };
    let Ok(d) = detect(&input.address) else {
        return error(404, "address_not_in_lookup");
    };
    let Some(index) = claims.index_of(&d.canonical) else {
        return error(404, "address_not_in_lookup");
    };
    let Some(cutoff) = time::cutoff_unix(&claims.date) else {
        return error(401, "bad_token");
    };

    let budget = ctx
        .env
        .var("SUBREQUEST_BUDGET")
        .ok()
        .and_then(|v| v.to_string().parse().ok())
        .unwrap_or(DEFAULT_SUBREQUEST_BUDGET);
    let http = upstream::Http::new(budget, ADDRESS_TIME_LIMIT);

    let (symbol, decimals, result) = match d.chain {
        Chain::Btc => {
            let src = upstream::Rest { http: &http, bases: chains::btc::PROVIDERS };
            let r = chains::btc::get_holdings(&src, &d.canonical, cutoff, chains::btc::MAX_TXS_AFTER_T).await;
            (chains::btc::SYMBOL, chains::btc::DECIMALS, r)
        }
        _ => return error(501, "chain_not_supported_yet"),
    };

    let total = claims.a.len();
    let report = |ok: bool| async move {
        let sid_key = sid_key(ctx);
        match ratelimit::report(&ctx.env, &sid_key, &claims.day, &lookup_id, total, index, ok).await {
            Ok(true) => {
                console_log!("lookup refunded");
                if let Err(e) = ratelimit::call(&ctx.env, &ip_key(ctx), "refund", &claims.day, 0).await {
                    console_error!("ip refund failed: {e}");
                }
            }
            Ok(false) => {}
            Err(e) => console_error!("outcome report failed: {e}"),
        }
    };

    match result {
        Ok(raw) => {
            report(true).await;
            Response::from_json(&Holdings {
                chain: d.chain,
                address: d.input,
                as_of_block: raw.as_of_block.map(|b| b.to_string()),
                as_of_time: format!("{}T23:59:59Z", claims.date),
                native: raw.native.map(|v| Amount { symbol: symbol.into(), amount: format_units(v, decimals), usd: None }),
                tokens: raw
                    .tokens
                    .into_iter()
                    .map(|t| TokenHolding {
                        contract: t.contract,
                        symbol: t.symbol,
                        amount: format_units(t.amount, t.decimals),
                        usd: None,
                    })
                    .collect(),
                warnings: raw.warnings.into_iter().map(String::from).collect(),
            })
        }
        Err(e) => {
            console_error!("{} upstream error: {}", d.chain, e.label());
            if e.is_our_side() {
                report(false).await;
            }
            error(502, "upstream_error")
        }
    }
}
