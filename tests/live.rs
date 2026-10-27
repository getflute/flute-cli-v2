//! Layer 5: the live suite. **Committed.**
//!
//! Uncommitted tests cannot be a development guarantee: nothing stops them
//! rotting, and the executable record of what the API actually accepts would
//! live on one machine. Account-specific values come from the three account
//! files; only those files are ignored.
//!
//! This crate builds only under the `live` feature, so a plain `cargo test`
//! stays hermetic and offline, and every test here is `#[ignore]`d as well.
//! Opt in with `scripts/live.sh`, or by hand:
//!
//! ```sh
//! cargo test --features live --test live -- --ignored --test-threads=1 \
//!     --skip attended --skip irreversible --skip needs_configured_user
//! ```
//!
//! **Each scenario runs once, as the account it names.** Its first line is
//! `account(..)` with one of the [`Account`] variants, and every command it
//! builds after that carries the credentials and ids from that account's
//! file, in `FLUTE2_LIVE_DIR` or the repository root. The merchant and
//! partner are production sandbox accounts; the onboarding partner and the
//! terminal account may be on another environment, never production live.
//!
//! **The credentials never reach the keychain.** A macOS keychain grant is
//! bound to the *binary*, and `cargo test` rebuilds. A rebuilt
//! `target/debug/flute2` is a different application to the OS, its access is
//! gone, and an unattended run then fails **every** scenario with
//! `Platform secure storage failure: User canceled the operation` — an auth
//! error where a result should be, after waiting on a prompt nobody is there
//! to answer.
//!
//! **`--test-threads=1` is not optional.** Scenarios share accounts: the POS
//! ones contend for a single terminal, and the settings round trip reads a
//! singleton, changes it, and puts it back. Run in parallel they interfere
//! with each other and report the interference as an API defect.
//!
//! **Three suffixes mark what an unattended run leaves out**, so the exclusion
//! is made at the command line rather than by a runtime gate that could
//! quietly pass:
//!
//! - `_attended` — needs a person. Somebody accepts a prompt on the terminal,
//!   paper comes out of a printer, or a real message is sent to a real
//!   recipient.
//! - `_irreversible` — cannot be undone, and changes what later scenarios
//!   see. Closing a settlement batch. Run it last or not at all.
//! - `_needs_configured_user` — needs a partner user whose Schedule As and
//!   Split Profiles are set up, which the onboarding partner's user is not.
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

#[path = "live/applications.rs"]
mod applications;

#[path = "live/users.rs"]
mod users;

#[path = "live/slice.rs"]
mod slice;

/// The account a scenario runs as, each from its own file in
/// `FLUTE2_LIVE_DIR` (the repository root when unset). Each scenario names
/// one with [`account`] before its first command.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Account {
    /// A production sandbox merchant: `.flute2-live-merchant.env`.
    Merchant,
    /// A production sandbox partner: `.flute2-live-partner.env`.
    Partner,
    /// A partner with the applications and user-configuration API
    /// permissions, anywhere but production live:
    /// `.flute2-live-partner-uat.env`.
    OnboardingPartner,
    /// A merchant with a semi-integrated terminal, anywhere but production
    /// live: `.flute2-live-terminal.env`.
    Terminal,
}
pub use Account::{Merchant, OnboardingPartner, Partner, Terminal};

impl Account {
    fn file(self) -> &'static str {
        match self {
            Merchant => ".flute2-live-merchant.env",
            Partner => ".flute2-live-partner.env",
            OnboardingPartner => ".flute2-live-partner-uat.env",
            Terminal => ".flute2-live-terminal.env",
        }
    }
}

thread_local! {
    static CURRENT: std::cell::RefCell<Option<(Account, std::collections::HashMap<String, String>)>> =
        const { std::cell::RefCell::new(None) };
}

