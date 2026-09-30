mod support;

use wiremock::matchers::{header, method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn ping_sends_bearer_to_v2_ping_and_reports_reachable() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path("/v2/ping"))
        .and(header("authorization", "Bearer tok-xyz"))
        .respond_with(
            ResponseTemplate::new(200).insert_header("x-arise-trace-correlationid", "corr-ping"),
        )
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args(["--output", "json", "ping"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["object"], "ping");
    assert_eq!(v["meta"]["correlation_id"], "corr-ping");
    assert_eq!(v["meta"]["environment"], "sandbox");
}

/// `GET /v2/ping` answers 200 with no body, so the envelope carries
/// reachability rather than a payload — and no `page_info`.
#[tokio::test]
async fn ping_reports_reachable_without_a_response_body() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path("/v2/ping"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args(["--output", "json", "ping"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["data"]["reachable"], true);
    assert!(v["meta"].get("page_info").is_none());
    assert!(v["meta"].get("correlation_id").is_none());
}

/// A resource command still fails cleanly with no credentials, before any
/// request is built.
#[test]
fn ping_without_credentials_exits_2() {
    support::bin_without_credentials()
        .args(["ping"])
        .assert()
        .code(2);
}

#[tokio::test]
async fn ping_in_quiet_mode_prints_ok() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path("/v2/ping"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    support::bin(&server)
        .args(["--output", "quiet", "ping"])
        .assert()
        .success()
        .stdout(predicates::str::contains("ok"));
}

/// The mock is driven by the contract fixture, so the bytes asserted here and
/// the bytes validated by conformance are one value.
#[tokio::test]
async fn ping_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-get-ping", "default").await;
    support::bin(&server).args(["ping"]).assert().success();
    support::assert_exchange_observed(&server, &ex).await;
}

/// A redirect is an instruction to send credentials somewhere the caller never
/// named, so the client does not follow one: the 307 is reported instead.
#[tokio::test]
async fn a_redirect_is_reported_rather_than_followed() {
    let elsewhere = wiremock::MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/ping"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&elsewhere)
        .await;

    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path("/v2/ping"))
        .respond_with(
            ResponseTemplate::new(307)
                .insert_header("location", format!("{}/v2/ping", elsewhere.uri()).as_str()),
        )
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args(["--output", "json", "ping"])
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert!(
        v["kind"] == "api" || v["kind"] == "transport",
        "a redirect should surface as a failure: {v}"
    );
    assert_eq!(
        elsewhere.received_requests().await.unwrap().len(),
        0,
        "the client followed a redirect off the configured host"
    );
}
