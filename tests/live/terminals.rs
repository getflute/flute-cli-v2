//! Live scenarios for `terminals`. **Committed.**
//!
//! Both operations are reads, so this is the one group whose live coverage
//! destroys nothing and needs nobody at the device.

use crate::*;

#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_terminal_list_first_page() {
    let listed = json(&["terminals", "list"]);
    assert_eq!(listed["object"], "terminal_list");
    assert!(listed["data"].is_array(), "{listed}");
}

/// **The evidence for the two `terminalStatus` enums.**
///
/// The query parameter declares `Ready`, `Busy`, `Offline`; the response
/// declares `Active`, `Busy`, `Offline`. `Ready` and `Active` cannot both be
/// the same state's name, and this is the half that shows whether the
/// *filter* accepts the value it declares.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_terminal_list_filtered_by_the_declared_query_status() {
    let listed = json(&["terminals", "list", "--status", "ready"]);
    assert!(listed["data"].is_array(), "{listed}");
}

#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_terminal_list_filtered_by_mode_and_connection() {
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

/// The other half of the enum question: what the runtime actually *reports*.
///
/// The fixtures send `Active`, because the response schema declares it and an
/// example is not normative. If the runtime answers `Ready`, the response
/// enum is wrong rather than its example, and both fixtures change.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_terminal_status_reports_a_declared_status_needs_terminal() {
    let status = json(&["terminals", "status", &terminal_id()]);
    assert_eq!(status["object"], "terminal_status");
    let reported = status["data"]["terminalStatus"]
        .as_str()
        .expect("the status response carried no terminalStatus");
    assert!(
        ["Active", "Busy", "Offline"].contains(&reported),
        "terminalStatus was {reported}, which GetTerminalStatusResponseDto does \
         not declare. The response enum and its own example disagree, and this \
         is which one the runtime follows."
    );
}

/// A terminal id that does not exist is a not-found read, exit 4 — the plain
/// rule, since the idempotent-404 exemption is confined to delete, revoke and
/// cancel.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_terminal_status_of_an_unknown_terminal_exits_four() {
    live_bin()
        .args([
            "terminals",
            "status",
            "00000000-0000-4000-8000-000000000000",
        ])
        .assert()
        .code(4);
}
