mod support;

use predicates::prelude::*;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

const LINK: &str = "3f8b2c1d-6e40-4a92-b7c5-1d2e3f4a5b60";
const CARD_PP: &str = "8db2ff47-b143-4adb-ab58-a11111111111";
const ACH_PP: &str = "6bbfbe3e-04dd-41cd-82bf-1466e0159007";
const CUS: &str = "588f57a5-fe6a-4844-851e-e98914e81980";

#[tokio::test]
async fn payment_link_create_required_only_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-post-payment-links", "required only").await;
    support::bin(&server)
        .args([
            "payment-links",
            "create",
            "--card-enabled",
            "--currency-code",
            "USD",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn payment_link_create_full_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-post-payment-links", "fully specified").await;
    support::bin(&server)
        .args([
            "payment-links",
            "create",
            "--card-enabled",
            "--card-processor-id",
            CARD_PP,
            "--ach-enabled",
            "--ach-processor-id",
            ACH_PP,
            "--amount",
            "25.00",
            "--currency-code",
            "USD",
            "--link-type",
            "multi-use",
            "--customer-id",
            CUS,
            "--reference-id",
            "ORDER-1042",
            "--name",
            "Spring campaign",
            "--description",
            "Shared with the campaign team",
            "--expires-on",
            "2026-09-15T00:00:00.000Z",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn payment_link_get_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-get-payment-links-paymentLinkId",
        "default",
    )
    .await;
    support::bin(&server)
        .args(["payment-links", "get", LINK])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn payment_link_list_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-get-payment-links",
        "first page, server defaults",
    )
    .await;
    support::bin(&server)
        .args(["payment-links", "list"])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn payment_link_list_every_filter_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-get-payment-links", "every filter").await;
    support::bin(&server)
        .args([
            "payment-links",
            "list",
            "--page-index",
            "1",
            "--page-size",
            "5",
            "--desc",
            "--sort-by",
            "createdOn",
            "--search",
            "spring",
            "--link-type",
            "single-use",
            "--status",
            "active",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn payment_link_update_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-patch-payment-links-paymentLinkId",
        "rename",
    )
    .await;
    support::bin(&server)
        .args([
            "payment-links",
            "update",
            LINK,
            "--name",
            "Spring campaign (extended)",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

/// The merge-patch half: an explicit null clears a clearable field, which an
/// omitted key cannot express.
#[tokio::test]
async fn payment_link_update_clear_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-patch-payment-links-paymentLinkId",
        "clearing a field",
    )
    .await;
    support::bin(&server)
        .args(["payment-links", "update", LINK, "--description", ""])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

/// **204, not 200.** Every other delete in the API answers 200, and a
/// renderer that tried to parse a body would fail here.
#[tokio::test]
async fn payment_link_delete_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-delete-payment-links-paymentLinkId",
        "default",
    )
    .await;
    support::bin(&server)
        .args(["payment-links", "delete", LINK, "--yes"])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

/// Also 204.
#[tokio::test]
async fn payment_link_share_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-post-payment-links-paymentLinkId-share",
        "by sms",
    )
    .await;
    support::bin(&server)
        .args([
            "payment-links",
            "share",
            LINK,
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

/// Destructive, so it is refused client-side with no request issued.
#[tokio::test]
async fn payment_link_delete_without_yes_issues_no_request() {
    let server = support::mock_with_token().await;
    let out = support::bin(&server)
        .args(["payment-links", "delete", LINK])
        .assert()
        .code(3)
        .get_output()
        .clone();
    assert_eq!(
        String::from_utf8_lossy(&out.stderr).trim_end(),
        format!(
            "Error: deletion requires --yes to confirm \
             (e.g. `payment-links delete {LINK} --yes`)"
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

/// The idempotent-404 rule, on the one delete that answers 204 when it works.
#[tokio::test]
async fn payment_link_delete_on_a_missing_link_exits_zero() {
    let server = support::mock_with_token().await;
    Mock::given(method("DELETE"))
        .and(path(format!("/v2/payment-links/{LINK}")))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(serde_json::json!({"Title": "Not found", "CorrelationId": "c-1"})),
        )
        .mount(&server)
        .await;
    let out = support::bin(&server)
        .args(["--output", "json", "payment-links", "delete", LINK, "--yes"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["data"]["paymentLinkId"], LINK);
    assert_eq!(v["data"]["deleted"], false, "{v}");
    assert_eq!(v["data"]["found"], false, "{v}");
}

/// `paymentMethods` is the only required field, so a create naming no method
/// is refused before the wire rather than sending an empty object.
#[tokio::test]
async fn payment_link_create_without_a_payment_method_issues_no_request() {
    let server = support::mock_with_token().await;
    support::bin(&server)
        .args(["payment-links", "create", "--amount", "25.00"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("--card-enabled"));
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.url.path() == "/oauth2/token"),
    );
}

/// An update with nothing in it is a round trip that cannot change anything.
#[tokio::test]
async fn payment_link_update_with_no_fields_issues_no_request() {
    let server = support::mock_with_token().await;
    support::bin(&server)
        .args(["payment-links", "update", LINK])
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
    );
}

/// The schema says `name` and `currencyCode` cannot be cleared, so an empty
/// value for either is refused here — no null is spent on a rejection.
#[tokio::test]
async fn payment_link_update_cannot_clear_a_field_the_schema_freezes() {
    let server = support::mock_with_token().await;
    for flag in ["--name", "--currency-code"] {
        support::bin(&server)
            .args(["payment-links", "update", LINK, flag, ""])
            .assert()
            .code(3)
            .stderr(predicate::str::contains("cannot be cleared"));
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

/// **A flag the command cannot run without has to say so where a caller
/// looks.** `--currency-code` is optional in the bundle and required by this
/// command, so clap puts it under `[OPTIONS]` and lists no `Required` marker
/// of its own. Every other required flag on every other command says so in
/// its help; a caller reading this one would otherwise be told the opposite
/// of what the command does, and only find out by running it.
#[tokio::test]
async fn the_create_help_says_currency_code_is_required() {
    let server = support::mock_with_token().await;
    support::bin(&server)
        .args(["payment-links", "create", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Currency code, e.g. `USD` (required)",
        ));
}

/// A create's booleans are bare switches; an update's take a value, because a
/// PATCH has to be able to turn a payment method **off**.
#[tokio::test]
async fn an_update_can_disable_a_payment_method() {
    let server = support::mock_with_token().await;
    Mock::given(method("PATCH"))
        .and(path(format!("/v2/payment-links/{LINK}")))
        .and(body_json(serde_json::json!({
            "paymentMethods": {"ach": {"enabled": false}}})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "paymentLinkId": LINK, "paymentLinkStatus": "Active"})))
        .mount(&server)
        .await;
    support::bin(&server)
        .args(["payment-links", "update", LINK, "--ach-enabled", "false"])
        .assert()
        .success();
}

/// A link with no amount is a flexible-amount link, so the key is absent
/// rather than zero or null. Only the absent case can establish that.
#[tokio::test]
async fn a_create_without_an_amount_omits_the_key_entirely() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/payment-links"))
        .and(body_json(serde_json::json!({
            "paymentMethods": {"card": {"enabled": true}},
            "currencyCode": "USD",
            "linkType": "MultiUse"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "paymentLinkId": LINK, "paymentLinkStatus": "Active"})))
        .mount(&server)
        .await;
    support::bin(&server)
        .args([
            "payment-links",
            "create",
            "--card-enabled",
            "--currency-code",
            "USD",
            "--link-type",
            "multi-use",
        ])
        .assert()
        .success();
}

#[tokio::test]
async fn payment_link_list_table_shows_the_url_the_status_and_the_takings() {
    let server = support::mock_with_token().await;
    support::mount(
        &server,
        "flute-v2-get-payment-links",
        "first page, server defaults",
    )
    .await;
    support::bin(&server)
        .args(["--output", "table", "payment-links", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("URL"))
        .stdout(predicate::str::contains("https://pay.example.com/l/abc123"))
        .stdout(predicate::str::contains("Active"))
        .stdout(predicate::str::contains("25.00"));
}

/// A bodyless 204 has no resource to print, so both verbs confirm from the id
/// the caller supplied.
#[tokio::test]
async fn the_two_bodyless_verbs_confirm_from_the_request() {
    let server = support::mock_with_token().await;
    support::mount(
        &server,
        "flute-v2-delete-payment-links-paymentLinkId",
        "default",
    )
    .await;
    support::mount(
        &server,
        "flute-v2-post-payment-links-paymentLinkId-share",
        "by sms",
    )
    .await;

    support::bin(&server)
        .args(["--output", "json", "payment-links", "delete", LINK, "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"paymentLinkId\""))
        .stdout(predicate::str::contains("\"deleted\": true"));

    support::bin(&server)
        .args([
            "--output",
            "json",
            "payment-links",
            "share",
            LINK,
            "--share-by",
            "sms",
            "--recipient",
            "+14155552309",
            "--consent",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"shared\": true"));
}

/// `share` sends a message to somebody, so its two required fields are
/// enforced by clap and nothing is sent without them.
#[tokio::test]
async fn share_without_a_recipient_is_a_usage_error() {
    let server = support::mock_with_token().await;
    support::bin(&server)
        .args(["payment-links", "share", LINK, "--share-by", "sms"])
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

/// `SharePaymentLinkRequestDto.shareBy` declares exactly `Email` and `Sms`.
/// The transaction receipt's field of the same name declares a third value,
/// `None`, and offering it here would send something this schema rejects.
#[tokio::test]
async fn share_by_offers_only_the_two_values_this_schema_declares() {
    let server = support::mock_with_token().await;
    support::bin(&server)
        .args([
            "payment-links",
            "share",
            LINK,
            "--share-by",
            "none",
            "--recipient",
            "+14155552309",
            "--consent",
        ])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("email"))
        .stderr(predicate::str::contains("sms"));
}

/// The API refuses a share without the customer's consent, so a share
/// without `--consent` is refused before the wire, exit 3, naming the flag.
#[tokio::test]
async fn share_without_consent_is_refused_before_the_wire() {
    let server = support::mock_with_token().await;
    support::bin(&server)
        .args([
            "payment-links",
            "share",
            LINK,
            "--share-by",
            "email",
            "--recipient",
            "ada@example.com",
        ])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("--consent is required"));
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.url.path() == "/oauth2/token"),
    );
}

/// Both bodyless verbs say what they did, in a sentence.
#[tokio::test]
async fn the_confirmation_lines_are_sentences() {
    let server = support::mock_with_token().await;
    support::mount(
        &server,
        "flute-v2-delete-payment-links-paymentLinkId",
        "default",
    )
    .await;
    support::mount(
        &server,
        "flute-v2-post-payment-links-paymentLinkId-share",
        "by sms",
    )
    .await;

    support::bin(&server)
        .args([
            "payment-links",
            "share",
            LINK,
            "--share-by",
            "sms",
            "--recipient",
            "+14155552309",
            "--consent",
        ])
        .assert()
        .success()
        .stdout(format!("Shared payment link {LINK}.\n"));

    support::bin(&server)
        .args(["payment-links", "delete", LINK, "--yes"])
        .assert()
        .success()
        .stdout(format!("Deleted payment link {LINK}.\n"));
}

/// An expiry the API cannot read is refused before the wire, naming the
/// flag, on create and on update alike.
#[tokio::test]
async fn an_expiry_that_is_not_a_utc_date_time_is_refused_before_the_wire() {
    let server = support::mock_with_token().await;
    for args in [
        vec![
            "payment-links",
            "create",
            "--card-enabled",
            "--currency-code",
            "USD",
            "--expires-on",
            "tomorrow",
        ],
        vec![
            "payment-links",
            "update",
            "8f0e1d2c-3b4a-4c5d-9e6f-7a8b9c0d1e2f",
            "--expires-on",
            "2026-10-15",
        ],
    ] {
        support::bin(&server)
            .args(&args)
            .assert()
            .code(3)
            .stderr(predicate::str::contains("--expires-on"))
            .stderr(predicate::str::contains("ending in Z"));
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
