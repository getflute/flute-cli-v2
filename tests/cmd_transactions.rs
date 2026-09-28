mod support;

use predicates::prelude::*;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

/// The exact card body, including the two fields the published schema omits.
#[tokio::test]
async fn create_posts_the_documented_and_undocumented_fields() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/transactions"))
        .and(body_json(serde_json::json!({
            "paymentProcessorId": "pp-1",
            "baseAmount": support::spec::amount("10.50"),
            "transactionDetails": { "cardData": {
                "captureMethod": "Auto",
                "paymentMethodDetails": {
                    "cardNumber": "4111111111111111",
                    "securityCode": "123",
                    "expirationMonth": 12,
                    "expirationYear": 2032 } } }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "transactionId": "txn_1", "transactionStatus": "Captured",
            "processedAmount": support::spec::amount("10.50"), "currencyCode": "USD"
        })))
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args([
            "--output",
            "json",
            "transactions",
            "create",
            "--payment-processor-id",
            "pp-1",
            "--amount",
            "10.50",
            "--card",
            "4111111111111111",
            "--cvv",
            "123",
            "--exp",
            "12/2032",
            "--capture-method",
            "auto",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    // The response is a bare transaction object, not a page.
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["object"], "transaction");
    assert_eq!(v["data"]["transactionId"], "txn_1");
    assert!(v["meta"].get("page_info").is_none());
}

/// Manual capture is what makes an authorization reachable at all.
#[tokio::test]
async fn create_manual_capture_sends_the_capitalised_wire_value() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/transactions"))
        .and(body_json(serde_json::json!({
            "paymentProcessorId": "pp-1",
            "baseAmount": support::spec::amount("1.00"),
            "transactionDetails": { "cardData": {
                "captureMethod": "Manual",
                "paymentMethodDetails": {
                    "cardNumber": "4111111111111111",
                    "securityCode": "123",
                    "expirationMonth": 12,
                    "expirationYear": 2032 } } }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "transactionId": "txn_2", "transactionStatus": "Pending"
        })))
        .mount(&server)
        .await;

    support::bin(&server)
        .args([
            "transactions",
            "create",
            "--payment-processor-id",
            "pp-1",
            "--amount",
            "1.00",
            "--card",
            "4111111111111111",
            "--cvv",
            "123",
            "--exp",
            "12/2032",
            "--capture-method",
            "manual",
        ])
        .assert()
        .success();
}

/// Transitional: the published schema says PageOfGetTransactionResponseDto.
/// If the server ever converges on it, a one-item page must still work.
/// Delete this arm and this test when the server converges on it.
#[tokio::test]
async fn create_also_accepts_a_one_item_page() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/transactions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [{"transactionId": "txn_1", "transactionStatus": "Captured"}],
            "pageInfo": {"pageIndex": 0, "pageSize": 1, "totalItems": 1,
                         "totalPages": 1, "hasMore": false}
        })))
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args([
            "--output",
            "json",
            "transactions",
            "create",
            "--payment-processor-id",
            "pp-1",
            "--amount",
            "10.50",
            "--card",
            "4111111111111111",
            "--cvv",
            "123",
            "--exp",
            "12/2032",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["data"]["transactionId"], "txn_1");
    assert!(v["meta"].get("page_info").is_none());
}

/// Under the transitional page arm, any count but one is a contract break
/// rather than an invitation to pick the first element.
#[tokio::test]
async fn create_returning_two_items_is_a_decode_error() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/transactions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [{"transactionId": "a"}, {"transactionId": "b"}]
        })))
        .mount(&server)
        .await;

    support::bin(&server)
        .args([
            "transactions",
            "create",
            "--payment-processor-id",
            "pp-1",
            "--amount",
            "1.00",
            "--card",
            "4111111111111111",
            "--cvv",
            "123",
            "--exp",
            "12/2032",
        ])
        .assert()
        .code(1);
}

/// A decline is HTTP 200 with a declined status. The caller reads
/// transactionStatus; the exit code stays 0.
#[tokio::test]
async fn declined_transaction_exits_zero() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/transactions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "transactionId": "txn_2", "transactionStatus": "Declined",
            "declineDetails": {"message": "DeclineBinNotFound"}
        })))
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args([
            "--output",
            "json",
            "transactions",
            "create",
            "--payment-processor-id",
            "pp-1",
            "--amount",
            "1.00",
            "--card",
            "4111111111111111",
            "--cvv",
            "123",
            "--exp",
            "12/2032",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["data"]["transactionStatus"], "Declined");
    assert_eq!(v["data"]["declineDetails"]["message"], "DeclineBinNotFound");
}

/// Client-side validation spends no round trip.
#[tokio::test]
async fn missing_instrument_exits_3_without_calling_the_api() {
    let server = support::mock_with_token().await;
    support::bin(&server)
        .args([
            "transactions",
            "create",
            "--payment-processor-id",
            "pp-1",
            "--amount",
            "1.00",
        ])
        .assert()
        .code(3);
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.url.path() == "/oauth2/token")
    );
}

/// The same rule for the other three client-side checks, each proving no
/// request was issued.
#[tokio::test]
async fn every_client_side_rule_refuses_before_the_wire() {
    for args in [
        vec![
            "transactions",
            "create",
            "--payment-processor-id",
            "pp-1",
            "--amount",
            "0.00",
            "--card",
            "4111111111111111",
            "--cvv",
            "123",
            "--exp",
            "12/2032",
        ],
        vec![
            "transactions",
            "create",
            "--payment-processor-id",
            "",
            "--amount",
            "1.00",
            "--card",
            "4111111111111111",
            "--cvv",
            "123",
            "--exp",
            "12/2032",
        ],
        vec![
            "transactions",
            "create",
            "--payment-processor-id",
            "pp-1",
            "--amount",
            "1.00",
            "--card",
            "4111111111111111",
        ],
    ] {
        let server = support::mock_with_token().await;
        support::bin(&server).args(&args).assert().code(3);
        assert!(
            server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .all(|r| r.url.path() == "/oauth2/token"),
            "a client-side refusal issued a request: {args:?}"
        );
    }
}

/// `customerId` is accepted, not required, so the absent case is asserted too
/// — asserting only the present one would leave it looking mandatory.
#[tokio::test]
async fn create_succeeds_without_customer_id_and_omits_the_field() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/transactions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "transactionId": "txn_1", "transactionStatus": "Captured"
        })))
        .mount(&server)
        .await;

    support::bin(&server)
        .args([
            "transactions",
            "create",
            "--payment-processor-id",
            "pp-1",
            "--amount",
            "1.00",
            "--card",
            "4111111111111111",
            "--cvv",
            "123",
            "--exp",
            "12/2032",
        ])
        .assert()
        .success();

    let reqs = server.received_requests().await.unwrap();
    let txn = reqs
        .iter()
        .find(|r| r.url.path() == "/v2/transactions")
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&txn.body).unwrap();
    assert!(
        body.get("customerId").is_none(),
        "an absent flag must not become a field"
    );
}

