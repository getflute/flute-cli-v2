//! Live scenarios for `applications` and `pricing-templates`. **Committed.**
//!
//! Reads only. Merchant applications exist only in production, where a create
//! invites a real merchant, so `applications create` has no live scenario.
//! Every scenario runs as the partner, whose account needs the applications
//! and user-configuration API permissions.

use crate::*;

#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_application_list_first_page() {
    account(Partner);
    let listed = json(&["applications", "list"]);
    assert_eq!(listed["object"], "application_list");
    assert!(listed["data"].is_array(), "{listed}");
}

#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_application_list_filtered() {
    account(Partner);
    let listed = json(&[
        "applications",
        "list",
        "--status",
        "in-progress",
        "--created-from",
        "2026-01-01",
        "--page-size",
        "5",
    ]);
    assert!(listed["data"].is_array(), "{listed}");
}

#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_pricing_template_list() {
    account(Partner);
    let listed = json(&["pricing-templates", "list"]);
    assert_eq!(listed["object"], "pricing_template_list");
    assert!(listed["data"].is_array(), "{listed}");
    assert!(listed["meta"].get("page_info").is_none(), "{listed}");
}
