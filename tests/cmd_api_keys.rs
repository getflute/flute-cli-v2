mod support;

use predicates::prelude::*;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

const CLIENT: &str = "aa98aa89-07ae-4a23-b441-48725f0386e6";
const MERCHANT: &str = "8db2ff47-b143-4adb-ab58-a11111111111";

#[tokio::test]
async fn api_key_create_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-post-api-keys", "default").await;
    support::bin(&server)
        .args([
            "api-keys",
            "create",
            "--merchant-id",
            MERCHANT,
            "--name",
            "Production API Key",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn api_key_list_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-get-api-keys", "every key").await;
    support::bin(&server)
        .args(["api-keys", "list"])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn api_key_list_for_one_merchant_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-get-api-keys", "for one merchant").await;
    support::bin(&server)
        .args(["api-keys", "list", "--merchant-id", MERCHANT])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn api_key_revoke_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-delete-api-keys-clientId", "default").await;
    support::bin(&server)
        .args(["api-keys", "revoke", "--client-id", CLIENT, "--yes"])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

/// **`api-keys list` is not paginated.** `GetApiKeysResponseDto` declares one
/// property and no `pageInfo`, so the shared `PaginationArgs` must not be
/// wired in by reflex — each flag is rejected as unknown.
#[tokio::test]
async fn the_unpaginated_list_rejects_every_pagination_flag() {
    let server = support::mock_with_token().await;
    for flag in [
        vec!["--page-index", "0"],
        vec!["--page-size", "20"],
        vec!["--all"],
    ] {
        let mut args = vec!["api-keys", "list"];
        args.extend(flag.iter().copied());
        support::bin(&server)
            .args(&args)
            .assert()
            .code(3)
            .stderr(predicate::str::contains("unexpected argument"));
    }
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.url.path() == "/oauth2/token"),
        "a rejected flag must not reach the API"
    );
}

/// And no `page_info` in the envelope either: there is no `pageInfo` to
/// reproduce.
#[tokio::test]
async fn the_unpaginated_list_reports_no_page_info() {
    let server = support::mock_with_token().await;
    support::mount(&server, "flute-v2-get-api-keys", "every key").await;
    let out = support::bin(&server)
        .args(["--output", "json", "api-keys", "list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["object"], "api_token_list");
    // The collection is the `apiKeys` array, not a transport wrapper.
    assert_eq!(v["data"].as_array().unwrap().len(), 1);
    assert_eq!(v["data"][0]["clientId"], CLIENT);
    assert!(v["meta"].get("page_info").is_none(), "{v}");
}

/// Destructive, so it is refused client-side with no request issued.
#[tokio::test]
async fn api_key_revoke_without_yes_issues_no_request() {
    let server = support::mock_with_token().await;
    let out = support::bin(&server)
        .args(["api-keys", "revoke", "--client-id", CLIENT])
        .assert()
        .code(3)
        .get_output()
        .clone();
    assert_eq!(
        String::from_utf8_lossy(&out.stderr).trim_end(),
        format!(
            "Error: revocation requires --yes to confirm \
             (e.g. `api-keys revoke --client-id {CLIENT} --yes`)"
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

/// The client id is a flag, so a bare id is a usage error rather than a key
/// revoked by accident.
#[tokio::test]
async fn api_key_revoke_refuses_a_positional_client_id() {
    let server = support::mock_with_token().await;
    support::bin(&server)
        .args(["api-keys", "revoke", CLIENT, "--yes"])
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

/// **The collision, resolved.** This group answers 404 when its feature flag
/// is off, and a delete answers 404 when the thing is already gone. `revoke`
/// takes the second reading: exit 0, no feature-flag advice.
#[tokio::test]
async fn a_404_on_revoke_is_success_and_says_nothing_about_the_feature_flag() {
    let server = support::mock_with_token().await;
    Mock::given(method("DELETE"))
        .and(path(format!("/v2/api-keys/{CLIENT}")))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(serde_json::json!({"Title": "Not found", "CorrelationId": "c-1"})),
        )
        .mount(&server)
        .await;
    let out = support::bin(&server)
        .args([
            "--output",
            "json",
            "api-keys",
            "revoke",
            "--client-id",
            CLIENT,
            "--yes",
        ])
        .assert()
        .stderr(predicate::str::contains("feature").not())
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["data"]["clientId"], CLIENT);
    assert_eq!(v["data"]["revoked"], false, "{v}");
    assert_eq!(v["data"]["found"], false, "{v}");
}

/// The other side of the collision: a 404 from a read is the feature flag, not
/// a missing key, because there is no id in the request to be missing.
#[tokio::test]
async fn a_404_on_list_explains_the_feature_flag() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path("/v2/api-keys"))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(serde_json::json!({"Title": "Not found", "CorrelationId": "c-1"})),
        )
        .mount(&server)
        .await;
    support::bin(&server)
        .args(["api-keys", "list"])
        .assert()
        .code(4)
        .stderr(predicate::str::contains("feature"))
        .stderr(predicate::str::contains("api-keys"));
}

#[tokio::test]
async fn a_404_on_create_explains_the_feature_flag() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/api-keys"))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(serde_json::json!({"Title": "Not found", "CorrelationId": "c-1"})),
        )
        .mount(&server)
        .await;
    support::bin(&server)
        .args([
            "api-keys",
            "create",
            "--merchant-id",
            MERCHANT,
            "--name",
            "Production API Key",
        ])
        .assert()
        .code(4)
        .stderr(predicate::str::contains("feature"));
}

