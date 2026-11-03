//! Outbound HTTP to blockchain APIs: retries, provider fallback, and a
//! per-invocation budget: a cap on outbound requests (Cloudflare allows 50
//! per invocation on the free plan, 1,000 on Workers Paid) and on wall time,
//! so one busy address can't keep its card spinning for minutes.

use serde::de::DeserializeOwned;
use std::cell::Cell;
use std::time::Duration;
use futures_util::future::{select, Either};
use worker::{AbortController, Date, Delay, Fetch, Headers, Method, Request, RequestInit};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpstreamError {
    /// Out of subrequests or time for this invocation.
    Budget,
    /// Non-success HTTP status after retries.
    Status(u16),
    Network,
    /// The provider didn't answer in time. Not retried on the same provider:
    /// a hanging provider is usually throttling us, so move to the next one.
    Timeout,
    Decode,
}

impl UpstreamError {
    /// A failure on our side (provider down, throttled, unreachable) rather
    /// than something about the address. Only these earn a quota refund.
    pub fn is_our_side(&self) -> bool {
        match self {
            UpstreamError::Status(s) => *s == 429 || *s >= 500,
            UpstreamError::Network | UpstreamError::Timeout | UpstreamError::Decode => true,
            UpstreamError::Budget => false,
        }
    }

    /// Short label for logs (never includes the address).
    pub fn label(&self) -> String {
        match self {
            UpstreamError::Budget => "budget".into(),
            UpstreamError::Status(s) => format!("status_{s}"),
            UpstreamError::Network => "network".into(),
            UpstreamError::Timeout => "timeout".into(),
            UpstreamError::Decode => "decode".into(),
        }
    }
}

/// Something that answers `GET <path>` with JSON. Implemented by the live
/// HTTP client and by recorded fixtures in tests.
#[allow(async_fn_in_trait)]
pub trait JsonSource {
    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, UpstreamError>;

    /// JSON POST (JSON-RPC). Sources that only serve GET answer 405.
    async fn post<T: DeserializeOwned>(&self, _path: &str, _body: &serde_json::Value) -> Result<T, UpstreamError> {
        Err(UpstreamError::Status(405))
    }
}

pub struct Http {
    budget: Cell<u32>,
    deadline_ms: u64,
}

const RETRIES: u32 = 2;
/// No single provider request may take longer than this.
const PER_REQUEST_TIMEOUT_MS: u64 = 6_000;

impl Http {
    pub fn new(budget: u32, time_limit: Duration) -> Self {
        Self { budget: Cell::new(budget), deadline_ms: Date::now().as_millis() + time_limit.as_millis() as u64 }
    }

    fn exhausted(&self) -> bool {
        self.budget.get() == 0 || Date::now().as_millis() >= self.deadline_ms
    }

    /// GET with up to two retries (exponential backoff) on 429 / 5xx /
    /// network errors. Each attempt is cut off at the remaining time.
    pub async fn get_json<T: DeserializeOwned>(&self, url: &str, headers: &[(&str, String)]) -> Result<T, UpstreamError> {
        self.send_json(url, None, headers).await
    }

    pub async fn post_json<T: DeserializeOwned>(&self, url: &str, body: &serde_json::Value) -> Result<T, UpstreamError> {
        self.send_json(url, Some(body.to_string()), &[]).await
    }

    async fn send_json<T: DeserializeOwned>(
        &self,
        url: &str,
        body: Option<String>,
        extra: &[(&str, String)],
    ) -> Result<T, UpstreamError> {
        let mut last = UpstreamError::Network;
        for attempt in 0..=RETRIES {
            if attempt > 0 {
                Delay::from(Duration::from_millis(250 * 4u64.pow(attempt - 1))).await;
            }
            if self.exhausted() {
                return Err(UpstreamError::Budget);
            }
            self.budget.set(self.budget.get() - 1);
            let remaining = self.deadline_ms.saturating_sub(Date::now().as_millis());
            let timeout = Duration::from_millis(remaining.min(PER_REQUEST_TIMEOUT_MS));
            match self.attempt(url, body.as_deref(), extra, timeout).await {
                Ok(v) => return Ok(v),
                Err(UpstreamError::Timeout) => return Err(UpstreamError::Timeout),
                Err(e) if e.is_our_side() => last = e,
                Err(e) => return Err(e),
            }
        }
        Err(last)
    }

