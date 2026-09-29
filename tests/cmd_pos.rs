mod support;

use predicates::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, Request, Respond, ResponseTemplate};

const POS_TXN: &str = "7c9a3d1e-5b42-4f80-9a1c-6d3e2f8b4a05";
const TERMINAL: &str = "b1d4c0a7-3e18-4a6f-8c25-9f7e6d5b4a31";
const DEVICE: &str = "POS-DEVICE-001";
const PP: &str = "8db2ff47-b143-4adb-ab58-a11111111111";
const CUS: &str = "588f57a5-fe6a-4844-851e-e98914e81980";

/// Answer differently on each call, so a poll sequence is deterministic
/// rather than dependent on which of two identical mocks wiremock picks.
struct Sequence(Vec<serde_json::Value>, AtomicUsize);

impl Respond for Sequence {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        let at = self.1.fetch_add(1, Ordering::SeqCst);
        let body = self.0.get(at).unwrap_or_else(|| self.0.last().unwrap());
        ResponseTemplate::new(200).set_body_json(body.clone())
    }
}

fn sequence(bodies: &[serde_json::Value]) -> Sequence {
    Sequence(bodies.to_vec(), AtomicUsize::new(0))
}

fn in_progress() -> serde_json::Value {
    serde_json::json!({
        "posTransactionId": POS_TXN, "posTransactionStatus": "InProgress"})
}

fn completed() -> serde_json::Value {
    serde_json::json!({
        "posTransactionId": POS_TXN, "posTransactionStatus": "Completed"})
}

