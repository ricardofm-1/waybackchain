// Expected values for the recorded ETH fixtures (included into
// `chains::eth::recorded`). Token balances were cross-checked against an
// independent sum of transfers with test/check_eth.py; ETH balances come from
// Alchemy's archive node.

/// 7 transfers of 6 tokens (3 of them "claim rewards" spam), 2021–2025.
const LIGHT: &str = "0xe4effbc36b1614d708c9d0a9a7affdf79aaccb9f";
/// vitalik.eth, with tiny limits so paging and caps run on small fixtures.
const BUSY: &str = "0xd8da6bf26964af9d7eed9e03e53415d37aa96045";
const BUSY_LIMITS: Limits = Limits { tokentx_page: 20, max_pages: 2, max_tokens: 300 };
const BUSY_CAP: Limits = Limits { tokentx_page: 20, max_pages: 2, max_tokens: 2 };

const QNT: &str = "0x4a220E6096B25EADb88358cb44068A3248254675";
const REEF: &str = "0xFE3E6a25e6b192A42a44ecDDCd13796471735ACf";

fn amount(h: &RawHoldings, symbol: &str) -> String {
    token(h, symbol).map(|t| t.amount.to_string()).unwrap_or_default()
}

#[test]
fn light_before_multicall3_uses_rpc_batch() {
    let h = run(LIGHT, "2021-06-01", Limits::DEFAULT);
    assert_eq!(h.as_of_block, Some(12_551_696));
    assert!(h.as_of_block.unwrap() < MULTICALL3_FROM_BLOCK);
    assert_eq!(h.native, Some(20_100_513_100_000_000));
    assert_eq!(h.tokens.len(), 2);
    assert_eq!(token(&h, "QNT").unwrap().contract, QNT);
    assert_eq!(amount(&h, "QNT"), "13170560770278419109");
    assert_eq!(token(&h, "REEF").unwrap().contract, REEF);
    assert_eq!(amount(&h, "REEF"), "12027859648770993375397");
    assert!(h.warnings.is_empty());
}

#[test]
fn light_after_multicall3() {
    let h = run(LIGHT, "2024-01-01", Limits::DEFAULT);
    assert_eq!(h.as_of_block, Some(18_916_001));
    assert_eq!(h.native, Some(91_811_893_200_000_000));
    assert_eq!(h.tokens.len(), 4);
    assert_eq!(amount(&h, "QNT"), "13170560770278419109");
    assert_eq!(amount(&h, "REEF"), "12027859648770993375397");
    assert_eq!(amount(&h, "ELON"), "659674430870154627440676081");
    // Spam token with a URL for a symbol: kept here, hidden by the
    // "unpriced tokens" toggle in step 8. Symbol truncated to 20 chars.
    assert!(h.tokens.iter().any(|t| t.symbol == "Visit quantprotocol." && t.decimals == 6));
}

#[test]
fn busy_paging_stops_with_warning() {
    let h = run(BUSY, "2017-12-31", BUSY_LIMITS);
    assert_eq!(h.as_of_block, Some(4_832_685));
    assert_eq!(h.native, Some(722_411_475_753_161_095_258));
    assert_eq!(h.warnings, vec![warn::TOKENS_INCOMPLETE]);
    assert!(!h.tokens.is_empty());
}

#[test]
fn busy_token_cap() {
    let h = run(BUSY, "2017-12-31", BUSY_CAP);
    assert!(h.warnings.contains(&warn::TOKEN_CAP_REACHED), "{:?}", h.warnings);
    assert!(h.tokens.len() <= 2);
}
