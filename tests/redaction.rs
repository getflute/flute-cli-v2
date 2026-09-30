//! Layer 6: the redaction guard. **This test must never be weakened.**
//!
//! If it fails, fix `redact.rs` — never the assertion.

mod support;

use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

/// Under `--debug` the CLI traces the request. Nothing that would be a
/// compliance incident may appear in that trace.
#[tokio::test]
async fn debug_traces_never_leak_pan_cvv_token_or_secret() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/transactions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "transactionId": "txn_1", "transactionStatus": "Captured"
        })))
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args([
            "--debug",
            "transactions",
            "create",
            "--payment-processor-id",
            "pp-1",
            "--amount",
            "1.00",
            "--card",
            "4111111111111111",
            "--cvv",
            "8371",
            "--exp",
            "12/2032",
        ])
        .assert()
        .get_output()
        .clone();

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(!combined.contains("4111111111111111"), "PAN leaked");
    assert!(!contains_number(&combined, "8371"), "CVV leaked");
    assert!(
        !combined.contains("securityCode\":\"8371"),
        "CVV survived redaction"
    );
    assert!(!combined.contains("tok-xyz"), "bearer token leaked");
    assert!(!combined.contains("test-secret"), "client secret leaked");
    assert!(
        combined.contains("1111"),
        "masking should keep the last four:\n{combined}"
    );
}

/// The guard is only a guard if the trace it inspects actually happened.
/// Without this, a change that stopped tracing the body entirely would make
/// every assertion above vacuously true.
#[tokio::test]
async fn the_debug_trace_really_contains_the_request_body() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/transactions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "transactionId": "txn_1", "transactionStatus": "Captured"
        })))
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args([
            "--debug",
            "transactions",
            "create",
            "--payment-processor-id",
            "pp-unmistakable",
            "--amount",
            "1.00",
            "--card",
            "4111111111111111",
            "--cvv",
            "8371",
            "--exp",
            "12/2032",
        ])
        .assert()
        .get_output()
        .clone();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();

    assert!(
        stderr.contains("pp-unmistakable"),
        "the request body was never traced, so the redaction assertions would \
         pass on an empty trace:\n{stderr}"
    );
    assert!(
        stderr.contains("securityCode"),
        "the field name should survive so a trace stays diagnosable:\n{stderr}"
    );
}

/// A non-2xx response is traced too, and an error body can reflect the
/// values that caused it.
#[tokio::test]
async fn an_error_response_body_is_redacted_as_well() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/transactions"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "Title": "Validation failed",
            "Errors": {"cardNumber": ["4111111111111111 is not acceptable"]}
        })))
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args([
            "--debug",
            "transactions",
            "create",
            "--payment-processor-id",
            "pp-1",
            "--amount",
            "1.00",
            "--card",
            "4111111111111111",
            "--cvv",
            "8371",
            "--exp",
            "12/2032",
        ])
        .assert()
        .code(3)
        .get_output()
        .clone();
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !combined.contains("4111111111111111"),
        "a PAN echoed inside an error message leaked:\n{combined}"
    );
}

/// A client secret is a credential the API hands back exactly once, and the
/// trace of the response that carries it must not be where it ends up.
///
/// This is the only response body in the API that contains a live credential,
/// so it is the one place the response-side redaction of a secret is reachable
/// through a real command.
#[tokio::test]
async fn a_created_client_secret_is_redacted_from_the_debug_trace() {
    const SECRET: &str = "1f80d6e1-9901-48b6-a862-6cfad1612e99";
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/api-keys"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "clientId": "aa98aa89-07ae-4a23-b441-48725f0386e6",
            "clientSecret": SECRET})))
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args([
            "--debug",
            "api-keys",
            "create",
            "--merchant-id",
            "8db2ff47-b143-4adb-ab58-a11111111111",
            "--name",
            "Production API Key",
        ])
        .assert()
        .success()
        .get_output()
        .clone();

    // The secret is the *point* of the command, so stdout carries it; the
    // trace on stderr must not.
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stdout.contains(SECRET),
        "the command withheld the secret it exists to deliver:\n{stdout}"
    );
    assert!(
        !stderr.contains(SECRET),
        "a client secret leaked into the debug trace:\n{stderr}"
    );
    // And the guard is only a guard if the trace happened at all.
    assert!(
        stderr.contains("clientSecret"),
        "no response trace to inspect, so the assertion above is vacuous:\n{stderr}"
    );
}

/// Redaction is not conditional on `--debug`: an error path must not print a
/// PAN either.
#[tokio::test]
async fn no_pan_reaches_the_output_without_debug() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/transactions"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "Title": "Validation failed",
            "Errors": {"cardNumber": ["4111111111111111 is not acceptable"]}
        })))
        .mount(&server)
        .await;

    let out = support::bin(&server)
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
            "8371",
            "--exp",
            "12/2032",
        ])
        .assert()
        .code(3)
        .get_output()
        .clone();
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !combined.contains("4111111111111111"),
        "a PAN reached the user-facing error path:\n{combined}"
    );
}

