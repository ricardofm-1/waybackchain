//! Daily lookup counters in a Durable Object (strictly consistent).
//!
//! One object per key (`sid:<id>` or `ip:<hash>`). Each stores a single
//! `{date, n}` record, so yesterday's count is dropped automatically on the
//! first call of a new UTC day.

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
        if rec.date != date {
            rec = Record { date, n: 0 };
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
