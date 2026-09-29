//! Live scenarios for `payment-sessions`. **Committed.**
//!
//! Creating a session moves no money — the payer does that, on the checkout
//! page the session describes — so these are safe to run repeatedly.

use crate::*;

#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_payment_session_create_get_cancel() {
    let created = json(&[
        "payment-sessions",
        "create",
        "--amount",
        &unique_amount(),
        "--reference-id",
        &unique_reference_id(),
        "--page-name",
        "flute2 live",
    ]);
    assert_eq!(created["object"], "payment_session");
    let id = created["data"]["id"]
        .as_str()
        .expect("create returned no id")
        .to_string();

    let fetched = json(&["payment-sessions", "get", &id]);
    assert_eq!(fetched["object"], "payment_session");

    live_bin()
        .args(["payment-sessions", "cancel", &id, "--yes"])
        .assert()
        .success();
}

/// **The evidence for the missing identifier on `get`.**
///
/// `CreatePaymentSessionResponseDto` declares `id`.
/// `GetPaymentSessionResponseDto` declares no identifier at all — not `id`,
/// not `paymentSessionId` — so nothing in a read correlates it back to the
/// session that was read. If the runtime returns one anyway, the schema is
/// missing a property and the resource's `quiet` mode can use it.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_payment_session_get_carries_no_identifier() {
    let created = json(&[
        "payment-sessions",
        "create",
        "--amount",
        &unique_amount(),
        "--reference-id",
        &unique_reference_id(),
    ]);
    let id = created["data"]["id"].as_str().unwrap().to_string();
    let fetched = json(&["payment-sessions", "get", &id]);

    assert!(
        fetched["data"].get("id").is_none() && fetched["data"].get("paymentSessionId").is_none(),
        "the get response carries an identifier the schema does not declare, so \
         GetPaymentSessionResponseDto is missing a property: {}",
        fetched["data"]
    );

    live_bin()
        .args(["payment-sessions", "cancel", &id, "--yes"])
        .assert()
        .success();
}

/// **The evidence for the conditional amount rule.**
///
/// The schema declares nothing required, and the `amount` description carries
/// three mutually exclusive rules OpenAPI cannot express: greater than zero
/// for `Payment` and `PaymentAndSave`, exactly zero for `SaveMethod`, and
/// null for a flexible-amount session. A vault-only session is where the CLI
/// fills the amount in rather than leaving it null.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_payment_session_vault_only_sends_a_zero_amount() {
    let created = json(&[
        "payment-sessions",
        "create",
        "--mode",
        "save-method",
        "--customer-handling",
        "create-customer",
    ]);
    let id = created["data"]["id"].as_str().unwrap().to_string();
    live_bin()
        .args(["payment-sessions", "cancel", &id, "--yes"])
        .assert()
        .success();
}

/// A session with no amount at all is a flexible-amount session, set at
/// checkout — and with nothing else either, it is the smallest legal request
/// this endpoint takes.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_payment_session_with_no_amount_is_flexible() {
    let created = json(&["payment-sessions", "create"]);
    let id = created["data"]["id"].as_str().unwrap().to_string();
    live_bin()
        .args(["payment-sessions", "cancel", &id, "--yes"])
        .assert()
        .success();
}

/// **A second cancel is a 400, not a 404 — so the idempotent rule does not
/// reach this verb.**
///
/// `payment-sessions cancel` stands apart from the deletes and revokes, where
/// a repeat is success because the caller's intent already holds. A cancelled
/// session answers
/// `Payment Session can be canceled only in Created status`, which is a
/// validation failure, exit 3.
///
/// The CLI's 404-to-success mapping stays, because a genuinely missing session
/// is still a 404 and still success. It simply never fires for a double
/// cancel, and mapping this 400 to success instead would hide a real state
/// error behind a rule written for a different one.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_payment_session_cancel_twice_reports_the_second_as_a_state_error() {
    let created = json(&[
        "payment-sessions",
        "create",
        "--amount",
        &unique_amount(),
        "--reference-id",
        &unique_reference_id(),
    ]);
    let id = created["data"]["id"].as_str().unwrap().to_string();

    live_bin()
        .args(["payment-sessions", "cancel", &id, "--yes"])
        .assert()
        .success();

    live_bin()
        .args(["payment-sessions", "cancel", &id, "--yes"])
        .assert()
        .code(3)
        .stderr(predicates::str::contains("Created status"));
}

/// Metadata is stored and handed back on the read, which is the only thing it
/// is for — so a round trip is the only way to see it arrived.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_payment_session_metadata_round_trip() {
    let created = json(&[
        "payment-sessions",
        "create",
        "--amount",
        &unique_amount(),
        "--reference-id",
        &unique_reference_id(),
        "--metadata",
        "orderId=9921",
        "--metadata",
        "guestNotes=Member of the loyalty scheme",
    ]);
    let id = created["data"]["id"].as_str().unwrap().to_string();

    let fetched = json(&["payment-sessions", "get", &id]);
    assert_eq!(fetched["data"]["metadata"]["orderId"], "9921");

    live_bin()
        .args(["payment-sessions", "cancel", &id, "--yes"])
        .assert()
        .success();
}
