//! Live scenarios for the rest of `customers`. **Committed.**

use crate::*;

/// A read of the first page, with no pagination flags at all — so the
/// server's own defaults govern and the CLI sends no query.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_customer_list_first_page() {
    let v = json(&["customers", "list"]);
    assert_eq!(v["object"], "customer_list");
    assert!(v["data"].is_array(), "data must be the collection: {v}");
    // pageInfo is reproduced field for field, so an agent can paginate
    // without parsing table output.
    assert!(v["meta"]["page_info"].is_object(), "{v}");
}

/// The four named filters express what a single `--search` cannot.
/// A filter that matches nothing is still a successful empty page, not a 404.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_customer_list_filtered_by_email() {
    let created = json(&[
        "customers",
        "create",
        "--first-name",
        "Ada",
        "--last-name",
        "Lovelace",
        "--email",
        "ada@example.com",
    ]);
    let id = created["data"]["customerId"].as_str().unwrap().to_string();

    let v = json(&[
        "customers",
        "list",
        "--email",
        "ada@example.com",
        "--page-size",
        "5",
    ]);
    let ids: Vec<&str> = v["data"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["customerId"].as_str())
        .collect();
    assert!(ids.contains(&id.as_str()), "{v}");

    live_bin()
        .args(["customers", "delete", &id, "--yes"])
        .assert()
        .success();
}

/// `update` and `delete` both answer 200 with **no body**, so the only way to
/// see that the update took is to read the customer back.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_customer_update_then_delete() {
    let created = json(&[
        "customers",
        "create",
        "--first-name",
        "Ada",
        "--last-name",
        "Lovelace",
    ]);
    let id = created["data"]["customerId"].as_str().unwrap().to_string();

    let updated = json(&["customers", "update", &id, "--company", "Analytical"]);
    assert_eq!(updated["data"]["customerId"], id);
    assert_eq!(updated["data"]["updated"], true);

    let fetched = json(&["customers", "get", &id]);
    assert_eq!(fetched["data"]["companyName"], "Analytical");

    let deleted = json(&["customers", "delete", &id, "--yes"]);
    assert_eq!(deleted["data"]["deleted"], true);
}

/// The idempotent-404 rule, against the running API rather than a mock: the
/// caller's intent held, so a second delete is still exit 0.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_customer_delete_twice_is_still_success() {
    let created = json(&[
        "customers",
        "create",
        "--first-name",
        "Ada",
        "--last-name",
        "Lovelace",
    ]);
    let id = created["data"]["customerId"].as_str().unwrap().to_string();

    for _ in 0..2 {
        live_bin()
            .args(["customers", "delete", &id, "--yes"])
            .assert()
            .success();
    }
}

/// `--all` exhausts the collection, so the count it returns cannot be smaller
/// than one page of it.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_customer_list_all_exhausts_the_collection() {
    let one_page = json(&["customers", "list", "--page-size", "1"]);
    let all = json(&["customers", "list", "--all", "--page-size", "1"]);
    let page_items = one_page["data"].as_array().unwrap().len();
    let all_items = all["data"].as_array().unwrap().len();
    assert!(all_items >= page_items, "{all_items} < {page_items}");
    // The data spans every page, so no single pageInfo describes it.
    assert!(all["meta"].get("page_info").is_none(), "{all}");
}