/// The amount reaches the wire as exact decimal bytes, not a float.
#[tokio::test]
async fn base_amount_reaches_the_wire_with_its_scale_intact() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/transactions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "transactionId": "txn_1", "transactionStatus": "Captured"
        })))
        .mount(&server)
        .await;

    support::bin(&server)
        .args([
            "transactions",
            "create",
            "--payment-processor-id",
            "pp-1",
            "--amount",
            "10.50",
            "--card",
            "4111111111111111",
            "--cvv",
            "123",
            "--exp",
            "12/2032",
        ])
        .assert()
        .success();

    let reqs = server.received_requests().await.unwrap();
    let txn = reqs
        .iter()
        .find(|r| r.url.path() == "/v2/transactions")
        .unwrap();
    let raw = String::from_utf8(txn.body.clone()).unwrap();
    assert!(
        raw.contains("\"baseAmount\":10.50"),
        "the amount lost its scale on the wire: {raw}"
    );
}

#[tokio::test]
async fn get_renders_the_transaction_envelope() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path("/v2/transactions/txn_1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "transactionId": "txn_1", "transactionStatus": "Captured"
        })))
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args(["--output", "json", "transactions", "get", "txn_1"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["object"], "transaction");
    assert_eq!(v["data"]["transactionId"], "txn_1");
}

#[tokio::test]
async fn transaction_create_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-post-transactions",
        "new card, automatic capture",
    )
    .await;
    support::bin(&server)
        .args([
            "--output",
            "json",
            "transactions",
            "create",
            "--payment-processor-id",
            "8db2ff47-b143-4adb-ab58-a11111111111",
            "--amount",
            "10.50",
            "--card",
            "4111111111111111",
            "--cvv",
            "123",
            "--exp",
            "12/2032",
            "--capture-method",
            "auto",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn transaction_create_manual_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-post-transactions",
        "new card, manual capture",
    )
    .await;
    support::bin(&server)
        .args([
            "transactions",
            "create",
            "--payment-processor-id",
            "8db2ff47-b143-4adb-ab58-a11111111111",
            "--amount",
            "10.50",
            "--card",
            "4111111111111111",
            "--cvv",
            "123",
            "--exp",
            "12/2032",
            "--capture-method",
            "manual",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn transaction_get_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-get-transactions-transactionId",
        "default",
    )
    .await;
    support::bin(&server)
        .args([
            "transactions",
            "get",
            "90d084d6-55b8-4fb8-b658-861534d07f9a",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

/// The create response carries four containers the *get* response schema does
/// not name — `amountDetails`, `processorResponse`, `responseDetails` and
/// `receipt`. A descriptor built from one shape must not drop the other's
/// fields, and a decline message is the field that must never vanish.
#[tokio::test]
async fn create_table_keeps_container_fields_the_descriptor_does_not_name() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/transactions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "transactionId": "txn_1",
            "transactionStatus": "Declined",
            "processorResponse": {"responseCode": "05", "responseMessage": "Do not honor"},
            "responseDetails": null
        })))
        .mount(&server)
        .await;

    support::bin(&server)
        .args([
            "--output",
            "table",
            "transactions",
            "create",
            "--payment-processor-id",
            "pp-1",
            "--amount",
            "10.50",
            "--card",
            "4111111111111111",
            "--cvv",
            "123",
            "--exp",
            "12/2032",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "processorResponse.responseMessage",
        ))
        .stdout(predicate::str::contains("Do not honor"))
        // An explicit null is a dash, not an omission.
        .stdout(predicate::str::contains("responseDetails"));
}

