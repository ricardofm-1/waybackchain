//! Daily lookup counters in a Durable Object (strictly consistent).
//!
//! One object per key (`sid:<id>` or `ip:<hash>`). Each stores a single
//! `{date, n}` record, so yesterday's count is dropped automatically on the
//! first call of a new UTC day.
//!
//! The visitor's object also tracks per-lookup outcomes so a lookup can be
//! refunded when every address failed for reasons on our side. Addresses are
//! tracked by their index in the signed list, never by value.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use worker::*;

pub const BINDING: &str = "LIMITER";
pub const PER_VISITOR: u32 = 10;
pub const PER_IP: u32 = 30;

#[derive(Serialize, Deserialize, Default)]
struct Record {
    date: String,
    n: u32,
    #[serde(default)]
    lookups: HashMap<String, Track>,
}

#[derive(Serialize, Deserialize, Default)]
struct Track {
    ok: u32,
    /// Indices of addresses that failed on our side.
    failed: Vec<u32>,
    refunded: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct Report {
    /// True when this report triggered the refund.
    pub refunded: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct Outcome {
    pub allowed: bool,
    /// Count after the operation.
    pub count: u32,
}

#[durable_object]
pub struct Limiter {
    state: State,
}

impl DurableObject for Limiter {
    fn new(state: State, _env: Env) -> Self {
        Self { state }
    }

    async fn fetch(&self, req: Request) -> Result<Response> {
        let url = req.url()?;
        let q: HashMap<String, String> = url.query_pairs().into_owned().collect();
        let date = q.get("date").ok_or_else(|| Error::RustError("missing date".into()))?.clone();
        let storage = self.state.storage();

        // Storage-only awaits keep the DO's input gate closed, so this
        // read-check-write can't interleave with another request.
        let mut rec: Record = storage.get("c").await?.unwrap_or_default();

        if url.path() == "/outcome" {
            // Only counts for the day the quota was spent.
            if rec.date != date {
                return Response::from_json(&Report { refunded: false });
            }
            let param = |k: &str| q.get(k).and_then(|v| v.parse::<u32>().ok());
            let (Some(lookup), Some(total), Some(index), Some(ok)) =
                (q.get("lookup").cloned(), param("total"), param("index"), param("ok"))
            else {
                return Response::error("bad outcome", 400);
            };
            let t = rec.lookups.entry(lookup).or_default();
            if ok == 1 {
                t.ok += 1;
            } else if !t.failed.contains(&index) {
                t.failed.push(index);
            }
            let refund = !t.refunded && t.ok == 0 && t.failed.len() as u32 >= total;
            if refund {
                t.refunded = true;
                rec.n = rec.n.saturating_sub(1);
            }
            storage.put("c", &rec).await?;
            return Response::from_json(&Report { refunded: refund });
        }

        if rec.date != date {
            rec = Record { date, n: 0, lookups: HashMap::new() };
        }

        let allowed = match url.path() {
            "/peek" => true,
            "/take" => {
                let limit: u32 = q
                    .get("limit")
                    .and_then(|l| l.parse().ok())
                    .ok_or_else(|| Error::RustError("missing limit".into()))?;
                if rec.n >= limit {
                    false
                } else {
                    rec.n += 1;
                    storage.put("c", &rec).await?;
                    true
                }
            }
            "/refund" => {
                rec.n = rec.n.saturating_sub(1);
                storage.put("c", &rec).await?;
                true
            }
            _ => return Response::error("not found", 404),
        };
        Response::from_json(&Outcome { allowed, count: rec.n })
    }
}

/// Client side: call one counter object.
pub async fn call(env: &Env, key: &str, op: &str, date: &str, limit: u32) -> Result<Outcome> {
    let stub = env.durable_object(BINDING)?.id_from_name(key)?.get_stub()?;
    let mut resp = stub
        .fetch_with_str(&format!("https://limiter/{op}?date={date}&limit={limit}"))
        .await?;
    resp.json().await
}

/// Records one address result for a lookup. Returns whether the visitor's
/// lookup was refunded as a result.
pub async fn report(env: &Env, key: &str, day: &str, lookup: &str, total: usize, index: usize, ok: bool) -> Result<bool> {
    let stub = env.durable_object(BINDING)?.id_from_name(key)?.get_stub()?;
    let mut resp = stub
        .fetch_with_str(&format!(
            "https://limiter/outcome?date={day}&lookup={lookup}&total={total}&index={index}&ok={}",
            u8::from(ok)
        ))
        .await?;
    Ok(resp.json::<Report>().await?.refunded)
}
