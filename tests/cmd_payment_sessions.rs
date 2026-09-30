mod support;

use predicates::prelude::*;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

const SESSION: &str = "9c1f7a3e-2b58-4d06-8e93-4a5b6c7d8e90";
const CARD_PP: &str = "8db2ff47-b143-4adb-ab58-a11111111111";
const ACH_PP: &str = "6bbfbe3e-04dd-41cd-82bf-1466e0159007";
const CUS: &str = "588f57a5-fe6a-4844-851e-e98914e81980";

/// Nothing is required, and an absent amount is documented as a
/// flexible-amount session — so `create` with no flags is a whole request.
#[tokio::test]
async fn payment_session_flexible_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-post-payment-sessions", "flexible amount").await;
    support::bin(&server)
        .args(["payment-sessions", "create"])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

/// A vault-only session's amount must be **zero**, not absent: absent means
/// flexible, which a session that takes no payment cannot be.
#[tokio::test]
async fn payment_session_vault_only_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-post-payment-sessions", "vault only").await;
    support::bin(&server)
        .args([
            "payment-sessions",
            "create",
            "--mode",
            "save-method",
            "--customer-handling",
            "create-customer",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn payment_session_full_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-post-payment-sessions", "fully specified").await;
    support::bin(&server)
        .args([
            "payment-sessions",
            "create",
            "--mode",
            "payment-and-save",
            "--amount",
            "25.00",
            "--tip-amount",
            "14.50",
            "--customer-id",
            CUS,
            "--customer-handling",
            "token-only",
            "--reference-id",
            "ORDER-10001",
            "--return-url",
            "https://example.com/done",
            "--skip-address-verification",
            "--page-name",
            "Analytical Engines checkout",
            "--payment-notes",
            "Repair deposit",
            "--after-completion-message",
            "Thank you.",
            "--expires-at",
            "2027-02-19T20:24:52.934Z",
            "--metadata",
            "orderId=9921",
            "--card-enabled",
            "--card-processor-id",
            CARD_PP,
            "--ach-enabled",
            "--ach-processor-id",
            ACH_PP,
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn payment_session_get_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-get-payment-sessions-paymentSessionId",
        "default",
    )
    .await;
    support::bin(&server)
        .args(["payment-sessions", "get", SESSION])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn payment_session_cancel_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-post-payment-sessions-paymentSessionId-cancel",
        "default",
    )
    .await;
    support::bin(&server)
        .args(["payment-sessions", "cancel", SESSION, "--yes"])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

/// Destructive, so it is refused client-side with no request issued — the one
/// destructive verb in the API that is neither a delete nor a revoke.
#[tokio::test]
async fn payment_session_cancel_without_yes_issues_no_request() {
    let server = support::mock_with_token().await;
    let out = support::bin(&server)
        .args(["payment-sessions", "cancel", SESSION])
        .assert()
        .code(3)
        .get_output()
        .clone();
    assert_eq!(
        String::from_utf8_lossy(&out.stderr).trim_end(),
        format!(
            "Error: cancellation requires --yes to confirm \
             (e.g. `payment-sessions cancel {SESSION} --yes`)"
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

/// A cancelled session still exists, so the only 404 a cancel can meet is an
/// id the server never had — which is not found, not a cancellation.
#[tokio::test]
async fn payment_session_cancel_on_a_missing_session_exits_4() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path(format!("/v2/payment-sessions/{SESSION}/cancel")))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(serde_json::json!({"Title": "Not found", "CorrelationId": "c-1"})),
        )
        .mount(&server)
        .await;
    support::bin(&server)
        .args(["payment-sessions", "cancel", SESSION, "--yes"])
        .assert()
        .code(4)
        .stdout(predicate::str::contains("Cancelled").not());
}

/// A `Payment` session's amount must be greater than zero, so a zero is
/// refused before the wire — with the flexible alternative named, because the
/// two are easy to confuse.
#[tokio::test]
async fn a_zero_amount_on_a_paying_session_is_refused() {
    let server = support::mock_with_token().await;
    support::bin(&server)
        .args(["payment-sessions", "create", "--amount", "0"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("Omit"));
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.url.path() == "/oauth2/token"),
    );
}

/// And a vault-only session's amount must be zero, so a positive one is
/// refused rather than sent to be rejected.
#[tokio::test]
async fn a_nonzero_amount_on_a_vault_only_session_is_refused() {
    let server = support::mock_with_token().await;
    support::bin(&server)
        .args([
            "payment-sessions",
            "create",
            "--mode",
            "save-method",
            "--amount",
            "25.00",
        ])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("save-method"));
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.url.path() == "/oauth2/token"),
    );
}

