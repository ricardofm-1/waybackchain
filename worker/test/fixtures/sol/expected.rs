// Expected values for the recorded SOL fixtures (included into
// `chains::sol::recorded`). Cross-checked with test/check_sol.py, which
// rebuilds each balance by summing every transaction's change.

/// 12 transactions, all in June–July 2023; no token accounts.
const LIGHT: &str = "6dice3eV5uDG2H2YApBg3Hz98G18RjR6bszm9NALrHgB";
/// 374 transactions and 15 token accounts (2024–2026).
const MEDIUM: &str = "HdcxNedhEU1gJx1T62Eah7irquPnySnjSyMxSDjyPQhZ";
/// Owns over a million token accounts; tiny limits keep fixtures small.
const BUSY: &str = "5Q544fKrFoe6tsEbD7S8EmxGTJYAKtTVhAW5Q5pge4j1";
const BUSY_LIMITS: Limits = Limits { sig_page: 50, max_rounds: 2, max_token_accounts: 3 };

fn token<'a>(h: &'a RawHoldings, mint: &str) -> Option<&'a RawToken> {
    h.tokens.iter().find(|t| t.contract == mint)
}

#[test]
fn light_before_first_transaction_is_zero() {
    let h = run(LIGHT, "2023-06-16", Limits::DEFAULT);
    assert_eq!(h.native, Some(0));
    assert_eq!(h.warnings, vec![warn::SOL_CLOSED_ACCOUNTS]);
}

#[test]
fn light_after_activity() {
    for date in ["2023-06-17", "2023-07-31"] {
        let h = run(LIGHT, date, Limits::DEFAULT);
        assert_eq!(h.native, Some(2_185_440), "{date}");
        assert!(h.tokens.is_empty());
    }
}

#[test]
fn medium_with_tokens_early_2024() {
    let h = run(MEDIUM, "2024-01-29", Limits::DEFAULT);
    assert_eq!(h.native, Some(10_532_577_642));
    assert_eq!(h.tokens.len(), 2);
    let kpop = token(&h, "DcUoGUeNTLhhzyrcz49LE7z3MEFwca2N9uSw1xbVi1gm").unwrap();
    assert_eq!((kpop.symbol.as_str(), kpop.decimals), ("KPOP", 9));
    assert_eq!(kpop.amount, U256::from(1_142_382_705_900_164u64));
    assert_eq!(token(&h, "GTkeRzyQkyG7SkhDhq9ThjVm1HiR7LnbkR8WTz5aTJ9v").unwrap().amount, U256::from(5u64));
    assert_eq!(h.warnings, vec![warn::SOL_CLOSED_ACCOUNTS]);
}

#[test]
fn medium_with_tokens_late_2025() {
    let h = run(MEDIUM, "2025-09-01", Limits::DEFAULT);
    assert_eq!(h.native, Some(381_464_255));
    assert_eq!(h.tokens.len(), 4);
    assert_eq!(token(&h, "5gcMBTPeogu1d9M6jjxWdHkHSgQyWsxWMBjpnf6bpump").unwrap().amount, U256::from(7_185_167_795_907u64));
    assert_eq!(token(&h, "Hax9LTgsQkze1YFychnBLtFH8gYbQKtKfWKKg2SP6gdD").unwrap().amount, U256::from(9_758_101_995_102u64));
    assert_eq!(token(&h, "A8YHuvQBMAxXoZAZE72FyC8B7jKHo8RJyByXRRffpump").unwrap().amount, U256::from(1u64));
}

#[test]
fn busy_warns_instead_of_partial_data() {
    let h = run(BUSY, "2024-01-01", BUSY_LIMITS);
    assert_eq!(h.native, None);
    assert!(h.tokens.is_empty());
    assert!(h.warnings.contains(&warn::TOKEN_CAP_REACHED), "{:?}", h.warnings);
    assert!(h.warnings.contains(&warn::TOO_MANY_TXS), "{:?}", h.warnings);
}
