//! Visual "flavours" for trying out a rebrand locally. A flavour is a
//! `data-flavour` attribute on <html>; all styling lives in flavours.css.
//!
//! Pick one with `?theme=time-circuits|neon-1985|clock-tower` (or
//! `?theme=default`). The choice is remembered in this browser, and a small
//! switcher appears once a theme has been chosen. Visitors who never use the
//! parameter see the normal site.

use leptos::prelude::*;

pub const FLAVOURS: &[(&str, &str)] = &[
    ("default", "Current"),
    ("time-circuits", "Time Circuits"),
    ("neon-1985", "Neon 1985"),
    ("clock-tower", "Clock Tower 1955"),
];

const STORAGE_KEY: &str = "wbc-flavour";

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

fn query_theme() -> Option<String> {
    let search = web_sys::window()?.location().search().ok()?;
    search
        .trim_start_matches('?')
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .find(|(k, _)| *k == "theme")
        .map(|(_, v)| v.to_string())
        .filter(|v| FLAVOURS.iter().any(|(id, _)| id == v))
}

fn apply(id: &str) {
    let Some(root) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.document_element()) else { return };
    if id == "default" {
        let _ = root.remove_attribute("data-flavour");
    } else {
        let _ = root.set_attribute("data-flavour", id);
    }
}

/// Applies the flavour from the URL or from this browser's last choice.
/// Returns it, or `None` if theming was never used here (no switcher).
pub fn init() -> Option<String> {
    let chosen = query_theme().or_else(|| storage().and_then(|s| s.get_item(STORAGE_KEY).ok().flatten()))?;
    if let Some(s) = storage() {
        let _ = s.set_item(STORAGE_KEY, &chosen);
    }
    apply(&chosen);
    Some(chosen)
}

fn choose(id: &str) {
    apply(id);
    if let Some(s) = storage() {
        let _ = s.set_item(STORAGE_KEY, id);
    }
    // Keep the URL shareable: ?theme=<id>
    if let Some(w) = web_sys::window() {
        if let Ok(h) = w.history() {
            let _ = h.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&format!("?theme={id}")));
        }
    }
}

#[component]
pub fn FlavourSwitcher(initial: String) -> impl IntoView {
    let current = RwSignal::new(initial);
    view! {
        <nav class="flavour-switcher no-print" aria-label="Design preview">
            {FLAVOURS
                .iter()
                .map(|(id, label)| {
                    let id = *id;
                    view! {
                        <button
                            type="button"
                            class:active=move || current.get() == id
                            aria-pressed=move || (current.get() == id).to_string()
                            on:click=move |_| {
                                choose(id);
                                current.set(id.to_string());
                            }
                        >
                            {*label}
                        </button>
                    }
                })
                .collect_view()}
        </nav>
    }
}

const MONTHS: [&str; 12] = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];

/// "2024-03-02" → "MAR 02 2024".
pub fn display_date(iso: &str) -> String {
    let mut p = iso.split('-');
    match (p.next(), p.next().and_then(|m| m.parse::<usize>().ok()), p.next()) {
        (Some(y), Some(m @ 1..=12), Some(d)) => format!("{} {d} {y}", MONTHS[m - 1]),
        _ => iso.to_string(),
    }
}

/// One row of the time-circuit display: MONTH DAY YEAR AM/PM HOUR MIN.
#[component]
fn CircuitRow(class: &'static str, label: &'static str, value: Signal<Option<(String, u32, u32)>>) -> impl IntoView {
    // value: (iso date, hour 0-23, minute)
    let seg = move |i: usize| {
        value.get().map_or_else(
            || "--".to_string(),
            |(iso, h, m)| {
                let mut p = iso.split('-');
                let (y, mo, d) = (p.next().unwrap_or("----"), p.next().unwrap_or("--"), p.next().unwrap_or("--"));
                let month = mo.parse::<usize>().ok().filter(|m| (1..=12).contains(m)).map_or("---", |m| MONTHS[m - 1]);
                let h12 = if h % 12 == 0 { 12 } else { h % 12 };
                match i {
                    0 => month.to_string(),
                    1 => d.to_string(),
                    2 => y.to_string(),
                    3 => if h < 12 { "AM".into() } else { "PM".into() },
                    4 => format!("{h12:02}"),
                    _ => format!("{m:02}"),
                }
            },
        )
    };
    let heads = ["MONTH", "DAY", "YEAR", "AM/PM", "HOUR", "MIN"];
    view! {
        <div class=format!("tc-row {class}")>
            <div class="tc-segs">
                {heads.iter().enumerate().map(|(i, h)| view! {
                    <div class="tc-cell">
                        <span class="tc-head">{*h}</span>
                        <span class="seg">{move || seg(i)}</span>
                    </div>
                }).collect_view()}
            </div>
            <span class="tc-label">{label}</span>
        </div>
    }
}

/// The time-circuit readout shown by the Time Circuits flavour (hidden
/// otherwise): destination = chosen date at 23:59, present = now (UTC),
/// last = the last looked-up date.
#[component]
pub fn TimeCircuits(date: Signal<String>, last: Signal<Option<String>>) -> impl IntoView {
    let now = js_sys::Date::new_0();
    let present = format!("{:04}-{:02}-{:02}", now.get_utc_full_year(), now.get_utc_month() + 1, now.get_utc_date());
    let present = (present, now.get_utc_hours(), now.get_utc_minutes());
    let dest = Signal::derive(move || Some(date.get()).filter(|d| !d.is_empty()).map(|d| (d, 23, 59)));
    let pres = Signal::derive(move || Some(present.clone()));
    let lastv = Signal::derive(move || last.get().map(|d| (d, 23, 59)));
    view! {
        <div class="time-circuits" aria-hidden="true">
            <CircuitRow class="dest" label="Destination date" value=dest />
            <CircuitRow class="present" label="Present time (UTC)" value=pres />
            <CircuitRow class="last" label="Last lookup" value=lastv />
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::display_date;

    #[test]
    fn dates() {
        assert_eq!(display_date("2024-03-02"), "MAR 02 2024");
        assert_eq!(display_date("1985-10-26"), "OCT 26 1985");
        assert_eq!(display_date("nope"), "nope");
    }
}