/// No `--capture-method` here: the flag defaults, so `Auto` is part of the
/// minimal exchange rather than absent from it.
#[tokio::test]
async fn pos_create_required_only_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-post-pos-transactions", "required only").await;
    support::bin(&server)
        .args([
            "pos",
            "create",
            "--terminal-id",
            TERMINAL,
            "--pos-device-id",
            DEVICE,
            "--amount",
            "42.75",
            "--currency-code",
            "USD",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn pos_create_full_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-post-pos-transactions", "fully specified").await;
    support::bin(&server)
        .args([
            "pos",
            "create",
            "--terminal-id",
            TERMINAL,
            "--pos-device-id",
            DEVICE,
            "--amount",
            "42.75",
            "--currency-code",
            "USD",
            "--capture-method",
            "manual",
            "--initiation-channel",
            "deeplink",
            "--reading-method",
            "keyed-entry",
            "--pricing-type",
            "cash",
            "--payment-processor-id",
            PP,
            "--customer-id",
            CUS,
            "--reference-id",
            "REF-POS-1",
            "--request-storage-consent",
            "--tip-amount",
            "5.00",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

/// The create half of `--wait`, with the poll it starts driven from the
/// long-polling `get` fixture so neither request is hand-written.
#[tokio::test]
async fn pos_create_wait_sends_the_acceptance_control() {
    let server = support::mock_with_token().await;
    let created = support::mount(
        &server,
        "flute-v2-post-pos-transactions",
        "waiting for terminal acceptance",
    )
    .await;
    support::mount(
        &server,
        "flute-v2-get-pos-transactions-posTransactionId",
        "long polling",
    )
    .await;
    support::bin(&server)
        .args([
            "pos",
            "create",
            "--terminal-id",
            TERMINAL,
            "--pos-device-id",
            DEVICE,
            "--amount",
            "42.75",
            "--currency-code",
            "USD",
            "--wait",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &created).await;
}

/// A deadline is an instant on a monotonic clock, so the seconds a caller may
/// name are bounded to a day rather than to `u64`.
#[tokio::test]
async fn pos_create_refuses_a_wait_timeout_past_a_day() {
    let server = support::mock_with_token().await;
    support::bin(&server)
        .args([
            "pos",
            "create",
            "--terminal-id",
            TERMINAL,
            "--pos-device-id",
            DEVICE,
            "--amount",
            "42.75",
            "--currency-code",
            "USD",
            "--wait",
            "--wait-timeout",
            "86401",
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
        "a refused argument must not reach the API"
    );
}

#[tokio::test]
async fn pos_create_accepts_a_wait_timeout_of_a_whole_day() {
    let server = support::mock_with_token().await;
    let created = support::mount(
        &server,
        "flute-v2-post-pos-transactions",
        "waiting for terminal acceptance",
    )
    .await;
    support::mount(
        &server,
        "flute-v2-get-pos-transactions-posTransactionId",
        "long polling",
    )
    .await;
    support::bin(&server)
        .args([
            "pos",
            "create",
            "--terminal-id",
            TERMINAL,
            "--pos-device-id",
            DEVICE,
            "--amount",
            "42.75",
            "--currency-code",
            "USD",
            "--wait",
            "--wait-timeout",
            "86400",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &created).await;
}

#[tokio::test]
async fn pos_get_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-get-pos-transactions-posTransactionId",
        "default",
    )
    .await;
    support::bin(&server)
        .args(["pos", "get", POS_TXN])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

/// `pos get --wait` is the long-poll query on its own, without a create in
/// front of it.
#[tokio::test]
async fn pos_get_long_polling_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-get-pos-transactions-posTransactionId",
        "long polling",
    )
    .await;
    support::bin(&server)
        .args(["pos", "get", POS_TXN, "--wait"])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn pos_list_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-get-pos-transactions",
        "first page, server defaults",
    )
    .await;
    support::bin(&server)
        .args(["pos", "list"])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn pos_list_every_filter_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-get-pos-transactions", "every filter").await;
    support::bin(&server)
        .args([
            "pos",
            "list",
            "--page-index",
            "1",
            "--page-size",
            "5",
            "--sort-by",
            "createdOn",
            "--desc",
            "--terminal-id",
            TERMINAL,
            "--from",
            "2026-01-01T00:00:00Z",
            "--to",
            "2026-12-31T23:59:59Z",
            "--status",
            "completed",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn pos_cancel_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-post-pos-transactions-posTransactionId-cancel",
        "default",
    )
    .await;
    support::bin(&server)
        .args(["pos", "cancel", POS_TXN, "--yes"])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn pos_print_receipt_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-post-pos-transactions-posTransactionId-print-receipt",
        "default",
    )
    .await;
    support::bin(&server)
        .args(["pos", "print-receipt", POS_TXN, "--terminal-id", TERMINAL])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

/// Destructive, so it is refused client-side with no request issued.
#[tokio::test]
async fn pos_cancel_without_yes_issues_no_request() {
    let server = support::mock_with_token().await;
    let out = support::bin(&server)
        .args(["pos", "cancel", POS_TXN])
        .assert()
        .code(3)
        .get_output()
        .clone();
    assert_eq!(
        String::from_utf8_lossy(&out.stderr).trim_end(),
        format!(
            "Error: cancellation requires --yes to confirm (e.g. `pos cancel {POS_TXN} --yes`)"
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

/// **A cancel that succeeds reports the transaction the API sent back**, not
/// the confirmation the other delete and revoke verbs print. The endpoint
/// declares a body, so a success carrying none is a decode failure rather
/// than an envelope invented from the request.
#[tokio::test]
async fn pos_cancel_reports_the_api_body_and_refuses_a_bodyless_success() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path(format!("/v2/pos/transactions/{POS_TXN}/cancel")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "posTransactionId": POS_TXN, "posTransactionStatus": "Cancelled"})))
        .mount(&server)
        .await;
    let out = support::bin(&server)
        .args(["--output", "json", "pos", "cancel", POS_TXN, "--yes"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["data"]["posTransactionStatus"], "Cancelled", "{v}");
    assert!(v["data"].get("cancelled").is_none(), "{v}");

    let bodyless = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path(format!("/v2/pos/transactions/{POS_TXN}/cancel")))
        .respond_with(ResponseTemplate::new(204))
        .mount(&bodyless)
        .await;
    let out = support::bin(&bodyless)
        .args(["--output", "json", "pos", "cancel", POS_TXN, "--yes"])
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["kind"], "decode", "{v}");
}

/// A cancelled transaction still exists, so the only 404 a cancel can meet is
/// an id the server never had — which is not found, not a cancellation.
#[tokio::test]
async fn pos_cancel_on_a_missing_transaction_exits_4() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path(format!("/v2/pos/transactions/{POS_TXN}/cancel")))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(serde_json::json!({"Title": "Not found", "CorrelationId": "c-1"})),
        )
        .mount(&server)
        .await;
    support::bin(&server)
        .args(["pos", "cancel", POS_TXN, "--yes"])
        .assert()
        .code(4)
        .stdout(predicate::str::contains("Cancelled").not());
}

