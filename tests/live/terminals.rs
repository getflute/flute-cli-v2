//! Live scenarios for `terminals`. **Committed.**
//!
//! Both operations are reads, so this is the one group whose live coverage
//! destroys nothing and needs nobody at the device.

use crate::*;

#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_terminal_list_first_page() {
    account(Merchant);
    let listed = json(&["terminals", "list"]);
    assert_eq!(listed["object"], "terminal_list");
    assert!(listed["data"].is_array(), "{listed}");
}

/// The filter accepts the `terminalStatus` value it declares.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_terminal_list_filtered_by_the_declared_query_status() {
    account(Merchant);
    let listed = json(&["terminals", "list", "--status", "ready"]);
    assert!(listed["data"].is_array(), "{listed}");
}

#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_terminal_list_filtered_by_mode_and_connection() {
    account(Merchant);
    let listed = json(&[
        "terminals",
        "list",
        "--mode",
        "semi-integrated",
        "--connection",
        "online",
        "--page-size",
        "5",
    ]);
    assert!(listed["data"].is_array(), "{listed}");
}

/// What the runtime reports for a terminal's status: `Ready`, `Busy` or
/// `Offline`, the values `--status` filters on.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_terminal_status_reports_a_declared_status() {
    account(Terminal);
    let status = json(&["terminals", "status", &terminal_id()]);
    assert_eq!(status["object"], "terminal_status");
    let reported = status["data"]["terminalStatus"]
        .as_str()
        .expect("the status response carried no terminalStatus");
    assert!(
        ["Ready", "Busy", "Offline"].contains(&reported),
        "terminalStatus was {reported}, outside the three values the CLI \
         documents and filters on."
    );
}

/// A terminal id that does not exist is a not-found read, exit 4 — the plain
/// rule, since the idempotent-404 exemption is confined to delete, revoke and
/// cancel.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_terminal_status_of_an_unknown_terminal_exits_four() {
    account(Merchant);
    live_bin()
        .args([
            "terminals",
            "status",
            "00000000-0000-4000-8000-000000000000",
        ])
        .assert()
        .code(4);
}
