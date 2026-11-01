mod cookie;

use leptos::prelude::*;
use wayback_core::address_list::{parse_lines, submittable, Line, LineState};
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
    let saved = cookie::load();
    let (text, set_text) = signal(saved.join("\n"));
    let today = utc_date(0.0);
    let (date, set_date) = signal(utc_date(-1.0));
    let (show_usd, set_show_usd) = signal(true);
    // Line numbers to highlight after a rejected submit.
    let (flagged, set_flagged) = signal(Vec::<usize>::new());
    let (message, set_message) = signal(Option::<(bool, String)>::None);

    let lines = Memo::new(move |_| parse_lines(&text.get()));
    let unique_count = Memo::new(move |_| {
        lines.with(|ls| ls.iter().filter(|l| matches!(l.state, LineState::Valid(_))).count())
    });
    // Enabled whenever there's something to submit; the submit handler rejects
    // invalid lines and highlights them (spec: "highlight the bad lines").
    // TODO(step 2): also require a completed Turnstile widget.
    let can_submit = Memo::new(move |_| lines.with(|ls| !ls.is_empty()) && !date.get().is_empty());

    let today_for_submit = today.clone();
    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let d = date.get();
        if d.as_str() < MIN_DATE || d > today_for_submit {
            set_message.set(Some((false, format!("Pick a date between {MIN_DATE} and {today_for_submit}."))));
            return;
        }
        match lines.with(|ls| submittable(ls)) {
            Err(e) => {
                if let wayback_core::address_list::SubmitError::InvalidLines(ref ns) = e {
                    set_flagged.set(ns.clone());
                }
                set_message.set(Some((false, e.to_string())));
            }
            Ok(addresses) => {
                set_flagged.set(Vec::new());
                // TODO(step 2–3): call /api/lookup/start and save the cookie
                // only once the lookup succeeds.
                let kept = cookie::save(&addresses);
                let mut msg = format!(
                    "{} address{} ready for {d} (USD {}). Lookups need the Worker, coming in build step 2.",
                    addresses.len(),
                    if addresses.len() == 1 { "" } else { "es" },
                    if show_usd.get() { "on" } else { "off" },
                );
                if kept < addresses.len() {
                    msg.push_str(&format!(" Only the first {kept} fit in the saved list."));
                }
                set_message.set(Some((true, msg)));
            }
        }
    };

    let on_clear = move |_| {
        cookie::clear();
        set_text.set(String::new());
        set_flagged.set(Vec::new());
        set_message.set(Some((true, "Saved addresses cleared.".into())));
    };

    view! {
        <header class="top">
            <h1>"Wayback Wallet"</h1>
            <p class="quota">
                "Lookups left today: " <strong>"–"</strong> {format!(" of {DAILY_LOOKUPS}")}
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

                <div class="captcha" aria-label="Captcha">"Captcha (Cloudflare Turnstile) goes here in build step 2."</div>

                <div class="actions">
                    <button type="submit" class="primary" disabled=move || !can_submit.get()>"Look up"</button>
                    <button type="button" class="secondary" on:click=on_clear>"Clear saved addresses"</button>
                </div>

                {move || message.get().map(|(ok, m)| view! {
                    <p class="message" class:error=!ok role="status">{m}</p>
                })}

                <p class="notice">
                    "We use two cookies: one keeps your address list on this device, one counts your daily lookups. "
                    <a href="privacy.html">"Privacy"</a>
                </p>
            </form>
        </main>

        <footer>
            "Data comes from public blockchain APIs and may be incomplete for very active addresses."
        </footer>
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