/// **The poll's stop condition is the status, not a boolean.** The first poll
/// answers `InProgress` and the second `Completed`; a loop reading anything
/// else would either stop early or never stop.
#[tokio::test]
async fn pos_create_wait_polls_until_the_status_leaves_in_progress() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/pos/transactions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(in_progress()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v2/pos/transactions/{POS_TXN}")))
        .and(query_param("waitForTransactionProcessing", "true"))
        .respond_with(sequence(&[in_progress(), completed()]))
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args([
            "--output",
            "json",
            "pos",
            "create",
            "--terminal-id",
            TERMINAL,
            "--pos-device-id",
            DEVICE,
            "--amount",
            "42.75",
            "--currency-code",
            "USD",
            "--wait",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["object"], "pos_transaction");
    assert_eq!(v["data"]["posTransactionStatus"], "Completed");

    let polls = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.method.as_str().eq_ignore_ascii_case("GET"))
        .count();
    assert_eq!(polls, 2, "the loop stopped on the wrong condition");
}

/// An expired `--wait-timeout` prints the last-known envelope to stdout, a
/// warning to stderr, and exits 1 — not 3, which is where an untyped error
/// would land.
#[tokio::test]
async fn pos_create_wait_timeout_prints_the_last_status_and_exits_one() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/pos/transactions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(in_progress()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v2/pos/transactions/{POS_TXN}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(in_progress()))
        .mount(&server)
        .await;

    support::bin(&server)
        .args([
            "--output",
            "json",
            "pos",
            "create",
            "--terminal-id",
            TERMINAL,
            "--pos-device-id",
            DEVICE,
            "--amount",
            "42.75",
            "--currency-code",
            "USD",
            "--wait",
            "--wait-timeout",
            "0",
        ])
        .assert()
        .code(1)
        .stdout(predicate::str::contains(
            "\"posTransactionStatus\": \"InProgress\"",
        ))
        .stderr(predicate::str::contains("--wait-timeout"));
}

/// A timeout must not emit a second JSON document on stdout: the envelope is
/// already there, and the general error path would append an error envelope.
#[tokio::test]
async fn a_wait_timeout_writes_exactly_one_json_document_to_stdout() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/pos/transactions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(in_progress()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v2/pos/transactions/{POS_TXN}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(in_progress()))
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args([
            "--output",
            "json",
            "pos",
            "create",
            "--terminal-id",
            TERMINAL,
            "--pos-device-id",
            DEVICE,
            "--amount",
            "42.75",
            "--currency-code",
            "USD",
            "--wait",
            "--wait-timeout",
            "0",
        ])
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice::<serde_json::Value>(&out)
        .expect("stdout was not exactly one JSON document");
}

/// The API requires `waitForAcceptanceByTerminal` to be false on a deeplink
/// channel, so the combination is refused before a round trip.
#[tokio::test]
async fn pos_create_wait_with_a_deeplink_channel_is_refused() {
    let server = support::mock_with_token().await;
    support::bin(&server)
        .args([
            "pos",
            "create",
            "--terminal-id",
            TERMINAL,
            "--pos-device-id",
            DEVICE,
            "--amount",
            "42.75",
            "--currency-code",
            "USD",
            "--initiation-channel",
            "deeplink",
            "--wait",
        ])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("Deeplink"));
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

/// A create without `--wait` returns as soon as the POST answers, and polls
/// nothing.
#[tokio::test]
async fn pos_create_without_wait_polls_nothing() {
    let server = support::mock_with_token().await;
    support::mount(&server, "flute-v2-post-pos-transactions", "required only").await;
    support::bin(&server)
        .args([
            "pos",
            "create",
            "--terminal-id",
            TERMINAL,
            "--pos-device-id",
            DEVICE,
            "--amount",
            "42.75",
            "--currency-code",
            "USD",
        ])
        .assert()
        .success();
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| !r.method.as_str().eq_ignore_ascii_case("GET")),
        "a create without --wait must not poll"
    );
}