/// An amount that renders as `10.5` is a wrong answer about money.
#[tokio::test]
async fn get_table_renders_the_processed_amount_with_its_exact_digits() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path("/v2/transactions/txn_1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"transactionId": "txn_1", "processedAmount": support::spec::amount("10.50")}),
        ))
        .mount(&server)
        .await;

    support::bin(&server)
        .args(["--output", "table", "transactions", "get", "txn_1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("10.50"));
}

// ── the other three instrument shapes ────────────────────────────────────────

const PP: &str = "8db2ff47-b143-4adb-ab58-a11111111111";
const CARD_PM: &str = "6e22f0f3-f3ed-407c-a46f-ec9cffde5d5b";
const ACH_PM: &str = "6bbfbe3e-04dd-41cd-82bf-1466e0159007";

#[tokio::test]
async fn transaction_create_saved_card_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-post-transactions", "saved card").await;
    support::bin(&server)
        .args([
            "transactions",
            "create",
            "--payment-processor-id",
            PP,
            "--amount",
            "10.50",
            "--instrument",
            "card",
            "--payment-method-id",
            CARD_PM,
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn transaction_create_new_ach_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-post-transactions", "new ACH").await;
    support::bin(&server)
        .args([
            "transactions",
            "create",
            "--payment-processor-id",
            PP,
            "--amount",
            "10.50",
            "--ach-account-number",
            "123456789",
            "--ach-routing-number",
            "021000021",
            "--ach-account-type",
            "checking",
            "--ach-account-holder-type",
            "personal",
            "--sec-code",
            "web",
            "--requester-ip",
            "203.0.113.10",
            "--billing-line1",
            "1 Main St",
            "--billing-city",
            "Austin",
            "--billing-state",
            "TX",
            "--billing-postal-code",
            "78701",
            "--billing-country",
            "US",
            "--contact-first-name",
            "Ada",
            "--contact-last-name",
            "Lovelace",
            "--contact-phone",
            "+14155552309",
            "--contact-email",
            "ada@example.com",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn transaction_create_saved_ach_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-post-transactions", "saved ACH").await;
    support::bin(&server)
        .args([
            "transactions",
            "create",
            "--payment-processor-id",
            PP,
            "--amount",
            "10.50",
            "--instrument",
            "ach",
            "--payment-method-id",
            ACH_PM,
            "--sec-code",
            "ppd",
            "--requester-ip",
            "203.0.113.10",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn transaction_create_level_three_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-post-transactions",
        "card with level three data",
    )
    .await;
    support::bin(&server)
        .args([
            "transactions",
            "create",
            "--payment-processor-id",
            PP,
            "--amount",
            "10.50",
            "--card",
            "4111111111111111",
            "--cvv",
            "123",
            "--exp",
            "12/2032",
            "--customer-initiated",
            "--pricing-type",
            "card",
            "--tip-amount",
            "1.50",
            "--discount-amount",
            "0.50",
            "--surcharge-rate",
            "0.0300",
            "--contact-first-name",
            "Ada",
            "--contact-last-name",
            "Lovelace",
            "--contact-company",
            "Analytical Engines",
            "--contact-email",
            "ada@example.com",
            "--contact-phone",
            "+14155552309",
            "--contact-sms-consent",
            "--l2-tax-rate",
            "8.25",
            "--l3-invoice",
            "INV1",
            "--l3-po",
            "PO-1",
            "--l3-shipping",
            "4.99",
            "--l3-product",
            "productName=Widget,productDescription=A widget,productCode=W-1,\
             unitPrice=9.99,measurementUnit=EA,quantity=2,taxAmount=0.82,\
             discountRate=1.50",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

/// **Refused before the wire.** A *new* ACH account needs a billing address,
/// a mobile number, and a name pair or a company name.
#[tokio::test]
async fn a_new_ach_without_the_conditional_fields_is_refused_before_the_wire() {
    let server = support::mock_with_token().await;
    let base = [
        "transactions",
        "create",
        "--payment-processor-id",
        PP,
        "--amount",
        "10.50",
        "--ach-account-number",
        "123456789",
        "--ach-routing-number",
        "021000021",
        "--ach-account-type",
        "checking",
        "--ach-account-holder-type",
        "personal",
        "--sec-code",
        "web",
        "--requester-ip",
        "203.0.113.10",
    ];
    // No billing address, no contact info at all.
    support::bin(&server).args(base).assert().code(3);

    // Billing address, but no mobile number.
    support::bin(&server)
        .args(base)
        .args([
            "--billing-line1",
            "1 Main St",
            "--contact-first-name",
            "Ada",
            "--contact-last-name",
            "Lovelace",
        ])
        .assert()
        .code(3);

    // Billing address and mobile number, but neither a name pair nor a
    // company name.
    support::bin(&server)
        .args(base)
        .args([
            "--billing-line1",
            "1 Main St",
            "--contact-phone",
            "+14155552309",
        ])
        .assert()
        .code(3);

    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.url.path() == "/oauth2/token"),
        "a client-side refusal must issue no request"
    );
}

/// **The other direction.** The saved-ACH route requires none of them, and
/// only the pair proves the rule is conditional rather than universal.
#[tokio::test]
async fn a_saved_ach_needs_none_of_the_conditional_fields() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-post-transactions", "saved ACH").await;
    support::bin(&server)
        .args([
            "transactions",
            "create",
            "--payment-processor-id",
            PP,
            "--amount",
            "10.50",
            "--instrument",
            "ach",
            "--payment-method-id",
            ACH_PM,
            "--sec-code",
            "ppd",
            "--requester-ip",
            "203.0.113.10",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

/// A company name satisfies the name requirement on its own.
#[tokio::test]
async fn a_new_ach_with_a_company_name_and_no_person_is_accepted() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/transactions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"transactionId": "txn_1", "transactionStatus": "Pending"}),
        ))
        .mount(&server)
        .await;

    support::bin(&server)
        .args([
            "transactions",
            "create",
            "--payment-processor-id",
            PP,
            "--amount",
            "10.50",
            "--ach-account-number",
            "123456789",
            "--ach-routing-number",
            "021000021",
            "--ach-account-type",
            "checking",
            "--ach-account-holder-type",
            "personal",
            "--sec-code",
            "web",
            "--requester-ip",
            "203.0.113.10",
            "--billing-line1",
            "1 Main St",
            "--contact-phone",
            "+14155552309",
            "--contact-email",
            "ada@example.com",
            "--contact-company",
            "Analytical Engines",
        ])
        .assert()
        .success();
}

/// ACH needs `secCode` and `requesterIpAddress` on **both** routes: the
/// schema declares them required on `AchDataDto` itself. `--requester-ip`
/// defaults rather than being absent, so the reachable way to send nothing is
/// to pass it empty.
#[tokio::test]
async fn an_ach_charge_without_sec_code_or_with_a_blank_requester_ip_is_refused() {
    let server = support::mock_with_token().await;
    let base = [
        "transactions",
        "create",
        "--payment-processor-id",
        PP,
        "--amount",
        "10.50",
        "--instrument",
        "ach",
        "--payment-method-id",
        ACH_PM,
        "--sec-code",
        "ppd",
        "--requester-ip",
        "203.0.113.10",
    ];

    let mut without_sec_code = base.to_vec();
    let at = without_sec_code
        .iter()
        .position(|a| *a == "--sec-code")
        .unwrap();
    without_sec_code.drain(at..at + 2);
    support::bin(&server)
        .args(without_sec_code)
        .assert()
        .code(3);

    let mut blank_ip = base.to_vec();
    let at = blank_ip.iter().position(|a| *a == "203.0.113.10").unwrap();
    blank_ip[at] = "";
    support::bin(&server).args(blank_ip).assert().code(3);

    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.url.path() == "/oauth2/token"),
    );
}

/// **`--capture-method manual` is refused on ACH**, new or saved, before the
/// wire. `AchDataDto` declares no `captureMethod`, so the flag has nowhere to
/// go and the charge would run as an immediate debit rather than the
/// authorization it asked for.
#[tokio::test]
async fn manual_capture_on_an_ach_charge_is_refused_before_the_wire() {
    let server = support::mock_with_token().await;
    let saved = [
        "transactions",
        "create",
        "--payment-processor-id",
        PP,
        "--amount",
        "10.50",
        "--instrument",
        "ach",
        "--payment-method-id",
        ACH_PM,
        "--sec-code",
        "ppd",
        "--capture-method",
        "manual",
    ];
    let new = [
        "transactions",
        "create",
        "--payment-processor-id",
        PP,
        "--amount",
        "10.50",
        "--ach-account-number",
        "123456789",
        "--ach-routing-number",
        "021000021",
        "--ach-account-type",
        "checking",
        "--ach-account-holder-type",
        "personal",
        "--sec-code",
        "web",
        "--billing-line1",
        "1 Main St",
        "--billing-city",
        "Austin",
        "--billing-state",
        "TX",
        "--billing-postal-code",
        "78701",
        "--billing-country",
        "US",
        "--contact-first-name",
        "Ada",
        "--contact-last-name",
        "Lovelace",
        "--contact-phone",
        "+14155552309",
        "--contact-email",
        "ada@example.com",
        "--capture-method",
        "manual",
    ];
    for args in [&saved[..], &new[..]] {
        support::bin(&server)
            .args(["--output", "json"])
            .args(args)
            .assert()
            .code(3)
            .stdout(predicate::str::contains("\"kind\": \"client\""))
            .stdout(predicate::str::contains("--capture-method"));
    }
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.url.path() == "/oauth2/token"),
        "a manual capture on ACH reached the API"
    );
}

/// The default reaches the wire: an ACH charge that names no `--requester-ip`
/// still sends one, because `AchDataDto` requires it.
#[tokio::test]
async fn an_ach_charge_sends_the_default_requester_ip() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/transactions"))
        .and(body_json(serde_json::json!({
            "paymentProcessorId": PP,
            "baseAmount": support::spec::amount("10.50"),
            "transactionDetails": {"achData": {
                "secCode": "PPD",
                "requesterIpAddress": "127.0.0.1",
                "paymentMethodId": ACH_PM}}
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"transactionId": "txn_1", "transactionStatus": "Pending"}),
        ))
        .mount(&server)
        .await;

    support::bin(&server)
        .args([
            "transactions",
            "create",
            "--payment-processor-id",
            PP,
            "--amount",
            "10.50",
            "--instrument",
            "ach",
            "--payment-method-id",
            ACH_PM,
            "--sec-code",
            "ppd",
        ])
        .assert()
        .success();
}