/// Run the rest of this scenario as `which`, from its file.
///
/// The merchant and partner are production sandbox accounts, so their files
/// name no other host. The onboarding partner and the terminal account may be
/// on the production sandbox or another environment, never production live:
/// one invites merchants and the other takes cards.
pub fn account(which: Account) {
    let dir =
        std::env::var("FLUTE2_LIVE_DIR").unwrap_or_else(|_| env!("CARGO_MANIFEST_DIR").into());
    let path = std::path::Path::new(&dir).join(which.file());
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "this scenario runs as the {which:?} account, from {}: {e}. Copy \
             .flute2-live.env.example there and fill it in.",
            path.display()
        )
    });
    let vars = parse_env_file(&text);
    let host = vars.get("FLUTE2_LIVE_API_BASE_URL");
    match which {
        Merchant | Partner => assert!(
            host.is_none(),
            "{}: this account runs on the production sandbox, so the file sets \
             no FLUTE2_LIVE_API_BASE_URL",
            path.display()
        ),
        OnboardingPartner | Terminal => assert!(
            host.is_none_or(|h| !h.trim_end_matches('/').ends_with("//api.flute.com")),
            "{}: the {which:?} account may not be on production live",
            path.display()
        ),
    }
    CURRENT.with(|c| *c.borrow_mut() = Some((which, vars)));
}

/// `KEY=value` lines, each optionally after `export `, quoted, or followed by
/// ` # comment`.
fn parse_env_file(text: &str) -> std::collections::HashMap<String, String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.trim_start_matches("export ").split_once('='))
        .map(|(k, v)| {
            let v = v.split(" #").next().unwrap_or(v).trim();
            (
                k.trim().to_string(),
                v.trim_matches(['"', '\'']).to_string(),
            )
        })
        .collect()
}

/// One value from the scenario's account file.
///
/// The ids are pinned rather than discovered at run time, so one broken read
/// cannot fail every scenario that needed an id from it. `where_to_find_it`
/// says how to look one up.
pub fn required(var: &str, where_to_find_it: &str) -> String {
    optional(var).unwrap_or_else(|| {
        let which = CURRENT
            .with(|c| c.borrow().as_ref().map(|(w, _)| *w))
            .unwrap();
        panic!(
            "{var} is not set in {}.\n\n  {where_to_find_it}",
            which.file()
        )
    })
}

fn optional(var: &str) -> Option<String> {
    CURRENT.with(|c| {
        let current = c.borrow();
        let (_, vars) = current
            .as_ref()
            .expect("the scenario named no account: call account(..) before its first command");
        vars.get(var).cloned()
    })
}

/// The card and ACH processors are **two different values**, and both are
/// marked `isDefault` — the flag is per payment method, not per account. A
/// single processor variable would send an ACH debit to a card processor,
/// which fails for a reason that looks nothing like the cause.
pub fn card_processor_id() -> String {
    required(
        "FLUTE2_LIVE_CARD_PROCESSOR_ID",
        "`flute2 settings payment-config` lists the account's processors; take \
         the paymentProcessorId of the one whose type is SandboxCard or Tsys.",
    )
}
pub fn ach_processor_id() -> String {
    required(
        "FLUTE2_LIVE_ACH_PROCESSOR_ID",
        "`flute2 settings payment-config` lists the account's processors; take \
         the paymentProcessorId of the one whose type is SandboxAch or Ach.",
    )
}
pub fn merchant_id() -> String {
    required(
        "FLUTE2_LIVE_MERCHANT_ID",
        "`flute2 terminals list` shows merchantId on every row. In the partner \
         file, a merchant under that partner.",
    )
}
pub fn terminal_id() -> String {
    required(
        "FLUTE2_LIVE_TERMINAL_ID",
        "`flute2 terminals list`: the ID of a row whose MODE is SemiIntegrated \
         and whose LINE is Online.",
    )
}
pub fn pos_device_id() -> String {
    required(
        "FLUTE2_LIVE_POS_DEVICE_ID",
        "the external identifier configured on the terminal itself; v2 declares \
         no device endpoint that could hand it back.",
    )
}
pub fn partner_user_id() -> String {
    required(
        "FLUTE2_LIVE_PARTNER_USER_ID",
        "`flute2 users list --user-type partner` on the partner account.",
    )
}
pub fn share_recipient() -> String {
    required(
        "FLUTE2_LIVE_SHARE_RECIPIENT",
        "your own mobile number in E.164 form or your own email address. A \
         share sends a real message, so there is no default to fall back on.",
    )
}
/// The account's client id, to tell its own key from a new one.
pub fn client_id() -> String {
    required("FLUTE2_CLIENT_ID", "the account's client id.")
}