/// `waitForAcceptanceByTerminal` can settle the transaction outright, and a
/// poll for a status already in hand is a wasted round trip.
#[tokio::test]
async fn pos_create_wait_does_not_poll_when_the_create_is_already_final() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/pos/transactions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(completed()))
        .mount(&server)
        .await;
    support::bin(&server)
        .args([
            "pos",
            "create",
            "--terminal-id",
            TERMINAL,
            "--pos-device-id",
            DEVICE,
            "--amount",
            "42.75",
            "--currency-code",
            "USD",
            "--wait",
        ])
        .assert()
        .success();
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| !r.method.as_str().eq_ignore_ascii_case("GET")),
        "a create that already answered with a terminal status has nothing to poll for"
    );
}

/// A response with no status has no stop condition, so it is a malformed
/// response — exit 1 — rather than a state to spend the timeout on.
#[tokio::test]
async fn a_wait_on_a_response_carrying_no_status_is_a_decode_error() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/pos/transactions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"posTransactionId": POS_TXN})),
        )
        .mount(&server)
        .await;
    support::bin(&server)
        .args([
            "pos",
            "create",
            "--terminal-id",
            TERMINAL,
            "--pos-device-id",
            DEVICE,
            "--amount",
            "42.75",
            "--currency-code",
            "USD",
            "--wait",
        ])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("posTransactionStatus"));
}

#[tokio::test]
async fn pos_list_table_shows_the_status_and_the_processed_amount() {
    let server = support::mock_with_token().await;
    support::mount(
        &server,
        "flute-v2-get-pos-transactions",
        "first page, server defaults",
    )
    .await;
    support::bin(&server)
        .args(["--output", "table", "pos", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("TERMINAL ID"))
        .stdout(predicate::str::contains("STATUS"))
        .stdout(predicate::str::contains("Completed"))
        .stdout(predicate::str::contains("47.75"))
        .stdout(predicate::str::contains("2026-06-03"));
}

/// `quiet` prints the POS transaction id and nothing else, from the response
/// key the schema declares.
#[tokio::test]
async fn pos_create_quiet_prints_only_the_pos_transaction_id() {
    let server = support::mock_with_token().await;
    support::mount(&server, "flute-v2-post-pos-transactions", "required only").await;
    support::bin(&server)
        .args([
            "--output",
            "quiet",
            "pos",
            "create",
            "--terminal-id",
            TERMINAL,
            "--pos-device-id",
            DEVICE,
            "--amount",
            "42.75",
            "--currency-code",
            "USD",
        ])
        .assert()
        .success()
        .stdout(format!("{POS_TXN}\n"));
}

/// A bodyless success has no resource to print, so the confirmation is built
/// from the id the caller supplied.
#[tokio::test]
async fn pos_print_receipt_confirms_from_the_request() {
    let server = support::mock_with_token().await;
    support::mount(
        &server,
        "flute-v2-post-pos-transactions-posTransactionId-print-receipt",
        "default",
    )
    .await;
    support::bin(&server)
        .args([
            "--output",
            "json",
            "pos",
            "print-receipt",
            POS_TXN,
            "--terminal-id",
            TERMINAL,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"posTransactionId\""))
        .stdout(predicate::str::contains("\"sent\": true"))
        .stdout(predicate::str::contains("\"printed\"").not());
}

/// The one confirmation POS prints says what happened, in a sentence. The API
/// answers a print request with an empty 200 whether or not paper comes out,
/// so the sentence claims delivery to the terminal and no more. A cancel has
/// no confirmation: it answers with the transaction itself and prints that.
#[tokio::test]
async fn the_confirmation_lines_are_sentences() {
    let server = support::mock_with_token().await;
    support::mount(
        &server,
        "flute-v2-post-pos-transactions-posTransactionId-print-receipt",
        "default",
    )
    .await;

    support::bin(&server)
        .args(["pos", "print-receipt", POS_TXN, "--terminal-id", TERMINAL])
        .assert()
        .success()
        .stdout(format!(
            "Sent receipt for POS transaction {POS_TXN} to terminal {TERMINAL}.\n"
        ));
}

/// The timeout warning names the budget that ran out and points at what was
/// already printed, so an operator reading stderr alone knows where to look.
#[tokio::test]
async fn a_wait_timeout_warns_with_the_budget_it_spent() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/pos/transactions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(in_progress()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v2/pos/transactions/{POS_TXN}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(in_progress()))
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args([
            "--output",
            "quiet",
            "pos",
            "create",
            "--terminal-id",
            TERMINAL,
            "--pos-device-id",
            DEVICE,
            "--amount",
            "42.75",
            "--currency-code",
            "USD",
            "--wait",
            "--wait-timeout",
            "0",
        ])
        .assert()
        .code(1)
        .get_output()
        .clone();
    assert_eq!(
        String::from_utf8_lossy(&out.stderr).trim_end(),
        "Warning: --wait-timeout (0s) expired; last status shown above."
    );
}

