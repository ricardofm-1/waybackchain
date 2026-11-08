//! Page end: support the project (donation addresses), disclaimer, and the
//! footer line (data note, cookies, copyright). Text as published on
//! proofofownership.tech by Daedalus Labs.

use leptos::prelude::*;

const DONATIONS: &[(&str, &str)] = &[
    ("Bitcoin", "bc1qeywc96rfefjr4220zewj9zxmds4yz0ctl0h63z"),
    ("Ethereum and EVM chains", "0x189419C6A19d11263c28E5391a473741e44EF459"),
    ("Solana", "9zgzGhbMYSj7Sac19jgSwP1UoZ6kWqXURHKs6ARe3w8X"),
    ("XRP", "rKGTvURZpK4ug3TrHPikoafywyaBjuMaXh"),
    (
        "Cardano",
        "addr1q8qwc455erdfevgj8xv78r262fs8eu6xln9u3s7lps8vvygckhpjgqxgmkcxvr2zhjjrdd9ty9ggev4yahznlzymg8cs5n6pt6",
    ),
];

fn copy_to_clipboard(text: &str) -> bool {
    let Some(window) = web_sys::window() else { return false };
    // Fire and forget: the promise resolves after the write.
    let _ = window.navigator().clipboard().write_text(text);
    true
}

#[component]
fn Donation(chain: &'static str, address: &'static str) -> impl IntoView {
    let copied = RwSignal::new(false);
    view! {
        <li class="donation">
            <span class="donation-chain">{chain}</span>
            <code class="donation-address">{address}</code>
            <button
                type="button"
                class="secondary small"
                aria-label=format!("Copy the {chain} address")
                on:click=move |_| {
                    if copy_to_clipboard(address) {
                        copied.set(true);
                        set_timeout(move || copied.set(false), std::time::Duration::from_millis(1600));
                    }
                }
            >
                {move || if copied.get() { "Copied" } else { "Copy" }}
            </button>
        </li>
    }
}

#[component]
pub fn SiteFooter() -> impl IntoView {
    view! {
        <footer class="site-footer">
            <section class="support" aria-labelledby="support-title">
                <h2 id="support-title">"Support the project"</h2>
                <p>
                    "This tool was built by a member of the crypto community, for the community. It is provided free of charge, without tracking or registration and it will remain so. The source code is open and may be freely inspected, forked, and self-hosted by anyone at request."
                </p>
                <p>
                    "The project has no institutional backing and generates no revenue. Contributions from users are entirely voluntary and are used to cover domain registration, development time, and the ongoing work of extending support to additional blockchains and signing standards."
                </p>
                <p>
                    "If this tool has been useful to you, a donation to any of the addresses below is genuinely appreciated. Contributions in code, issue reports, and translations are equally welcome through the project repository."
                </p>
                <p class="fine">
                    "Donations are voluntary, non-refundable, and confer no entitlement, service obligation, warranty, or commercial relationship of any kind."
                </p>
                <ul class="donations">
                    {DONATIONS.iter().map(|(chain, address)| view! { <Donation chain=chain address=address /> }).collect_view()}
                </ul>
            </section>

            <section class="disclaimer" aria-labelledby="disclaimer-title">
                <h2 id="disclaimer-title">"Disclaimer"</h2>
                <p>
                    "This tool is provided free of charge, “as is” and without warranties of any kind, express or implied, including but not limited to warranties of merchantability, fitness for a particular purpose, accuracy, or non-infringement."
                </p>
                <p>
                    "The source code is fully open and publicly available for independent review and audit. All cryptographic operations — message signing and signature verification — are performed locally in your browser. This site never requests, transmits, or stores private keys, seed phrases, messages, or signatures. However, open-source availability does not constitute a security guarantee. No audit, formal or informal, is implied, and the authors make no representation that the code is free of errors or vulnerabilities."
                </p>
                <p>
                    "Under no circumstances shall the authors or contributors be liable for any direct, indirect, incidental, or consequential damages arising from the use of, or inability to use, this tool. Use at your own risk."
                </p>
            </section>

            <div class="footer-line">
                <p>"Data comes from public blockchain APIs and may be incomplete for very active addresses."</p>
                <p>
                    "We use two cookies: one keeps your address list on this device, one counts your daily lookups. "
                    <a href="privacy.html">"Privacy"</a>
                </p>
                <p>
                    "Open source (MIT): "
                    <a href="https://github.com/ricardofm-1/waybackchain">"source code on GitHub"</a>
                </p>
                <p class="copyright">"Copyright © 2026 Daedalus Labs"</p>
            </div>
        </footer>
    }
}