/// Exactly one instrument, still. Two of them is as wrong as none.
#[tokio::test]
async fn supplying_two_instruments_is_refused_before_the_wire() {
    let server = support::mock_with_token().await;
    support::bin(&server)
        .args([
            "transactions",
            "create",
            "--payment-processor-id",
            PP,
            "--amount",
            "10.50",
            "--card",
            "4111111111111111",
            "--cvv",
            "123",
            "--exp",
            "12/2032",
            "--ach-account-number",
            "123456789",
            "--ach-routing-number",
            "021000021",
            "--ach-account-type",
            "checking",
            "--ach-account-holder-type",
            "personal",
            "--sec-code",
            "web",
            "--requester-ip",
            "203.0.113.10",
        ])
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

/// A saved instrument id says nothing about which container it belongs in,
/// so `--instrument` is what decides between `cardData` and `achData`.
#[tokio::test]
async fn a_payment_method_id_without_an_instrument_is_refused() {
    let server = support::mock_with_token().await;
    support::bin(&server)
        .args([
            "transactions",
            "create",
            "--payment-processor-id",
            PP,
            "--amount",
            "10.50",
            "--payment-method-id",
            CARD_PM,
        ])
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

/// v2's field is `isSameDayProcessing`, and the flag names the thing it sets.
#[tokio::test]
async fn an_ach_charge_can_request_same_day_processing() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/transactions"))
        .and(body_json(serde_json::json!({
            "paymentProcessorId": PP,
            "baseAmount": support::spec::amount("10.50"),
            "transactionDetails": {"achData": {
                "secCode": "PPD",
                "requesterIpAddress": "203.0.113.10",
                "isSameDayProcessing": true,
                "paymentMethodId": ACH_PM}}
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"transactionId": "txn_1", "transactionStatus": "Pending"}),
        ))
        .mount(&server)
        .await;

    support::bin(&server)
        .args([
            "transactions",
            "create",
            "--payment-processor-id",
            PP,
            "--amount",
            "10.50",
            "--instrument",
            "ach",
            "--payment-method-id",
            ACH_PM,
            "--sec-code",
            "ppd",
            "--requester-ip",
            "203.0.113.10",
            "--same-day",
        ])
        .assert()
        .success();
}

// ── lifecycle, reads and utilities ───────────────────────────────────────────

const TXN: &str = "90d084d6-55b8-4fb8-b658-861534d07f9a";

#[tokio::test]
async fn transaction_list_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-get-transactions",
        "first page, server defaults",
    )
    .await;
    support::bin(&server)
        .args(["transactions", "list"])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn transaction_list_every_filter_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-get-transactions", "every filter").await;
    support::bin(&server)
        .args([
            "transactions",
            "list",
            "--page-index",
            "1",
            "--page-size",
            "5",
            "--sort-by",
            "transactionDateTime",
            "--desc",
            "--from",
            "2026-01-01T00:00:00Z",
            "--to",
            "2026-12-31T23:59:59Z",
            "--source-type",
            "api-key",
            "--source-id",
            "588f57a5-fe6a-4844-851e-e98914e81980",
            "--batch-id",
            "batch-1",
            "--status",
            "captured",
            "--payment-method-type",
            "Card",
            "--customer-id",
            "588f57a5-fe6a-4844-851e-e98914e81980",
            "--merchant-id",
            "8db2ff47-b143-4adb-ab58-a11111111111",
            "--min-amount",
            "1.00",
            "--max-amount",
            "999.99",
            "--reference-id",
            "ref-1",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn transaction_capture_full_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-post-transactions-transactionId-capture",
        "full",
    )
    .await;
    support::bin(&server)
        .args(["transactions", "capture", "--transaction-id", TXN])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn transaction_capture_partial_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-post-transactions-transactionId-capture",
        "partial",
    )
    .await;
    support::bin(&server)
        .args([
            "transactions",
            "capture",
            "--transaction-id",
            TXN,
            "--amount",
            "5.00",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn transaction_reversal_full_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-post-transactions-transactionId-reversal",
        "full",
    )
    .await;
    support::bin(&server)
        .args(["transactions", "reversal", "--transaction-id", TXN])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn transaction_reversal_partial_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    mount_transaction_state(&server, "Card", "Settled").await;
    let ex = support::mount(
        &server,
        "flute-v2-post-transactions-transactionId-reversal",
        "partial",
    )
    .await;
    support::bin(&server)
        .args([
            "transactions",
            "reversal",
            "--transaction-id",
            TXN,
            "--amount",
            "5.00",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn transaction_tip_adjust_amount_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-post-transactions-transactionId-tip-adjustment",
        "amount",
    )
    .await;
    support::bin(&server)
        .args([
            "transactions",
            "tip-adjust",
            "--transaction-id",
            TXN,
            "--tip-amount",
            "1.00",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn transaction_tip_adjust_rate_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-post-transactions-transactionId-tip-adjustment",
        "rate",
    )
    .await;
    support::bin(&server)
        .args([
            "transactions",
            "tip-adjust",
            "--transaction-id",
            TXN,
            "--tip-rate",
            "0.1500",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn transaction_ach_hold_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-post-transactions-transactionId-ach-hold",
        "default",
    )
    .await;
    support::bin(&server)
        .args(["transactions", "ach-hold", TXN])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

/// `referenceId` is the merchant's own handle on the transaction, and
/// `GetTransactionResponseDtoShort` declares it on both ACH actions. Naming it
/// in the descriptor is what makes a response that *drops* it say so: an
/// undeclared field the server omits leaves no row at all, while a declared
/// one holds a dashed one.
#[tokio::test]
async fn an_ach_hold_that_returns_no_reference_says_so() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path(format!("/v2/transactions/{TXN}/ach-hold")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "transactionId": TXN,
            "transactionStatus": "Held"})))
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args(["--output", "table", "transactions", "ach-hold", TXN])
        .output()
        .unwrap();
    let hold = String::from_utf8(out.stdout).unwrap();
    assert!(hold.contains("referenceId:"), "{hold}");
}

/// `share-receipt` is SMS-only: the endpoint's validator answers
/// `ShareBy must be Sms; it is the only supported channel.` for anything else,
/// so a channel the API refuses is refused here instead of on a round trip.
/// The published `shareBy` still documents an Email/None/Sms table.
#[tokio::test]
async fn share_receipt_refuses_a_channel_the_endpoint_does_not_accept() {
    let server = support::mock_with_token().await;
    for channel in ["email", "none"] {
        support::bin(&server)
            .args([
                "transactions",
                "share-receipt",
                TXN,
                "--share-by",
                channel,
                "--recipient",
                "+14155552309",
                "--consent",
            ])
            .assert()
            .failure();
    }
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| !r.url.path().contains("share-receipt")),
        "a refused channel still reached the API"
    );
}

