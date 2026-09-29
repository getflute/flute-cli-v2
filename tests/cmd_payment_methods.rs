mod support;

use predicates::prelude::*;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

const PM: &str = "6e22f0f3-f3ed-407c-a46f-ec9cffde5d5b";
const ACH_PM: &str = "6bbfbe3e-04dd-41cd-82bf-1466e0159007";
const CUS: &str = "588f57a5-fe6a-4844-851e-e98914e81980";

#[tokio::test]
async fn payment_method_list_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-get-payment-methods",
        "first page, server defaults",
    )
    .await;
    support::bin(&server)
        .args(["payment-methods", "list"])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn payment_method_list_filtered_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-get-payment-methods",
        "filtered by customer",
    )
    .await;
    support::bin(&server)
        .args([
            "payment-methods",
            "list",
            "--search",
            "visa",
            "--customer-id",
            CUS,
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn payment_method_get_card_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-get-payment-methods-paymentMethodId",
        "card",
    )
    .await;
    support::bin(&server)
        .args(["payment-methods", "get", PM])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn payment_method_get_ach_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-get-payment-methods-paymentMethodId",
        "ach",
    )
    .await;
    support::bin(&server)
        .args(["payment-methods", "get", ACH_PM])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn payment_method_add_card_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-post-payment-methods-cards",
        "required only",
    )
    .await;
    support::bin(&server)
        .args([
            "payment-methods",
            "add-card",
            "--card",
            "4111111111111111",
            "--exp",
            "12/2033",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn payment_method_add_card_full_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-post-payment-methods-cards",
        "linked to a customer and named",
    )
    .await;
    support::bin(&server)
        .args([
            "payment-methods",
            "add-card",
            "--card",
            "4111111111111111",
            "--cvv",
            "123",
            "--exp",
            "12/2033",
            "--customer-id",
            CUS,
            "--name",
            "My Visa Card",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn payment_method_add_ach_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-post-payment-methods-ach",
        "required only",
    )
    .await;
    support::bin(&server)
        .args([
            "payment-methods",
            "add-ach",
            "--account",
            "123456789",
            "--routing",
            "021000021",
            "--account-holder-type",
            "personal",
            "--account-type",
            "checking",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

/// The same bytes with `--account-type` left out: the flag carries the
/// default the API's required `accountType` needs.
#[tokio::test]
async fn add_ach_without_an_account_type_sends_checking() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-post-payment-methods-ach",
        "required only",
    )
    .await;
    support::bin(&server)
        .args([
            "payment-methods",
            "add-ach",
            "--account",
            "123456789",
            "--routing",
            "021000021",
            "--account-holder-type",
            "personal",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn payment_method_add_ach_business_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-post-payment-methods-ach",
        "business account, fully specified",
    )
    .await;
    support::bin(&server)
        .args([
            "payment-methods",
            "add-ach",
            "--account",
            "123456789",
            "--routing",
            "021000021",
            "--account-holder-type",
            "business",
            "--account-type",
            "savings",
            "--tax-id",
            "123456789",
            "--customer-id",
            CUS,
            "--name",
            "Business Savings",
            "--company-name",
            "Analytical Engines",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn payment_method_update_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-patch-payment-methods-paymentMethodId",
        "rename",
    )
    .await;
    support::bin(&server)
        .args(["payment-methods", "update", PM, "--name", "After"])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn payment_method_delete_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-delete-payment-methods-paymentMethodId",
        "default",
    )
    .await;
    support::bin(&server)
        .args(["payment-methods", "delete", PM, "--yes"])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn payment_method_set_default_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-post-payment-methods-paymentMethodId-set-default",
        "default",
    )
    .await;
    support::bin(&server)
        .args(["payment-methods", "set-default", PM, "--customer-id", CUS])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

/// The required query parameter is enforced by clap, so nothing is sent.
#[tokio::test]
async fn set_default_without_a_customer_id_is_a_usage_error() {
    let server = support::mock_with_token().await;
    support::bin(&server)
        .args(["payment-methods", "set-default", PM])
        .assert()
        .code(3);
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.url.path() == "/oauth2/token"),
    );
}

/// Destructive, so it is refused client-side with no request issued.
#[tokio::test]
async fn payment_method_delete_without_yes_issues_no_request() {
    let server = support::mock_with_token().await;
    let out = support::bin(&server)
        .args(["payment-methods", "delete", PM])
        .assert()
        .code(3)
        .get_output()
        .clone();
    assert_eq!(
        String::from_utf8_lossy(&out.stderr).trim_end(),
        format!(
            "Error: removal requires --yes to confirm \
             (e.g. `payment-methods delete {PM} --yes`)"
        )
    );
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.url.path() == "/oauth2/token"),
        "a refused destructive command must not reach the API"
    );
}

#[tokio::test]
async fn payment_method_delete_on_a_missing_instrument_exits_zero() {
    let server = support::mock_with_token().await;
    Mock::given(method("DELETE"))
        .and(path(format!("/v2/payment-methods/{PM}")))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(serde_json::json!({"Title": "Not found", "CorrelationId": "c-1"})),
        )
        .mount(&server)
        .await;
    let out = support::bin(&server)
        .args(["--output", "json", "payment-methods", "delete", PM, "--yes"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["data"]["paymentMethodId"], PM);
    assert_eq!(v["data"]["deleted"], false, "{v}");
    assert_eq!(v["data"]["found"], false, "{v}");
}

