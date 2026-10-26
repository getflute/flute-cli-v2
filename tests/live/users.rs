//! Live scenarios for `users`. **Committed.**
//!
//! Reads only. Every scenario runs as the partner: `schedule-as` and
//! `split-profiles` need a partner user, and every verb needs an account with
//! the user-configuration API permission.

use crate::*;

#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_user_list_first_page() {
    account(Partner);
    let listed = json(&["users", "list"]);
    assert_eq!(listed["object"], "user_list");
    assert!(listed["data"].is_array(), "{listed}");
}

#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_user_list_filtered() {
    account(Partner);
    let listed = json(&[
        "users",
        "list",
        "--user-type",
        "partner",
        "--page-size",
        "5",
    ]);
    assert!(listed["data"].is_array(), "{listed}");
}

#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_user_schedule_as_and_split_profiles() {
    account(Partner);
    let user = partner_user_id();
    let schedules = json(&["users", "schedule-as", &user]);
    assert_eq!(schedules["object"], "schedule_a_list");
    assert!(schedules["data"].is_array(), "{schedules}");
    let splits = json(&["users", "split-profiles", &user]);
    assert_eq!(splits["object"], "split_profile_list");
    assert!(splits["data"].is_array(), "{splits}");
}