#[tokio::test]
async fn transaction_ach_release_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-post-transactions-transactionId-ach-release",
        "default",
    )
    .await;
    support::bin(&server)
        .args(["transactions", "ach-release", TXN])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn transaction_share_receipt_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-post-transactions-transactionId-share-receipt",
        "sms",
    )
    .await;
    support::bin(&server)
        .args([
            "transactions",
            "share-receipt",
            TXN,
            "--share-by",
            "sms",
            "--recipient",
            "+14155552309",
            "--consent",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn transaction_calculate_amount_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-post-transactions-calculate-amount",
        "default",
    )
    .await;
    support::bin(&server)
        .args([
            "transactions",
            "calculate-amount",
            "--amount",
            "100.00",
            "--currency-code",
            "USD",
            "--pricing-type",
            "card",
            "--tip-amount",
            "15.00",
            "--discount-amount",
            "5.00",
            "--surcharge-rate",
            "0.0300",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

/// A full capture sends **`{}`**, and the bytes are asserted rather than the
/// shape.
///
/// `assert_exchange_observed` compares parsed JSON, so `{}` and an absent
/// body both satisfy a fixture that expects no fields — the two are only
/// distinguishable at the byte level, and the API distinguishes them: it
/// answers `400: A non-empty request body is required` to the absent one.
/// That is the whole finding, so this is where it is pinned.
#[tokio::test]
async fn a_full_capture_sends_an_empty_object_rather_than_no_body() {
    let server = support::mock_with_token().await;
    support::mount(
        &server,
        "flute-v2-post-transactions-transactionId-capture",
        "full",
    )
    .await;
    support::bin(&server)
        .args(["transactions", "capture", "--transaction-id", TXN])
        .assert()
        .success();
    let reqs = server.received_requests().await.unwrap();
    let sent = reqs
        .iter()
        .find(|r| r.url.path().ends_with("/capture"))
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&sent.body),
        "{}",
        "a full capture must send an empty object, not an empty request"
    );
    assert_eq!(
        sent.headers
            .get("content-type")
            .map(|v| v.to_str().unwrap().split(';').next().unwrap().to_string())
            .as_deref(),
        Some("application/json"),
    );
}

/// The id is a flag on these three, so a bare id is a usage error rather
/// than a transaction acted on by accident.
#[tokio::test]
async fn capture_reversal_and_tip_adjust_refuse_a_positional_id() {
    let server = support::mock_with_token().await;
    for args in [
        vec!["transactions", "capture", TXN],
        vec!["transactions", "reversal", TXN],
        // The tip flag is what makes this leg discriminate: without it the
        // command is refused for want of a tip whether the positional is
        // rejected or not.
        vec!["transactions", "tip-adjust", TXN, "--tip-amount", "1.00"],
    ] {
        support::bin(&server).args(args).assert().code(3);
    }
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.url.path() == "/oauth2/token"),
    );
}

/// A tip adjustment with both an amount and a rate says two different things.
#[tokio::test]
async fn tip_adjust_with_both_an_amount_and_a_rate_is_refused() {
    let server = support::mock_with_token().await;
    support::bin(&server)
        .args([
            "transactions",
            "tip-adjust",
            "--transaction-id",
            TXN,
            "--tip-amount",
            "1.00",
            "--tip-rate",
            "0.15",
        ])
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

/// And with neither there is nothing to adjust.
#[tokio::test]
async fn tip_adjust_with_no_tip_is_refused() {
    let server = support::mock_with_token().await;
    support::bin(&server)
        .args(["transactions", "tip-adjust", "--transaction-id", TXN])
        .assert()
        .code(3);
}

/// `share-receipt` answers 200 with no body, so the confirmation is built
/// from the id in the request.
#[tokio::test]
async fn share_receipt_confirms_from_the_request() {
    let server = support::mock_with_token().await;
    support::mount(
        &server,
        "flute-v2-post-transactions-transactionId-share-receipt",
        "sms",
    )
    .await;
    let out = support::bin(&server)
        .args([
            "--output",
            "json",
            "transactions",
            "share-receipt",
            TXN,
            "--share-by",
            "sms",
            "--recipient",
            "+14155552309",
            "--consent",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["data"]["transactionId"], TXN);
    assert_eq!(v["data"]["shared"], true);
}

/// `inspect` has no endpoint: it reads the transaction and prints the
/// curated view, so the request is the same `GET` that `get` issues.
#[tokio::test]
async fn inspect_reads_through_the_get_endpoint_and_curates_the_table() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path(format!("/v2/transactions/{TXN}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "transactionId": TXN,
            "transactionStatus": "Declined",
            "declineDetails": {"code": "05", "message": "Do not honor"},
            "somethingNewTheApiAdded": "surprise"})))
        .mount(&server)
        .await;

    // `get` prints everything, including the field the CLI does not know.
    support::bin(&server)
        .args(["--output", "table", "transactions", "get", TXN])
        .assert()
        .success()
        .stdout(predicate::str::contains("somethingNewTheApiAdded"));

    // `inspect` prints the curated set, and the decline reason is in it.
    support::bin(&server)
        .args(["--output", "table", "transactions", "inspect", TXN])
        .assert()
        .success()
        .stdout(predicate::str::contains("declineDetails.message"))
        .stdout(predicate::str::contains("Do not honor"))
        .stdout(predicate::str::contains("somethingNewTheApiAdded").not());
}

/// **Under `--output json`, `inspect` is `get`.** The curation is a `table`
/// presentation, so a caller that parses JSON gets the same document from
/// either verb and spends a second round trip for nothing.
#[tokio::test]
async fn inspect_and_get_are_identical_under_output_json() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path(format!("/v2/transactions/{TXN}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "transactionId": TXN,
            "transactionStatus": "Declined",
            "declineDetails": {"code": "05", "message": "Do not honor"},
            "somethingNewTheApiAdded": "surprise"})))
        .mount(&server)
        .await;

    let read = |verb: &str| {
        support::bin(&server)
            .args(["--output", "json", "transactions", verb, TXN])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone()
    };
    assert_eq!(read("inspect"), read("get"));
}

/// **Object keys are emitted sorted, at every depth.**
///
/// The response is served as raw bytes in a deliberately unsorted order,
/// because a `serde_json` map is already sorted on the way out and a fixture
/// built from one could not tell the two orderings apart.
#[tokio::test]
async fn the_envelope_sorts_object_keys_at_every_depth() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path(format!("/v2/transactions/{TXN}")))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            br#"{"zebra":1,"transactionId":"t","alpha":2,"nested":{"zz":1,"aa":2}}"#.to_vec(),
            "application/json",
        ))
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args(["--output", "json", "transactions", "get", TXN])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).expect("utf-8 stdout");
    let at = |key: &str| {
        text.find(&format!("\"{key}\""))
            .unwrap_or_else(|| panic!("{key} is not in the envelope:\n{text}"))
    };
    assert!(at("alpha") < at("nested"), "{text}");
    assert!(at("nested") < at("transactionId"), "{text}");
    assert!(at("transactionId") < at("zebra"), "{text}");
    assert!(at("aa") < at("zz"), "{text}");
}

/// A detail view answers for the shape it was given, and a field belonging to
/// the other shape is not a field this response left out.
///
/// `GET /v2/transactions/{id}` declares `amountBreakdown`, `processorDetails`
/// and `declineDetails`; `processorResponse`, `amountDetails` and the
/// `ach-hold` verb's `type` are the write responses' own, and a read that
/// reported them as absent would describe a response the endpoint cannot
/// send.
#[tokio::test]
async fn get_table_reports_no_field_belonging_to_a_write_response() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path(format!("/v2/transactions/{TXN}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "transactionId": TXN,
            "transactionStatus": "Captured",
            "transactionType": "Sale",
            "processedAmount": support::spec::amount("106.50"),
            "currencyCode": "USD",
            "amountBreakdown": {"baseAmount": support::spec::amount("100.00")},
            "processorDetails": {"authCode": "GOLD42"},
            "cardDetails": {"maskedCardNumber": "411111XXXXXX1111"}})))
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args(["--output", "table", "transactions", "get", TXN])
        .output()
        .unwrap();
    let table = String::from_utf8(out.stdout).unwrap();

    // The read shape's own fields are there, dashed where the response is
    // silent, which is what makes the absence of the others meaningful.
    assert!(table.contains("amountBreakdown.tipAmount:"), "{table}");
    assert!(table.contains("declineDetails.message:"), "{table}");
    for write_only in ["processorResponse.", "amountDetails.", "\ntype:"] {
        assert!(
            !table.contains(write_only),
            "a read reported {write_only:?}:\n{table}"
        );
    }
}

