//! Per-address progress rows and result cards.

use crate::api;
use leptos::prelude::*;
use leptos::task::spawn_local;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;
use wayback_core::api::{AddressRef, Holdings, LookupAddressRequest};

/// Address calls in flight at once.
const CONCURRENCY: usize = 4;

#[derive(Clone, Debug)]
pub enum RowStatus {
    Queued,
    Fetching,
    Done(Box<Holdings>),
    Failed { message: String, retryable: bool },
}

#[derive(Clone)]
pub struct Row {
    pub address: AddressRef,
    pub status: ArcRwSignal<RowStatus>,
}

#[derive(Clone)]
pub struct ActiveLookup {
    pub token: Arc<str>,
    pub date: String,
    pub rows: Vec<Row>,
    #[allow(dead_code)] // prices arrive in build step 8
    pub show_usd: bool,
}

impl ActiveLookup {
    pub fn new(token: String, date: String, addresses: Vec<AddressRef>, show_usd: bool) -> Self {
        let rows = addresses
            .into_iter()
            .map(|address| Row { address, status: ArcRwSignal::new(RowStatus::Queued) })
            .collect();
        Self { token: token.into(), date, rows, show_usd }
    }

    /// Fetches every row, `CONCURRENCY` at a time.
    pub fn run(&self) {
        let queue = Rc::new(RefCell::new(self.rows.iter().cloned().collect::<VecDeque<_>>()));
        for _ in 0..CONCURRENCY {
            let queue = queue.clone();
            let token = self.token.clone();
            spawn_local(async move {
                loop {
                    let next = queue.borrow_mut().pop_front();
                    let Some(row) = next else { break };
                    fetch_row(&token, &row).await;
                }
            });
        }
    }
}

async fn fetch_row(token: &str, row: &Row) {
    row.status.set(RowStatus::Fetching);
    let req = LookupAddressRequest { lookup_token: token.to_string(), address: row.address.address.clone() };
    let status = match api::lookup_address(&req).await {
        Ok(h) => RowStatus::Done(Box::new(h)),
        Err(e) => RowStatus::Failed {
            // Retrying only helps when the provider or network was at fault.
            retryable: !matches!(
                e.code(),
                Some("bad_token" | "chain_not_supported_yet" | "chain_not_configured" | "address_not_in_lookup" | "invalid_input")
            ),
            message: e.user_message(),
        },
    };
    row.status.set(status);
}

fn warning_text(code: &str) -> String {
    match code {
        "too_many_txs" => {
            "This address has too much activity since that date to replay in time, so no balance is shown.".into()
        }
        "tokens_incomplete" => "This address has a very long token history; some tokens may be missing.".into(),
        "token_cap_reached" => "This address has held more token types than we check; only the first 300 are shown.".into(),
        "sol_closed_accounts" => {
            "Solana token accounts that were closed before today can't be found, so tokens held in them on that date may be missing.".into()
        }
        "no_history" => "The XRP Ledger's recorded history starts in January 2013, so there's no data for this date.".into(),
        "xrp_includes_reserve" => "The XRP balance includes the reserve the account can't spend.".into(),
        "likely_exchange" => {
            "This address holds a very large balance and is probably an exchange account shared by many customers.".into()
        }
        other => format!("Note: {other}"),
    }
}

fn short(addr: &str) -> String {
    if addr.chars().count() <= 20 {
        return addr.to_string();
    }
    let head: String = addr.chars().take(8).collect();
    let tail: String = addr.chars().rev().take(6).collect::<Vec<_>>().into_iter().rev().collect();
    format!("{head}…{tail}")
}

fn group_digits(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[component]
pub fn Results(lookup: ActiveLookup) -> impl IntoView {
    let statuses: Vec<ArcRwSignal<RowStatus>> = lookup.rows.iter().map(|r| r.status.clone()).collect();
    let total = statuses.len();
    let done = move || {
        statuses
            .iter()
            .filter(|s| matches!(s.get(), RowStatus::Done(_) | RowStatus::Failed { .. }))
            .count()
    };
    let token = lookup.token.clone();

    view! {
        <section class="results" aria-live="polite">
            <h2>{format!("Holdings at 23:59:59 UTC on {}", lookup.date)}</h2>
            <p class="progress">{move || format!("{} of {total} addresses done", done())}</p>
            {lookup
                .rows
                .into_iter()
                .map(|row| view! { <Card row=row token=token.clone() /> })
                .collect_view()}
        </section>
    }
}

#[component]
fn Card(row: Row, token: Arc<str>) -> impl IntoView {
    let chain = row.address.chain;
    let addr = row.address.address.clone();
    let status = row.status.clone();
    let summary_status = row.status.clone();

    let summary = move || match summary_status.get() {
        RowStatus::Queued => view! { <span class="state muted">"queued"</span> }.into_any(),
        RowStatus::Fetching => view! { <span class="state fetching">"fetching…"</span> }.into_any(),
        RowStatus::Done(h) => match &h.native {
            Some(n) => view! { <span class="state amount">{format!("{} {}", n.amount, n.symbol)}</span> }.into_any(),
            None => view! { <span class="state warn">"no balance"</span> }.into_any(),
        },
        RowStatus::Failed { .. } => view! { <span class="state error">"error"</span> }.into_any(),
    };

    let body = move || match status.get() {
        RowStatus::Queued | RowStatus::Fetching => ().into_any(),
        RowStatus::Done(h) => view! {
            <div class="card-body">
                {h.native.clone().map(|n| view! {
                    <p class="native"><strong>{n.amount}</strong>" "{n.symbol}</p>
                })}
                <p class="meta">
                    {h.as_of_block.clone().map(|b| {
                        let label = if h.chain == wayback_core::Chain::Xrp { "Ledger" } else { "Block" };
                        format!("{label} {} · ", group_digits(&b))
                    })}
                    {h.address.clone()}
                </p>
                {h.warnings.iter().map(|w| view! { <p class="warning">{warning_text(w)}</p> }).collect_view()}
                {(!h.tokens.is_empty()).then(|| view! {
                    <table class="tokens">
                        <thead><tr><th>"Token"</th><th class="num">"Amount"</th></tr></thead>
                        <tbody>
                            {h.tokens.iter().map(|t| view! {
                                <tr><td title=t.contract.clone()>{t.symbol.clone()}</td><td class="num">{t.amount.clone()}</td></tr>
                            }).collect_view()}
                        </tbody>
                    </table>
                })}
            </div>
        }
        .into_any(),
        RowStatus::Failed { message, retryable } => {
            let row = row.clone();
            let token = token.clone();
            view! {
                <div class="card-body">
                    <p class="error-text">{message}</p>
                    {retryable.then(|| view! {
                        <button type="button" class="secondary small" on:click=move |_| {
                            let (row, token) = (row.clone(), token.clone());
                            spawn_local(async move { fetch_row(&token, &row).await });
                        }>"Retry"</button>
                    })}
                </div>
            }
            .into_any()
        }
    };

    view! {
        <details class="card" open>
            <summary>
                <span class=format!("badge {}", chain.ticker().to_lowercase())>{chain.ticker()}</span>
                <span class="addr" title=addr.clone()>{short(&addr)}</span>
                {summary}
            </summary>
            {body}
        </details>
    }
}
