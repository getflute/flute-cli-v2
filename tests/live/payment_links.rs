//! Live scenarios for `payment-links`. **Committed.**
//!
//! `share` actually sends a message. Every scenario that shares uses a
//! recipient from the environment rather than a literal, so nothing here can
//! text a stranger.

use crate::*;

/// The whole lifecycle, so no long-lived link is assumed to exist.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_payment_link_create_get_update_delete() {
    let created = json(&[
        "payment-links",
        "create",
        "--card-enabled",
        "--amount",
        &unique_amount(),
        "--currency-code",
        "USD",
        "--name",
        "flute2 live",
        "--reference-id",
        &unique_reference_id(),
    ]);
    assert_eq!(created["object"], "payment_link");
    let id = created["data"]["paymentLinkId"]
        .as_str()
        .expect("create returned no paymentLinkId")
        .to_string();

    let fetched = json(&["payment-links", "get", &id]);
    assert_eq!(fetched["data"]["paymentLinkId"], id);

    let updated = json(&["payment-links", "update", &id, "--name", "flute2 live 2"]);
    assert_eq!(updated["data"]["name"], "flute2 live 2");

    live_bin()
        .args(["payment-links", "delete", &id, "--yes"])
        .assert()
        .success();
}

/// **The evidence for the 204s.** `DELETE` and `share` are the only two
/// operations in the API that answer 204 rather than 200, and a renderer that
/// tried to parse a body would fail on both.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_payment_link_delete_answers_no_content() {
    let created = json(&[
        "payment-links",
        "create",
        "--card-enabled",
        "--amount",
        &unique_amount(),
        "--currency-code",
        "USD",
    ]);
    let id = created["data"]["paymentLinkId"].as_str().unwrap();
    live_bin()
        .args(["payment-links", "delete", id, "--yes"])
        .assert()
        .success();
}

/// The idempotent-404 rule, on a delete that answers 204 the first time.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_payment_link_delete_twice_is_still_success() {
    let created = json(&[
        "payment-links",
        "create",
        "--card-enabled",
        "--amount",
        &unique_amount(),
        "--currency-code",
        "USD",
    ]);
    let id = created["data"]["paymentLinkId"].as_str().unwrap();
    for _ in 0..2 {
        live_bin()
            .args(["payment-links", "delete", id, "--yes"])
            .assert()
            .success();
    }
}

/// **Sends a real message.** The recipient comes from
/// `FLUTE2_LIVE_SHARE_RECIPIENT` so this cannot reach anybody who has not
/// opted in by setting it.
#[test]
#[ignore = "live sandbox, and sends a message; opt in with --ignored"]
fn live_payment_link_share_attended() {
    let created = json(&[
        "payment-links",
        "create",
        "--card-enabled",
        "--amount",
        &unique_amount(),
        "--currency-code",
        "USD",
    ]);
    let id = created["data"]["paymentLinkId"]
        .as_str()
        .unwrap()
        .to_string();

    live_bin()
        .args([
            "payment-links",
            "share",
            &id,
            "--share-by",
            "sms",
            "--recipient",
            &share_recipient(),
            "--consent",
        ])
        .assert()
        .success();

    live_bin()
        .args(["payment-links", "delete", &id, "--yes"])
        .assert()
        .success();
}

#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_payment_link_list_first_page() {
    let listed = json(&["payment-links", "list"]);
    assert_eq!(listed["object"], "payment_link_list");
    assert!(listed["data"].is_array(), "{listed}");
}

#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_payment_link_list_filtered() {
    let listed = json(&[
        "payment-links",
        "list",
        "--link-type",
        "single-use",
        "--status",
        "active",
        "--page-size",
        "5",
    ]);
    assert!(listed["data"].is_array(), "{listed}");
}

/// **The evidence that a flexible-amount link is legal.** `baseAmount` is
/// documented as omittable so the payer enters the amount at checkout, and
/// only the absent case can show the CLI is not sending a zero or a null.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_payment_link_without_an_amount_is_flexible() {
    let created = json(&[
        "payment-links",
        "create",
        "--card-enabled",
        // Required by the API though the schema marks it optional, and
        // required even here where there is no amount to denominate.
        "--currency-code",
        "USD",
        "--link-type",
        "multi-use",
        "--name",
        "flute2 flexible",
    ]);
    let id = created["data"]["paymentLinkId"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        created["data"]["baseAmount"].is_null(),
        "a link created with no amount reported one: {}",
        created["data"]
    );
    live_bin()
        .args(["payment-links", "delete", &id, "--yes"])
        .assert()
        .success();
}

/// **The evidence for the explicit-null clear.** `UpdatePaymentLinkRequestDto`
/// is documented as RFC 7396 merge patch, where a null clears a value. The CLI
/// spells that null as an empty value, and this is the only thing that can show
/// the API reads it that way rather than rejecting the null.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_payment_link_update_can_clear_a_field() {
    let created = json(&[
        "payment-links",
        "create",
        "--card-enabled",
        "--amount",
        &unique_amount(),
        "--currency-code",
        "USD",
        "--description",
        "to be cleared",
    ]);
    let id = created["data"]["paymentLinkId"]
        .as_str()
        .unwrap()
        .to_string();

    let updated = json(&["payment-links", "update", &id, "--description", ""]);
    assert!(
        updated["data"]["description"].is_null(),
        "the description survived an explicit null, so this is not a merge \
         patch: {}",
        updated["data"]
    );

    live_bin()
        .args(["payment-links", "delete", &id, "--yes"])
        .assert()
        .success();
}
