//! Bindings to the `wbTurnstile` shim in `turnstile.js`.

use wasm_bindgen::prelude::*;

/// Cloudflare's always-pass test key, used unless the build sets
/// `TURNSTILE_SITE_KEY` (e.g. `TURNSTILE_SITE_KEY=0x… trunk build --release`).
pub const SITE_KEY: &str = match option_env!("TURNSTILE_SITE_KEY") {
    Some(k) => k,
    None => "1x00000000000000000000AA",
};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = wbTurnstile, js_name = render)]
    fn render_js(el: &web_sys::Element, sitekey: &str, on_token: &js_sys::Function);

    #[wasm_bindgen(js_namespace = wbTurnstile, js_name = reset)]
    fn reset_js(el: &web_sys::Element);
}

/// Renders the widget once into `el`; `on_token` gets each new token, or an
/// empty string when it expires or fails.
pub fn render(el: &web_sys::Element, on_token: impl Fn(String) + 'static) {
    let cb = Closure::<dyn Fn(String)>::new(on_token).into_js_value();
    render_js(el, SITE_KEY, cb.unchecked_ref());
}

pub fn reset(el: &web_sys::Element) {
    reset_js(el);
}
