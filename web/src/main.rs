mod api;
mod about;
mod cookie;
mod logo;
mod footer;
mod results;
mod turnstile;

use leptos::html::Div;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wayback_core::address_list::{parse_lines, submittable, Line, LineState, SubmitError};
use wayback_core::api::{AddressRef, LookupStartRequest, Quota};
use wayback_core::MAX_ADDRESSES;

/// First date with a Bitcoin block.
const MIN_DATE: &str = "2009-01-03";
const DAILY_LOOKUPS: u32 = 10;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}

/// `YYYY-MM-DD` for today in UTC, shifted by `offset_days`.
fn utc_date(offset_days: f64) -> String {
    let now = js_sys::Date::now() + offset_days * 86_400_000.0;
    let d = js_sys::Date::new(&now.into());
    format!("{:04}-{:02}-{:02}", d.get_utc_full_year(), d.get_utc_month() + 1, d.get_utc_date())
}

#[component]
fn App() -> impl IntoView {
    let (logo_choice, logo_preview) = logo::init();
    let logo_current = RwSignal::new(logo_choice);
    let saved = cookie::load();
    let (text, set_text) = signal(saved.join("\n"));
    let today = utc_date(0.0);
    let (date, set_date) = signal(utc_date(-1.0));
    let (show_usd, set_show_usd) = signal(true);
    // Line numbers to highlight after a rejected submit.
    let (flagged, set_flagged) = signal(Vec::<usize>::new());
    let (message, set_message) = signal(Option::<(bool, String)>::None);
    let (quota, set_quota) = signal(Option::<Quota>::None);
    let (captcha_token, set_captcha_token) = signal(String::new());
    let (busy, set_busy) = signal(false);
    let (lookup, set_lookup) = signal(Option::<results::ActiveLookup>::None);
    let captcha_ref = NodeRef::<Div>::new();

    spawn_local(async move {
        if let Ok(q) = api::quota().await {
            set_quota.set(Some(q));
        }
    });

    Effect::new(move |_| {
        if let Some(el) = captcha_ref.get() {
            turnstile::render(&el, move |token| set_captcha_token.set(token));
        }
    });

    let lines = Memo::new(move |_| parse_lines(&text.get()));
    let unique_count = Memo::new(move |_| {
        lines.with(|ls| ls.iter().filter(|l| matches!(l.state, LineState::Valid(_))).count())
    });
    // Enabled once there's input, a date and a captcha token; the submit
    // handler rejects invalid lines and highlights them.
    let can_submit = Memo::new(move |_| {
        lines.with(|ls| !ls.is_empty())
            && !date.get().is_empty()
            && !captcha_token.get().is_empty()
            && !busy.get()
            && quota.get().is_none_or(|q| q.remaining > 0)
    });
    // Why the button is disabled, in the order a user would fix it.
    let blocked_reason = Memo::new(move |_| -> Option<&'static str> {
        if busy.get() {
            None
        } else if quota.get().is_some_and(|q| q.remaining == 0) {
            Some("No lookups left today. The limit resets at 00:00 UTC.")
        } else if lines.with(|ls| ls.is_empty()) {
            Some("Enter at least one address.")
        } else if date.get().is_empty() {
            Some("Pick a date.")
        } else if captcha_token.get().is_empty() {
            Some("Complete the check above to enable Look up. If it doesn't appear, reload the page.")
        } else {
            None
        }
    });

    let today_for_submit = today.clone();
    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let d = date.get();
        if d.as_str() < MIN_DATE || d > today_for_submit {
            set_message.set(Some((false, format!("Pick a date between {MIN_DATE} and {today_for_submit}."))));
            return;
        }
        let addresses = match lines.with(|ls| submittable(ls)) {
            Ok(a) => a,
            Err(e) => {
                if let SubmitError::InvalidLines(ref ns) = e {
                    set_flagged.set(ns.clone());
                }
                set_message.set(Some((false, e.to_string())));
                return;
            }
        };
        set_flagged.set(Vec::new());

        let request = LookupStartRequest {
            turnstile_token: captcha_token.get(),
            date: d.clone(),
            addresses: addresses
                .iter()
                .map(|a| AddressRef { address: a.input.clone(), chain: a.chain })
                .collect(),
        };
        // Tokens are single-use: reset the widget whatever happens next.
        set_captcha_token.set(String::new());
        if let Some(el) = captcha_ref.get_untracked() {
            turnstile::reset(&el);
        }
        set_busy.set(true);
        set_message.set(None);
        set_lookup.set(None);

        spawn_local(async move {
            match api::lookup_start(&request).await {
                Ok(resp) => {
                    let kept = cookie::save(&addresses);
                    set_quota.set(Some(Quota { remaining: resp.remaining, reset_at: resp.reset_at }));
                    let n = request.addresses.len();
                    if kept < n {
                        set_message.set(Some((true, format!("Only the first {kept} addresses fit in the saved list."))));
                    }
                    let active = results::ActiveLookup::new(
                        resp.lookup_token,
                        d,
                        request.addresses,
                        show_usd.get_untracked(),
                    );
                    active.run();
                    set_lookup.set(Some(active));
                }
                Err(e) => {
                    if let api::CallError::Api(body) = &e {
                        if body.error == "daily_limit" {
                            set_quota.set(Some(Quota {
                                remaining: 0,
                                reset_at: body.reset_at.clone().unwrap_or_default(),
                            }));
                        }
                    }
                    set_message.set(Some((false, e.user_message())));
                }
            }
            set_busy.set(false);
        });
    };

    let on_clear = move |_| {
        cookie::clear();
        set_text.set(String::new());
        set_flagged.set(Vec::new());
        set_message.set(Some((true, "Saved addresses cleared.".into())));
    };

    view! {
        <header class="top">
            <h1>
                <span class="sr-only">"Wayback Chain"</span>
                <span class="logo" inner_html=move || logo::svg(&logo_current.get())></span>
            </h1>
            <p class="tagline">
                "See what any "<strong>"crypto wallet"</strong>" held on any date. Check historical "
                <strong>"blockchain balances and tokens"</strong>
                " for Bitcoin, Ethereum, Solana, XRP Ledger and Cardano addresses, read straight from the chain, with USD values. Free, no sign-up."
            </p>
            <p class="quota">
                "Lookups left today: "
                <strong>{move || quota.get().map_or("–".to_string(), |q| q.remaining.to_string())}</strong>
                {format!(" of {DAILY_LOOKUPS}")}
                <span class="muted">" · resets 00:00 UTC"</span>
            </p>
        </header>

        <main>
            <form on:submit=on_submit novalidate>
                <label for="addresses">"Addresses"</label>
                <p class="hint">
                    {format!("One per line, up to {MAX_ADDRESSES}. Bitcoin, Ethereum, Solana, XRP Ledger or Cardano; the chain is detected for you.")}
                </p>
                <textarea
                    id="addresses"
                    rows="8"
                    spellcheck="false"
                    autocomplete="off"
                    autocapitalize="off"
                    prop:value=move || text.get()
                    on:input=move |ev| {
                        set_text.set(event_target_value(&ev));
                        set_flagged.set(Vec::new());
                    }
                ></textarea>
                <p class="count" class:over=move || { unique_count.get() > MAX_ADDRESSES }>
                    {move || format!("{} / {MAX_ADDRESSES} unique addresses", unique_count.get())}
                </p>

                <ol class="lines">
                    <For
                        each=move || lines.get()
                        key=|l: &Line| (l.number, l.text.clone())
                        children=move |l: Line| {
                            let n = l.number;
                            view! { <LineRow line=l flagged=Signal::derive(move || flagged.with(|f| f.contains(&n))) /> }
                        }
                    />
                </ol>

                <label for="date">"Date"</label>
                <input
                    id="date"
                    type="date"
                    min=MIN_DATE
                    max=today.clone()
                    prop:value=move || date.get()
                    on:input=move |ev| set_date.set(event_target_value(&ev))
                />
                <p class="hint">"Balances as of " <strong>"23:59:59 UTC"</strong> " on this date."</p>

                <label class="check">
                    <input
                        type="checkbox"
                        prop:checked=move || show_usd.get()
                        on:change=move |ev| set_show_usd.set(event_target_checked(&ev))
                    />
                    "Show USD values"
                </label>

                <div class="captcha" node_ref=captcha_ref aria-label="Captcha"></div>

                <div class="actions">
                    <button
                        type="submit"
                        class="primary"
                        disabled=move || !can_submit.get()
                        aria-describedby="blocked-reason"
                    >
                        {move || if busy.get() { "Starting…" } else { "Look up" }}
                    </button>
                    <button type="button" class="secondary" on:click=on_clear>"Clear saved addresses"</button>
                </div>
                <p id="blocked-reason" class="hint" aria-live="polite">{move || blocked_reason.get()}</p>

                {move || message.get().map(|(ok, m)| view! {
                    <p class="message" class:error=!ok role="status">{m}</p>
                })}

            </form>
        </main>

        {move || lookup.get().map(|l| view! { <results::Results lookup=l /> })}

        <div class="about-wrap no-print"><about::About /></div>
        <footer::SiteFooter />
        {logo_preview.then(|| view! { <logo::LogoSwitcher current=logo_current /> })}
    }
}

#[component]
fn LineRow(line: Line, flagged: Signal<bool>) -> impl IntoView {
    let (badge, badge_class, note) = match &line.state {
        LineState::Valid(d) => {
            let note = (d.canonical != d.input && d.chain == wayback_core::Chain::Xrp)
                .then(|| format!("→ {}", d.canonical));
            (d.chain.ticker().to_string(), format!("badge {}", d.chain.ticker().to_lowercase()), note)
        }
        LineState::Duplicate { of_line, .. } => {
            ("dup".into(), "badge dup".into(), Some(format!("duplicate of line {of_line}; ignored")))
        }
        LineState::Invalid(e) => ("invalid".into(), "badge invalid".into(), Some(e.to_string())),
    };
    view! {
        <li class:flagged=move || flagged.get()>
            <span class="ln">{line.number}</span>
            <span class=badge_class>{badge}</span>
            <span class="addr">{line.text}</span>
            {note.map(|n| view! { <span class="note">{n}</span> })}
        </li>
    }
}
