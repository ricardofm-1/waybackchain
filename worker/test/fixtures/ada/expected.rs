// Expected values for the recorded ADA fixtures (included into
// `chains::ada::recorded`). Cross-checked with test/check_ada.py, which sums
// every transaction forward from the first (the module replays backwards).

/// CIP-19's sample enterprise address: valid, never used.
const EMPTY: &str = "addr1vx2fxv2umyhttkxyxp8x0dlpdt3k6cwng5pxj3jhsydzers66hrl8";
/// 5 transactions between 2024-03-01 and 2024-06-11, two native tokens.
const LIGHT: &str = "addr1q8m3qmg4200mu9keg870eg7al29f92elqcm827lr006f9gtha6mt4lthadhelq9tgwejransrxl430tys0wdf0rv3a0q063fga";
/// Tiny limits: 2 transactions per page, give up past 3.
const TINY: Limits = Limits { tx_page: 2, max_txs: 3, max_assets: 50 };

const AGIX: &str = "f43a62fdc3965df486de8a0d32fe800963589c41b38946602a0dc53541474958";
const AAGIX: &str = "be9ca6e1eff3a3175368bcf9c5af4b5fd9ecb901450feb481ebac77c4141474958";

fn token<'a>(h: &'a RawHoldings, unit: &str) -> Option<&'a RawToken> {
    h.tokens.iter().find(|t| t.contract == unit)
}

#[test]
fn never_used_address() {
    let h = run(EMPTY, "2024-01-01", Limits::DEFAULT);
    assert_eq!(h.native, Some(0));
    assert!(h.tokens.is_empty());
}

#[test]
fn before_first_transaction_replays_to_zero() {
    let h = run(LIGHT, "2024-02-29", Limits::DEFAULT);
    assert_eq!(h.native, Some(0));
    assert!(h.tokens.is_empty());
    assert_eq!(h.warnings, vec![warn::ADA_PAYMENT_ADDRESS_ONLY]);
}

#[test]
fn mid_history_with_tokens() {
    let h = run(LIGHT, "2024-03-01", Limits::DEFAULT);
    assert_eq!(h.native, Some(4_500_000));
    let agix = token(&h, AGIX).unwrap();
    assert_eq!((agix.symbol.as_str(), agix.decimals, agix.amount), ("AGIX", 8, U256::from(998_500_000_000u64)));
    assert_eq!(h.tokens.len(), 1);

    let h = run(LIGHT, "2024-03-02", Limits::DEFAULT);
    assert_eq!(h.native, Some(5_650_770));
    assert_eq!(token(&h, AGIX).unwrap().amount, U256::from(998_500_000_000u64));
    let aagix = token(&h, AAGIX).unwrap();
    assert_eq!((aagix.symbol.as_str(), aagix.amount), ("AAGIX", U256::from(3_200u64)));
}

#[test]
fn after_last_transaction_is_current() {
    let h = run(LIGHT, "2024-07-01", Limits::DEFAULT);
    assert_eq!(h.native, Some(1_150_770));
    assert_eq!(h.tokens.len(), 1);
    assert_eq!(token(&h, AAGIX).unwrap().amount, U256::from(3_200u64));
}

#[test]
fn paging_and_cap() {
    let h = run(LIGHT, "2024-02-29", TINY);
    assert_eq!(h.native, None);
    assert!(h.tokens.is_empty());
    assert_eq!(h.warnings, vec![warn::ADA_PAYMENT_ADDRESS_ONLY, warn::TOO_MANY_TXS]);
}