/// The card and ACH shapes have different masks in different containers, so
/// one column has to reach both.
#[tokio::test]
async fn list_table_shows_the_masked_number_for_either_instrument() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path("/v2/payment-methods"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [
                {"paymentMethodId": "pm_card", "type": "Card", "isDefault": true,
                 "card": {"cardMask": "************1111",
                          "expirationMonth": 1, "expirationYear": 2033}},
                {"paymentMethodId": "pm_ach", "type": "ACH", "isDefault": false,
                 "ach": {"accountNumber": "****6789"}}],
            "pageInfo": {"hasMore": false}})))
        .mount(&server)
        .await;

    support::bin(&server)
        .args(["--output", "table", "payment-methods", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("PAN/ACCT"))
        .stdout(predicate::str::contains("************1111"))
        .stdout(predicate::str::contains("****6789"))
        // The expiry renders as MM/YYYY, zero-padded.
        .stdout(predicate::str::contains("01/2033"))
        .stdout(predicate::str::contains("yes"))
        .stdout(predicate::str::contains("no"));
}

/// The detail view reaches into whichever instrument container is present,
/// on a resource whose payload is entirely nested.
#[tokio::test]
async fn get_table_shows_the_nested_ach_fields() {
    let server = support::mock_with_token().await;
    support::mount(
        &server,
        "flute-v2-get-payment-methods-paymentMethodId",
        "ach",
    )
    .await;
    support::bin(&server)
        .args(["--output", "table", "payment-methods", "get", ACH_PM])
        .assert()
        .success()
        .stdout(predicate::str::contains("ach.routingNumber"))
        .stdout(predicate::str::contains("021000021"))
        .stdout(predicate::str::contains("ach.accountHolderType"));
}

/// `--name` sets the instrument's label. The API spells it
/// `paymentName` on the card route and `name` on the ACH route, so one flag
/// covers an inconsistency in the API rather than exposing it.
#[tokio::test]
async fn the_name_flag_maps_to_each_route_s_own_wire_key() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/payment-methods/cards"))
        .and(body_json(serde_json::json!({
            "cardNumber": "4111111111111111",
            "expirationMonth": 12,
            "expirationYear": 2033,
            "paymentName": "Label"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v2/payment-methods/ach"))
        .and(body_json(serde_json::json!({
            "routingNumber": "021000021",
            "accountNumber": "123456789",
            "accountHolderType": "Personal",
            "accountType": "Checking",
            "name": "Label"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .mount(&server)
        .await;

    support::bin(&server)
        .args([
            "payment-methods",
            "add-card",
            "--card",
            "4111111111111111",
            "--exp",
            "12/2033",
            "--name",
            "Label",
        ])
        .assert()
        .success();
    support::bin(&server)
        .args([
            "payment-methods",
            "add-ach",
            "--account",
            "123456789",
            "--routing",
            "021000021",
            "--account-holder-type",
            "personal",
            "--account-type",
            "checking",
            "--name",
            "Label",
        ])
        .assert()
        .success();
}

/// The two ACH enums are case-sensitive on the wire, and a lowercase
/// near-miss is not something an `additionalProperties: false` schema
/// forgives.
#[tokio::test]
async fn ach_enums_reach_the_wire_capitalised() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/payment-methods/ach"))
        .and(body_json(serde_json::json!({
            "routingNumber": "021000021",
            "accountNumber": "123456789",
            "accountHolderType": "Business",
            "accountType": "Savings"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .mount(&server)
        .await;

    support::bin(&server)
        .args([
            "payment-methods",
            "add-ach",
            "--account",
            "123456789",
            "--routing",
            "021000021",
            "--account-holder-type",
            "business",
            "--account-type",
            "savings",
        ])
        .assert()
        .success();
}

/// A card with no `--cvv` is legal: `securityCode` is not required, and only
/// the absent case can establish that it is omitted rather than nulled.
#[tokio::test]
async fn add_card_omits_the_security_code_when_it_is_absent() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/payment-methods/cards"))
        .and(body_json(serde_json::json!({
            "cardNumber": "4111111111111111",
            "expirationMonth": 12,
            "expirationYear": 2033})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .mount(&server)
        .await;

    support::bin(&server)
        .args([
            "payment-methods",
            "add-card",
            "--card",
            "4111111111111111",
            "--exp",
            "12/2033",
        ])
        .assert()
        .success();
}

/// Each bodyless write says what it did, in a sentence naming the instrument.
#[tokio::test]
async fn the_confirmation_lines_are_sentences() {
    let server = support::mock_with_token().await;
    support::mount(
        &server,
        "flute-v2-patch-payment-methods-paymentMethodId",
        "rename",
    )
    .await;
    support::mount(
        &server,
        "flute-v2-delete-payment-methods-paymentMethodId",
        "default",
    )
    .await;
    support::mount(
        &server,
        "flute-v2-post-payment-methods-paymentMethodId-set-default",
        "default",
    )
    .await;

    support::bin(&server)
        .args(["payment-methods", "update", PM, "--name", "After"])
        .assert()
        .success()
        .stdout(format!("Updated payment method {PM}.\n"));

    support::bin(&server)
        .args(["payment-methods", "set-default", PM, "--customer-id", CUS])
        .assert()
        .success()
        .stdout(format!("Set payment method {PM} as default.\n"));

    support::bin(&server)
        .args(["payment-methods", "delete", PM, "--yes"])
        .assert()
        .success()
        .stdout(format!("Removed payment method {PM}.\n"));
}
