//! Layer 5: the live sandbox suite. **Committed.**
//!
//! Uncommitted tests cannot be a development guarantee: nothing stops them
//! rotting, and the executable record of what the API actually accepts would
//! live on one machine. Account-specific values come from `FLUTE2_LIVE_*`;
//! only the file supplying them is ignored.
//!
//! This crate builds only under the `live` feature, so a plain `cargo test`
//! stays hermetic and offline, and every test here is `#[ignore]`d as well.
//! Opt in with:
//!
//! ```sh
//! source .flute2-live.env
//! cargo test --features live --test live -- --ignored --test-threads=1 \
//!     --skip attended --skip irreversible --skip needs_terminal
//! ```
//!
//! **Set the credentials in the environment, not the keychain, for any run
//! you are not watching.** `load_with_env_fallback` tries `FLUTE2_CLIENT_ID`
//! and `FLUTE2_CLIENT_SECRET` first and only then the keychain, so filling
//! them in `.flute2-live.env` bypasses the store entirely.
//!
//! It matters because a macOS keychain grant is bound to the *binary*, and
//! `cargo test` rebuilds. A rebuilt `target/debug/flute2` is a different
//! application to the OS, its access is gone, and an unattended run then
//! fails **every** scenario with
//! `Platform secure storage failure: User canceled the operation` — an auth
//! error where a result should be, after waiting on a prompt nobody is there
//! to answer. One such run took nine times as long as the real thing and
//! measured nothing.
//!
//! **`--test-threads=1` is not optional.** These scenarios share one sandbox
//! account: the eight POS ones contend for a single terminal, and the settings
//! round trip reads a singleton, changes it, and puts it back. Run in
//! parallel they interfere with each other and report the interference as an
//! API defect.
//!
//! **Three suffixes carry the tiers**, so the exclusions are made at the
//! command line rather than by a runtime gate that could quietly pass:
//!
//! - `_attended` — needs a person. Somebody accepts a prompt on the terminal,
//!   paper comes out of a printer, or a real message is sent to a real
//!   recipient.
//! - `_needs_terminal` — needs a terminal to be provisioned on the account.
//!   `terminals list` is what says whether one is, and until it answers a row
//!   these panic on the unset `FLUTE2_LIVE_TERMINAL_ID` rather than reporting
//!   an API defect.
//! - `_irreversible` — cannot be undone, and changes what later scenarios
//!   see. Closing a settlement batch. Run it last or not at all.
//!
//! A suffix is what excludes a scenario, never a name fragment naming a
//! module: skipping `pos::` wholesale also skips the two POS *list* reads,
//! which need no terminal at all and pass against an empty collection. An
//! over-broad exclusion is indistinguishable from coverage that was never
//! there.
//!
//! Everything else is reversible and unattended: it creates what it needs and
//! removes it again. Run the full set only with somebody at the terminal:
//!
//! ```sh
//! cargo test --features live --test live -- --ignored --test-threads=1
//! ```
//!
//! The helpers live in this crate root rather than in `tests/live/mod.rs`,
//! and the scenarios are reached with an explicit `#[path]`. Two rustc rules
//! rule out the tidier layout: `tests/live.rs` alongside
//! `tests/live/mod.rs` is ambiguous, and a `mod` in a crate root resolves
//! against the root's own directory — so a bare `mod slice;` here would look
//! for `tests/slice.rs`.

#![allow(dead_code)]

#[path = "live/customers.rs"]
mod customers;

#[path = "live/payment_methods.rs"]
mod payment_methods;

#[path = "live/transactions.rs"]
mod transactions;

#[path = "live/pos.rs"]
mod pos;

#[path = "live/terminals.rs"]
mod terminals;

#[path = "live/settlements.rs"]
mod settlements;

#[path = "live/settings.rs"]
mod settings;

#[path = "live/payment_links.rs"]
mod payment_links;

#[path = "live/payment_sessions.rs"]
mod payment_sessions;

#[path = "live/api_keys.rs"]
mod api_keys;

