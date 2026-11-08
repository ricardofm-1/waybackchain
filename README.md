# Wayback Chain

**See what any address held, on any date.** → [waybackchain.xyz](https://waybackchain.xyz)

Enter up to 30 Bitcoin, Ethereum, Solana, XRP Ledger or Cardano addresses and a
date. Wayback Chain shows what each address held at 23:59:59 UTC that day,
including tokens, with indicative USD values, totals across all addresses and a
chart, and lets you save the results as an A4 PDF.

No accounts, no tracking. Free to use, MIT licensed.

## How it works

| Chain | Provider | Method |
|---|---|---|
| Bitcoin | mempool.space (Blockstream fallback) | Current confirmed balance minus every transaction after the date, newest first |
| Ethereum + ERC-20 | Alchemy | Block at the date by timestamp search; archive `eth_getBalance`; tokens found from incoming transfers, balances via `balanceOf` batched through Multicall3 |
| Solana + SPL | Helius (public RPC fallback) | Newest signature at or before the date per account, then that transaction's post-balances |
| XRP Ledger | s2.ripple.com (xrplcluster.com fallback) | Ledger at the date by interpolation search; `account_info` and `account_lines` at that ledger |
| Cardano | Blockfrost | Same replay as Bitcoin, multi-asset, including failed Plutus transactions |
| USD prices | DefiLlama | Historical prices for native coins and ETH/SOL tokens |

Results for dates at least two days old are cached for 30 days under a keyed
hash of (chain, address, date); the cache never stores a readable address. See
the [privacy page](https://waybackchain.xyz/privacy) for exactly what is sent
where.

## Architecture

All Rust, compiled to WebAssembly, on Cloudflare's free tier:

```
core/    shared crate: address validation and chain detection for all five
         chains, amount formatting, API types (used by both sides)
web/     Leptos (client-side) frontend, built by Trunk into static files
worker/  Cloudflare Worker (workers-rs): serves web/dist as static assets and
         the API under /api/*
```

- **Captcha and limits:** Cloudflare Turnstile is verified server-side. A
  Durable Object counts lookups: 10 per day per visitor (signed `HttpOnly`
  cookie) and 30 per day per IP (stored only as a keyed hash).
- **Two-step lookups:** `/api/lookup/start` spends one lookup and returns a
  10-minute signed token covering the address list; `/api/lookup/address`
  then fetches each address (4 in parallel from the browser), so a lookup of
  30 addresses never exceeds the Worker's per-request limits, and retries are
  free.
- **Amounts** are integer base units, formatted as exact decimal strings; USD
  values are computed with exact decimal arithmetic, never floats.
- **Security:** a Content-Security-Policy allowing scripts only from the site
  itself, Turnstile, and Trunk's loader (by SHA-256 hash), generated at build
  time by `web/hooks/csp.py`.

## Develop

Requirements: Rust (with the `wasm32-unknown-unknown` target),
[Trunk](https://trunkrs.dev), `worker-build` (`cargo install worker-build`),
Node.js, Python 3.

```sh
# API keys for local development (free tiers)
cp worker/.dev.vars.example worker/.dev.vars   # then fill in the keys
(cd worker && npm install)

# Terminal 1: the Worker (API) on :8787
cd worker && npm run dev

# Terminal 2: the frontend on :8080, proxying /api to the Worker
cd web && trunk serve
```

Open http://127.0.0.1:8080. Locally the captcha uses Cloudflare's test keys.

## Test

```sh
cargo test -p wayback-core -p wayback-worker   # offline, no keys needed
cd worker && npm run test:ratelimit            # captcha + daily limits against wrangler dev
cd worker && bash test/lookup.sh               # live end-to-end, needs a running Worker and keys
```

Chain modules are tested against recorded provider responses in
`worker/test/fixtures/` (no network in tests). Expected values are
cross-checked by independent scripts (`worker/test/check_*.py`) that compute
the same balances a different way. To re-record:
`set -a; source .dev.vars; set +a; RECORD=1 cargo test -p wayback-worker <chain>::recorded`.

## Deploy

One-time setup (from `worker/`):

```sh
npx wrangler kv namespace create CACHE      # put the id in wrangler.toml
for s in HMAC_SECRET TURNSTILE_SECRET ALCHEMY_KEY HELIUS_KEY BLOCKFROST_KEY; do
  npx wrangler secret put $s
done
```

Then every deploy, from the repo root:

```sh
./deploy.sh   # builds web/ with the Turnstile site key, then `wrangler deploy`
```

The free plan is enough to run it; the $5/month Workers Paid plan lifts the
10 ms CPU and 50-subrequest limits that busy addresses can hit.

## Support the project

Donation addresses are listed at the bottom of
[waybackchain.xyz](https://waybackchain.xyz). Code, issue reports and
translations are equally welcome.

## License

[MIT](LICENSE) © 2026 Daedalus Labs
