//! The Wayback Chain wordmark: three original 1980s-style options for
//! review, all with the sunset colours and the badge layout. Compare them
//! with `?logo=chrome-badge|neon-badge|dark-badge`; the choice is
//! remembered in this browser and a small switcher appears. Without the
//! parameter the site shows the default option and no switcher.
//!
//! Static SVG markup (no user data). Text is sized with textLength so each
//! logo renders the same whatever heavy font the system has. The page
//! heading keeps the plain text "Wayback Chain" for screen readers and
//! search engines; these SVGs are aria-hidden.

use leptos::prelude::*;

pub const LOGOS: &[(&str, &str)] = &[("chrome-badge", "D · Chrome badge"), ("neon-badge", "E · Neon badge"), ("dark-badge", "F · Dark badge")];
pub const DEFAULT: &str = "chrome-badge";
const STORAGE_KEY: &str = "wbc-logo";

/// Shared top line for D, E and F: full-width slanted WAYBACK in the
/// sunset gradient with a light horizon band and a dark outline, plus an
/// outer glow in `glow` colour. Gradient/filter ids are prefixed per logo.
fn wayback(p: &str, glow: &str) -> String {
    format!(
        r##"<defs>
  <linearGradient id="{p}Sun" x1="0" y1="0" x2="0" y2="1">
    <stop offset="0" stop-color="#fff59a"/><stop offset="0.42" stop-color="#ffb03a"/>
    <stop offset="0.5" stop-color="#ffe9c2"/><stop offset="0.54" stop-color="#ff8a3d"/>
    <stop offset="1" stop-color="#ff2fb3"/>
  </linearGradient>
  <linearGradient id="{p}SunH" x1="0" y1="0" x2="1" y2="0">
    <stop offset="0" stop-color="#ffe45c"/><stop offset="0.5" stop-color="#ff8a3d"/><stop offset="1" stop-color="#ff2fb3"/>
  </linearGradient>
  <linearGradient id="{p}Chrome" x1="0" y1="0" x2="0" y2="1">
    <stop offset="0" stop-color="#ffffff"/><stop offset="0.42" stop-color="#dfe5ff"/><stop offset="0.5" stop-color="#5a66e8"/>
    <stop offset="0.56" stop-color="#9aa6ff"/><stop offset="0.8" stop-color="#eef1ff"/><stop offset="1" stop-color="#ffffff"/>
  </linearGradient>
  <linearGradient id="{p}Streak" x1="0" y1="0" x2="1" y2="0">
    <stop offset="0" stop-color="#19e3ff" stop-opacity="0"/><stop offset="1" stop-color="#19e3ff"/>
  </linearGradient>
  <linearGradient id="{p}StreakR" x1="1" y1="0" x2="0" y2="0">
    <stop offset="0" stop-color="#19e3ff" stop-opacity="0"/><stop offset="1" stop-color="#19e3ff"/>
  </linearGradient>
  <filter id="{p}Glow" x="-20%" y="-50%" width="140%" height="200%">
    <feGaussianBlur stdDeviation="5" result="b"/><feMerge><feMergeNode in="b"/><feMergeNode in="SourceGraphic"/></feMerge>
  </filter>
</defs>
<g transform="skewX(-10)" font-family="'Arial Black','Arial Bold',Impact,'Helvetica Neue',sans-serif" font-weight="900">
  <text x="40" y="100" font-size="94" textLength="500" lengthAdjust="spacingAndGlyphs" fill="none" stroke="{glow}" stroke-width="11" stroke-linejoin="round" filter="url(#{p}Glow)" opacity="0.55">WAYBACK</text>
  <text x="40" y="100" font-size="94" textLength="500" lengthAdjust="spacingAndGlyphs" fill="url(#{p}Sun)" stroke="#1b0a33" stroke-width="6.5" stroke-linejoin="round" paint-order="stroke">WAYBACK</text>
</g>"##
    )
}

const BADGE: &str = "M190 122 L420 122 L404 178 L174 178 Z";
const CHAIN_TEXT: &str = r##"transform="skewX(-10)" font-family="'Arial Black','Arial Bold',Impact,'Helvetica Neue',sans-serif" font-weight="900""##;

fn svg_open() -> &'static str {
    r#"<svg viewBox="0 0 560 190" xmlns="http://www.w3.org/2000/svg" aria-hidden="true" focusable="false">"#
}

