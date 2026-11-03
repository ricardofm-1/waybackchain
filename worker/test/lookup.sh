#!/usr/bin/env bash
# End-to-end check of /api/lookup/start + /api/lookup/address against a running
# `wrangler dev` (default http://127.0.0.1:8787). Makes LIVE provider calls,
# so it's for local use, not CI. Costs one lookup from today's quota.
set -euo pipefail
BASE=${BASE:-http://127.0.0.1:8787}
LIGHT=14taMumBrvWCVw214fExgvX1FdmiK2FFy6
ETH=0xe4effbc36b1614d708c9d0a9a7affdf79aaccb9f
OTHER=9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM

pass=0; fail=0
check() { if [[ "$2" == "$3" ]]; then pass=$((pass+1)); echo "ok   $1"; else fail=$((fail+1)); echo "FAIL $1: expected $2, got $3"; fi; }

SID=$(curl -s -D - -o /dev/null "$BASE/api/quota" | grep -i '^set-cookie: hl_sid=' | sed -E 's/^[^=]*=([^;]*).*/\1/' | tr -d '\r')
START=$(curl -s -X POST "$BASE/api/lookup/start" -H 'Content-Type: application/json' -H "Cookie: hl_sid=$SID" \
  --data '{"turnstileToken":"XXXX.DUMMY.TOKEN.XXXX","date":"2024-01-01","addresses":[{"address":"'$LIGHT'","chain":"btc"},{"address":"'$ETH'","chain":"eth"},{"address":"'$OTHER'","chain":"sol"}]}')
TOKEN=$(echo "$START" | python3 -c 'import sys,json; print(json.load(sys.stdin)["lookupToken"])')

addr() { # address [token] [cookie] -> "<status> <body>"
  curl -s -w '\n%{http_code}' -X POST "$BASE/api/lookup/address" -H 'Content-Type: application/json' \
    -H "Cookie: hl_sid=${3:-$SID}" --data '{"lookupToken":"'${2:-$TOKEN}'","address":"'$1'"}'
}

r=$(addr "$LIGHT")
check "BTC light address -> 200" 200 "$(echo "$r" | tail -1)"
check "BTC balance at 2024-01-01 = 0.0034155" '"amount":"0.0034155"' "$(echo "$r" | head -1 | grep -o '"amount":"[^"]*"')"
# asOfBlock comes from mempool.space only; absent when it fell back to Blockstream.
b=$(echo "$r" | head -1 | grep -o '"asOfBlock":"[^"]*"' || true)
check "asOfBlock = 823940 (or absent on fallback)" ok "$([[ -z "$b" || "$b" == '"asOfBlock":"823940"' ]] && echo ok || echo "$b")"
[[ -z "$b" ]] && echo "     (note: mempool.space didn't answer; Blockstream fallback used)"
r=$(addr "$ETH")
check "ETH light address -> 200" 200 "$(echo "$r" | tail -1)"
check "ETH balance at 2024-01-01" '"amount":"0.0918118932"' "$(echo "$r" | head -1 | grep -o '"amount":"[^"]*"' | head -1)"
check "ETH asOfBlock = 18916001" '"asOfBlock":"18916001"' "$(echo "$r" | head -1 | grep -o '"asOfBlock":"[^"]*"')"
check "QNT token = 13.170560770278419109" '"symbol":"QNT","amount":"13.170560770278419109"' "$(echo "$r" | head -1 | grep -o '"symbol":"QNT","amount":"[^"]*"')"
check "SOL not built yet -> 501" 501 "$(addr "$OTHER" | tail -1)"
check "address outside lookup -> 404" 404 "$(addr 1JG56Dftg8Ygy4XZR7eZ4e2fZxMCsaHEFd | tail -1)"
check "tampered token -> 401" 401 "$(addr "$LIGHT" "${TOKEN%?}x" | tail -1)"
OTHER_SID=$(curl -s -D - -o /dev/null "$BASE/api/quota" | grep -i '^set-cookie: hl_sid=' | sed -E 's/^[^=]*=([^;]*).*/\1/' | tr -d '\r')
check "token from another visitor -> 401" 401 "$(addr "$LIGHT" "$TOKEN" "$OTHER_SID" | tail -1)"
check "retry reuses token, no quota cost" '"remaining":9' "$(addr "$LIGHT" >/dev/null; curl -s -H "Cookie: hl_sid=$SID" "$BASE/api/quota" | grep -o '"remaining":[0-9]*')"

echo "$pass passed, $fail failed"
[[ $fail -eq 0 ]]
