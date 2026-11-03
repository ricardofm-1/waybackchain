#!/usr/bin/env python3
"""Independent cross-check for ADA fixtures: sum every transaction's net
effect FORWARD from the address's first transaction up to the cutoff (the
Worker replays BACKWARDS from today's balance). Needs BLOCKFROST_KEY.
Usage: check_ada.py <address> <YYYY-MM-DD> <lovelace> [unit=quantity ...]"""
import calendar, datetime, json, os, sys, time, urllib.request

K = os.environ["BLOCKFROST_KEY"]
def bf(p):
    time.sleep(0.15)
    req = urllib.request.Request("https://cardano-mainnet.blockfrost.io/api/v0" + p, headers={"project_id": K})
    return json.load(urllib.request.urlopen(req, timeout=60))

addr, date, lovelace = sys.argv[1], sys.argv[2], int(sys.argv[3])
expected = {"lovelace": lovelace, **{u: int(q) for u, q in (a.split("=") for a in sys.argv[4:])}}
cutoff = calendar.timegm(datetime.date.fromisoformat(date).timetuple()) + 86399
held = {}
page = 1
while True:
    txs = bf(f"/addresses/{addr}/transactions?order=asc&count=100&page={page}")
    for t in txs:
        if t["block_time"] > cutoff:
            break
        u = bf(f"/txs/{t['tx_hash']}/utxos")
        valid = True
        if any(i["address"] == addr and i["collateral"] for i in u["inputs"]):
            valid = bf(f"/txs/{t['tx_hash']}")["valid_contract"]
        for o in u["outputs"]:
            if o["address"] == addr and o["collateral"] != valid:
                for a in o["amount"]:
                    held[a["unit"]] = held.get(a["unit"], 0) + int(a["quantity"])
        for i in u["inputs"]:
            if i["address"] == addr and not i["reference"] and i["collateral"] != valid:
                for a in i["amount"]:
                    held[a["unit"]] = held.get(a["unit"], 0) - int(a["quantity"])
    else:
        if len(txs) == 100:
            page += 1
            continue
    break
held = {k: v for k, v in held.items() if v or k == "lovelace"}
held.setdefault("lovelace", 0)
ok = held == expected
for k in sorted(set(held) | set(expected)):
    print(("ok  " if held.get(k) == expected.get(k) else "DIFF"), k[:24], held.get(k), "vs", expected.get(k))
sys.exit(0 if ok else 1)