/// The same rule the other way round: a write response is not missing the
/// containers only a read carries.
#[tokio::test]
async fn create_table_reports_no_field_belonging_to_a_read_response() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/transactions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "transactionId": TXN,
            "transactionStatus": "Approved",
            "processedAmount": support::spec::amount("115.50"),
            "currencyCode": "USD",
            "amountDetails": {"baseAmount": support::spec::amount("100.00")},
            "processorResponse": {"responseCode": "00", "responseMessage": "Approved"}})))
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args([
            "--output",
            "table",
            "transactions",
            "create",
            "--payment-processor-id",
            "pp-1",
            "--amount",
            "115.50",
            "--card",
            "4111111111111111",
            "--cvv",
            "123",
            "--exp",
            "12/2032",
        ])
        .output()
        .unwrap();
    let table = String::from_utf8(out.stdout).unwrap();

    assert!(table.contains("amountDetails.tipAmount:"), "{table}");
    assert!(
        table.contains("processorResponse.responseDefinition:"),
        "{table}"
    );
    for read_only in [
        "amountBreakdown.",
        "processorDetails.",
        "declineDetails.",
        "achDetails.",
        "cardDetails.",
        "refundDetails.",
    ] {
        assert!(
            !table.contains(read_only),
            "a write reported {read_only:?}:\n{table}"
        );
    }
}

/// The seven writes are three declared shapes, not one. A reversal's response
/// declares the processor's answer and the amount and nothing else, so an
/// `amountDetails` or `type` row would report a field this operation cannot
/// send — the same rule that separates the reads from the writes, one level
/// down.
#[tokio::test]
async fn reversal_and_ach_hold_report_only_their_own_declared_shapes() {
    let server = support::mock_with_token().await;
    support::mount(
        &server,
        "flute-v2-post-transactions-transactionId-reversal",
        "full",
    )
    .await;
    support::mount(
        &server,
        "flute-v2-post-transactions-transactionId-ach-hold",
        "default",
    )
    .await;

    let reversal = support::bin(&server)
        .args([
            "--output",
            "table",
            "transactions",
            "reversal",
            "--transaction-id",
            TXN,
        ])
        .output()
        .unwrap();
    let reversal = String::from_utf8(reversal.stdout).unwrap();
    assert!(
        reversal.contains("processorResponse.responseCode:"),
        "{reversal}"
    );
    assert!(reversal.contains("processedAmount:"), "{reversal}");
    for absent in ["amountDetails.", "\ntype:", "addressVerificationService"] {
        assert!(
            !reversal.contains(absent),
            "a reversal reported {absent:?}:\n{reversal}"
        );
    }

    // The ACH action names the operation in `type` and carries no amount of
    // its own at all.
    let hold = support::bin(&server)
        .args(["--output", "table", "transactions", "ach-hold", TXN])
        .output()
        .unwrap();
    let hold = String::from_utf8(hold.stdout).unwrap();
    assert!(hold.contains("type:"), "{hold}");
    assert!(hold.contains("processorResponse.processorName:"), "{hold}");
    for absent in ["amountDetails.", "processedAmount:", "currencyCode:"] {
        assert!(
            !hold.contains(absent),
            "an ACH hold reported {absent:?}:\n{hold}"
        );
    }
}

/// `inspect` is a sectioned view: the fields that decide whether a payment
/// worked, then the amounts that make up the total.
#[tokio::test]
async fn inspect_sections_the_processor_answer_and_the_amount_breakdown() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path(format!("/v2/transactions/{TXN}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "transactionId": TXN,
            "transactionStatus": "Approved",
            "currencyCode": "USD",
            "processedAmount": support::spec::amount("106.50"),
            "processorDetails": {"authCode": "INSP77"},
            "cardDetails": {
                "cardDataSource": "Internet",
                "maskedCardNumber": "411111XXXXXX1111"},
            "addressVerificationServiceResponse": {
                "responseCode": "Y",
                "description": "Address and ZIP match",
                "action": "Allow"},
            "amountBreakdown": {
                "baseAmount": support::spec::amount("100.00"),
                "tipAmount": support::spec::amount("5"),
                "surchargeAmount": support::spec::amount("1.5")}})))
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args(["--output", "table", "transactions", "inspect", TXN])
        .output()
        .unwrap();
    let table = String::from_utf8(out.stdout).unwrap();

    assert!(table.starts_with("transactionId:"), "{table}");
    // An approved transaction has no decline reason, so the authorization
    // code is the evidence that it worked.
    assert!(
        table
            .lines()
            .any(|l| l.starts_with("processorDetails.authCode:") && l.ends_with("INSP77")),
        "{table}"
    );
    assert!(table.contains("cardDetails.maskedCardNumber:"), "{table}");
    // The AVS answer is one line: the code, what it means, and what the
    // processor did about it.
    assert!(
        table.contains("Y — Address and ZIP match — Allow"),
        "{table}"
    );
    assert!(
        table.contains("\n\nAmount breakdown:\n  baseAmount:"),
        "{table}"
    );
    // Two decimal places on every amount, and the breakdown indented under
    // its heading.
    for (label, amount) in [
        ("baseAmount", "100.00"),
        ("tipAmount", "5.00"),
        ("surchargeAmount", "1.50"),
        ("processedAmount", "106.50"),
    ] {
        assert!(
            table
                .lines()
                .any(|l| l.starts_with(&format!("  {label}:")) && l.ends_with(amount)),
            "{label} {amount} is not an indented row:\n{table}"
        );
    }
    // `GetTransactionResponseDtoFull` declares no list of the operations
    // still open on a transaction, so the view does not claim one.
    assert!(!table.contains("Available operations"), "{table}");
}

/// A list under `--all` walks the pages, as `customers list` does.
#[tokio::test]
async fn transaction_list_all_walks_every_page() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path("/v2/transactions"))
        .and(wiremock::matchers::query_param("pageIndex", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [{"transactionId": "a"}],
            "pageInfo": {"hasMore": true}})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/transactions"))
        .and(wiremock::matchers::query_param("pageIndex", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [{"transactionId": "b"}],
            "pageInfo": {"hasMore": false}})))
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args(["--output", "json", "transactions", "list", "--all"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["object"], "transaction_list");
    assert_eq!(v["data"].as_array().unwrap().len(), 2);
}

