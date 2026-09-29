//! Live scenarios for `api-keys`. **Committed.**
//!
//! **Live coverage here is constrained by what it can destroy.** A revoke can
//! take away the credentials the rest of the suite is authenticating with, so
//! every scenario revokes only a key it created in the same test, and none of
//! them touches the key in `FLUTE2_CLIENT_ID`.
//!
//! Every scenario carries `_needs_partner`: the `api-keys` endpoints need a
//! partner token, and merchant credentials cannot pass them.

use crate::*;

/// The whole lifecycle against a key this test brought into existence.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_api_key_create_list_revoke_needs_partner() {
    let created = json(&[
        "api-keys",
        "create",
        "--merchant-id",
        &merchant_id(),
        "--name",
        "flute2 live scratch key",
    ]);
    assert_eq!(created["object"], "api_token");
    let client_id = created["data"]["clientId"]
        .as_str()
        .expect("create returned no clientId")
        .to_string();
    assert!(
        created["data"]["clientSecret"].is_string(),
        "the secret is returned once, on creation, and this is that once: {}",
        created["data"]
    );
    assert_ne!(
        client_id,
        std::env::var("FLUTE2_CLIENT_ID").unwrap_or_default(),
        "the API returned the authenticating key as a newly created one"
    );

    let listed = json(&["api-keys", "list", "--merchant-id", &merchant_id()]);
    assert_eq!(listed["object"], "api_token_list");
    let keys = listed["data"].as_array().cloned().unwrap_or_default();
    assert!(
        keys.iter().any(|k| k["clientId"] == client_id.as_str()),
        "the key just created is not in the list"
    );
    assert!(
        keys.iter().all(|k| k.get("clientSecret").is_none()),
        "the list handed back a client secret; it is documented as returned \
         only on creation"
    );

    live_bin()
        .args(["api-keys", "revoke", "--client-id", &client_id, "--yes"])
        .assert()
        .success();
}

/// The idempotent-404 rule, which on this group has to win over the
/// feature-flag message that every other 404 here carries.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_api_key_revoke_twice_is_still_success_needs_partner() {
    let created = json(&[
        "api-keys",
        "create",
        "--merchant-id",
        &merchant_id(),
        "--name",
        "flute2 live scratch key",
    ]);
    let client_id = created["data"]["clientId"].as_str().unwrap().to_string();
    for _ in 0..2 {
        live_bin()
            .args(["api-keys", "revoke", "--client-id", &client_id, "--yes"])
            .assert()
            .success();
    }
}

/// The unfiltered read, which is the only variant that sends no query at all.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_api_key_list_without_a_merchant_filter_needs_partner() {
    let listed = json(&["api-keys", "list"]);
    assert_eq!(listed["object"], "api_token_list");
    assert!(listed["data"].is_array(), "{listed}");
    // No `pageInfo` on `GetApiKeysResponseDto`, so no `page_info` in meta.
    assert!(
        listed["meta"].get("page_info").is_none(),
        "this endpoint is not paginated, so meta must carry no page_info: {listed}"
    );
}

/// **The evidence for the feature-flag reading of a 404.**
///
/// With the flag disabled the endpoint answers 404, which reads as a wrong URL
/// rather than a disabled feature — so a 404 from `create` or `list` says so.
/// This scenario cannot turn the flag off; it records what to look for when an
/// account has it off, and asserts the ordinary case in the meantime.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_api_key_list_on_an_enabled_account_is_not_a_404_needs_partner() {
    live_bin().args(["api-keys", "list"]).assert().success();
}
