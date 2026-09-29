mod support;

use predicates::prelude::*;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

const BATCH: &str = "21c75430-a316-456f-9126-365760dca33a";
const OTHER_BATCH: &str = "0f19d58c-3d4e-4f5a-6b7c-8d9e0f1a2b58";
const PP: &str = "8db2ff47-b143-4adb-ab58-a11111111111";

#[tokio::test]
async fn settlement_list_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-get-settlements-batches",
        "first page, server defaults",
    )
    .await;
    support::bin(&server)
        .args(["settlements", "list"])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

/// The API's only two array parameters, sent as repeated pairs, alongside the
/// other seven.
#[tokio::test]
async fn settlement_list_every_filter_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-get-settlements-batches", "every filter").await;
    support::bin(&server)
        .args([
            "settlements",
            "list",
            "--page-index",
            "1",
            "--page-size",
            "5",
            "--sort-by",
            "createdOn",
            "--asc",
            "--from",
            "2026-01-01T00:00:00Z",
            "--to",
            "2026-12-31T23:59:59Z",
            "--batch-ids",
            BATCH,
            "--batch-ids",
            OTHER_BATCH,
            "--processor-ids",
            PP,
            "--status",
            "open",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

/// `settlements get` has no endpoint: it is the list with the documented
/// `batchIds` filter applied **server-side**, and the exact-query assertion is
/// what proves it does not fetch a page and scan it.
#[tokio::test]
async fn settlement_get_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-get-settlements-batches",
        "one batch by id",
    )
    .await;
    support::bin(&server)
        .args(["settlements", "get", BATCH])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn settlement_close_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-post-settlements-batches-close",
        "default",
    )
    .await;
    support::bin(&server)
        .args(["settlements", "close", "--payment-processor-id", PP])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

/// An empty filter result is a not-found read, exit 4. Only the absent case
/// can establish that, because a filter that matches nothing is a 200.
#[tokio::test]
async fn settlement_get_of_an_unknown_batch_exits_four() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path("/v2/settlements/batches"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [],
            "pageInfo": {"hasMore": false}})))
        .mount(&server)
        .await;
    support::bin(&server)
        .args(["settlements", "get", BATCH])
        .assert()
        .code(4)
        .stderr(predicate::str::contains(BATCH));
}

/// The not-found envelope keeps `kind: "api"` and status 404, and its message
/// says the filtered list came back empty.
#[tokio::test]
async fn settlement_get_of_an_unknown_batch_says_the_filtered_list_was_empty() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path("/v2/settlements/batches"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [],
            "pageInfo": {"hasMore": false}})))
        .mount(&server)
        .await;
    let out = support::bin(&server)
        .args(["--output", "json", "settlements", "get", BATCH])
        .assert()
        .code(4)
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["kind"], "api", "{v}");
    assert_eq!(v["status"], 404, "{v}");
    assert_eq!(
        v["message"],
        format!(
            "no settlement batch {BATCH}: the settlements list filtered by that \
             batch id returned no batch"
        )
    );
}

/// More than one match for a single batch id would mean the filter is not the
/// identity it is being used as, so the command says so rather than picking
/// the first.
#[tokio::test]
async fn settlement_get_refuses_to_choose_between_two_matches() {
    let server = support::mock_with_token().await;
    let batch = serde_json::json!({"batchId": BATCH, "batchStatus": "Open"});
    Mock::given(method("GET"))
        .and(path("/v2/settlements/batches"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [batch, serde_json::json!({"batchId": OTHER_BATCH})],
            "pageInfo": {"hasMore": false}})))
        .mount(&server)
        .await;
    support::bin(&server)
        .args(["settlements", "get", BATCH])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("2"));
}

/// clap enforces the one required field, so nothing is sent.
#[tokio::test]
async fn settlement_close_without_a_processor_is_a_usage_error() {
    let server = support::mock_with_token().await;
    support::bin(&server)
        .args(["settlements", "close"])
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

/// The table shows the id, processor, date, count and three amounts, all of
/// which the list item carries.
#[tokio::test]
async fn settlement_list_table_shows_the_processor_the_counts_and_the_amounts() {
    let server = support::mock_with_token().await;
    support::mount(
        &server,
        "flute-v2-get-settlements-batches",
        "first page, server defaults",
    )
    .await;
    support::bin(&server)
        .args(["--output", "table", "settlements", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("PROCESSOR"))
        .stdout(predicate::str::contains("BATCH DATE"))
        .stdout(predicate::str::contains("TSYS"))
        .stdout(predicate::str::contains("15"))
        .stdout(predicate::str::contains("1250.00"))
        .stdout(predicate::str::contains("Settled"))
        .stdout(predicate::str::contains("2026-03-23"));
}

/// The close response carries the resulting status and no identifier at all,
/// so that status is what `quiet` prints — there is nothing else in it.
#[tokio::test]
async fn settlement_close_reports_the_batch_status_in_every_mode() {
    let server = support::mock_with_token().await;
    support::mount(
        &server,
        "flute-v2-post-settlements-batches-close",
        "default",
    )
    .await;
    support::bin(&server)
        .args([
            "--output",
            "quiet",
            "settlements",
            "close",
            "--payment-processor-id",
            PP,
        ])
        .assert()
        .success()
        .stdout("PendingSettlement\n");

    support::bin(&server)
        .args([
            "--output",
            "json",
            "settlements",
            "close",
            "--payment-processor-id",
            PP,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"object\": \"batch_closure\""))
        .stdout(predicate::str::contains(
            "\"batchStatus\": \"PendingSettlement\"",
        ));
}

/// `settlements get` renders one batch, not a collection, so it carries the
/// singular envelope name.
#[tokio::test]
async fn settlement_get_carries_the_singular_envelope_name() {
    let server = support::mock_with_token().await;
    support::mount(
        &server,
        "flute-v2-get-settlements-batches",
        "one batch by id",
    )
    .await;
    let out = support::bin(&server)
        .args(["--output", "json", "settlements", "get", BATCH])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["object"], "settlement");
    assert_eq!(v["data"]["batchId"], BATCH);
    // One resource, so there is no page to describe.
    assert!(v["meta"].get("page_info").is_none(), "{v}");
}