/// D · Chrome badge: cyan-glow sunset WAYBACK; chrome badge with dark
/// CHAIN; cyan neon rules either side.
fn chrome_badge() -> String {
    format!(
        r##"{open}{top}
<path d="{BADGE}" fill="url(#dChrome)" stroke="#1b0a33" stroke-width="4"/>
<g {CHAIN_TEXT}><text x="226" y="166" font-size="40" textLength="190" lengthAdjust="spacing" fill="#1b0a33">CHAIN</text></g>
<g fill="#19e3ff" filter="url(#dGlow)">
  <rect x="44" y="148" width="116" height="4" rx="2"/><rect x="432" y="148" width="104" height="4" rx="2"/>
</g>
</svg>"##,
        open = svg_open(),
        top = wayback("d", "#19e3ff"),
    )
}

/// E · Neon badge: sunset WAYBACK; outlined cyan neon badge with glowing
/// CHAIN; speed streaks either side.
fn neon_badge() -> String {
    format!(
        r##"{open}{top}
<path d="{BADGE}" fill="rgba(10,4,22,0.85)" stroke="#19e3ff" stroke-width="4" filter="url(#eGlow)"/>
<g {CHAIN_TEXT}>
  <text x="226" y="166" font-size="40" textLength="190" lengthAdjust="spacing" fill="none" stroke="#19e3ff" stroke-width="6" opacity="0.6" filter="url(#eGlow)">CHAIN</text>
  <text x="226" y="166" font-size="40" textLength="190" lengthAdjust="spacing" fill="#d9fbff">CHAIN</text>
</g>
<g fill="url(#eStreak)">
  <rect x="40" y="134" width="118" height="6" rx="3"/><rect x="74" y="148" width="84" height="6" rx="3"/><rect x="104" y="162" width="54" height="6" rx="3"/>
</g>
<g fill="url(#eStreakR)">
  <rect x="418" y="134" width="118" height="6" rx="3"/><rect x="418" y="148" width="84" height="6" rx="3"/><rect x="418" y="162" width="54" height="6" rx="3"/>
</g>
</svg>"##,
        open = svg_open(),
        top = wayback("e", "#19e3ff"),
    )
}

/// F · Dark badge: pink-glow sunset WAYBACK; dark badge with a cyan border
/// and CHAIN in the sunset gradient; small chain links either side.
fn dark_badge() -> String {
    format!(
        r##"{open}{top}
<path d="{BADGE}" fill="#120822" stroke="#19e3ff" stroke-width="3.5" filter="url(#fGlow)"/>
<g {CHAIN_TEXT}><text x="226" y="166" font-size="40" textLength="190" lengthAdjust="spacing" fill="url(#fSunH)" stroke="#1b0a33" stroke-width="2" paint-order="stroke">CHAIN</text></g>
<g fill="none" stroke="#19e3ff" stroke-width="4.5" filter="url(#fGlow)">
  <rect x="96" y="140" width="34" height="19" rx="9.5"/><rect x="118" y="140" width="34" height="19" rx="9.5"/>
  <rect x="426" y="140" width="34" height="19" rx="9.5"/><rect x="448" y="140" width="34" height="19" rx="9.5"/>
</g>
</svg>"##,
        open = svg_open(),
        top = wayback("f", "#ff2fb3"),
    )
}

pub fn svg(id: &str) -> String {
    match id {
        "neon-badge" => neon_badge(),
        "dark-badge" => dark_badge(),
        _ => chrome_badge(),
    }
}

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

fn query_logo() -> Option<String> {
    let search = web_sys::window()?.location().search().ok()?;
    search
        .trim_start_matches('?')
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .find(|(k, _)| *k == "logo")
        .map(|(_, v)| v.to_string())
        .filter(|v| LOGOS.iter().any(|(id, _)| id == v))
}

/// The logo to show, and whether the preview switcher should appear.
pub fn init() -> (String, bool) {
    let stored = storage()
        .and_then(|s| s.get_item(STORAGE_KEY).ok().flatten())
        .filter(|v| LOGOS.iter().any(|(id, _)| id == v));
    match query_logo().or(stored) {
        Some(id) => {
            if let Some(s) = storage() {
                let _ = s.set_item(STORAGE_KEY, &id);
            }
            (id, true)
        }
        None => (DEFAULT.to_string(), false),
    }
}

#[component]
pub fn LogoSwitcher(current: RwSignal<String>) -> impl IntoView {
    view! {
        <nav class="logo-switcher no-print" aria-label="Logo preview">
            {LOGOS
                .iter()
                .map(|(id, label)| {
                    let id = *id;
                    view! {
                        <button
                            type="button"
                            class:active=move || current.get() == id
                            aria-pressed=move || (current.get() == id).to_string()
                            on:click=move |_| {
                                current.set(id.to_string());
                                if let Some(s) = storage() {
                                    let _ = s.set_item(STORAGE_KEY, id);
                                }
                                if let Some(h) = web_sys::window().and_then(|w| w.history().ok()) {
                                    let _ = h.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&format!("?logo={id}")));
                                }
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
