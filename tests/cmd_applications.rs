mod support;

use predicates::prelude::*;
use std::io::Write;

const PARTNER_USER: &str = "3b9f2c4e-7a1d-4e6b-9c8f-2d5e1a7b3c90";
const SCHEDULE_A: &str = "5c1e8f2a-9b3d-4a7e-8f6c-1d2b3e4f5a60";
const SPLIT_PROFILE: &str = "a7d3e9b1-4c2f-4e8a-9b6d-0f1e2d3c4b5a";

/// The seven required flags, with `--delivery-method` as given.
fn required(delivery_method: &'static str) -> Vec<&'static str> {
    vec![
        "applications",
        "create",
        "--email",
        "marcus@example.com",
        "--delivery-method",
        delivery_method,
        "--partner-user-id",
        PARTNER_USER,
        "--processor-id",
        "7",
        "--schedule-a-id",
        SCHEDULE_A,
        "--split-profile-id",
        SPLIT_PROFILE,
        "--dba-name",
        "Corner Coffee",
    ]
}

/// The rest of an application, as a partner would hold it. The file's own
/// `deliveryMethod`, `email` and `dbaName` lose to the flags.
const REST_OF_THE_APPLICATION: &str = r#"{
    "deliveryMethod": "Email",
    "email": "someone-else@example.com",
    "pricingTemplateIds": ["0d6e2f1a-3b4c-4d5e-8f7a-9b0c1d2e3f40"],
    "metadata": {"locationId": "loc_00441"},
    "businessInformation": {
        "dbaName": "Old Name",
        "legalName": "Corner Coffee LLC",
        "taxId": "123456789",
        "bankInfo": {"routing": "021000021", "account": "000123456789"}},
    "owner1": {"firstName": "Marcus", "lastName": "Reed", "ssn": "123456789", "ownership": 100}
}"#;

fn file_with(contents: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::NamedTempFile::new().unwrap();
    f.write_all(contents.as_bytes()).unwrap();
    f
}

#[tokio::test]
async fn application_create_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-post-applications", "required only").await;
    support::bin(&server)
        .args(required("partner-direct"))
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

/// The file supplies the rest of the application and every flag wins over
/// the file's own value for the same field.
#[tokio::test]
async fn application_create_from_a_file_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-post-applications", "from a file").await;
    let file = file_with(REST_OF_THE_APPLICATION);
    support::bin(&server)
        .args(required("iframe"))
        .args(["--file", file.path().to_str().unwrap()])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

/// `-` reads the file from stdin, so an application never has to touch disk.
#[tokio::test]
async fn application_create_reads_the_file_from_stdin() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-post-applications", "from a file").await;
    support::bin(&server)
        .args(required("iframe"))
        .args(["--file", "-"])
        .write_stdin(REST_OF_THE_APPLICATION)
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

/// Quiet prints the new application's id, from `applicationId`.
#[tokio::test]
async fn application_create_quiet_prints_the_application_id() {
    let server = support::mock_with_token().await;
    support::mount(&server, "flute-v2-post-applications", "required only").await;
    support::bin(&server)
        .args(["--output", "quiet"])
        .args(required("partner-direct"))
        .assert()
        .success()
        .stdout("8e3b1c5a-2f4d-4a6b-9c7e-1d2f3a4b5c6d\n");
}

/// Anything but a JSON object is refused before the wire: the file is the
/// body, and only an object can be one.
#[tokio::test]
async fn a_file_that_is_not_one_json_object_is_refused_before_the_wire() {
    for (contents, says) in [
        ("[]", "JSON object"),
        ("\"text\"", "JSON object"),
        ("{not json", "--file"),
        (
            r#"{"businessInformation": "Corner Coffee"}"#,
            "businessInformation",
        ),
    ] {
        let server = support::mock_with_token().await;
        let file = file_with(contents);
        let out = support::bin(&server)
            .args(["--output", "json"])
            .args(required("email"))
            .args(["--file", file.path().to_str().unwrap()])
            .assert()
            .code(3)
            .get_output()
            .stdout
            .clone();
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["kind"], "client", "{contents}: {v}");
        assert!(
            v["message"].as_str().unwrap().contains(says),
            "{contents}: {v}"
        );
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "{contents}: a refused file still reached the network"
        );
    }
}

#[tokio::test]
async fn a_missing_file_is_refused_and_names_the_path() {
    let server = support::mock_with_token().await;
    support::bin(&server)
        .args(required("email"))
        .args(["--file", "/nonexistent/application.json"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("/nonexistent/application.json"));
    assert!(server.received_requests().await.unwrap().is_empty());
}

/// A required value that went missing, such as an unset shell variable, is
/// refused rather than sent as an empty string.
#[tokio::test]
async fn an_empty_required_value_is_refused_before_the_wire() {
    for flag in [
        "--email",
        "--partner-user-id",
        "--schedule-a-id",
        "--split-profile-id",
        "--dba-name",
    ] {
        let server = support::mock_with_token().await;
        let mut args = required("email");
        let at = args.iter().position(|a| *a == flag).unwrap();
        args[at + 1] = " ";
        support::bin(&server)
            .args(&args)
            .assert()
            .code(3)
            .stderr(predicate::str::contains(flag));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "{flag}"
        );
    }
}

#[test]
fn every_required_flag_is_required() {
    support::bin_without_credentials()
        .args(["applications", "create", "--email", "a@example.com"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("--delivery-method"));
}

#[tokio::test]
async fn application_list_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-get-applications",
        "first page, server defaults",
    )
    .await;
    support::bin(&server)
        .args(["applications", "list"])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn application_list_with_every_filter_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-get-applications", "every filter").await;
    support::bin(&server)
        .args([
            "applications",
            "list",
            "--page-index",
            "1",
            "--page-size",
            "5",
            "--status",
            "information-needed",
            "--application-id",
            "8e3b1c5a-2f4d-4a6b-9c7e-1d2f3a4b5c6d",
            "--created-from",
            "2026-06-01",
            "--created-to",
            "2026-06-30",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

/// The list's envelope carries the API's `pageInfo`, and quiet prints each
/// application's `id`.
#[tokio::test]
async fn application_list_reports_page_info_and_quiet_prints_ids() {
    let server = support::mock_with_token().await;
    support::mount(
        &server,
        "flute-v2-get-applications",
        "first page, server defaults",
    )
    .await;
    let out = support::bin(&server)
        .args(["--output", "json", "applications", "list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["object"], "application_list");
    assert_eq!(v["meta"]["page_info"]["totalItems"], 1);
    support::bin(&server)
        .args(["--output", "quiet", "applications", "list"])
        .assert()
        .success()
        .stdout("8e3b1c5a-2f4d-4a6b-9c7e-1d2f3a4b5c6d\n");
}
