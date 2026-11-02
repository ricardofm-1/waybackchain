// Thin wrapper over Cloudflare Turnstile for the Rust/Wasm app.
// The Turnstile script loads async; calls made before it's ready are queued.
(() => {
  let resolveReady;
  const ready = new Promise((r) => (resolveReady = r));
  window.wbTurnstileLoaded = () => resolveReady(window.turnstile);
  window.wbTurnstile = {
    // onToken(token) is called with the token, or "" when it expires or errors.
    render(el, sitekey, onToken) {
      ready.then((t) => {
        if (el.dataset.widgetId) return;
        el.dataset.widgetId = t.render(el, {
          sitekey,
          callback: (token) => onToken(token),
          "expired-callback": () => onToken(""),
          "error-callback": () => onToken(""),
        });
      });
    },
    // Tokens are single-use: reset after every submit.
    reset(el) {
      ready.then((t) => el.dataset.widgetId && t.reset(el.dataset.widgetId));
    },
  };
})();
