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
    pub async fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T, UpstreamError> {
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
            match self.attempt(url, Duration::from_millis(remaining.min(PER_REQUEST_TIMEOUT_MS))).await {
                Ok(v) => return Ok(v),
                Err(UpstreamError::Timeout) => return Err(UpstreamError::Timeout),
                Err(e) if e.is_our_side() => last = e,
                Err(e) => return Err(e),
            }
        }
        Err(last)
    }

    async fn attempt<T: DeserializeOwned>(&self, url: &str, timeout: Duration) -> Result<T, UpstreamError> {
        let headers = Headers::new();
        let _ = headers.set("Accept", "application/json");
        let _ = headers.set("User-Agent", "wayback-wallet/0.1");
        let mut init = RequestInit::new();
        init.with_method(Method::Get).with_headers(headers);
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

/// Esplora-style REST API with ordered fallback providers.
pub struct Rest<'a> {
    pub http: &'a Http,
    pub bases: &'static [&'static str],
}

impl JsonSource for Rest<'_> {
    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, UpstreamError> {
        let mut last = UpstreamError::Network;
        for base in self.bases {
            match self.http.get_json(&format!("{base}{path}")).await {
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
}
