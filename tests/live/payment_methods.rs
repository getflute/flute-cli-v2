//! Live scenarios for `payment-methods`. **Committed.**

use crate::*;

/// A card is added, read back and removed, so no long-lived instrument is
/// assumed to exist.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_payment_method_add_card_get_delete() {
    let added = json(&[
        "payment-methods",
        "add-card",
        "--card",
        "4111111111111111",
        "--cvv",
        "123",
        "--exp",
        "12/2033",
        "--name",
        "My Visa Card",
    ]);
    let id = added["data"]["paymentMethodId"]
        .as_str()
        .expect("add-card returned no paymentMethodId")
        .to_string();

    let fetched = json(&["payment-methods", "get", &id]);
    assert_eq!(fetched["data"]["paymentMethodId"], id);
    assert_eq!(fetched["data"]["type"], "Card");

    live_bin()
        .args(["payment-methods", "delete", &id, "--yes"])
        .assert()
        .success();
}

/// The ACH shape is where the unfamiliar required fields are, and its two
/// enums are case-sensitive on the wire.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_payment_method_add_ach_business() {
    let added = json(&[
        "payment-methods",
        "add-ach",
        "--account",
        "123456789",
        "--routing",
        "021000021",
        "--account-type",
        "checking",
        "--account-holder-type",
        "business",
        "--company-name",
        "Analytical Engines",
        "--name",
        "Business Checking",
    ]);
    let id = added["data"]["paymentMethodId"]
        .as_str()
        .unwrap()
        .to_string();

    live_bin()
        .args(["payment-methods", "delete", &id, "--yes"])
        .assert()
        .success();
}

/// **The evidence for the enum-versus-example disagreement.**
///
/// `PaymentMethodResponseDto.type` declares exactly `Card`, `ACH` and `Cash`,
/// while the operation's own ACH response example says `ElectronicCheck`.
/// Both cannot be right, and this is the only thing that can say which.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_payment_method_type_matches_the_declared_enum() {
    let added = json(&[
        "payment-methods",
        "add-ach",
        "--account",
        "123456789",
        "--routing",
        "021000021",
        "--account-type",
        "checking",
        "--account-holder-type",
        "personal",
    ]);
    let id = added["data"]["paymentMethodId"]
        .as_str()
        .unwrap()
        .to_string();

    let fetched = json(&["payment-methods", "get", &id]);
    let observed = fetched["data"]["type"].as_str().unwrap_or_default();
    assert!(
        ["Card", "ACH", "Cash"].contains(&observed),
        "type was {observed:?}, which the declared enum does not contain. If \
         this is `ElectronicCheck`, the schema's enum is wrong rather than the \
         example, and every fixture asserting a declared value must change."
    );

    live_bin()
        .args(["payment-methods", "delete", &id, "--yes"])
        .assert()
        .success();
}

#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_payment_method_list_for_a_customer() {
    let customer = json(&[
        "customers",
        "create",
        "--first-name",
        "Ada",
        "--last-name",
        "Lovelace",
    ]);
    let customer_id = customer["data"]["customerId"].as_str().unwrap().to_string();

    let added = json(&[
        "payment-methods",
        "add-card",
        "--card",
        "4111111111111111",
        "--cvv",
        "123",
        "--exp",
        "12/2033",
        "--customer-id",
        &customer_id,
    ]);
    let id = added["data"]["paymentMethodId"]
        .as_str()
        .unwrap()
        .to_string();

    let listed = json(&["payment-methods", "list", "--customer-id", &customer_id]);
    let ids: Vec<&str> = listed["data"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m["paymentMethodId"].as_str())
        .collect();
    assert!(ids.contains(&id.as_str()), "{listed}");

    live_bin()
        .args(["customers", "delete", &customer_id, "--yes"])
        .assert()
        .success();
}

/// `update` answers 200 with no body, so the change is only visible on a
/// read back.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_payment_method_update_name() {
    let added = json(&[
        "payment-methods",
        "add-card",
        "--card",
        "4111111111111111",
        "--cvv",
        "123",
        "--exp",
        "12/2033",
        "--name",
        "Before",
    ]);
    let id = added["data"]["paymentMethodId"]
        .as_str()
        .unwrap()
        .to_string();

    json(&["payment-methods", "update", &id, "--name", "After"]);
    let fetched = json(&["payment-methods", "get", &id]);
    assert_eq!(fetched["data"]["name"], "After");

    live_bin()
        .args(["payment-methods", "delete", &id, "--yes"])
        .assert()
        .success();
}

/// `set-default` takes a **required `customerId` query parameter** and
/// answers 200 with no body — both easy to miss, and both fail conformance
/// if missed.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_payment_method_set_default() {
    let customer = json(&[
        "customers",
        "create",
        "--first-name",
        "Ada",
        "--last-name",
        "Lovelace",
    ]);
    let customer_id = customer["data"]["customerId"].as_str().unwrap().to_string();
    let added = json(&[
        "payment-methods",
        "add-card",
        "--card",
        "4111111111111111",
        "--cvv",
        "123",
        "--exp",
        "12/2033",
        "--customer-id",
        &customer_id,
    ]);
    let id = added["data"]["paymentMethodId"]
        .as_str()
        .unwrap()
        .to_string();

    json(&[
        "payment-methods",
        "set-default",
        &id,
        "--customer-id",
        &customer_id,
    ]);
    let fetched = json(&["payment-methods", "get", &id]);
    assert_eq!(fetched["data"]["isDefault"], true);

    live_bin()
        .args(["customers", "delete", &customer_id, "--yes"])
        .assert()
        .success();
}

/// The idempotent-404 rule on a second delete.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_payment_method_delete_twice_is_still_success() {
    let added = json(&[
        "payment-methods",
        "add-card",
        "--card",
        "4111111111111111",
        "--cvv",
        "123",
        "--exp",
        "12/2033",
    ]);
    let id = added["data"]["paymentMethodId"]
        .as_str()
        .unwrap()
        .to_string();
    for _ in 0..2 {
        live_bin()
            .args(["payment-methods", "delete", &id, "--yes"])
            .assert()
            .success();
    }
}
