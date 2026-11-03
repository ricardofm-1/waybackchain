//! Per-address progress rows and result cards.

use crate::api;
use leptos::prelude::*;
use leptos::task::spawn_local;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;
use wayback_core::amount::add_decimal_str;
use wayback_core::api::{AddressRef, Holdings, LookupAddressRequest, TokenHolding};
use wayback_core::Chain;

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
            let with_usd = self.show_usd;
            spawn_local(async move {
                loop {
                    let next = queue.borrow_mut().pop_front();
                    let Some(row) = next else { break };
                    fetch_row(&token, &row, with_usd).await;
                }
            });
        }
    }
}

async fn fetch_row(token: &str, row: &Row, with_usd: bool) {
    row.status.set(RowStatus::Fetching);
    let req = LookupAddressRequest { lookup_token: token.to_string(), address: row.address.address.clone(), with_usd };
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
        "ada_payment_address_only" => {
            "This is one payment address. A Cardano wallet usually has several, and unwithdrawn staking rewards aren't included.".into()
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

/// "1234.56" → cents. USD strings from the Worker always have 2 decimals.
fn cents(usd: &str) -> Option<u128> {
    let (int, frac) = usd.split_once('.')?;
    Some(int.parse::<u128>().ok()? * 100 + frac.parse::<u128>().ok()?)
}

fn fmt_cents(c: u128) -> String {
    format!("${}.{:02}", group_digits(&(c / 100).to_string()), c % 100)
}

/// "$1,234.56"; a non-zero amount worth under a cent shows "<$0.01".
fn fmt_usd(usd: &str, amount: &str) -> String {
    match cents(usd) {
        Some(0) if amount.trim_start_matches(['0', '.']).is_empty() => "$0.00".into(),
        Some(0) => "<$0.01".into(),
        Some(c) => fmt_cents(c),
        None => usd.to_string(),
    }
}

/// Sum of every priced holding in a finished card.
fn card_cents(h: &Holdings) -> u128 {
    let native = h.native.as_ref().and_then(|n| n.usd.as_deref()).and_then(cents).unwrap_or(0);
    native + h.tokens.iter().filter_map(|t| t.usd.as_deref().and_then(cents)).sum::<u128>()
}

/// One asset summed across every finished address.
#[derive(Clone, Debug, PartialEq)]
struct AggRow {
    chain: Chain,
    symbol: String,
    /// Token contract / mint / currency.issuer; `None` for the native coin.
    contract: Option<String>,
    amount: String,
    cents: Option<u128>,
    addresses: usize,
}

/// Natives first in chain order, then tokens by USD value (unpriced last).
fn aggregate(cards: &[Holdings], include_unpriced: bool) -> Vec<AggRow> {
    let mut rows: Vec<AggRow> = Vec::new();
    let mut add = |chain: Chain, symbol: &str, contract: Option<&str>, amount: &str, usd: Option<&str>| {
        let c = usd.and_then(cents);
        match rows.iter_mut().find(|r| r.chain == chain && r.contract.as_deref() == contract) {
            Some(r) => {
                r.amount = add_decimal_str(&r.amount, amount).unwrap_or_else(|| r.amount.clone());
                r.cents = match (r.cents, c) {
                    (Some(a), Some(b)) => Some(a + b),
                    (a, b) => a.or(b),
                };
                r.addresses += 1;
            }
            None => rows.push(AggRow {
                chain,
                symbol: symbol.to_string(),
                contract: contract.map(str::to_string),
                amount: amount.to_string(),
                cents: c,
                addresses: 1,
            }),
        }
    };
    for h in cards {
        if let Some(n) = &h.native {
            add(h.chain, &n.symbol, None, &n.amount, n.usd.as_deref());
        }
        for t in h.tokens.iter().filter(|t| include_unpriced || t.usd.is_some()) {
            add(h.chain, &t.symbol, Some(&t.contract), &t.amount, t.usd.as_deref());
        }
    }
    let chain_rank = |c: Chain| Chain::ALL.iter().position(|x| *x == c).unwrap_or(0);
    rows.sort_by(|a, b| {
        (a.contract.is_some(), chain_rank(a.chain))
            .cmp(&(b.contract.is_some(), chain_rank(b.chain)))
            .then_with(|| b.cents.unwrap_or(0).cmp(&a.cents.unwrap_or(0)))
            .then_with(|| a.symbol.cmp(&b.symbol))
    });
    rows
}

/// One donut slice: a priced asset, or "Other" for the folded tail.
#[derive(Clone, Debug, PartialEq)]
struct Slice {
    label: String,
    cents: u128,
}

/// Up to 6 slices by USD value: all of them if 6 or fewer, else the top 5
/// plus "Other". Unpriced assets can't be placed on a USD chart.
fn slices(rows: &[AggRow]) -> Vec<Slice> {
    let mut priced: Vec<Slice> = rows
        .iter()
        .filter_map(|r| {
            let c = r.cents.filter(|c| *c > 0)?;
            let label = match &r.contract {
                None => r.symbol.clone(),
                Some(_) => format!("{} ({})", r.symbol, r.chain.ticker()),
            };
            Some(Slice { label, cents: c })
        })
        .collect();
    priced.sort_by(|a, b| b.cents.cmp(&a.cents).then_with(|| a.label.cmp(&b.label)));
    if priced.len() > 6 {
        let other: u128 = priced[5..].iter().map(|s| s.cents).sum();
        priced.truncate(5);
        priced.push(Slice { label: "Other".into(), cents: other });
    }
    priced
}

/// SVG path for a donut segment from angle `a0` to `a1` (radians, 0 = top,
/// clockwise) in a 200×200 box.
fn arc_path(a0: f64, a1: f64) -> String {
    const C: f64 = 100.0;
    const R: f64 = 92.0;
    const IR: f64 = 58.0;
    let pt = |r: f64, a: f64| (C + r * a.sin(), C - r * a.cos());
    let large = u8::from(a1 - a0 > std::f64::consts::PI);
    let (x0, y0) = pt(R, a0);
    let (x1, y1) = pt(R, a1);
    let (x2, y2) = pt(IR, a1);
    let (x3, y3) = pt(IR, a0);
    format!(
        "M{x0:.2} {y0:.2}A{R} {R} 0 {large} 1 {x1:.2} {y1:.2}L{x2:.2} {y2:.2}A{IR} {IR} 0 {large} 0 {x3:.2} {y3:.2}Z"
    )
}

fn share(part: u128, whole: u128) -> String {
    let tenths = (part * 1000 + whole / 2) / whole.max(1);
    format!("{}.{}%", tenths / 10, tenths % 10)
}

/// Donut of the USD split, shown once every address has finished so slice
/// colors are assigned once and never repaint mid-load.
#[component]
fn Donut(rows: Vec<AggRow>) -> impl IntoView {
    let parts = slices(&rows);
    let total: u128 = parts.iter().map(|s| s.cents).sum();
    let active = RwSignal::new(Option::<usize>::None);
    let n = parts.len();
    // Fixed categorical order by rank; "Other" is neutral gray.
    let color = move |i: usize, label: &str| {
        if label == "Other" { "var(--cat-other)".to_string() } else { format!("var(--cat-{})", i + 1) }
    };
    let mut angle = 0.0f64;
    let geometry: Vec<(String, f64)> = parts
        .iter()
        .map(|s| {
            let sweep = std::f64::consts::TAU * s.cents as f64 / total.max(1) as f64;
            let path = arc_path(angle, angle + sweep);
            angle += sweep;
            (path, sweep)
        })
        .collect();
    let aria = parts.iter().map(|s| format!("{} {}", s.label, share(s.cents, total))).collect::<Vec<_>>().join(", ");
    let center_parts = parts.clone();
    let value_parts = parts.clone();

    view! {
        <div class="donut">
            <svg viewBox="0 0 200 200" role="img" aria-label=format!("USD split: {aria}")>
                {if n == 1 {
                    // A single asset is the whole ring.
                    view! { <circle cx="100" cy="100" r="75" fill="none" stroke=color(0, &parts[0].label) stroke-width="34" /> }.into_any()
                } else {
                    parts
                        .iter()
                        .zip(geometry)
                        .enumerate()
                        .map(|(i, (s, (d, _)))| {
                            let fill = color(i, &s.label);
                            view! {
                                <path
                                    d=d
                                    fill=fill
                                    class="slice"
                                    class:dim=move || active.get().is_some_and(|a| a != i)
                                    on:mouseenter=move |_| active.set(Some(i))
                                    on:mouseleave=move |_| active.set(None)
                                    on:click=move |_| active.update(|a| *a = if *a == Some(i) { None } else { Some(i) })
                                />
                            }
                        })
                        .collect_view()
                        .into_any()
                }}
                <text x="100" y="94" class="donut-label">
                    {move || active.get().map_or("Total".to_string(), |i| center_parts[i].label.clone())}
                </text>
                <text x="100" y="116" class="donut-value">
                    {move || match active.get() {
                        Some(i) => share(value_parts[i].cents, total),
                        None => fmt_cents(total),
                    }}
                </text>
            </svg>
            <ul class="donut-legend">
                {parts
                    .iter()
                    .enumerate()
                    .map(|(i, s)| {
                        let swatch = format!("background: {}", color(i, &s.label));
                        view! {
                            <li
                                class:active=move || active.get() == Some(i)
                                on:mouseenter=move |_| active.set(Some(i))
                                on:mouseleave=move |_| active.set(None)
                            >
                                <span class="swatch" style=swatch></span>
                                <span class="legend-name">{s.label.clone()}</span>
                                <span class="legend-share">{share(s.cents, total)}</span>
                                <span class="legend-usd">{fmt_cents(s.cents)}</span>
                            </li>
                        }
                    })
                    .collect_view()}
            </ul>
        </div>
    }
}

#[component]
fn Summary(
    cards: Signal<Vec<Holdings>>,
    left_out: Signal<usize>,
    all_done: Signal<bool>,
    show_usd: bool,
    show_unpriced: RwSignal<bool>,
) -> impl IntoView {
    let rows = move || aggregate(&cards.get(), !show_usd || show_unpriced.get());
    let total = move || cards.get().iter().map(card_cents).sum::<u128>();
    view! {
        <div class="summary">
            <div class="summary-head">
                <h3>"Totals across all addresses"</h3>
                {show_usd.then(|| view! {
                    <p class="grand-total">"≈ " <strong>{move || fmt_cents(total())}</strong></p>
                })}
            </div>
            {move || {
                if !show_usd {
                    return ().into_any();
                }
                if !all_done.get() {
                    return view! { <p class="hint">"The chart appears when every address has finished."</p> }.into_any();
                }
                let rows = aggregate(&cards.get(), false);
                if slices(&rows).is_empty() {
                    return ().into_any();
                }
                view! { <Donut rows=rows /> }.into_any()
            }}
            {move || {
                let rows = rows();
                if rows.is_empty() {
                    return view! { <p class="hint">"Totals appear as addresses finish."</p> }.into_any();
                }
                view! {
                    <div class="table-scroll">
                        <table class="tokens summary-table">
                            <thead><tr>
                                <th>"Asset"</th>
                                <th class="num">"Amount"</th>
                                {show_usd.then(|| view! { <th class="num">"USD"</th> })}
                                <th class="num">"Addresses"</th>
                            </tr></thead>
                            <tbody>
                                {rows.into_iter().map(|r| {
                                    let title = r.contract.clone().unwrap_or_else(|| "native".into());
                                    let usd = match r.cents {
                                        Some(c) => fmt_usd(&format!("{}.{:02}", c / 100, c % 100), &r.amount),
                                        None => "–".into(),
                                    };
                                    view! {
                                        <tr class:native-row=r.contract.is_none()>
                                            <td title=title>
                                                <span class=format!("badge mini {}", r.chain.ticker().to_lowercase())>{r.chain.ticker()}</span>
                                                " "{r.symbol}
                                            </td>
                                            <td class="num">{r.amount}</td>
                                            {show_usd.then(|| view! { <td class="num">{usd}</td> })}
                                            <td class="num">{r.addresses}</td>
                                        </tr>
                                    }
                                }).collect_view()}
                            </tbody>
                        </table>
                    </div>
                }
                .into_any()
            }}
            {move || (left_out.get() > 0).then(|| {
                let n = left_out.get();
                view! { <p class="hint">{format!("{n} address{} not included (error, or too much activity to replay).", if n == 1 { "" } else { "es" })}</p> }
            })}
            {show_usd.then(|| view! {
                <label class="check small">
                    <input
                        type="checkbox"
                        prop:checked=move || show_unpriced.get()
                        on:change=move |ev| show_unpriced.set(event_target_checked(&ev))
                    />
                    "Show unpriced tokens"
                </label>
                <p class="hint">"Indicative prices from DefiLlama on that date. Tokens without a price (often airdrop spam) are hidden and left out of the total."</p>
            })}
        </div>
    }
}

#[component]
pub fn Results(lookup: ActiveLookup) -> impl IntoView {
    let statuses: Vec<ArcRwSignal<RowStatus>> = lookup.rows.iter().map(|r| r.status.clone()).collect();
    let total = statuses.len();
    let done_statuses = statuses.clone();
    let done = move || {
        done_statuses
            .iter()
            .filter(|s| matches!(s.get(), RowStatus::Done(_) | RowStatus::Failed { .. }))
            .count()
    };
    let done_count = Signal::derive(done.clone());
    let all_done = Signal::derive(move || done_count.get() == total);
    let card_statuses = statuses.clone();
    let cards = Signal::derive(move || {
        card_statuses
            .iter()
            .filter_map(|s| match s.get() {
                RowStatus::Done(h) => Some(*h),
                _ => None,
            })
            .collect::<Vec<_>>()
    });
    let left_out = Signal::derive(move || {
        statuses
            .iter()
            .filter(|s| match s.get() {
                RowStatus::Failed { .. } => true,
                RowStatus::Done(h) => h.native.is_none(),
                _ => false,
            })
            .count()
    });
    let show_usd = lookup.show_usd;
    let show_unpriced = RwSignal::new(false);
    let token = lookup.token.clone();

    view! {
        <section class="results" aria-live="polite">
            <h2>{format!("Holdings at 23:59:59 UTC on {}", lookup.date)}</h2>
            <p class="progress">{move || format!("{} of {total} addresses done", done_count.get())}</p>
            <Summary cards=cards left_out=left_out all_done=all_done show_usd=show_usd show_unpriced=show_unpriced />
            <h3 class="detail-head">"By address"</h3>
            {lookup
                .rows
                .into_iter()
                .map(|row| view! { <Card row=row token=token.clone() show_usd=show_usd show_unpriced=show_unpriced /> })
                .collect_view()}
        </section>
    }
}

#[component]
fn Card(row: Row, token: Arc<str>, show_usd: bool, show_unpriced: RwSignal<bool>) -> impl IntoView {
    let chain = row.address.chain;
    let addr = row.address.address.clone();
    let status = row.status.clone();
    let summary_status = row.status.clone();

    let summary = move || match summary_status.get() {
        RowStatus::Queued => view! { <span class="state muted">"queued"</span> }.into_any(),
        RowStatus::Fetching => view! { <span class="state fetching">"fetching…"</span> }.into_any(),
        RowStatus::Done(h) => match (&h.native, show_usd) {
            (Some(_), true) if card_cents(&h) > 0 => {
                view! { <span class="state amount">{fmt_cents(card_cents(&h))}</span> }.into_any()
            }
            (Some(n), _) => view! { <span class="state amount">{format!("{} {}", n.amount, n.symbol)}</span> }.into_any(),
            (None, _) => view! { <span class="state warn">"no balance"</span> }.into_any(),
        },
        RowStatus::Failed { .. } => view! { <span class="state error">"error"</span> }.into_any(),
    };

    let body = move || match status.get() {
        RowStatus::Queued | RowStatus::Fetching => ().into_any(),
        RowStatus::Done(h) => {
            let tokens = h.tokens.clone();
            let visible = move || -> Vec<TokenHolding> {
                tokens.iter().filter(|t| !show_usd || show_unpriced.get() || t.usd.is_some()).cloned().collect()
            };
            let unpriced = h.tokens.iter().filter(|t| t.usd.is_none()).count();
            view! {
                <div class="card-body">
                    {h.native.clone().map(|n| view! {
                        <p class="native">
                            <strong>{n.amount.clone()}</strong>" "{n.symbol}
                            {(show_usd).then(|| n.usd.clone().map(|u| view! { <span class="usd">{format!(" ≈ {}", fmt_usd(&u, &n.amount))}</span> }))}
                        </p>
                    })}
                    <p class="meta">
                        {h.as_of_block.clone().map(|b| {
                            let label = if h.chain == wayback_core::Chain::Xrp { "Ledger" } else { "Block" };
                            format!("{label} {} · ", group_digits(&b))
                        })}
                        {h.address.clone()}
                    </p>
                    {h.warnings.iter().map(|w| view! { <p class="warning">{warning_text(w)}</p> }).collect_view()}
                    {move || {
                        let rows = visible();
                        (!rows.is_empty()).then(|| view! {
                            <table class="tokens">
                                <thead><tr>
                                    <th>"Token"</th>
                                    <th class="num">"Amount"</th>
                                    {show_usd.then(|| view! { <th class="num">"USD"</th> })}
                                </tr></thead>
                                <tbody>
                                    {rows.into_iter().map(|t| view! {
                                        <tr>
                                            <td title=t.contract.clone()>{t.symbol.clone()}</td>
                                            <td class="num">{t.amount.clone()}</td>
                                            {show_usd.then(|| view! {
                                                <td class="num">{t.usd.as_deref().map(|u| fmt_usd(u, &t.amount)).unwrap_or_else(|| "–".into())}</td>
                                            })}
                                        </tr>
                                    }).collect_view()}
                                </tbody>
                            </table>
                        })
                    }}
                    {move || (show_usd && !show_unpriced.get() && unpriced > 0).then(|| view! {
                        <p class="hint">{format!("{unpriced} unpriced token{} hidden", if unpriced == 1 { "" } else { "s" })}</p>
                    })}
                </div>
            }
            .into_any()
        }
        RowStatus::Failed { message, retryable } => {
            let row = row.clone();
            let token = token.clone();
            view! {
                <div class="card-body">
                    <p class="error-text">{message}</p>
                    {retryable.then(|| view! {
                        <button type="button" class="secondary small" on:click=move |_| {
                            let (row, token) = (row.clone(), token.clone());
                            spawn_local(async move { fetch_row(&token, &row, show_usd).await });
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