/// A 404 is the only status that gets the feature-flag reading. A 403 is an
/// authorisation failure and keeps its own message and its own exit code.
#[tokio::test]
async fn a_403_on_list_is_not_reported_as_a_feature_flag() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path("/v2/api-keys"))
        .respond_with(
            ResponseTemplate::new(403)
                .set_body_json(serde_json::json!({"Title": "Forbidden", "CorrelationId": "c-1"})),
        )
        .mount(&server)
        .await;
    support::bin(&server)
        .args(["api-keys", "list"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("feature").not());
}

/// Both fields are required by schema, so clap enforces them and nothing is
/// sent without either.
#[tokio::test]
async fn create_without_both_required_fields_is_a_usage_error() {
    let server = support::mock_with_token().await;
    for args in [
        vec!["api-keys", "create", "--merchant-id", MERCHANT],
        vec!["api-keys", "create", "--name", "Production API Key"],
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
    );
}

/// The secret is returned by this one operation and never again, so the table
/// says so under the two rows that carry it.
#[tokio::test]
async fn create_table_warns_that_the_secret_is_shown_once() {
    let server = support::mock_with_token().await;
    support::mount(&server, "flute-v2-post-api-keys", "default").await;
    let out = support::bin(&server)
        .args([
            "--output",
            "table",
            "api-keys",
            "create",
            "--merchant-id",
            MERCHANT,
            "--name",
            "Production API Key",
        ])
        .output()
        .unwrap();
    let table = String::from_utf8(out.stdout).unwrap();

    assert!(table.starts_with("clientId:"), "{table}");
    assert!(
        table.ends_with("\n\nNOTE: The clientSecret is shown ONLY ONCE. Store it securely.\n"),
        "{table}"
    );
    assert_eq!(table.lines().count(), 4, "{table}");
}

/// v1's envelope names are kept: `api_token` for one and `api_token_list` for
/// the collection, even though the group is now called `api-keys`. Renaming
/// them would be a change to the output contract the design does not
/// enumerate, and an agent branching on `object` would see it.
#[tokio::test]
async fn the_envelope_keeps_v1s_object_names() {
    let server = support::mock_with_token().await;
    support::mount(&server, "flute-v2-post-api-keys", "default").await;
    support::bin(&server)
        .args([
            "--output",
            "json",
            "api-keys",
            "create",
            "--merchant-id",
            MERCHANT,
            "--name",
            "Production API Key",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"object\": \"api_token\""));
}

#[tokio::test]
async fn api_key_list_table_shows_the_name_and_both_ids() {
    let server = support::mock_with_token().await;
    support::mount(&server, "flute-v2-get-api-keys", "every key").await;
    let out = support::bin(&server)
        .args(["--output", "table", "api-keys", "list"])
        .output()
        .unwrap();
    let table = String::from_utf8(out.stdout).unwrap();
    let header = table.lines().next().unwrap();

    assert!(header.starts_with("CLIENT ID"), "{table}");
    assert!(header.contains("NAME"), "{table}");
    // The merchant column is headed as v1 headed it.
    assert!(
        header.contains("MERCHANT") && !header.contains("MERCHANT ID"),
        "{table}"
    );
    // v1 carried a CREATED column; the v2 list item declares no timestamp.
    assert!(!header.contains("CREATED"), "{table}");
    assert!(table.contains(CLIENT), "{table}");
    assert!(table.contains("Production API Key"), "{table}");
}

/// `quiet` prints the client id, which is the only half of a new key that can
/// be looked up again.
#[tokio::test]
async fn create_quiet_prints_the_client_id_and_not_the_secret() {
    let server = support::mock_with_token().await;
    support::mount(&server, "flute-v2-post-api-keys", "default").await;
    support::bin(&server)
        .args([
            "--output",
            "quiet",
            "api-keys",
            "create",
            "--merchant-id",
            MERCHANT,
            "--name",
            "Production API Key",
        ])
        .assert()
        .success()
        .stdout(format!("{CLIENT}\n"));
}

/// A bodyless success has no resource to print, so the revoke confirms from
/// the id the caller supplied.
#[tokio::test]
async fn revoke_confirms_from_the_request() {
    let server = support::mock_with_token().await;
    support::mount(&server, "flute-v2-delete-api-keys-clientId", "default").await;
    support::bin(&server)
        .args([
            "--output",
            "json",
            "api-keys",
            "revoke",
            "--client-id",
            CLIENT,
            "--yes",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"clientId\""))
        .stdout(predicate::str::contains("\"revoked\": true"));
}

/// A revoke answers with no body, so it says what it did in a sentence.
#[tokio::test]
async fn the_confirmation_line_is_a_sentence() {
    let server = support::mock_with_token().await;
    support::mount(&server, "flute-v2-delete-api-keys-clientId", "default").await;
    support::bin(&server)
        .args(["api-keys", "revoke", "--client-id", CLIENT, "--yes"])
        .assert()
        .success()
        .stdout(format!("Revoked key {CLIENT}.\n"));
}

/// A collection field of the wrong type is a shape this CLI cannot read.
/// Reporting it as an account holding no keys would answer "none" to a
/// question nobody managed to ask.
#[tokio::test]
async fn a_wrong_typed_collection_field_is_a_decode_error() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path("/v2/api-keys"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"apiKeys": "wrong type"})),
        )
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args(["--output", "json", "api-keys", "list"])
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["kind"], "decode", "{v}");
}
