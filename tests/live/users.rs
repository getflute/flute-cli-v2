//! Live scenarios for `users`. **Committed.**
//!
//! Reads only. Every scenario runs as the onboarding partner: `schedule-as`
//! and `split-profiles` need a partner user, and every verb needs an account
//! with the user-configuration API permission.

use crate::*;

#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_user_list_first_page() {
    account(OnboardingPartner);
    let listed = json(&["users", "list"]);
    assert_eq!(listed["object"], "user_list");
    assert!(listed["data"].is_array(), "{listed}");
}

#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_user_list_filtered() {
    account(OnboardingPartner);
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
/// Needs a partner user whose Schedule As and Split Profiles are set up; for
/// any other user both commands answer 404.
fn live_user_schedule_as_and_split_profiles_needs_configured_user() {
    account(OnboardingPartner);
    let user = partner_user_id();
    let schedules = json(&["users", "schedule-as", &user]);
    assert_eq!(schedules["object"], "schedule_a_list");
    assert!(schedules["data"].is_array(), "{schedules}");
    let splits = json(&["users", "split-profiles", &user]);
    assert_eq!(splits["object"], "split_profile_list");
    assert!(splits["data"].is_array(), "{splits}");
}
