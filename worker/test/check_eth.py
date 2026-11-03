#!/usr/bin/env python3
"""Independent cross-check for ETH token fixtures: sum each token's transfers
in and out of the address up to the block (Alchemy's transfer history), and
compare with balanceOf from the recorded fixtures. Needs ALCHEMY_KEY.
Usage: check_eth.py <address> <block> [contract=amount ...]"""
import json, os, sys, time, urllib.request

URL = "https://eth-mainnet.g.alchemy.com/v2/" + os.environ["ALCHEMY_KEY"]
addr, block = sys.argv[1].lower(), int(sys.argv[2])
expected = {c.lower(): int(v) for c, v in (a.split("=") for a in sys.argv[3:])}

def transfers(direction):
    out, key = [], None
    while True:
        q = {"fromBlock": "0x0", "toBlock": hex(block), direction: addr, "category": ["erc20"],
             "withMetadata": False, "excludeZeroValue": False, "maxCount": "0x3e8"}
        if key:
            q["pageKey"] = key
        time.sleep(0.3)
        body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": "alchemy_getAssetTransfers", "params": [q]}).encode()
        req = urllib.request.Request(URL, data=body, headers={"Content-Type": "application/json"})
        r = json.load(urllib.request.urlopen(req, timeout=30))["result"]
        out += r["transfers"]
        key = r.get("pageKey")
        if not key:
            return out

sums = {}
for t in transfers("toAddress"):
    c = t["rawContract"]["address"].lower()
    sums[c] = sums.get(c, 0) + int(t["rawContract"]["value"], 16)
for t in transfers("fromAddress"):
    c = t["rawContract"]["address"].lower()
    sums[c] = sums.get(c, 0) - int(t["rawContract"]["value"], 16)
ok = True
for c, amount in expected.items():
    got = sums.get(c, 0)
    ok &= got == amount
    print(("ok  " if got == amount else "DIFF"), c, "transfers", got, "balanceOf", amount)
sys.exit(0 if ok else 1)
