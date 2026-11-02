#!/usr/bin/env bash
# Integration test for /api/quota and /api/lookup/start against `wrangler dev`.
# Uses a throwaway state dir so every run starts from zero, and Cloudflare's
# always-pass Turnstile test secret (from .dev.vars).
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

PORT=8799
BASE="http://127.0.0.1:$PORT"
STATE=$(mktemp -d)
LOG=$(mktemp)
npx wrangler dev --port "$PORT" --persist-to "$STATE" >"$LOG" 2>&1 &
PID=$!
trap 'kill $PID 2>/dev/null; rm -rf "$STATE"' EXIT
for _ in $(seq 1 120); do curl -sf "$BASE/api/quota" >/dev/null 2>&1 && break; sleep 1; done

pass=0; fail=0
check() { # name expected actual
  if [[ "$2" == "$3" ]]; then pass=$((pass+1)); echo "ok   $1"; else fail=$((fail+1)); echo "FAIL $1: expected $2, got $3"; fi
}

DATE=2024-01-01
BODY='{"turnstileToken":"XXXX.DUMMY.TOKEN.XXXX","date":"'$DATE'","addresses":[{"address":"1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa","chain":"btc"}]}'

# Prints "<status> <hl_sid value or ->"
start() { # cookie-value body
  local hdr; hdr=$(mktemp)
  local code; code=$(curl -s -o /dev/null -D "$hdr" -w '%{http_code}' -X POST "$BASE/api/lookup/start" \
    -H 'Content-Type: application/json' ${1:+-H "Cookie: hl_sid=$1"} --data "$2")
  local sid; sid=$(grep -i '^set-cookie: hl_sid=' "$hdr" | sed -E 's/^[^=]*=([^;]*).*/\1/' | tr -d '\r' || true)
  rm -f "$hdr"; echo "$code ${sid:--}"
}

# New visitor gets a cookie and a full quota.
hdr=$(curl -s -D - -o /tmp/wb_q.json "$BASE/api/quota")
SID_A=$(echo "$hdr" | grep -i '^set-cookie: hl_sid=' | sed -E 's/^[^=]*=([^;]*).*/\1/' | tr -d '\r')
check "quota issues hl_sid" "yes" "$([[ -n "$SID_A" ]] && echo yes || echo no)"
check "cookie flags" "yes" "$(echo "$hdr" | grep -i '^set-cookie: hl_sid=' | grep -qi 'HttpOnly' && echo "$hdr" | grep -qi 'SameSite=Strict' && echo "$hdr" | grep -qi 'Path=/api' && echo yes || echo no)"
check "fresh quota is 10" '"remaining":10' "$(grep -o '"remaining":[0-9]*' /tmp/wb_q.json)"

# Input checks.
check "empty captcha token -> 403" "403" "$(start "$SID_A" '{"turnstileToken":"","date":"'$DATE'","addresses":[{"address":"1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa","chain":"btc"}]}' | cut -d' ' -f1)"
check "wrong chain claim -> 400" "400" "$(start "$SID_A" '{"turnstileToken":"XXXX.DUMMY.TOKEN.XXXX","date":"'$DATE'","addresses":[{"address":"1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa","chain":"eth"}]}' | cut -d' ' -f1)"
check "future date -> 400" "400" "$(start "$SID_A" '{"turnstileToken":"XXXX.DUMMY.TOKEN.XXXX","date":"2999-01-01","addresses":[{"address":"1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa","chain":"btc"}]}' | cut -d' ' -f1)"

# Visitor A: 10 lookups pass, the 11th is refused.
for i in $(seq 1 10); do r=$(start "$SID_A" "$BODY"); [[ ${r%% *} == 200 ]] || { check "lookup $i" 200 "${r%% *}"; }; done
check "11th lookup -> 429" "429" "$(start "$SID_A" "$BODY" | cut -d' ' -f1)"
check "quota now 0" '"remaining":0' "$(curl -s -H "Cookie: hl_sid=$SID_A" "$BASE/api/quota" | grep -o '"remaining":[0-9]*')"

# Forged cookie: rejected, replaced, fresh counter (IP total 10 -> 20).
r=$(start "${SID_A%.*}.$(printf '0%.0s' $(seq 1 64))" "$BODY")
check "forged hl_sid -> 200 with new cookie" "200 new" "${r%% *} $([[ ${r#* } != - && ${r#* } != "$SID_A" ]] && echo new || echo same)"
SID_B=${r#* }
for i in $(seq 2 10); do start "$SID_B" "$BODY" >/dev/null; done

# Visitor C (cookies cleared) uses IP lookups 21-30; the 31st is refused.
r=$(start "" "$BODY"); SID_C=${r#* }
for i in $(seq 2 10); do start "$SID_C" "$BODY" >/dev/null; done
r=$(start "" "$BODY")
check "IP backstop: 31st lookup -> 429" "429" "${r%% *}"

echo "$pass passed, $fail failed"
[[ $fail -eq 0 ]] || { echo "--- wrangler log ---"; tail -30 "$LOG"; exit 1; }
