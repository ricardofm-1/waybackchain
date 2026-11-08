//! "How it works" and FAQ: real, visible content for people and search
//! engines. The FAQ text is also in index.html as FAQPage structured data;
//! a test keeps the two identical, as search engines require.

use leptos::prelude::*;

pub const FAQ: &[(&str, &str)] = &[
    (
        "Is Wayback Chain free? Do I need an account?",
        "Yes, it is free, with no sign-up and no tracking. Each visitor can run 10 lookups per day, each covering up to 30 wallet addresses.",
    ),
    (
        "Do you need my private keys or seed phrase?",
        "No. Wayback Chain only reads public blockchain data for the addresses you enter. Never share your private keys or seed phrase with any website.",
    ),
    (
        "How accurate are the historical balances?",
        "Balances are read from public blockchain data providers for the exact end of the chosen day, 23:59:59 UTC. USD values are indicative daily prices from DefiLlama, and tokens without a price are hidden by default.",
    ),
    (
        "Can I check a whole wallet or an exchange account?",
        "Wayback Chain checks individual addresses. Wallets that use many addresses, which is common on Bitcoin and Cardano, need each address entered, and funds held on an exchange are not visible on-chain under your own address.",
    ),
];

#[component]
pub fn About() -> impl IntoView {
    view! {
        <section class="about" aria-labelledby="about-title">
            <h2 id="about-title">"Check historical crypto wallet balances on any date"</h2>
            <p>
                "Wayback Chain is a free blockchain time machine. Paste one or more crypto wallet addresses, pick a date, and see what each address held at 23:59:59 UTC that day: the native coin and its tokens, with indicative USD values and a total across all your wallets. Handy for tax and accounting snapshots, portfolio history, audits, estate records, or simply remembering what you held."
            </p>

            <h3>"Supported blockchains and tokens"</h3>
            <ul>
                <li><strong>"Bitcoin (BTC)"</strong>": legacy, SegWit and Taproot addresses"</li>
                <li><strong>"Ethereum (ETH)"</strong>" and ERC-20 tokens"</li>
                <li><strong>"Solana (SOL)"</strong>" and SPL tokens"</li>
                <li><strong>"XRP Ledger (XRP)"</strong>" and issued tokens"</li>
                <li><strong>"Cardano (ADA)"</strong>" and native tokens"</li>
            </ul>

            <h3>"How it works"</h3>
            <ol>
                <li>"Enter up to 30 wallet addresses, from any mix of blockchains. The chain is detected for you."</li>
                <li>"Choose any date from 2009 to today."</li>
                <li>"Get each address's balance and tokens at the end of that day, totals per asset, a chart, and a PDF you can save."</li>
            </ol>

            <h3>"Frequently asked questions"</h3>
            <div class="faq">
                {FAQ.iter().map(|(q, a)| view! {
                    <details>
                        <summary>{*q}</summary>
                        <p>{*a}</p>
                    </details>
                }).collect_view()}
            </div>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::FAQ;

    /// The FAQPage structured data in index.html must match the visible FAQ.
    #[test]
    fn structured_data_matches_visible_faq() {
        let html = include_str!("../index.html");
        for (q, a) in FAQ {
            assert!(html.contains(&format!("\"name\": \"{q}\"")), "question missing from index.html: {q}");
            assert!(html.contains(&format!("\"text\": \"{a}\"")), "answer missing from index.html: {a}");
        }
    }
}
