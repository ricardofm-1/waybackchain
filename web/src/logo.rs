//! The Wayback Chain wordmark: three original 1980s-style options for
//! review. Compare them with `?logo=streak|chrome|tube`; the choice is
//! remembered in this browser and a small switcher appears. Without the
//! parameter the site shows the default option and no switcher.
//!
//! Static SVG markup (no user data). Text is sized with textLength so each
//! logo renders the same whatever heavy font the system has. The page
//! heading keeps the plain text "Wayback Chain" for screen readers and
//! search engines; these SVGs are aria-hidden.

use leptos::prelude::*;

pub const LOGOS: &[(&str, &str)] = &[("streak", "A · Streak"), ("chrome", "B · Chrome"), ("tube", "C · Neon tube")];
pub const DEFAULT: &str = "streak";
const STORAGE_KEY: &str = "wbc-logo";

/// A · Streak: sunset-gradient slanted lettering with a light horizon band,
/// cyan glow and trailing speed streaks; chrome CHAIN with a chain link.
const STREAK: &str = r##"<svg viewBox="0 0 560 190" xmlns="http://www.w3.org/2000/svg" aria-hidden="true" focusable="false">
<defs>
  <linearGradient id="aSun" x1="0" y1="0" x2="0" y2="1">
    <stop offset="0" stop-color="#fff59a"/><stop offset="0.42" stop-color="#ffb03a"/>
    <stop offset="0.5" stop-color="#ffe9c2"/><stop offset="0.54" stop-color="#ff8a3d"/>
    <stop offset="1" stop-color="#ff2fb3"/>
  </linearGradient>
  <linearGradient id="aChrome" x1="0" y1="0" x2="0" y2="1">
    <stop offset="0" stop-color="#ffffff"/><stop offset="0.45" stop-color="#cfd8ff"/><stop offset="0.5" stop-color="#4c5cf2"/>
    <stop offset="0.6" stop-color="#e7ecff"/><stop offset="1" stop-color="#ffffff"/>
  </linearGradient>
  <linearGradient id="aStreak" x1="0" y1="0" x2="1" y2="0">
    <stop offset="0" stop-color="#19e3ff" stop-opacity="0"/><stop offset="1" stop-color="#19e3ff"/>
  </linearGradient>
  <filter id="aGlow" x="-20%" y="-50%" width="140%" height="200%">
    <feGaussianBlur stdDeviation="4" result="b"/><feMerge><feMergeNode in="b"/><feMergeNode in="SourceGraphic"/></feMerge>
  </filter>
</defs>
<g fill="url(#aStreak)">
  <rect x="0" y="30" width="96" height="7" rx="3.5"/><rect x="22" y="47" width="74" height="7" rx="3.5"/>
  <rect x="8" y="64" width="88" height="7" rx="3.5"/><rect x="36" y="81" width="60" height="7" rx="3.5"/>
  <rect x="52" y="98" width="44" height="7" rx="3.5"/>
</g>
<g transform="skewX(-14)" font-family="'Arial Black','Arial Bold',Impact,'Helvetica Neue',sans-serif" font-weight="900">
  <text x="128" y="106" font-size="90" textLength="420" lengthAdjust="spacingAndGlyphs" fill="none" stroke="#19e3ff" stroke-width="12" stroke-linejoin="round" filter="url(#aGlow)" opacity="0.45">WAYBACK</text>
  <text x="128" y="106" font-size="90" textLength="420" lengthAdjust="spacingAndGlyphs" fill="url(#aSun)" stroke="#1b0a33" stroke-width="7" stroke-linejoin="round" paint-order="stroke">WAYBACK</text>
  <text x="318" y="170" font-size="50" textLength="220" lengthAdjust="spacing" fill="url(#aChrome)" stroke="#1b0a33" stroke-width="5" stroke-linejoin="round" paint-order="stroke">CHAIN</text>
</g>
<g fill="none" stroke="#19e3ff" stroke-width="5" filter="url(#aGlow)">
  <rect x="196" y="140" width="38" height="21" rx="10.5"/><rect x="220" y="140" width="38" height="21" rx="10.5"/>
</g>
</svg>"##;