    async fn attempt<T: DeserializeOwned>(
        &self,
        url: &str,
        body: Option<&str>,
        extra: &[(&str, String)],
        timeout: Duration,
    ) -> Result<T, UpstreamError> {
        let headers = Headers::new();
        let _ = headers.set("Accept", "application/json");
        let _ = headers.set("User-Agent", "wayback-wallet/0.1");
        for (k, v) in extra {
            let _ = headers.set(k, v);
        }
        let mut init = RequestInit::new();
        match body {
            Some(b) => {
                let _ = headers.set("Content-Type", "application/json");
                init.with_method(Method::Post).with_body(Some(b.into()));
            }
            None => {
                init.with_method(Method::Get);
            }
        }
        init.with_headers(headers);
        let req = Request::new_with_init(url, &init).map_err(|_| UpstreamError::Network)?;

        let controller = AbortController::default();
        let signal = controller.signal();
        let work = async move {
            let mut resp = Fetch::Request(req).send_with_signal(&signal).await.map_err(|_| UpstreamError::Network)?;
            let status = resp.status_code();
            if !(200..300).contains(&status) {
                return Err(UpstreamError::Status(status));
            }
            resp.json::<T>().await.map_err(|_| UpstreamError::Decode)
        };
        match select(Box::pin(work), Box::pin(Delay::from(timeout))).await {
            Either::Left((result, _)) => result,
            Either::Right(_) => {
                controller.abort();
                Err(UpstreamError::Timeout)
            }
        }
    }
}

/// REST API with ordered fallback providers and optional extra headers
/// (e.g. Blockfrost's `project_id`).
pub struct Rest<'a> {
    pub http: &'a Http,
    pub bases: &'static [&'static str],
    pub headers: Vec<(&'static str, String)>,
}

impl JsonSource for Rest<'_> {
    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, UpstreamError> {
        let mut last = UpstreamError::Network;
        for base in self.bases {
            match self.http.get_json(&format!("{base}{path}"), &self.headers).await {
                Ok(v) => return Ok(v),
                // Our side (provider trouble): try the next provider.
                Err(e) if e.is_our_side() => last = e,
                // Budget or a 4xx: another provider won't help.
                Err(e) => return Err(e),
            }
        }
        Err(last)
    }
}

/// A JSON-RPC endpoint (URL includes the key, e.g. Alchemy's), with
/// optional fallback URLs tried when the previous one fails on our side.
pub struct Rpc<'a> {
    pub http: &'a Http,
    pub urls: Vec<String>,
}

impl JsonSource for Rpc<'_> {
    async fn get<T: DeserializeOwned>(&self, _path: &str) -> Result<T, UpstreamError> {
        Err(UpstreamError::Status(405))
    }

    async fn post<T: DeserializeOwned>(&self, _path: &str, body: &serde_json::Value) -> Result<T, UpstreamError> {
        let mut last = UpstreamError::Network;
        for url in &self.urls {
            match self.http.post_json(url, body).await {
                Ok(v) => return Ok(v),
                Err(e) if e.is_our_side() => last = e,
                Err(e) => return Err(e),
            }
        }
        Err(last)
    }
}

#[cfg(test)]
pub mod fixtures {
    //! Recorded API responses under `test/fixtures/<chain>/`. A path maps to
    //! a file by replacing `/` with `__`; a missing file is a 404.
    use super::*;
    use std::path::PathBuf;

    pub struct Fixtures {
        pub dir: PathBuf,
    }

    impl Fixtures {
        pub fn new(chain: &str) -> Self {
            Self { dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test/fixtures").join(chain) }
        }
    }

    impl JsonSource for Fixtures {
        async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, UpstreamError> {
            let file = self.dir.join(format!("{}.json", path.trim_start_matches('/').replace('/', "__")));
            let text = std::fs::read_to_string(&file).map_err(|_| UpstreamError::Status(404))?;
            serde_json::from_str(&text).map_err(|_| UpstreamError::Decode)
        }
    }

