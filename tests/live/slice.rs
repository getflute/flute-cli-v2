//! Live scenarios spanning `ping`, `customers` and `transactions`.
//! **Committed.**

use crate::*;

#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_ping_succeeds() {
    let v = json(&["ping"]);
    assert_eq!(v["object"], "ping");
    assert_eq!(v["data"]["reachable"], true);
    assert_eq!(v["meta"]["environment"], "sandbox");
}

/// Creates its own customer and deletes it, so no long-lived id is assumed.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_customer_create_get_delete() {
    let created = json(&[
        "customers",
        "create",
        "--first-name",
        "Ada",
        "--last-name",
        "Lovelace",
    ]);
    let id = created["data"]["customerId"]
        .as_str()
        .expect("create returned no customerId")
        .to_string();

    let fetched = json(&["customers", "get", &id]);
    assert_eq!(fetched["object"], "customer");
    assert_eq!(fetched["data"]["customerId"], id);
    assert_eq!(fetched["data"]["firstName"], "Ada");

    live_bin()
        .args(["customers", "delete", &id, "--yes"])
        .assert()
        .success();
}

/// Named by the contract matrix as the evidence for the new-card variant, and
/// as the evidence that the response is a single transaction object, not the
/// page the published schema declares.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_card_sale_auto_capture() {
    let amount = unique_amount();
    let reference = unique_reference_id();
    let v = json(&[
        "transactions",
        "create",
        "--payment-processor-id",
        &card_processor_id(),
        "--amount",
        &amount,
        "--card",
        "4111111111111111",
        "--cvv",
        "123",
        "--exp",
        "12/2032",
        // The address the sandbox's AVS fixture approves; without it the
        // charge is declined and the assertion below means nothing.
        "--billing-line1",
        "123 Test St",
        "--billing-postal-code",
        "10001",
        "--billing-country",
        "US",
        "--capture-method",
        "auto",
        "--reference-id",
        &reference,
    ]);
    assert_eq!(v["object"], "transaction");
    assert!(
        v["data"]["transactionId"].is_string(),
        "no transactionId in {v}"
    );
    // A single object, not a page.
    assert!(
        v["data"].get("items").is_none(),
        "the API returned a page; the published schema may now be met -- \
         delete the transitional arm and this assertion together: {v}"
    );
    assert!(v["meta"].get("page_info").is_none());
    // In the read's shape, which is the schema the divergence validates
    // against: the containers the published examples show are absent, and
    // the read's own are present.
    for absent in ["processorResponse", "amountDetails", "type"] {
        assert!(
            v["data"].get(absent).is_none(),
            "the create answered with the example shape's {absent}: {v}"
        );
    }
    for present in ["amountBreakdown", "declineDetails", "processorDetails"] {
        assert!(
            v["data"].get(present).is_some(),
            "the create answered without the read shape's {present}: {v}"
        );
    }
    // Two fields the schema omits and the table formats as money.
    assert!(
        v["data"]["amountBreakdown"].get("taxAmount").is_some(),
        "no amountBreakdown.taxAmount: {v}"
    );
    assert!(
        v["data"]["transactionEvents"][0].get("amount").is_some(),
        "no transactionEvents[0].amount: {v}"
    );

    // And the transaction is readable back by id.
    let id = v["data"]["transactionId"].as_str().unwrap();
    let fetched = json(&["transactions", "get", id]);
    assert_eq!(fetched["data"]["transactionId"], id);
}

/// `captureMethod` is absent from the published `CardDataDto` and accepted at
/// runtime. Manual capture is the only way to create an
/// authorization, so if this fails the field is not merely undocumented — the
/// capability is gone.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_card_auth_manual_capture() {
    let v = json(&[
        "transactions",
        "create",
        "--payment-processor-id",
        &card_processor_id(),
        "--amount",
        &unique_amount(),
        "--card",
        "4111111111111111",
        "--cvv",
        "123",
        "--exp",
        "12/2032",
        // The address the sandbox's AVS fixture approves; without it the
        // charge is declined and the assertion below means nothing.
        "--billing-line1",
        "123 Test St",
        "--billing-postal-code",
        "10001",
        "--billing-country",
        "US",
        "--capture-method",
        "manual",
        "--reference-id",
        &unique_reference_id(),
    ]);
    assert!(v["data"]["transactionId"].is_string(), "{v}");
    // An authorization is not yet captured, whatever the sandbox calls that
    // state; asserting the exact string would pin a value this test cannot
    // verify is stable.
    assert_ne!(v["data"]["transactionStatus"], "Captured", "{v}");
}

/// `customerId` is absent from the published request schema and accepted at
/// runtime, linking the transaction to a customer.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_card_sale_with_customer() {
    let customer = json(&[
        "customers",
        "create",
        "--first-name",
        "Ada",
        "--last-name",
        "Lovelace",
    ]);
    let customer_id = customer["data"]["customerId"].as_str().unwrap().to_string();

    let v = json(&[
        "transactions",
        "create",
        "--payment-processor-id",
        &card_processor_id(),
        "--amount",
        &unique_amount(),
        "--card",
        "4111111111111111",
        "--cvv",
        "123",
        "--exp",
        "12/2032",
        // The address the sandbox's AVS fixture approves; without it the
        // charge is declined and the assertion below means nothing.
        "--billing-line1",
        "123 Test St",
        "--billing-postal-code",
        "10001",
        "--billing-country",
        "US",
        "--customer-id",
        &customer_id,
        "--reference-id",
        &unique_reference_id(),
    ]);
    assert!(v["data"]["transactionId"].is_string(), "{v}");
}
