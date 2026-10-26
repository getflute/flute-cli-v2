//! Live scenarios for `pos`. **Committed.**
//!
//! POS is the one group whose happy path needs a person. A create against a
//! terminal in semi-integrated mode puts a prompt on that terminal's screen,
//! and nothing here can accept or decline it — so the scenarios that wait are
//! written to be run with somebody standing at the device, and say so.
//!
//! Ctrl-C during a `--wait` poll is the CLI's only exit-130 path and has no
//! scenario at all: `assert_cmd` cannot deliver a signal, and neither can a
//! test here. It is exercised by hand.

use crate::*;

/// The unwaiting path, end to end: a create returns immediately, the same
/// transaction reads back, and the cancel closes it out.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_pos_create_get_cancel() {
    account(Terminal);
    let id = pos_create(&["--reference-id", &unique_reference_id()]);

    let fetched = json(&["pos", "get", &id]);
    assert_eq!(fetched["data"]["posTransactionId"], id);

    pos_cancel(&id);
}

/// **The evidence that `referenceId` is optional.**
///
/// `CreatePosTransactionRequestDto` does not declare it required,
/// so v2 leaves it optional — and if this fails, the rejection is a
/// documentation defect to file, not a flag to force back on.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_pos_create_without_a_reference_id() {
    account(Terminal);
    let id = pos_create(&[]);
    pos_cancel(&id);
}

/// **Needs a person at the terminal.** Accept or decline the prompt this
/// raises; the assertion is only that the poll ended somewhere other than
/// `InProgress`, because either answer is a terminal state.
///
/// This is the only thing that can show the two wait controls are both doing
/// something: `waitForAcceptanceByTerminal` holds the create until the device
/// answers, and `waitForTransactionProcessing=true` holds each poll open
/// until the state moves.
#[test]
#[ignore = "live sandbox, and needs somebody at the terminal; opt in with --ignored"]
fn live_pos_create_with_wait_ends_on_a_terminal_status_attended() {
    account(Terminal);
    let finished = json(&[
        "pos",
        "create",
        "--terminal-id",
        &terminal_id(),
        "--pos-device-id",
        &pos_device_id(),
        "--amount",
        &unique_amount(),
        "--currency-code",
        "USD",
        "--reference-id",
        &unique_reference_id(),
        "--wait",
        "--wait-timeout",
        "180",
    ]);
    let status = finished["data"]["posTransactionStatus"]
        .as_str()
        .expect("the waited create returned no posTransactionStatus");
    assert_ne!(
        status, "InProgress",
        "the poll returned while the transaction was still in progress, so it \
         is not terminating on posTransactionStatus"
    );
}

/// A cancelled transaction still exists and refuses a second cancel as a state
/// error, so the repeat is a 400 and exit 3, and the transaction stays
/// cancelled.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_pos_cancel_twice_reports_the_second_as_a_state_error() {
    account(Terminal);
    let id = pos_create(&["--reference-id", &unique_reference_id()]);
    pos_cancel(&id);
    live_bin()
        .args(["pos", "cancel", &id, "--yes"])
        .assert()
        .code(3);
    let fetched = json(&["pos", "get", &id]);
    assert_eq!(fetched["data"]["posTransactionStatus"], "Cancelled");
}

#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_pos_list_first_page() {
    account(Merchant);
    let listed = json(&["pos", "list"]);
    assert_eq!(listed["object"], "pos_transaction_list");
    assert!(listed["data"].is_array(), "{listed}");
}

/// The status filter is an equality filter over the four-value enum, and a
/// value the CLI spells wrongly comes back as a 400 rather than an empty page.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_pos_list_filtered_by_status() {
    account(Merchant);
    let listed = json(&["pos", "list", "--status", "completed", "--page-size", "5"]);
    assert!(listed["data"].is_array(), "{listed}");
}

/// **Needs a terminal with paper, and somebody at it.** The API prints a
/// receipt only for a `Completed` transaction, so the sale is made on the
/// terminal first — present a card when it prompts. The print answers an
/// empty 200 whether or not paper comes out — a terminal reporting
/// `printerStatus: NotNormal` with no paper loaded answers the same — so exit 0
/// shows only that the request was accepted. Whether it printed is read off the
/// device.
#[test]
#[ignore = "live sandbox, and prints on a real device; opt in with --ignored"]
fn live_pos_print_receipt_attended() {
    account(Terminal);
    let sale = json(&[
        "pos",
        "create",
        "--terminal-id",
        &terminal_id(),
        "--pos-device-id",
        &pos_device_id(),
        "--amount",
        &unique_amount(),
        "--currency-code",
        "USD",
        "--reference-id",
        &unique_reference_id(),
        "--wait",
        "--wait-timeout",
        "180",
    ]);
    assert_eq!(sale["data"]["posTransactionStatus"], "Completed", "{sale}");
    let id = sale["data"]["posTransactionId"].as_str().unwrap();

    live_bin()
        .args(["pos", "print-receipt", id, "--terminal-id", &terminal_id()])
        .assert()
        .success();
}

/// **The evidence for the get response's own example.**
///
/// `GetPosTransactionResponseDto` declares `posTransactionStatus` under
/// `additionalProperties: false`, and the operation's own example answers with
/// `status` and `targetTransactionId` — two keys the schema does not declare.
/// The fixtures follow the schema, because a schema is normative and an
/// example is not; this is the only thing that can say which the runtime
/// sends.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_pos_get_uses_the_declared_status_key() {
    account(Terminal);
    let id = pos_create(&["--reference-id", &unique_reference_id()]);
    let fetched = json(&["pos", "get", &id]);
    pos_cancel(&id);
    assert!(
        fetched["data"].get("posTransactionStatus").is_some(),
        "the get response carries no posTransactionStatus: {}",
        fetched["data"]
    );
    assert!(
        fetched["data"].get("status").is_none(),
        "the get response carries `status`, which the schema does not declare: {}",
        fetched["data"]
    );
}