#[path = "live/slice.rs"]
mod slice;

/// Sandbox identifiers come from the environment. Nothing account-specific is
/// ever written into this repository, which is public.
///
/// `where_to_find_it` is not decoration. Three of these five values are
/// sitting in the API already, and a caller told only that a variable is unset
/// goes looking in a dashboard for something one command would have printed.
/// The suite deliberately does **not** discover them at run time: that would
/// couple every transaction scenario to `settings payment-config`, so one
/// broken read would fail eight unrelated scenarios for the wrong reason.
/// Pinning the values keeps a failure attributable to the endpoint under test.
pub fn required(var: &str, where_to_find_it: &str) -> String {
    std::env::var(var).unwrap_or_else(|_| {
        panic!(
            "{var} is not set.\n\n  {where_to_find_it}\n\nCopy \
             .flute2-live.env.example to .flute2-live.env, fill it in, and \
             source it."
        )
    })
}

/// The card and ACH processors are **two different values**, and both are
/// marked `isDefault` — the flag is per payment method, not per account. A
/// single processor variable would send an ACH debit to a card processor,
/// which fails for a reason that looks nothing like the cause, so there is one
/// accessor per instrument and no general one to reach for by mistake.
pub fn card_processor_id() -> String {
    required(
        "FLUTE2_LIVE_CARD_PROCESSOR_ID",
        "`./target/debug/flute2 settings payment-config` lists the account's \
         processors; take the paymentProcessorId of the one whose type is \
         SandboxCard or Tsys. Both processors report isDefault, because that \
         flag is per payment method rather than per account.",
    )
}

pub fn ach_processor_id() -> String {
    required(
        "FLUTE2_LIVE_ACH_PROCESSOR_ID",
        "`./target/debug/flute2 settings payment-config` lists the account's \
         processors; take the paymentProcessorId of the one whose type is \
         SandboxAch or Ach. Sending an ACH request to the card processor is \
         the failure this separate variable exists to prevent.",
    )
}
pub fn merchant_id() -> String {
    required(
        "FLUTE2_LIVE_MERCHANT_ID",
        "`./target/debug/flute2 terminals list` shows merchantId on every row, and \
         so does `./target/debug/flute2 api-keys list`.",
    )
}
pub fn terminal_id() -> String {
    required(
        "FLUTE2_LIVE_TERMINAL_ID",
        "`./target/debug/flute2 terminals list` — take the ID of a row whose MODE is \
         SemiIntegrated and whose LINE is Online. POS cannot use a terminal in \
         any other mode, and an offline one fails every POS scenario for a \
         reason that is not the CLI's.",
    )
}
pub fn pos_device_id() -> String {
    required(
        "FLUTE2_LIVE_POS_DEVICE_ID",
        "the external identifier configured on the terminal itself. This is \
         the one value the API cannot hand back: v2 declares no device \
         endpoints at all. `./target/debug/flute2 pos list` shows posDeviceId, but \
         only for transactions that already exist, and creating one needs this \
         value first.",
    )
}
pub fn share_recipient() -> String {
    required(
        "FLUTE2_LIVE_SHARE_RECIPIENT",
        "your own mobile number in E.164 form (+15551234567) or your own email \
         address. A share sends a real message, so there is deliberately no \
         default to fall back on.",
    )
}

/// The billing address the sandbox's address-verification fixture **approves**.
///
/// Every card scenario needs it. AVS is enabled on this account: an address
/// it does not recognise answers `N` — "Neither the Street Address or ZIP Code
/// match the information on file" — and the charge is refused, which a
/// scenario asserting only success cannot tell from an approval.
///
/// This street and postcode together are the address that verifies on the
/// sandbox, so send both.
/// The country is not optional once any address is present:
/// `POST /v2/transactions/credit` answers
/// `countryId: Country Id must not be null when address is provided`, though
/// every `BillingAddress` field is declared nullable. `create` accepts a
/// partial address and `credit` does not, so the pair is sent in full here
/// and both endpoints are satisfied.
pub const AVS_MATCHES: [&str; 6] = [
    "--billing-line1",
    "123 Test St",
    "--billing-postal-code",
    "10001",
    "--billing-country",
    "US",
];