/// **A sensitive value too short for the digit-run rule is redacted from an
/// error response, on stdout and in the `--debug` trace alike.** A CVV is
/// three digits and an ACH account number nine, and a hyphenated PAN is four
/// runs of four; none is distinguishable by shape, only by the field name it
/// arrives under — and the API puts it in an array of messages under that
/// name, not in a string value.
#[tokio::test]
async fn a_short_sensitive_value_is_redacted_from_an_error_response() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/transactions"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "Title": "Validation failed",
            "Errors": {
                "securityCode": ["837 is invalid"],
                "accountNumber": ["123456789 is not valid"],
                "cardNumber": ["4111-1111-1111-1111 is not valid"]
            }
        })))
        .mount(&server)
        .await;

    let args = [
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
        "837",
        "--exp",
        "12/2032",
    ];
    let out = support::bin(&server)
        .args(args)
        .assert()
        .code(3)
        .get_output()
        .stdout
        .clone();
    let envelope = String::from_utf8(out).unwrap();

    // None reaches the user-facing envelope...
    assert!(!contains_number(&envelope, "837"), "{envelope}");
    assert!(!envelope.contains("123456789"), "{envelope}");
    assert!(!envelope.contains("4111-1111-1111"), "{envelope}");
    // ...and the message still says which fields failed.
    assert!(envelope.contains("securityCode"), "{envelope}");
    assert!(envelope.contains("accountNumber"), "{envelope}");

    let traced = support::bin(&server)
        .arg("--debug")
        .args(args)
        .assert()
        .code(3)
        .get_output()
        .stderr
        .clone();
    let trace = String::from_utf8(traced).unwrap();
    assert!(
        trace.contains("HTTP response"),
        "no response was traced:\n{trace}"
    );
    assert!(!contains_number(&trace, "837"), "{trace}");
    assert!(!trace.contains("123456789"), "{trace}");
    assert!(!trace.contains("4111-1111-1111"), "{trace}");
}

/// Redaction must not cost the message its meaning. An account-keyed error
/// masks the number and keeps the sentence; a secret-keyed one removes the
/// value outright, because a secret is not always digits.
#[tokio::test]
async fn redaction_keeps_the_reason_a_request_failed() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/transactions"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "Title": "Rejected",
            "Cause": "amount 1234.56 exceeds the 5000 limit",
            "Errors": {"cardNumber": ["4111111111111111 is not acceptable"]}
        })))
        .mount(&server)
        .await;

    let out = support::bin(&server)
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
            "837",
            "--exp",
            "12/2032",
        ])
        .assert()
        .code(3)
        .get_output()
        .stderr
        .clone();
    let stderr = String::from_utf8(out).unwrap();

    assert!(!stderr.contains("4111111111111111"), "{stderr}");
    assert!(stderr.contains("is not acceptable"), "{stderr}");
    // The numbers worth reading survive.
    assert!(stderr.contains("1234.56"), "{stderr}");
    assert!(stderr.contains("5000"), "{stderr}");
}

/// A tax id is a government identifier, not an account number: nine digits,
/// below the digit-run threshold, and worth as little in a trace as a CVV.
#[tokio::test]
async fn an_ach_tax_id_never_reaches_the_debug_trace() {
    const TAX_ID: &str = "987654321";
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/transactions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "transactionId": "90d084d6-55b8-4fb8-b658-861534d07f9a",
            "transactionStatus": "Pending"
        })))
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args([
            "--debug",
            "transactions",
            "create",
            "--payment-processor-id",
            "8db2ff47-b143-4adb-ab58-a11111111111",
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
            "--ach-tax-id",
            TAX_ID,
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
        .get_output()
        .clone();
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(!combined.contains(TAX_ID), "a tax id leaked:\n{combined}");
    // The guard is only a guard if the body was traced at all.
    assert!(
        combined.contains("taxId"),
        "no request trace to inspect, so the assertion above is vacuous:\n{combined}"
    );
}

/// The trace exists so an operator can see what a filter or sort flag put on
/// the wire, so the URL it reports carries the query string.
#[tokio::test]
async fn the_debug_trace_reports_the_query_string() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path("/v2/customers"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [], "totalCount": 0
        })))
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args([
            "--debug",
            "customers",
            "list",
            "--sort-by",
            "createdOn",
            "--desc",
            "--full-name",
            "Ada",
        ])
        .assert()
        .get_output()
        .clone();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();

    assert!(stderr.contains("sortBy=createdOn"), "{stderr}");
    assert!(stderr.contains("asc=false"), "{stderr}");
    assert!(stderr.contains("fullName=Ada"), "{stderr}");
}

/// A filter value is caller-supplied text and reaches the trace inside the
/// URL, so it is masked there on the same terms as a request body field.
#[tokio::test]
async fn a_card_number_in_a_filter_value_is_masked_in_the_trace() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path("/v2/customers"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [], "totalCount": 0
        })))
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args([
            "--debug",
            "customers",
            "list",
            "--full-name",
            "4111111111111111",
        ])
        .assert()
        .get_output()
        .clone();
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(
        !combined.contains("4111111111111111"),
        "a card number pasted into a filter leaked:\n{combined}"
    );
    // The guard is only a guard if the query was traced at all.
    assert!(
        combined.contains("fullName="),
        "no query in the trace to inspect, so the assertion above is vacuous:\n{combined}"
    );
}

/// Whether `digits` appears in `text` as a number of its own, with no digit
/// on either side. A timestamp's microseconds or a port can contain a short
/// value by chance, as `44.108373Z` contains `837`; a leaked one stands alone.
fn contains_number(text: &str, digits: &str) -> bool {
    text.match_indices(digits).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + digits.len()..].chars().next();
        !before.is_some_and(|c| c.is_ascii_digit()) && !after.is_some_and(|c| c.is_ascii_digit())
    })
}

#[test]
fn a_number_inside_a_timestamp_or_port_is_not_a_leak() {
    assert!(!contains_number("2026-09-30T19:50:44.108373Z", "837"));
    assert!(!contains_number("http://127.0.0.1:18371/", "8371"));
    assert!(contains_number(r#""securityCode":"837""#, "837"));
    assert!(contains_number("837 is invalid", "837"));
}