    /// Fixtures keyed by a hash of the request, stored as
    /// `{"request": …, "response": …}` (no API keys). With `RECORD=1` and the
    /// live URL's env var set, missing fixtures are fetched once and saved.
    pub struct Recorded {
        dir: PathBuf,
        /// Live base: for GET, prefix + path + suffix; for POST, the URL.
        live: Option<(String, String)>,
        /// Header carrying the key on live calls (never stored).
        header: Option<(String, String)>,
    }

    impl Recorded {
        /// `live_env` names the env var holding the key; `make_live(key)`
        /// returns (prefix, suffix).
        pub fn new(chain: &str, live_env: &str, make_live: impl Fn(&str) -> (String, String)) -> Self {
            // An empty `live_env` means the provider needs no key.
            let live = (std::env::var("RECORD").as_deref() == Ok("1"))
                .then(|| if live_env.is_empty() { Some(String::new()) } else { std::env::var(live_env).ok() })
                .flatten()
                .map(|k| make_live(&k));
            Self { dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test/fixtures").join(chain), live, header: None }
        }

        /// Like `new`, but the key travels in header `name` on live calls.
        pub fn with_key_header(chain: &str, live_env: &str, base: &str, name: &str) -> Self {
            let mut r = Self::new(chain, live_env, |_| (base.to_string(), String::new()));
            if r.live.is_some() {
                r.header = std::env::var(live_env).ok().map(|k| (name.to_string(), k));
            }
            r
        }

        fn file(&self, request: &serde_json::Value) -> PathBuf {
            use sha2::{Digest, Sha256};
            let h = hex::encode(&Sha256::digest(request.to_string().as_bytes())[..8]);
            self.dir.join(format!("{h}.json"))
        }

        fn load_or_record(
            &self,
            request: serde_json::Value,
            fetch: impl FnOnce(&(String, String)) -> Result<serde_json::Value, String>,
        ) -> Result<serde_json::Value, UpstreamError> {
            let file = self.file(&request);
            if let Ok(text) = std::fs::read_to_string(&file) {
                let v: serde_json::Value = serde_json::from_str(&text).map_err(|_| UpstreamError::Decode)?;
                return Ok(v["response"].clone());
            }
            let Some(live) = &self.live else {
                eprintln!("missing fixture {} for {request}", file.display());
                return Err(UpstreamError::Status(404));
            };
            let response = fetch(live).map_err(|e| {
                eprintln!("live call failed: {e}");
                UpstreamError::Network
            })?;
            std::fs::create_dir_all(&self.dir).ok();
            let doc = serde_json::json!({ "request": request, "response": response });
            std::fs::write(&file, serde_json::to_string(&doc).unwrap()).unwrap();
            Ok(response)
        }
    }

    impl JsonSource for Recorded {
        async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, UpstreamError> {
            let request = serde_json::json!({ "method": "GET", "path": path });
            let v = self.load_or_record(request, |(pre, suf)| {
                std::thread::sleep(std::time::Duration::from_millis(400));
                let mut req = ureq::get(format!("{pre}{path}{suf}"));
                if let Some((k, v)) = &self.header {
                    req = req.header(k.as_str(), v.as_str());
                }
                match req.call() {
                    Ok(mut resp) => resp.body_mut().read_json().map_err(|e| e.to_string()),
                    // Record 404s too: some APIs answer "never seen" that way.
                    Err(ureq::Error::StatusCode(404)) => Ok(serde_json::json!({ "__status": 404 })),
                    Err(e) => Err(e.to_string()),
                }
            })?;
            if v.get("__status").and_then(|s| s.as_u64()) == Some(404) {
                return Err(UpstreamError::Status(404));
            }
            serde_json::from_value(v).map_err(|_| UpstreamError::Decode)
        }

        async fn post<T: DeserializeOwned>(&self, path: &str, body: &serde_json::Value) -> Result<T, UpstreamError> {
            let request = serde_json::json!({ "method": "POST", "path": path, "body": body });
            let v = self.load_or_record(request, |(url, _)| {
                std::thread::sleep(std::time::Duration::from_millis(400));
                ureq::post(url.as_str())
                    .send_json(body)
                    .map_err(|e| e.to_string())?
                    .body_mut()
                    .read_json()
                    .map_err(|e| e.to_string())
            })?;
            serde_json::from_value(v).map_err(|_| UpstreamError::Decode)
        }
    }
}