/// **A poll that fails after the create succeeded still names what was
/// created.** The transaction exists and may be running on the terminal, so a
/// caller needs its id to reconcile or cancel: the last-known envelope goes to
/// stdout, the poll's failure to stderr, and the exit code is the failure's.
#[tokio::test]
async fn a_failed_poll_still_reports_the_created_transaction() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/pos/transactions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(in_progress()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v2/pos/transactions/{POS_TXN}")))
        .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
            "Title": "Internal Server Error", "CorrelationId": "c-500"})))
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args([
            "--output",
            "json",
            "pos",
            "create",
            "--terminal-id",
            TERMINAL,
            "--pos-device-id",
            DEVICE,
            "--amount",
            "42.75",
            "--currency-code",
            "USD",
            "--wait",
        ])
        .assert()
        .code(1)
        .get_output()
        .clone();
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("stdout was not exactly one JSON document");
    assert_eq!(v["object"], "pos_transaction", "{v}");
    assert_eq!(v["data"]["posTransactionId"], POS_TXN, "{v}");
    assert_eq!(v["data"]["posTransactionStatus"], "InProgress", "{v}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains(POS_TXN), "{stderr}");
    assert!(stderr.contains("Internal Server Error"), "{stderr}");
}

/// Three required strings the schema will not accept blank. A blank one is
/// refused here rather than spent on a 400, the way every other group's
/// required strings already are.
#[tokio::test]
async fn pos_create_refuses_a_blank_required_field_before_the_wire() {
    let server = support::mock_with_token().await;
    for (flag, blank) in [
        ("--terminal-id", ""),
        ("--pos-device-id", "  "),
        ("--currency-code", ""),
    ] {
        let mut args = vec![
            "pos",
            "create",
            "--terminal-id",
            TERMINAL,
            "--pos-device-id",
            DEVICE,
            "--amount",
            "42.75",
            "--currency-code",
            "USD",
            "--tip-rate",
            "0.18",
        ];
        let at = args.iter().position(|a| *a == flag).expect("the flag");
        args[at + 1] = blank;
        let out = support::bin(&server)
            .args(&args)
            .assert()
            .code(3)
            .get_output()
            .clone();
        // The rate note is advice about a request that is going to be sent;
        // a refused one must not be annotated before it is refused.
        assert!(
            !String::from_utf8_lossy(&out.stderr).contains("means"),
            "{flag} was refused but still drew a rate note"
        );
    }
    assert!(
        server.received_requests().await.unwrap().is_empty(),
        "a blank required field still reached the network"
    );
}

/// A budget for a poll nobody asked for is an invocation that means something
/// other than what it says, so it is a usage error rather than a flag that
/// quietly does nothing.
#[tokio::test]
async fn pos_create_refuses_a_wait_timeout_without_wait() {
    let server = support::mock_with_token().await;
    support::bin(&server)
        .args([
            "pos",
            "create",
            "--terminal-id",
            TERMINAL,
            "--pos-device-id",
            DEVICE,
            "--amount",
            "42.75",
            "--currency-code",
            "USD",
            "--wait-timeout",
            "5",
        ])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("--wait"));
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.url.path() == "/oauth2/token"),
        "a usage error must not reach the API"
    );
}