// ── credit ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn transaction_credit_ach_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-post-transactions-credit", "new ACH").await;
    support::bin(&server)
        .args([
            "transactions",
            "credit",
            "--payment-processor-id",
            PP,
            "--amount",
            "10.50",
            "--reference-id",
            "credit-1",
            "--ach-account-number",
            "123456789",
            "--ach-routing-number",
            "021000021",
            "--ach-account-type",
            "checking",
            "--ach-account-holder-type",
            "personal",
            "--sec-code",
            "ppd",
            "--requester-ip",
            "203.0.113.10",
            "--billing-line1",
            "1 Main St",
            "--billing-city",
            "Austin",
            "--billing-state",
            "TX",
            "--billing-postal-code",
            "78701",
            "--billing-country",
            "US",
            "--contact-first-name",
            "Ada",
            "--contact-last-name",
            "Lovelace",
            "--contact-phone",
            "+14155552309",
            "--contact-email",
            "ada@example.com",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn transaction_credit_card_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-post-transactions-credit", "new card").await;
    support::bin(&server)
        .args([
            "transactions",
            "credit",
            "--payment-processor-id",
            PP,
            "--amount",
            "10.50",
            "--reference-id",
            "credit-2",
            "--currency-code",
            "USD",
            "--customer-id",
            "588f57a5-fe6a-4844-851e-e98914e81980",
            "--billing-city",
            "Austin",
            "--billing-state",
            "TX",
            "--contact-email",
            "ada@example.com",
            "--card",
            "4111111111111111",
            "--cvv",
            "123",
            "--exp",
            "12/2032",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

/// `referenceId` is **required** on credit and optional on create, so clap
/// refuses before anything is built.
#[tokio::test]
async fn a_credit_without_a_reference_id_is_a_usage_error() {
    let server = support::mock_with_token().await;
    support::bin(&server)
        .args([
            "transactions",
            "credit",
            "--payment-processor-id",
            PP,
            "--amount",
            "10.50",
            "--card",
            "4111111111111111",
            "--cvv",
            "123",
            "--exp",
            "12/2032",
        ])
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

/// The same four shapes and the same mutual exclusion as `create`, because
/// they are one declaration.
#[tokio::test]
async fn a_credit_needs_exactly_one_instrument() {
    let server = support::mock_with_token().await;
    support::bin(&server)
        .args([
            "transactions",
            "credit",
            "--payment-processor-id",
            PP,
            "--amount",
            "10.50",
            "--reference-id",
            "credit-3",
        ])
        .assert()
        .code(3);
    support::bin(&server)
        .args([
            "transactions",
            "credit",
            "--payment-processor-id",
            PP,
            "--amount",
            "10.50",
            "--reference-id",
            "credit-4",
            "--instrument",
            "ach",
            "--payment-method-id",
            ACH_PM,
        ])
        .assert()
        // ACH still needs its sec code and requester ip on both routes.
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

/// A saved instrument credits through `paymentMethodId`, and
/// `creditDetails.cardData` declares **no** `captureMethod` — a credit is not
/// an authorization.
#[tokio::test]
async fn a_saved_card_credit_sends_no_capture_method() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/transactions/credit"))
        .and(body_json(serde_json::json!({
            "paymentProcessorId": PP,
            "baseAmount": support::spec::amount("10.50"),
            "referenceId": "credit-5",
            "creditDetails": {"cardData": {"paymentMethodId": CARD_PM}}
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"transactionId": "txn_1", "transactionStatus": "Approved"}),
        ))
        .mount(&server)
        .await;

    support::bin(&server)
        .args([
            "transactions",
            "credit",
            "--payment-processor-id",
            PP,
            "--amount",
            "10.50",
            "--reference-id",
            "credit-5",
            "--instrument",
            "card",
            "--payment-method-id",
            CARD_PM,
        ])
        .assert()
        .success();
}

/// And a credit carries the same default, from the same shared flag.
#[tokio::test]
async fn an_ach_credit_sends_the_default_requester_ip() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/transactions/credit"))
        .and(body_json(serde_json::json!({
            "paymentProcessorId": PP,
            "baseAmount": support::spec::amount("10.50"),
            "referenceId": "credit-7",
            "creditDetails": {"achData": {
                "secCode": "PPD",
                "requesterIpAddress": "127.0.0.1",
                "paymentMethodId": ACH_PM}}
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"transactionId": "txn_1", "transactionStatus": "Pending"}),
        ))
        .mount(&server)
        .await;

    support::bin(&server)
        .args([
            "transactions",
            "credit",
            "--payment-processor-id",
            PP,
            "--amount",
            "10.50",
            "--reference-id",
            "credit-7",
            "--instrument",
            "ach",
            "--payment-method-id",
            ACH_PM,
            "--sec-code",
            "ppd",
        ])
        .assert()
        .success();
}

/// **The conditional ACH rule reaches `/credit`.** The endpoint answers
/// `BillingAddress is required for new ACH credits.; ContactInfo is required
/// for new ACH credits.`, so the refusal is real and is made before the wire
/// rather than paid for in a round trip.
#[tokio::test]
async fn a_new_ach_credit_without_a_billing_address_issues_no_request() {
    let server = support::mock_with_token().await;
    support::bin(&server)
        .args([
            "transactions",
            "credit",
            "--payment-processor-id",
            PP,
            "--amount",
            "10.50",
            "--reference-id",
            "credit-6",
            "--ach-account-number",
            "123456789",
            "--ach-routing-number",
            "021000021",
            "--ach-account-type",
            "checking",
            "--ach-account-holder-type",
            "personal",
            "--sec-code",
            "ppd",
            "--requester-ip",
            "203.0.113.10",
        ])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("--billing-"));
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.url.path() == "/oauth2/token"),
        "a client-side refusal must not reach the API"
    );
}

/// The bodyless share says what it did, in a sentence.
#[tokio::test]
async fn the_confirmation_line_is_a_sentence() {
    let server = support::mock_with_token().await;
    support::mount(
        &server,
        "flute-v2-post-transactions-transactionId-share-receipt",
        "sms",
    )
    .await;
    support::bin(&server)
        .args([
            "transactions",
            "share-receipt",
            TXN,
            "--share-by",
            "sms",
            "--recipient",
            "+14155552309",
            "--consent",
        ])
        .assert()
        .success()
        .stdout(format!("Shared receipt for transaction {TXN}.\n"));
}

/// `items` is the declared array of a page. A scalar in its place is a body
/// this CLI cannot read, and reporting it as an empty collection would tell
/// the caller there are no transactions.
#[tokio::test]
async fn a_page_whose_items_are_not_an_array_is_a_decode_error() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path("/v2/transactions"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"items": "oops"})),
        )
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args(["--output", "json", "transactions", "list"])
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["kind"], "decode", "{v}");
}

/// A page is an object carrying `items` and `pageInfo`. A bare array is a
/// different shape entirely, and it carries no `hasMore` for a walk to read.
#[tokio::test]
async fn a_page_that_is_a_bare_array_is_a_decode_error() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path("/v2/transactions"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!([{"transactionId": "t1"}])),
        )
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args(["--output", "json", "transactions", "list"])
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["kind"], "decode", "{v}");
}

/// A rate flag is a percentage, so `0.18` is 0.18% — a legal rate, and almost
/// always eighteen percent typed as a fraction. The note goes to stderr and
/// changes neither the request nor the envelope.
#[tokio::test]
async fn a_rate_below_one_is_sent_as_typed_and_noted_on_stderr() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/transactions/calculate-amount"))
        .and(body_json(
            serde_json::from_str::<serde_json::Value>(r#"{"baseAmount":100.00,"tipRate":0.18}"#)
                .unwrap(),
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"totalAmount": 100.18})),
        )
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args([
            "--output",
            "json",
            "transactions",
            "calculate-amount",
            "--amount",
            "100.00",
            "--tip-rate",
            "0.18",
        ])
        .assert()
        .success()
        .get_output()
        .clone();

    assert_eq!(
        String::from_utf8_lossy(&out.stderr).trim(),
        "`--tip-rate 0.18` means 0.18%, not 18%"
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["object"], "amount_calculation", "{v}");
    assert_eq!(v["data"]["totalAmount"], 100.18, "{v}");
}

