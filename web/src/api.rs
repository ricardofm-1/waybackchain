//! Calls to the Worker. Same origin, so `hl_sid` rides along automatically.

use gloo_net::http::{Request, Response};
use wayback_core::api::{ApiError, LookupStartRequest, LookupStartResponse, Quota};

#[derive(Debug, Clone)]
pub enum CallError {
    /// The Worker answered with an error body.
    Api(ApiError),
    /// Network failure or an unexpected response.
    Transport(String),
}

impl CallError {
    /// A message for the user.
    pub fn user_message(&self) -> String {
        match self {
            CallError::Api(body) => match body.error.as_str() {
                "captcha_failed" => "The captcha check failed. Please complete it again.".into(),
                "captcha_unavailable" => "The captcha service is unreachable right now. Try again in a minute.".into(),
                "daily_limit" => "You've used all 10 lookups for today. The limit resets at 00:00 UTC.".into(),
                "invalid_input" => "The server rejected the addresses or date. Check the list and try again.".into(),
                other => format!("Something went wrong ({other}). Please try again."),
            },
            CallError::Transport(detail) => {
                leptos::logging::warn!("API call failed: {detail}");
                "Couldn't reach the server. Check your connection and try again.".into()
            }
        }
    }
}

async fn parse<T: serde::de::DeserializeOwned>(resp: Response) -> Result<T, CallError> {
    if resp.ok() {
        return resp.json().await.map_err(|e| CallError::Transport(e.to_string()));
    }
    let status = resp.status();
    match resp.json::<ApiError>().await {
        Ok(body) => Err(CallError::Api(body)),
        Err(_) => Err(CallError::Transport(format!("HTTP {status}"))),
    }
}

pub async fn quota() -> Result<Quota, CallError> {
    let resp = Request::get("/api/quota")
        .send()
        .await
        .map_err(|e| CallError::Transport(e.to_string()))?;
    parse(resp).await
}

pub async fn lookup_start(body: &LookupStartRequest) -> Result<LookupStartResponse, CallError> {
    let resp = Request::post("/api/lookup/start")
        .json(body)
        .map_err(|e| CallError::Transport(e.to_string()))?
        .send()
        .await
        .map_err(|e| CallError::Transport(e.to_string()))?;
    parse(resp).await
}