/// A save-method session has no checkout page, so the flags that configure
/// one are refused before any request, named in the message.
#[tokio::test]
async fn a_vault_only_session_refuses_checkout_flags_before_the_wire() {
    let server = support::mock_with_token().await;
    support::bin(&server)
        .args([
            "payment-sessions",
            "create",
            "--mode",
            "save-method",
            "--card-enabled",
            "--return-url",
            "https://example.com/done",
        ])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("--card-enabled, --return-url"));
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.url.path() == "/oauth2/token"),
    );
}

/// A save-method create answers with an id and a null `paymentMethods`, so
/// its table carries no payment-method rows that could never fill.
#[tokio::test]
async fn a_vault_only_create_table_has_no_payment_method_rows() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/payment-sessions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"id": SESSION, "paymentMethods": null})),
        )
        .mount(&server)
        .await;
    support::bin(&server)
        .args([
            "--output",
            "table",
            "payment-sessions",
            "create",
            "--mode",
            "save-method",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(SESSION))
        .stdout(predicate::str::contains("paymentMethods.").not());
}

/// Metadata is arbitrary key-value pairs, so a value with an `=` in it has to
/// survive: only the first separator splits.
#[tokio::test]
async fn metadata_splits_on_the_first_equals_only() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/payment-sessions"))
        .and(body_json(serde_json::json!({
            "metadata": {"query": "a=b&c=d"}})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"id": SESSION})))
        .mount(&server)
        .await;
    support::bin(&server)
        .args(["payment-sessions", "create", "--metadata", "query=a=b&c=d"])
        .assert()
        .success();
}

#[tokio::test]
async fn metadata_without_an_equals_is_a_usage_error() {
    let server = support::mock_with_token().await;
    support::bin(&server)
        .args(["payment-sessions", "create", "--metadata", "orderId"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("key=value"));
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.url.path() == "/oauth2/token"),
    );
}

/// **The get response carries no identifier of any kind**, so `quiet` on a
/// read has nothing to print — and prints nothing at all, not a blank line a
/// script would read as a value. Only the create response declares an `id`.
#[tokio::test]
async fn quiet_prints_an_id_on_create_and_nothing_on_get() {
    let server = support::mock_with_token().await;
    support::mount(&server, "flute-v2-post-payment-sessions", "flexible amount").await;
    support::mount(
        &server,
        "flute-v2-get-payment-sessions-paymentSessionId",
        "default",
    )
    .await;

    support::bin(&server)
        .args(["--output", "quiet", "payment-sessions", "create"])
        .assert()
        .success()
        .stdout(format!("{SESSION}\n"));

    support::bin(&server)
        .args(["--output", "quiet", "payment-sessions", "get", SESSION])
        .assert()
        .success()
        .stdout("");
}

/// A bodyless success has no resource to print, so the cancel confirms from
/// the id the caller supplied.
#[tokio::test]
async fn cancel_confirms_from_the_request() {
    let server = support::mock_with_token().await;
    support::mount(
        &server,
        "flute-v2-post-payment-sessions-paymentSessionId-cancel",
        "default",
    )
    .await;
    support::bin(&server)
        .args([
            "--output",
            "json",
            "payment-sessions",
            "cancel",
            SESSION,
            "--yes",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"id\""))
        .stdout(predicate::str::contains("\"cancelled\": true"));
}

/// The cancel answers with no body, so it says what it did in a sentence.
#[tokio::test]
async fn the_confirmation_line_is_a_sentence() {
    let server = support::mock_with_token().await;
    support::mount(
        &server,
        "flute-v2-post-payment-sessions-paymentSessionId-cancel",
        "default",
    )
    .await;
    support::bin(&server)
        .args(["payment-sessions", "cancel", SESSION, "--yes"])
        .assert()
        .success()
        .stdout(format!("Cancelled payment session {SESSION}.\n"));
}

/// An expiry the API cannot read is refused before the wire, naming the flag.
#[tokio::test]
async fn an_expiry_that_is_not_a_utc_date_time_is_refused_before_the_wire() {
    let server = support::mock_with_token().await;
    support::bin(&server)
        .args(["payment-sessions", "create", "--expires-at", "tomorrow"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("--expires-at"))
        .stderr(predicate::str::contains("ending in Z"));
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.url.path() == "/oauth2/token"),
    );
}
