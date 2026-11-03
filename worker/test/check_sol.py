#!/usr/bin/env python3
"""Independent cross-check for SOL fixtures: rebuild each balance by summing
every transaction's (post - pre) change from the account's first transaction
up to the cutoff, instead of reading the last transaction's post-balance.
Usage: check_sol.py <wallet> <YYYY-MM-DD> <lamports> [mint=amount ...]
Needs HELIUS_KEY. Only accounts with < 1000 signatures."""
import calendar, datetime, json, os, sys, time, urllib.error, urllib.request

U = "https://mainnet.helius-rpc.com/?api-key=" + os.environ["HELIUS_KEY"]

def call(body):
    for attempt in range(6):
        time.sleep(1.0 if isinstance(body, list) else 0.25)
        req = urllib.request.Request(U, data=json.dumps(body).encode(), headers={"Content-Type": "application/json"})
        try:
            return json.load(urllib.request.urlopen(req, timeout=60))
        except urllib.error.HTTPError as e:
            if e.code != 429:
                raise
            time.sleep(2 ** attempt)
    raise SystemExit("rate limited")

def rpc(method, params):
    return call({"jsonrpc": "2.0", "id": 1, "method": method, "params": params})["result"]

def txs_until(account, cutoff):
    sigs = [s["signature"] for s in rpc("getSignaturesForAddress", [account, {"limit": 1000}]) if s["blockTime"] <= cutoff]
    out = []
    for i in range(0, len(sigs), 5):
        batch = [{"jsonrpc": "2.0", "id": j, "method": "getTransaction",
                  "params": [s, {"encoding": "json", "maxSupportedTransactionVersion": 0}]} for j, s in enumerate(sigs[i:i + 5])]
        out += [r["result"] for r in sorted(call(batch), key=lambda r: r["id"])]
    return out

def keys(tx):
    la = tx["meta"].get("loadedAddresses") or {}
    return tx["transaction"]["message"]["accountKeys"] + la.get("writable", []) + la.get("readonly", [])

wallet, date, lamports = sys.argv[1], sys.argv[2], int(sys.argv[3])
expected = dict(a.split("=") for a in sys.argv[4:])
cutoff = calendar.timegm(datetime.date.fromisoformat(date).timetuple()) + 86399

sol = 0
for tx in txs_until(wallet, cutoff):
    i = keys(tx).index(wallet)
    sol += tx["meta"]["postBalances"][i] - tx["meta"]["preBalances"][i]
ok = sol == lamports
print(("ok  " if ok else "DIFF"), "SOL", sol, "vs", lamports)

tokens = {}
das = rpc("getTokenAccounts", {"owner": wallet, "limit": 51, "options": {"showZeroBalance": True}})
for ta in das["token_accounts"]:
    acct = ta["address"]
    for tx in txs_until(acct, cutoff):
        i = keys(tx).index(acct)
        amt = lambda bs: next((int(b["uiTokenAmount"]["amount"]) for b in (bs or []) if b["accountIndex"] == i), 0)
        tokens[ta["mint"]] = tokens.get(ta["mint"], 0) + amt(tx["meta"].get("postTokenBalances")) - amt(tx["meta"].get("preTokenBalances"))
nonzero = {m: v for m, v in tokens.items() if v}
for m, v in sorted(nonzero.items()):
    good = str(v) == expected.get(m)
    ok &= good
    print(("ok  " if good else "DIFF"), m, v, "vs", expected.get(m))
missing = set(expected) - set(nonzero)
ok &= not missing
for m in missing:
    print("DIFF", m, "expected", expected[m], "but summed to 0")
sys.exit(0 if ok else 1)
