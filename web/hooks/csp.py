#!/usr/bin/env python3
"""Trunk post-build hook: write a Cloudflare Pages `_headers` file with a
Content-Security-Policy that allows scripts only from this site, Cloudflare
Turnstile, and the exact inline loader Trunk generates (by SHA-256 hash, so
no 'unsafe-inline' for scripts).

Runs with TRUNK_STAGING_DIR set by Trunk; standard library only."""
import base64, hashlib, os, re, sys

staging = os.environ.get("TRUNK_STAGING_DIR") or (sys.argv[1] if len(sys.argv) > 1 else "")
index = os.path.join(staging, "index.html")
html = open(index, encoding="utf-8").read()

# Browsers hash the script element's exact text content.
inline = re.findall(r"<script(?![^>]*\bsrc=)[^>]*>(.*?)</script>", html, re.S)
hashes = " ".join(
    "'sha256-%s'" % base64.b64encode(hashlib.sha256(s.encode("utf-8")).digest()).decode() for s in inline
)

TURNSTILE = "https://challenges.cloudflare.com"
csp = "; ".join([
    "default-src 'self'",
    f"script-src 'self' {hashes} 'wasm-unsafe-eval' {TURNSTILE}",
    # Leptos sets a few style attributes (chart swatches).
    "style-src 'self' 'unsafe-inline'",
    "img-src 'self' data:",
    "connect-src 'self'",
    f"frame-src {TURNSTILE}",
    "object-src 'none'",
    "base-uri 'self'",
    "form-action 'self'",
    "frame-ancestors 'none'",
])

headers = f"""/*
  Content-Security-Policy: {csp}
  X-Content-Type-Options: nosniff
  Referrer-Policy: no-referrer
  Permissions-Policy: camera=(), microphone=(), geolocation=(), payment=()
"""
with open(os.path.join(staging, "_headers"), "w", encoding="utf-8") as f:
    f.write(headers)
print(f"csp.py: {len(inline)} inline script(s) hashed into _headers")