/// **A poll the server holds open past the budget is a timeout, not a
/// transport failure.** The wait is bounded by the deadline the caller named,
/// so the documented outcome — the last-known envelope on stdout, exit 1 —
/// survives a server that answers later than it was asked to wait.
#[tokio::test]
async fn pos_create_wait_times_out_while_a_poll_is_held_open() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/pos/transactions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(in_progress()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v2/pos/transactions/{POS_TXN}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(completed())
                .set_delay(std::time::Duration::from_secs(10)),
        )
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args([
            "--output",
            "json",
            "pos",
            "create",
            "--terminal-id",
            TERMINAL,
            "--pos-device-id",
            DEVICE,
            "--amount",
            "42.75",
            "--currency-code",
            "USD",
            "--wait",
            "--wait-timeout",
            "1",
        ])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("--wait-timeout"))
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["data"]["posTransactionStatus"], "InProgress", "{v}");
}

/// **Under `--wait` the create is bounded by the wait budget**, not by the
/// client-wide request timeout. `waitForAcceptanceByTerminal` holds the create
/// open until the terminal answers, so the budget the caller named is the
/// bound they asked for — plus the margin a poll gets. A create held past it
/// is a request timeout, reported as `transport`.
#[tokio::test]
async fn pos_create_wait_bounds_the_create_by_the_wait_budget() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/pos/transactions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(in_progress())
                .set_delay(std::time::Duration::from_secs(10)),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v2/pos/transactions/{POS_TXN}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(completed()))
        .mount(&server)
        .await;

    let started = std::time::Instant::now();
    let out = support::bin(&server)
        .args([
            "--output",
            "json",
            "pos",
            "create",
            "--terminal-id",
            TERMINAL,
            "--pos-device-id",
            DEVICE,
            "--amount",
            "42.75",
            "--currency-code",
            "USD",
            "--wait",
            "--wait-timeout",
            "0",
        ])
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["kind"], "transport", "{v}");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "the create waited for the server rather than for its budget"
    );
}

/// A budget of zero is a caller who will not wait at all, so the deadline is
/// reached during the first poll rather than after it.
#[tokio::test]
async fn pos_create_with_no_wait_budget_does_not_wait_for_a_poll() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/pos/transactions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(in_progress()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v2/pos/transactions/{POS_TXN}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(completed())
                .set_delay(std::time::Duration::from_secs(10)),
        )
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args([
            "--output",
            "json",
            "pos",
            "create",
            "--terminal-id",
            TERMINAL,
            "--pos-device-id",
            DEVICE,
            "--amount",
            "42.75",
            "--currency-code",
            "USD",
            "--wait",
            "--wait-timeout",
            "0",
        ])
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["data"]["posTransactionStatus"], "InProgress", "{v}");
}

