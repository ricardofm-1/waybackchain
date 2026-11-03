#!/usr/bin/env python3
"""Independent cross-check for ETH token fixtures: sum each token's transfers
in and out of the address up to the block, and compare with balanceOf from the
recorded fixtures. Needs ETHERSCAN_KEY in the environment."""
import json, os, sys, time, urllib.request

K = os.environ["ETHERSCAN_KEY"]
addr, block = sys.argv[1].lower(), int(sys.argv[2])
expected = dict(a.split("=") for a in sys.argv[3:])  # contract=amount

time.sleep(0.4)
url = (f"https://api.etherscan.io/v2/api?chainid=1&module=account&action=tokentx&address={addr}"
       f"&startblock=0&endblock={block}&page=1&offset=10000&sort=asc&apikey={K}")
rows = json.load(urllib.request.urlopen(url, timeout=30))["result"]
sums = {}
for r in rows:
    c, v = r["contractAddress"].lower(), int(r["value"])
    sums[c] = sums.get(c, 0) + (v if r["to"].lower() == addr else 0) - (v if r["from"].lower() == addr else 0)
ok = True
for c, amount in expected.items():
    got = sums.get(c.lower(), 0)
    status = "ok " if got == int(amount) else "DIFF"
    ok &= got == int(amount)
    print(f"{status} {c} transfers={got} balanceOf={amount}")
sys.exit(0 if ok else 1)
