#!/usr/bin/env python3
"""Independent cross-check for XRP fixtures against a second server
(xrplcluster.com): ledger L closes at or before the cutoff and L+1 after it,
and the XRP balance and positive trust lines at L match.
Usage: check_xrp.py <account> <YYYY-MM-DD> <ledger> <drops> [CUR.issuer=value ...]"""
import calendar, datetime, json, sys, time, urllib.request
from decimal import Decimal

def x(method, params):
    time.sleep(1.0)
    req = urllib.request.Request("https://xrplcluster.com/", data=json.dumps({"method": method, "params": [params]}).encode(),
                                 headers={"Content-Type": "application/json"})
    r = json.load(urllib.request.urlopen(req, timeout=30))["result"]
    if r.get("error") == "slowDown":
        time.sleep(35)
        return x(method, params)
    return r

acct, date, ledger, drops = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
expected = {k: Decimal(v) for k, v in (a.split("=") for a in sys.argv[5:])}
cutoff = calendar.timegm(datetime.date.fromisoformat(date).timetuple()) + 86399 - 946684800
ok = True
def report(good, msg):
    global ok
    ok &= good
    print(("ok  " if good else "DIFF"), msg)

c0 = x("ledger", {"ledger_index": ledger})["ledger"]["close_time"]
c1 = x("ledger", {"ledger_index": ledger + 1})["ledger"]["close_time"]
report(c0 <= cutoff < c1, f"ledger {ledger} closes {c0} <= {cutoff} < {c1} (next)")
info = x("account_info", {"account": acct, "ledger_index": ledger})
got = 0 if info.get("error") == "actNotFound" else int(info["account_data"]["Balance"])
report(got == drops, f"drops {got} vs {drops}")
lines = x("account_lines", {"account": acct, "ledger_index": ledger, "limit": 400}).get("lines", [])
pos = {f'{l["currency"]}.{l["account"]}': Decimal(l["balance"]) for l in lines if Decimal(l["balance"]) > 0}
for k in sorted(set(pos) | set(expected)):
    report(pos.get(k) == expected.get(k), f"{k} {pos.get(k)} vs {expected.get(k)}")
sys.exit(0 if ok else 1)