/// **Ctrl-C during `--wait` exits 130 with stdout empty in every output
/// mode**, and the last-known status on stderr. The signal lands while the
/// poll is held open, which is after the handler is installed.
#[cfg(unix)]
#[tokio::test]
async fn pos_create_wait_interrupted_exits_130_with_stdout_empty() {
    for mode in ["json", "table", "quiet"] {
        let server = support::mock_with_token().await;
        Mock::given(method("POST"))
            .and(path("/v2/pos/transactions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(in_progress()))
            .mount(&server)
            .await;
        let poll = format!("/v2/pos/transactions/{POS_TXN}");
        Mock::given(method("GET"))
            .and(path(poll.as_str()))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(in_progress())
                    .set_delay(std::time::Duration::from_secs(60)),
            )
            .mount(&server)
            .await;

        let mut child = support::raw_bin_without_credentials()
            .env("FLUTE2_API_BASE_URL", server.uri())
            .env("FLUTE2_OAUTH_URL", format!("{}/oauth2/token", server.uri()))
            .env("FLUTE2_CLIENT_ID", "test-id")
            .env("FLUTE2_CLIENT_SECRET", "test-secret")
            .args([
                "--output",
                mode,
                "pos",
                "create",
                "--terminal-id",
                TERMINAL,
                "--pos-device-id",
                DEVICE,
                "--amount",
                "42.75",
                "--currency-code",
                "USD",
                "--wait",
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawning flute2");

        let held = tokio::time::timeout(std::time::Duration::from_secs(20), async {
            loop {
                let reqs = server.received_requests().await.unwrap();
                if reqs.iter().any(|r| r.url.path() == poll) {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await;
        if held.is_err() {
            child.kill().ok();
            panic!("{mode}: the poll never reached the server");
        }

        let sent = std::process::Command::new("kill")
            .args(["-INT", &child.id().to_string()])
            .status()
            .expect("running kill");
        assert!(sent.success(), "{mode}: kill -INT failed");

        let out = child.wait_with_output().expect("waiting for flute2");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(
            out.status.code(),
            Some(130),
            "{mode}: stderr was:\n{stderr}"
        );
        assert!(
            out.stdout.is_empty(),
            "{mode}: stdout was:\n{}",
            String::from_utf8_lossy(&out.stdout)
        );
        assert!(
            stderr.contains(&format!(
                "Interrupted. Last known status: InProgress (id: {POS_TXN})"
            )),
            "{mode}: stderr was:\n{stderr}"
        );
    }
}

/// **Ctrl-C while the create is held open exits 130 with stdout empty in
/// every output mode.** `waitForAcceptanceByTerminal` holds the create until
/// the terminal answers, and the transaction may already be live on it, so
/// stderr says to reconcile before creating another.
#[cfg(unix)]
#[tokio::test]
async fn pos_create_interrupted_during_the_create_exits_130_with_stdout_empty() {
    for mode in ["json", "table", "quiet"] {
        let server = support::mock_with_token().await;
        Mock::given(method("POST"))
            .and(path("/v2/pos/transactions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(in_progress())
                    .set_delay(std::time::Duration::from_secs(60)),
            )
            .mount(&server)
            .await;

        let mut child = support::raw_bin_without_credentials()
            .env("FLUTE2_API_BASE_URL", server.uri())
            .env("FLUTE2_OAUTH_URL", format!("{}/oauth2/token", server.uri()))
            .env("FLUTE2_CLIENT_ID", "test-id")
            .env("FLUTE2_CLIENT_SECRET", "test-secret")
            .args([
                "--output",
                mode,
                "pos",
                "create",
                "--terminal-id",
                TERMINAL,
                "--pos-device-id",
                DEVICE,
                "--amount",
                "42.75",
                "--currency-code",
                "USD",
                "--wait",
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawning flute2");

        let held = tokio::time::timeout(std::time::Duration::from_secs(20), async {
            loop {
                let reqs = server.received_requests().await.unwrap();
                if reqs.iter().any(|r| r.url.path() == "/v2/pos/transactions") {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await;
        if held.is_err() {
            child.kill().ok();
            panic!("{mode}: the create never reached the server");
        }

        let sent = std::process::Command::new("kill")
            .args(["-INT", &child.id().to_string()])
            .status()
            .expect("running kill");
        assert!(sent.success(), "{mode}: kill -INT failed");

        let out = child.wait_with_output().expect("waiting for flute2");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(
            out.status.code(),
            Some(130),
            "{mode}: stderr was:\n{stderr}"
        );
        assert!(
            out.stdout.is_empty(),
            "{mode}: stdout was:\n{}",
            String::from_utf8_lossy(&out.stdout)
        );
        assert!(
            stderr.contains(
                "Interrupted while creating the POS transaction. It may exist on the \
                 terminal: reconcile with `pos list` before creating another."
            ),
            "{mode}: stderr was:\n{stderr}"
        );
    }
}

/// **One `--wait-timeout` budget covers the create and the poll together.**
/// A create the terminal holds for most of the budget leaves the poll only
/// what remains, so the wait ends near the budget rather than near the create
/// time plus a whole second budget.
#[tokio::test]
async fn pos_create_wait_spends_one_budget_across_the_create_and_the_poll() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/pos/transactions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(in_progress())
                .set_delay(std::time::Duration::from_secs(3)),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v2/pos/transactions/{POS_TXN}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(completed())
                .set_delay(std::time::Duration::from_secs(30)),
        )
        .mount(&server)
        .await;

    let started = std::time::Instant::now();
    let out = support::bin(&server)
        .args([
            "--output",
            "json",
            "pos",
            "create",
            "--terminal-id",
            TERMINAL,
            "--pos-device-id",
            DEVICE,
            "--amount",
            "42.75",
            "--currency-code",
            "USD",
            "--wait",
            "--wait-timeout",
            "4",
        ])
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let elapsed = started.elapsed();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["data"]["posTransactionStatus"], "InProgress", "{v}");
    assert!(
        elapsed < std::time::Duration::from_secs(6),
        "a 4 s budget with a 3 s create took {elapsed:?}"
    );
}
