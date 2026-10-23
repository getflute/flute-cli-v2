mod support;

use predicates::prelude::*;

const USER: &str = "3b9f2c4e-7a1d-4e6b-9c8f-2d5e1a7b3c90";

#[tokio::test]
async fn user_list_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-get-users", "first page, server defaults").await;
    support::bin(&server)
        .args(["users", "list"])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn user_list_with_every_filter_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-get-users", "every filter").await;
    support::bin(&server)
        .args([
            "users",
            "list",
            "--page-index",
            "1",
            "--page-size",
            "5",
            "--user-type",
            "partner",
            "--email",
            "ada@example.com",
        ])
        .assert()
        .success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn user_schedule_as_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-get-users-userId-schedule-as", "default").await;
    let out = support::bin(&server)
        .args(["--output", "json", "users", "schedule-as", USER])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    support::assert_exchange_observed(&server, &ex).await;
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["object"], "schedule_a_list");
    assert!(v["meta"].get("page_info").is_none(), "{v}");
}

#[tokio::test]
async fn user_split_profiles_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server,
        "flute-v2-get-users-userId-split-profiles",
        "default",
    )
    .await;
    let out = support::bin(&server)
        .args(["--output", "json", "users", "split-profiles", USER])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    support::assert_exchange_observed(&server, &ex).await;
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["object"], "split_profile_list");
    assert!(v["meta"].get("page_info").is_none(), "{v}");
}

/// The two per-user lists are not paginated: each answers every item at once
/// and declares no `pageInfo`, so the pagination flags are not offered.
#[test]
fn the_per_user_lists_reject_every_pagination_flag() {
    for verb in ["schedule-as", "split-profiles"] {
        for flag in [
            vec!["--page-index", "0"],
            vec!["--page-size", "20"],
            vec!["--all"],
        ] {
            let mut args = vec!["users", verb, USER];
            args.extend(flag.iter().copied());
            support::bin_without_credentials()
                .args(&args)
                .assert()
                .code(3)
                .stderr(predicate::str::contains("unexpected argument"));
        }
    }
}

#[tokio::test]
async fn pricing_template_list_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-get-pricing-templates", "default").await;
    let out = support::bin(&server)
        .args(["--output", "json", "pricing-templates", "list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    support::assert_exchange_observed(&server, &ex).await;
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["object"], "pricing_template_list");
    assert!(v["meta"].get("page_info").is_none(), "{v}");
}

#[test]
fn the_pricing_template_list_rejects_every_pagination_flag() {
    for flag in [
        vec!["--page-index", "0"],
        vec!["--page-size", "20"],
        vec!["--all"],
    ] {
        let mut args = vec!["pricing-templates", "list"];
        args.extend(flag.iter().copied());
        support::bin_without_credentials()
            .args(&args)
            .assert()
            .code(3)
            .stderr(predicate::str::contains("unexpected argument"));
    }
}
