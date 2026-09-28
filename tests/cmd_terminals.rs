mod support;

use predicates::prelude::*;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

const TERMINAL: &str = "b1d4c0a7-3e18-4a6f-8c25-9f7e6d5b4a31";

#[tokio::test]
async fn terminal_list_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-get-terminals",
        "first page, server defaults",
    )
    .await;
    support::bin(&server)
        .args(["terminals", "list"])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

/// All nine declared parameters, including the `terminalStatus` whose query
/// enum is not the response enum of the same name.
#[tokio::test]
async fn terminal_list_every_filter_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-get-terminals", "every filter").await;
    support::bin(&server)
        .args([
            "terminals",
            "list",
            "--page-index",
            "1",
            "--page-size",
            "5",
            "--sort-by",
            "terminalManufacturer",
            "--desc",
            "--status",
            "ready",
            "--mode",
            "semi-integrated",
            "--connection",
            "online",
            "--serial-number",
            "SN100001",
            "--search",
            "front counter",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn terminal_status_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-get-terminals-terminalId-status",
        "default",
    )
    .await;
    support::bin(&server)
        .args(["terminals", "status", TERMINAL])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

/// The two operations answer different resources, so they carry different
/// envelope names — `terminal_list` and `terminal_status`, as in v1.
#[tokio::test]
async fn the_two_terminal_reads_carry_their_own_envelope_names() {
    let server = support::mock_with_token().await;
    support::mount(
        &server,
        "flute-v2-get-terminals",
        "first page, server defaults",
    )
    .await;
    support::mount(
        &server,
        "flute-v2-get-terminals-terminalId-status",
        "default",
    )
    .await;

    let listed = support::bin(&server)
        .args(["--output", "json", "terminals", "list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let listed: serde_json::Value = serde_json::from_slice(&listed).unwrap();
    assert_eq!(listed["object"], "terminal_list");
    // The collection's own `pageInfo` reaches meta, field for field.
    assert_eq!(listed["meta"]["page_info"]["hasMore"], false);

    let status = support::bin(&server)
        .args(["--output", "json", "terminals", "status", TERMINAL])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let status: serde_json::Value = serde_json::from_slice(&status).unwrap();
    assert_eq!(status["object"], "terminal_status");
    assert_eq!(status["data"]["terminalStatus"], "Active");
}

/// The serial number and the two live states are what a caller picking a
/// terminal needs.
#[tokio::test]
async fn terminal_list_table_shows_the_serial_the_mode_and_both_states() {
    let server = support::mock_with_token().await;
    support::mount(
        &server,
        "flute-v2-get-terminals",
        "first page, server defaults",
    )
    .await;
    support::bin(&server)
        .args(["--output", "table", "terminals", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("SERIAL"))
        .stdout(predicate::str::contains("SN100001"))
        .stdout(predicate::str::contains("MODE"))
        .stdout(predicate::str::contains("SemiIntegrated"))
        .stdout(predicate::str::contains("CONNECTION"))
        .stdout(predicate::str::contains("Online"))
        .stdout(predicate::str::contains("LAST SEEN"))
        .stdout(predicate::str::contains("2026-08-11T18:44:45.638Z"));
}

/// The status table is a detail view, so every field the API sent is present
/// — including the ones a caller diagnoses a dead terminal with.
#[tokio::test]
async fn terminal_status_table_shows_the_diagnostic_fields() {
    let server = support::mock_with_token().await;
    support::mount(
        &server,
        "flute-v2-get-terminals-terminalId-status",
        "default",
    )
    .await;
    support::bin(&server)
        .args(["--output", "table", "terminals", "status", TERMINAL])
        .assert()
        .success()
        .stdout(predicate::str::contains("batteryLevel"))
        .stdout(predicate::str::contains("90"))
        .stdout(predicate::str::contains("debitPinKey"))
        .stdout(predicate::str::contains("printerStatus"))
        .stdout(predicate::str::contains("terminalAppVersion"));
}

/// `quiet` prints the terminal ids, one per line, on both reads.
#[tokio::test]
async fn terminal_reads_print_only_ids_in_quiet_mode() {
    let server = support::mock_with_token().await;
    support::mount(
        &server,
        "flute-v2-get-terminals",
        "first page, server defaults",
    )
    .await;
    support::mount(
        &server,
        "flute-v2-get-terminals-terminalId-status",
        "default",
    )
    .await;
    support::bin(&server)
        .args(["--output", "quiet", "terminals", "list"])
        .assert()
        .success()
        .stdout(format!("{TERMINAL}\n"));
    support::bin(&server)
        .args(["--output", "quiet", "terminals", "status", TERMINAL])
        .assert()
        .success()
        .stdout(format!("{TERMINAL}\n"));
}

/// The shared pagination bound, on a group that wires it in rather than
/// declaring it.
#[tokio::test]
async fn terminal_list_refuses_an_out_of_range_page_size_without_calling_the_api() {
    let server = support::mock_with_token().await;
    support::bin(&server)
        .args(["terminals", "list", "--page-size", "500"])
        .assert()
        .code(3);
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

/// `--all` walks the pages here too, and reports no `page_info`: the data
/// spans every page, so no single `pageInfo` describes it.
#[tokio::test]
async fn terminal_list_all_walks_every_page_and_reports_no_page_info() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path("/v2/terminals"))
        .and(wiremock::matchers::query_param("pageIndex", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [{"terminalId": "a"}],
            "pageInfo": {"hasMore": true}})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/terminals"))
        .and(wiremock::matchers::query_param("pageIndex", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [{"terminalId": "b"}],
            "pageInfo": {"hasMore": false}})))
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args(["--output", "json", "terminals", "list", "--all"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["data"].as_array().unwrap().len(), 2);
    assert!(v["meta"].get("page_info").is_none(), "{v}");
}

/// A 404 from a read is exit 4: the idempotent-404 rule is confined to
/// delete, revoke and cancel, and `terminals` has none of them.
#[tokio::test]
async fn terminal_status_of_an_unknown_terminal_exits_four() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path(format!("/v2/terminals/{TERMINAL}/status")))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(serde_json::json!({"Title": "Not found", "CorrelationId": "c-1"})),
        )
        .mount(&server)
        .await;
    support::bin(&server)
        .args(["terminals", "status", TERMINAL])
        .assert()
        .code(4);
}