/// A `flute2` invocation as the scenario's account.
///
/// The credentials reach the child's environment and nothing else: a failure
/// is reported from the output alone, because a printed `Command` lists every
/// variable set on it. The keychain is out of reach — a macOS grant is bound
/// to the binary, `cargo test` rebuilds, and a rebuilt binary would raise an
/// authorisation dialog per scenario.
pub struct LiveCommand(std::process::Command);

impl LiveCommand {
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        self.0.args(args);
        self
    }

    pub fn output(mut self) -> std::process::Output {
        self.0.output().unwrap()
    }

    pub fn assert(self) -> assert_cmd::assert::Assert {
        use assert_cmd::assert::OutputAssertExt;
        self.output().assert()
    }
}

pub fn live_bin() -> LiveCommand {
    let mut c = std::process::Command::new(assert_cmd::cargo::cargo_bin("flute2"));
    for (k, _) in std::env::vars() {
        if k.starts_with("FLUTE2_") {
            c.env_remove(k);
        }
    }
    c.env(
        "FLUTE2_CLIENT_ID",
        required("FLUTE2_CLIENT_ID", "the account's client id."),
    )
    .env(
        "FLUTE2_CLIENT_SECRET",
        required("FLUTE2_CLIENT_SECRET", "the account's client secret."),
    )
    .env("FLUTE2_NO_KEYCHAIN", "1")
    .env("FLUTE2_NO_UPDATE_CHECK", "1")
    .env("FLUTE2_PROFILE", "sandbox");
    if let Some(api) = optional("FLUTE2_LIVE_API_BASE_URL") {
        let oauth = required(
            "FLUTE2_LIVE_OAUTH_URL",
            "it is set alongside FLUTE2_LIVE_API_BASE_URL: a token from one \
             environment is refused by the other's API.",
        );
        c.env("FLUTE2_API_BASE_URL", api)
            .env("FLUTE2_OAUTH_URL", oauth);
    }
    LiveCommand(c)
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

/// Run a command and return its parsed JSON envelope.
pub fn json(args: &[&str]) -> serde_json::Value {
    let mut all = vec!["--output", "json"];
    all.extend_from_slice(args);
    let out = live_bin().args(&all).output();
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("the live response was not a JSON envelope");
    assert!(
        out.status.success(),
        "`flute2 {}` failed: {v}",
        args.join(" ")
    );
    v
}

/// `pos create` on the scenario terminal, with the given extra arguments,
/// returning the created transaction's id.
///
/// A terminal holds one transaction at a time, and after a cancel it goes on
/// refusing a create with "Transaction is already in progress" for tens of
/// seconds while its status already reads `Ready`. Back-to-back scenarios land
/// in that window, so a refusal saying so is retried for up to a minute;
/// every other failure fails the scenario at once.
pub fn pos_create(extra: &[&str]) -> String {
    let (terminal, device, amount) = (terminal_id(), pos_device_id(), unique_amount());
    let mut args = vec![
        "--output",
        "json",
        "pos",
        "create",
        "--terminal-id",
        &terminal,
        "--pos-device-id",
        &device,
        "--amount",
        &amount,
        "--currency-code",
        "USD",
    ];
    args.extend_from_slice(extra);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let out = live_bin().args(&args).output();
        let v: serde_json::Value =
            serde_json::from_slice(&out.stdout).expect("the live response was not a JSON envelope");
        if out.status.success() {
            return v["data"]["posTransactionId"]
                .as_str()
                .expect("create returned no posTransactionId")
                .to_string();
        }
        let busy = v["message"]
            .as_str()
            .is_some_and(|m| m.contains("already in progress"));
        assert!(
            busy && std::time::Instant::now() < deadline,
            "pos create failed: {v}"
        );
        std::thread::sleep(std::time::Duration::from_secs(5));
    }
}

/// Cancel a scenario's POS transaction, so the terminal is free for the next.
pub fn pos_cancel(id: &str) {
    live_bin()
        .args(["pos", "cancel", id, "--yes"])
        .assert()
        .success();
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
