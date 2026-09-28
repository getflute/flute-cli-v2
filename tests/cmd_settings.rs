mod support;

use predicates::prelude::*;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn settings_payment_config_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-get-settings-payment-config", "default").await;
    support::bin(&server)
        .args(["settings", "payment-config"])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn settings_contact_info_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-get-settings-contact-information",
        "default",
    )
    .await;
    support::bin(&server)
        .args(["settings", "contact-info"])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn settings_autofill_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-get-settings-transaction-autofill",
        "default",
    )
    .await;
    support::bin(&server)
        .args(["settings", "autofill"])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

/// The smallest legal PATCH: nothing is required, so one rate is a whole
/// request and the rest of the body is absent rather than nulled.
#[tokio::test]
async fn settings_update_autofill_one_rate_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-patch-settings-transaction-autofill",
        "one rate",
    )
    .await;
    support::bin(&server)
        .args(["settings", "update-autofill", "--l2-tax-rate", "7.25"])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn settings_update_autofill_every_field_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-patch-settings-transaction-autofill",
        "every field",
    )
    .await;
    support::bin(&server)
        .args([
            "settings",
            "update-autofill",
            "--l2-tax-rate",
            "8.5",
            "--l3-shipping-rate",
            "5.0",
            "--l3-duty-rate",
            "2.5",
            "--product-name",
            "Office Supplies",
            "--product-code",
            "OFF001",
            "--product-unit",
            "pcs",
            "--product-unit-price",
            "25.00",
            "--product-quantity",
            "10.0",
            "--product-discount",
            "5.0",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

/// A PATCH with nothing in it is a round trip that cannot change anything,
/// and the schema declares no required fields, so the API would accept it.
#[tokio::test]
async fn settings_update_autofill_with_no_fields_issues_no_request() {
    let server = support::mock_with_token().await;
    support::bin(&server)
        .args(["settings", "update-autofill"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("nothing to update"));
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

/// The declared bound on `level2Settings.taxRate` is 0 to 22 — **not** the
/// 0.01 to 100 the same idea carries on `transactions create`.
#[tokio::test]
async fn settings_update_autofill_refuses_a_rate_outside_the_declared_bound() {
    let server = support::mock_with_token().await;
    support::bin(&server)
        .args(["settings", "update-autofill", "--l2-tax-rate", "23"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("22"));
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.url.path() == "/oauth2/token"),
    );
}

/// Only the product fields that were given reach the wire, and the two
/// container objects appear only when something inside them does.
#[tokio::test]
async fn settings_update_autofill_omits_the_containers_it_has_nothing_for() {
    let server = support::mock_with_token().await;
    Mock::given(method("PATCH"))
        .and(path("/v2/settings/transaction-autofill"))
        .and(body_json(serde_json::json!({
            "level3Settings": {"product": {"code": "OFF001"}}})))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    support::bin(&server)
        .args(["settings", "update-autofill", "--product-code", "OFF001"])
        .assert()
        .success();
}

/// The command two other commands' errors point at, so its table has to show
/// the processor ids those callers came for.
#[tokio::test]
async fn payment_config_table_shows_the_processor_ids_first() {
    let server = support::mock_with_token().await;
    support::mount(&server, "flute-v2-get-settings-payment-config", "default").await;
    let out = support::bin(&server)
        .args(["--output", "table", "settings", "payment-config"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let out = String::from_utf8(out).unwrap();
    let processor_line = out
        .lines()
        .position(|l| l.contains("8db2ff47-b143-4adb-ab58-a11111111111"))
        .expect("the processor id is not in the table at all");
    let company_line = out
        .lines()
        .position(|l| l.contains("Analytical Engines"))
        .expect("companyName missing");
    assert!(
        processor_line < company_line,
        "the processor id has to come before the account trivia; two commands \
         send people here for it.\n{out}"
    );
    assert!(out.contains("TSYS"), "{out}");
    assert!(out.contains("isDefault"), "{out}");
}

/// A settings read is a nested document, and the shared detail renderer has
/// to reach all of it — the tip options are an array of scalars and the batch
/// time slots an array of objects.
#[tokio::test]
async fn payment_config_table_reaches_the_nested_and_repeated_fields() {
    let server = support::mock_with_token().await;
    support::mount(&server, "flute-v2-get-settings-payment-config", "default").await;
    support::bin(&server)
        .args(["--output", "table", "settings", "payment-config"])
        .assert()
        .success()
        .stdout(predicate::str::contains("10, 15, 20"))
        .stdout(predicate::str::contains(
            "availablePaymentProcessors[0].settlementBatchTimeSlots[0].timezoneName",
        ))
        .stdout(predicate::str::contains("America/New_York"))
        .stdout(predicate::str::contains(
            "addressVerificationServiceOptions.profile",
        ));
}

#[tokio::test]
async fn contact_info_table_reaches_into_the_repeated_addresses() {
    let server = support::mock_with_token().await;
    support::mount(
        &server,
        "flute-v2-get-settings-contact-information",
        "default",
    )
    .await;
    support::bin(&server)
        .args(["--output", "table", "settings", "contact-info"])
        .assert()
        .success()
        .stdout(predicate::str::contains("contactInfos[0].addressName"))
        .stdout(predicate::str::contains("Main Office"))
        .stdout(predicate::str::contains("+14155552309"));
}

/// The three reads carry their own envelope names, and none of them is a
/// collection: each answers one settings document.
#[tokio::test]
async fn each_settings_read_carries_its_own_envelope_name() {
    let server = support::mock_with_token().await;
    support::mount(&server, "flute-v2-get-settings-payment-config", "default").await;
    support::mount(
        &server,
        "flute-v2-get-settings-contact-information",
        "default",
    )
    .await;
    support::mount(
        &server,
        "flute-v2-get-settings-transaction-autofill",
        "default",
    )
    .await;

    for (command, object) in [
        ("payment-config", "payment_config"),
        ("contact-info", "contact_info"),
        ("autofill", "transaction_autofill"),
    ] {
        let out = support::bin(&server)
            .args(["--output", "json", "settings", command])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["object"], object);
        assert!(v["meta"].get("page_info").is_none(), "{v}");
    }
}

/// A bodyless write on a **singleton** resource: there is no id to confirm
/// from, so the confirmation names the resource and the verb, `quiet` prints
/// nothing at all, and the JSON envelope carries no invented id key.
#[tokio::test]
async fn update_autofill_confirms_without_an_id() {
    let server = support::mock_with_token().await;
    support::mount(
        &server,
        "flute-v2-patch-settings-transaction-autofill",
        "one rate",
    )
    .await;
    support::bin(&server)
        .args([
            "--output",
            "table",
            "settings",
            "update-autofill",
            "--l2-tax-rate",
            "7.25",
        ])
        .assert()
        .success()
        .stdout("Updated transaction autofill settings.\n");

    support::bin(&server)
        .args([
            "--output",
            "quiet",
            "settings",
            "update-autofill",
            "--l2-tax-rate",
            "7.25",
        ])
        .assert()
        .success()
        .stdout("");

    let out = support::bin(&server)
        .args([
            "--output",
            "json",
            "settings",
            "update-autofill",
            "--l2-tax-rate",
            "7.25",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["object"], "transaction_autofill");
    assert_eq!(v["data"]["updated"], true);
    assert_eq!(
        v["data"].as_object().unwrap().len(),
        1,
        "a singleton has no id, so the envelope must not carry an empty one: {v}"
    );
}

/// A singleton has no identifier, so `quiet` has nothing to print — and
/// prints nothing at all, not a blank line a script would read as a value.
#[tokio::test]
async fn a_read_with_no_identifier_prints_nothing_in_quiet_mode() {
    let server = support::mock_with_token().await;
    support::mount(
        &server,
        "flute-v2-get-settings-transaction-autofill",
        "default",
    )
    .await;
    support::bin(&server)
        .args(["--output", "quiet", "settings", "autofill"])
        .assert()
        .success()
        .stdout("");
}

/// The autofill product discount is a percentage like every other rate flag,
/// so a value between 0 and 1 earns the same stderr note.
#[tokio::test]
async fn a_fractional_product_discount_is_noted_on_stderr() {
    let server = support::mock_with_token().await;
    Mock::given(method("PATCH"))
        .and(path("/v2/settings/transaction-autofill"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    support::bin(&server)
        .args(["settings", "update-autofill", "--product-discount", "0.05"])
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "`--product-discount 0.05` means 0.05%, not 5%",
        ));
}

/// The endpoint has no member for a product description: a body carrying one
/// is rejected outright rather than ignored, so the CLI offers no spelling
/// that could reach it.
#[tokio::test]
async fn update_autofill_offers_no_product_description() {
    let server = support::mock_with_token().await;
    for args in [
        vec![
            "settings",
            "update-autofill",
            "--product-description",
            "Standard office supplies",
        ],
        vec![
            "settings",
            "update-autofill",
            "--clear",
            "product-description",
        ],
    ] {
        support::bin(&server).args(&args).assert().code(3);
    }
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.url.path() == "/oauth2/token"),
        "a field the API rejects must not reach it"
    );
}

/// `quiet` is the mode a caller chains into `--payment-processor-id`, and an
/// account can hold several processors — so every id is printed, one per
/// line. One of them chosen by position sends a card charge to the ACH
/// processor.
#[tokio::test]
async fn payment_config_quiet_prints_every_processor_id() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path("/v2/settings/payment-config"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "availablePaymentProcessors": [
                {
                    "paymentProcessorId": "6bbfbe3e-04dd-41cd-82bf-1466e0159007",
                    "processorName": "ACH",
                    "isDefault": true,
                    "type": "Ach"},
                {
                    "paymentProcessorId": "8db2ff47-b143-4adb-ab58-a11111111111",
                    "processorName": "TSYS",
                    "isDefault": false,
                    "type": "Tsys"}],
            "currency": "USD"})))
        .mount(&server)
        .await;

    support::bin(&server)
        .args(["--output", "quiet", "settings", "payment-config"])
        .assert()
        .success()
        .stdout(
            "6bbfbe3e-04dd-41cd-82bf-1466e0159007\n\
             8db2ff47-b143-4adb-ab58-a11111111111\n",
        );
}