/// And an address it does not recognise, which is a **reliable decline**.
///
/// That is worth having on purpose: the exit-code contract says a decline
/// exits 0 and the caller reads `transactionStatus`, and this is the only way
/// to produce one to check it against.
pub const AVS_DOES_NOT_MATCH: [&str; 6] = [
    "--billing-line1",
    "1 Nowhere Road",
    "--billing-postal-code",
    "99999",
    "--billing-country",
    "US",
];

/// A `flute2` invocation against the **real** sandbox.
///
/// Deliberately unlike `support::bin` in one way and like it in another: there
/// is no base-URL override, so this reaches the real API — and the keychain is
/// **off**, so it reads credentials from the environment.
///
/// The keychain is out of reach rather than merely unused: every scenario
/// spawns its own process, a macOS keychain grant is bound to the *binary*,
/// and `cargo test` rebuilds. A rebuilt binary holds no grant, so a reachable
/// keychain raises an authorisation dialog per process and then fails.
/// Convenience for one command is not worth a run that measures nothing.
///
/// The credentials are required up front rather than left to fail per
/// scenario, so a run without them stops on the first with one legible reason.
/// The update check is still suppressed: a GitHub round trip is not part of
/// what these tests measure.
pub fn live_bin() -> assert_cmd::Command {
    let reason = "the live suite reads credentials from the environment rather \
                  than the keychain, because it spawns a process per scenario \
                  and a keychain grant is bound to the binary — a rebuild would \
                  raise one authorisation dialog per scenario. Put it in \
                  .flute2-live.env and source that.";
    // Required for the message, **never passed on**: `assert_cmd` prints every
    // variable a command sets in its failure output, so setting these here put
    // the client id and secret into the text of every failing scenario. They
    // are inherited from the surrounding shell instead, where `assert_cmd`
    // cannot see them to print them.
    required("FLUTE2_CLIENT_ID", reason);
    required("FLUTE2_CLIENT_SECRET", reason);

    let mut c = assert_cmd::Command::cargo_bin("flute2").unwrap();
    c.env_remove("FLUTE2_API_BASE_URL")
        .env_remove("FLUTE2_OAUTH_URL")
        .env("FLUTE2_NO_KEYCHAIN", "1")
        .env("FLUTE2_PROFILE", "sandbox")
        .env("FLUTE2_NO_UPDATE_CHECK", "1");
    c
}

/// Run a command and return its parsed JSON envelope.
pub fn json(args: &[&str]) -> serde_json::Value {
    let mut all = vec!["--output", "json"];
    all.extend_from_slice(args);
    let out = live_bin()
        .args(&all)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&out).expect("the live response was not a JSON envelope")
}

/// `referenceId` is part of the duplicate-check key, and differing reference
/// ids let the same card and amount be charged again. So the
/// dependable way to avoid a false duplicate is a fresh reference id, not a
/// fresh amount — amounts are the workaround for endpoints that accept no
/// reference id.
pub fn unique_reference_id() -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static SEQ: AtomicU32 = AtomicU32::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!(
        "flute2-live-{nanos}-{}",
        SEQ.fetch_add(1, Ordering::Relaxed)
    )
}

/// Amounts still need to differ where no reference id is accepted.
pub fn unique_amount() -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    // 10.00 .. 999.99 — above trivial floors, below any sandbox ceiling.
    const LO: u32 = 1_000;
    const HI: u32 = 99_999;
    static NEXT: std::sync::OnceLock<AtomicU32> = std::sync::OnceLock::new();

    let counter = NEXT.get_or_init(|| {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u32;
        AtomicU32::new(LO + seed % (HI - LO))
    });
    let cents = LO + (counter.fetch_add(1, Ordering::Relaxed) - LO) % (HI - LO);
    format!("{}.{:02}", cents / 100, cents % 100)
}