/// B · Chrome: chrome lettering with a magenta glow; CHAIN knocked out of a
/// slanted sunset badge, framed by neon rules.
const CHROME: &str = r##"<svg viewBox="0 0 560 190" xmlns="http://www.w3.org/2000/svg" aria-hidden="true" focusable="false">
<defs>
  <linearGradient id="bChrome" x1="0" y1="0" x2="0" y2="1">
    <stop offset="0" stop-color="#ffffff"/><stop offset="0.38" stop-color="#dfe5ff"/><stop offset="0.5" stop-color="#3a3fb8"/>
    <stop offset="0.53" stop-color="#7f8cff"/><stop offset="0.75" stop-color="#e9edff"/><stop offset="1" stop-color="#ffffff"/>
  </linearGradient>
  <linearGradient id="bSun" x1="0" y1="0" x2="1" y2="0">
    <stop offset="0" stop-color="#ffe45c"/><stop offset="0.5" stop-color="#ff7a00"/><stop offset="1" stop-color="#ff2fb3"/>
  </linearGradient>
  <filter id="bGlow" x="-20%" y="-50%" width="140%" height="200%">
    <feGaussianBlur stdDeviation="5" result="b"/><feMerge><feMergeNode in="b"/><feMergeNode in="SourceGraphic"/></feMerge>
  </filter>
</defs>
<g transform="skewX(-10)" font-family="'Arial Black','Arial Bold',Impact,'Helvetica Neue',sans-serif" font-weight="900">
  <text x="40" y="100" font-size="94" textLength="500" lengthAdjust="spacingAndGlyphs" fill="none" stroke="#ff2fb3" stroke-width="10" stroke-linejoin="round" filter="url(#bGlow)" opacity="0.6">WAYBACK</text>
  <text x="40" y="100" font-size="94" textLength="500" lengthAdjust="spacingAndGlyphs" fill="url(#bChrome)" stroke="#1b0a33" stroke-width="6" stroke-linejoin="round" paint-order="stroke">WAYBACK</text>
</g>
<path d="M190 122 L420 122 L404 178 L174 178 Z" fill="url(#bSun)" stroke="#1b0a33" stroke-width="4"/>
<g transform="skewX(-10)" font-family="'Arial Black','Arial Bold',Impact,'Helvetica Neue',sans-serif" font-weight="900">
  <text x="226" y="166" font-size="40" textLength="190" lengthAdjust="spacing" fill="#1b0a33">CHAIN</text>
</g>
<g stroke="#19e3ff" stroke-width="3" stroke-linecap="round" filter="url(#bGlow)">
  <line x1="40" y1="150" x2="160" y2="150"/><line x1="434" y1="150" x2="540" y2="150"/>
</g>
</svg>"##;

/// C · Neon tube: outline lettering like a glowing shop sign, pink WAYBACK
/// over cyan CHAIN, with a small underline flourish.
const TUBE: &str = r##"<svg viewBox="0 0 560 190" xmlns="http://www.w3.org/2000/svg" aria-hidden="true" focusable="false">
<defs>
  <filter id="cGlowPink" x="-20%" y="-60%" width="140%" height="220%">
    <feGaussianBlur stdDeviation="6" result="b"/><feMerge><feMergeNode in="b"/><feMergeNode in="b"/><feMergeNode in="SourceGraphic"/></feMerge>
  </filter>
  <filter id="cGlowCyan" x="-20%" y="-60%" width="140%" height="220%">
    <feGaussianBlur stdDeviation="5" result="b"/><feMerge><feMergeNode in="b"/><feMergeNode in="SourceGraphic"/></feMerge>
  </filter>
</defs>
<g transform="skewX(-8)" font-family="'Arial Rounded MT Bold','Trebuchet MS','Arial Black',sans-serif" font-weight="800" fill="none" stroke-linejoin="round" stroke-linecap="round">
  <text x="44" y="100" font-size="92" textLength="500" lengthAdjust="spacingAndGlyphs" stroke="#ff2fb3" stroke-width="9" opacity="0.55" filter="url(#cGlowPink)">WAYBACK</text>
  <text x="44" y="100" font-size="92" textLength="500" lengthAdjust="spacingAndGlyphs" stroke="#ffd6f1" stroke-width="3">WAYBACK</text>
  <text x="196" y="166" font-size="50" textLength="250" lengthAdjust="spacing" stroke="#19e3ff" stroke-width="7" opacity="0.6" filter="url(#cGlowCyan)">CHAIN</text>
  <text x="196" y="166" font-size="50" textLength="250" lengthAdjust="spacing" stroke="#d9fbff" stroke-width="2.4">CHAIN</text>
</g>
<path d="M70 176 Q120 160 170 176" fill="none" stroke="#ffb03a" stroke-width="3.5" stroke-linecap="round" filter="url(#cGlowCyan)"/>
<path d="M440 176 Q480 160 520 176" fill="none" stroke="#ffb03a" stroke-width="3.5" stroke-linecap="round" filter="url(#cGlowCyan)"/>
</svg>"##;

pub fn svg(id: &str) -> &'static str {
    match id {
        "chrome" => CHROME,
        "tube" => TUBE,
        _ => STREAK,
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
    match query_logo().or_else(|| storage().and_then(|s| s.get_item(STORAGE_KEY).ok().flatten())) {
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
