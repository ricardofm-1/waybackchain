#!/usr/bin/env bash
# Build the frontend with the production Turnstile site key and deploy the
# Worker (which serves the site and the API) to Cloudflare.
# Secrets are set once with `npx wrangler secret put NAME` (see README).
set -euo pipefail
cd "$(dirname "$0")"
export PATH="$HOME/.cargo/bin:$PATH"

# Public by design: Turnstile site keys are embedded in the page.
export TURNSTILE_SITE_KEY="0x4AAAAAAF45CSyQPluEai5O"

(cd web && rm -rf dist && trunk build --release)
grep -q "$TURNSTILE_SITE_KEY" web/dist/*.wasm || { echo "site key missing from the build" >&2; exit 1; }
(cd worker && npx wrangler deploy)
