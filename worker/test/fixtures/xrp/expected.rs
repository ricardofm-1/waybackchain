// Expected values for the recorded XRP fixtures (included into
// `chains::xrp::recorded`). Cross-checked against xrplcluster.com with
// test/check_xrp.py: each ledger is the last one closing at or before the
// cutoff, and balances and trust lines match.

/// Never funded: sha256-derived account ID.
const EMPTY: &str = "rLKDEU1vugFWTzL88qkSQv1359Dv3Ua1MC";
/// 4 trust lines, 3 with positive balances.
const LIGHT: &str = "rMBzp8CgpE441cp5PVyA9rpVV7oT8hP3ys";
/// 20 trust lines; read 10 per page to exercise the page limit.
const MANY_LINES: &str = "rw2ciyaNshpHe7bCHo4bRWq6pqqynnWKQg";
/// The genesis account.
const GENESIS: &str = "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh";
const PAGED: Limits = Limits { lines_page: 10, max_pages: 1 };

const BITSTAMP_USD: &str = "USD.rvYAfWj5gh67oV6fW32ZzP3Aw4Eubs59B";

fn token<'a>(h: &'a RawHoldings, contract: &str) -> &'a RawToken {
    h.tokens.iter().find(|t| t.contract == contract).unwrap()
}

#[test]
fn unfunded_account_is_zero() {
    let h = run(EMPTY, "2024-01-01", Limits::DEFAULT);
    assert_eq!(h.native, Some(0));
    assert_eq!(h.as_of_block, Some(84_996_424));
    assert!(h.tokens.is_empty() && h.warnings.is_empty());
}

#[test]
fn light_2019() {
    let h = run(LIGHT, "2019-09-13", Limits::DEFAULT);
    assert_eq!(h.as_of_block, Some(50_017_878));
    assert_eq!(h.native, Some(54_057_230_447));
    assert_eq!(h.warnings, vec![warn::XRP_INCLUDES_RESERVE]);
    assert_eq!(h.tokens.len(), 3);
    let usd = token(&h, BITSTAMP_USD);
    // 6771.385214727229 USD
    assert_eq!((usd.symbol.as_str(), usd.amount, usd.decimals), ("USD", U256::from(6_771_385_214_727_229u64), 12));
}

#[test]
fn light_2024() {
    let h = run(LIGHT, "2024-01-01", Limits::DEFAULT);
    assert_eq!(h.as_of_block, Some(84_996_424));
    assert_eq!(h.native, Some(50_431_510));
    // 9.34562406753 USD
    let usd = token(&h, BITSTAMP_USD);
    assert_eq!((usd.amount, usd.decimals), (U256::from(934_562_406_753u64), 11));
    assert_eq!(token(&h, "SCZ.rScheetz28HPqwAo3sxCtqKxU5TPWbLEf").amount, U256::from(1_000_000u64));
}

#[test]
fn page_limit_warns() {
    let h = run(MANY_LINES, "2024-01-01", PAGED);
    assert_eq!(h.native, Some(1_164_749_960));
    assert_eq!(h.warnings, vec![warn::XRP_INCLUDES_RESERVE, warn::TOKENS_INCOMPLETE]);
}

#[test]
fn before_recorded_history() {
    let h = run(GENESIS, "2012-12-31", Limits::DEFAULT);
    assert_eq!((h.native, h.as_of_block), (None, None));
    assert_eq!(h.warnings, vec![warn::NO_HISTORY]);
}

#[test]
fn genesis_2013() {
    let h = run(GENESIS, "2013-06-01", Limits::DEFAULT);
    assert_eq!(h.as_of_block, Some(896_643));
    assert_eq!(h.native, Some(109_485_881));
}