/// The ACH hold, the ACH release and a reversal of an ACH transaction each
/// answer with a `referenceId` the API assigned, so the merchant's own
/// reference stops selecting the transaction and the transaction id is the
/// handle left to reconcile by. The note goes to **stderr**: stdout stays the
/// data, and the CLI never saw the value that was sent, so the note states
/// whose the returned one is rather than comparing two values.
#[tokio::test]
async fn an_ach_action_notes_the_reference_the_api_assigned() {
    for (verb, args, status) in [
        ("ach-hold", vec!["transactions", "ach-hold", TXN], "Held"),
        (
            "ach-release",
            vec!["transactions", "ach-release", TXN],
            "Scheduled",
        ),
        (
            "reversal",
            vec!["transactions", "reversal", "--transaction-id", TXN],
            "Cancelled",
        ),
    ] {
        let server = support::mock_with_token().await;
        Mock::given(method("POST"))
            .and(path(format!("/v2/transactions/{TXN}/{verb}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "transactionId": TXN, "transactionStatus": status,
                "paymentMethodType": "ACH", "referenceId": "srv-assigned"})))
            .mount(&server)
            .await;

        let out = support::bin(&server)
            .args(["--output", "json"])
            .args(&args)
            .assert()
            .success()
            .get_output()
            .clone();

        assert_eq!(
            String::from_utf8_lossy(&out.stderr).trim(),
            format!(
                "`{verb}` assigns its own `referenceId`; the value in this answer is the API\'s, \
                 not the merchant reference. Reconcile by transaction id."
            )
        );
        // stdout carries the response and nothing else.
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(v["data"]["referenceId"], "srv-assigned", "{v}");
    }
}

/// A card reversal keeps the reference the transaction was created with, so it
/// earns no note: the CLI reports the substitution where it happens and stays
/// quiet where it does not.
#[tokio::test]
async fn a_card_reversal_carries_no_reference_note() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path(format!("/v2/transactions/{TXN}/reversal")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "transactionId": TXN, "transactionStatus": "Cancelled",
            "paymentMethodType": "Card", "referenceId": "order-9"})))
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args([
            "--output",
            "json",
            "transactions",
            "reversal",
            "--transaction-id",
            TXN,
        ])
        .assert()
        .success()
        .get_output()
        .clone();
    assert_eq!(String::from_utf8_lossy(&out.stderr).trim(), "");
}

/// Zero moves no money, so a capture, a reversal or a tip adjustment asking
/// for it is refused where every other amount is — and `-0.00` is a negative
/// amount however `Decimal` stores it.
#[tokio::test]
async fn a_zero_or_negative_zero_amount_is_refused_before_the_wire() {
    for args in [
        vec![
            "transactions",
            "capture",
            "--transaction-id",
            TXN,
            "--amount",
            "0",
        ],
        vec![
            "transactions",
            "reversal",
            "--transaction-id",
            TXN,
            "--amount",
            "0.00",
        ],
        vec![
            "transactions",
            "tip-adjust",
            "--transaction-id",
            TXN,
            "--tip-amount",
            "0",
        ],
        vec![
            "transactions",
            "capture",
            "--transaction-id",
            TXN,
            "--amount",
            "-0.00",
        ],
        vec![
            "transactions",
            "tip-adjust",
            "--transaction-id",
            TXN,
            "--tip-rate",
            "0",
        ],
    ] {
        let server = support::mock_with_token().await;
        support::bin(&server).args(&args).assert().code(3);
        assert!(
            server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .all(|r| r.url.path() == "/oauth2/token"),
            "a client-side refusal issued a request: {args:?}"
        );
    }
}

/// The calculated totals name no resource, so `quiet` prints nothing at all.
/// A currency code is not an identifier: nothing can be fetched with it, and
/// a script chaining on it would feed the next command a country's money.
#[tokio::test]
async fn calculate_amount_prints_nothing_in_quiet_mode() {
    let server = support::mock_with_token().await;
    support::mount(
        &server,
        "flute-v2-post-transactions-calculate-amount",
        "default",
    )
    .await;
    support::bin(&server)
        .args([
            "--output",
            "quiet",
            "transactions",
            "calculate-amount",
            "--amount",
            "100.00",
            "--currency-code",
            "USD",
        ])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

/// `--sort-by` takes only the fields the API declares, so a typo is refused
/// with the valid names rather than answered with a 400, and `--asc` and
/// `--desc` cannot be combined.
#[tokio::test]
async fn transaction_list_sort_flags_are_checked_before_the_wire() {
    let server = support::mock_with_token().await;
    let out = support::bin(&server)
        .args(["transactions", "list", "--sort-by", "amount"])
        .assert()
        .code(3);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).to_string();
    assert!(stderr.contains("processedAmount"), "{stderr}");
    support::bin(&server)
        .args(["transactions", "list", "--asc", "--desc"])
        .assert()
        .code(3);
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.url.path() == "/oauth2/token"),
        "a client-side refusal must issue no request"
    );
}

/// `--status` accepts the capitalisation the API prints, so a value copied
/// from a response filters rather than being refused.
#[tokio::test]
async fn transaction_list_status_accepts_the_casing_the_api_prints() {
    let server = support::mock_with_token().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/v2/transactions"))
        .and(wiremock::matchers::query_param(
            "transactionStatus",
            "Declined",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"items": [], "pageInfo": {"hasMore": false}})),
        )
        .expect(1)
        .mount(&server)
        .await;
    support::bin(&server)
        .args(["transactions", "list", "--status", "Declined"])
        .assert()
        .success();
}

/// The lookup a partial reversal makes first: the transaction in the given
/// payment method and status.
async fn mount_transaction_state(server: &wiremock::MockServer, method_type: &str, status: &str) {
    Mock::given(method("GET"))
        .and(path(format!("/v2/transactions/{TXN}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "transactionId": TXN,
            "paymentMethodType": method_type,
            "transactionStatus": status,
        })))
        .mount(server)
        .await;
}

/// **A partial the API would turn into a full reversal is refused**, and
/// no reversal is sent: an unsettled card transaction is voided in full, and
/// an ACH one is reversed in full.
#[tokio::test]
async fn a_partial_reversal_the_api_would_not_honour_is_refused_before_the_reversal() {
    for (method_type, status) in [("Card", "Captured"), ("ACH", "Settled")] {
        let server = support::mock_with_token().await;
        mount_transaction_state(&server, method_type, status).await;
        support::bin(&server)
            .args([
                "transactions",
                "reversal",
                "--transaction-id",
                TXN,
                "--amount",
                "5.00",
            ])
            .assert()
            .code(3);
        assert!(
            server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .all(|r| r.method.as_str() != "POST" || r.url.path() == "/oauth2/token"),
            "{method_type} {status}: a refused partial must send no reversal"
        );
    }
}
