mod support;

/// The whole point of the command: it works when auth does not.
#[test]
fn status_without_credentials_reports_false_and_exits_zero() {
    let out = support::bin_without_credentials()
        .args(["--output", "json", "auth", "status"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["data"]["authenticated"], false);
}

#[tokio::test]
async fn status_with_credentials_pings_and_reports_true() {
    let server = support::mock_with_token().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/v2/ping"))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args(["--output", "json", "auth", "status"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["data"]["authenticated"], true);
    assert_eq!(v["data"]["client_id"], "test-id");
}

/// Credentials that do not authenticate leave `authenticated` false rather
/// than failing the command — the user is running it to find that out.
#[tokio::test]
async fn status_with_rejected_credentials_reports_false_and_exits_zero() {
    let server = support::mock_with_token().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/v2/ping"))
        .respond_with(wiremock::ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args(["--output", "json", "auth", "status"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["data"]["authenticated"], false);
}

/// A ping or token exchange that never reached the server says nothing about
/// the credentials, so it fails the command instead of reporting them bad.
#[tokio::test]
async fn status_that_cannot_reach_the_server_fails_instead_of_reporting_false() {
    let server = support::mock_with_token().await;
    for (var, unreachable) in [
        ("FLUTE2_API_BASE_URL", "http://127.0.0.1:1"),
        ("FLUTE2_OAUTH_URL", "http://127.0.0.1:1/oauth2/token"),
    ] {
        let out = support::bin(&server)
            .env(var, unreachable)
            .args(["--output", "json", "auth", "status"])
            .assert()
            .code(1)
            .get_output()
            .stdout
            .clone();
        assert!(
            !String::from_utf8_lossy(&out).contains("\"authenticated\""),
            "{var}: {}",
            String::from_utf8_lossy(&out)
        );
    }
}

/// A server fault is not an answer about the credentials either.
#[tokio::test]
async fn status_answered_with_a_server_error_fails_instead_of_reporting_false() {
    let server = support::mock_with_token().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/v2/ping"))
        .respond_with(wiremock::ResponseTemplate::new(503))
        .mount(&server)
        .await;

    support::bin(&server)
        .args(["--output", "json", "auth", "status"])
        .assert()
        .failure();
}

/// v1 writes default_profile and never reads it, so `auth switch` is a no-op
/// there. Here the write must actually change what the next command targets.
#[test]
fn switch_writes_default_profile_and_the_next_command_honours_it() {
    let home = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        support::bin_without_credentials_in(home.path())
            // The stored default is the subject, so nothing may pin the
            // profile ahead of it.
            .env_remove("FLUTE2_PROFILE")
            .args(args)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone()
    };
    run(&["auth", "switch", "production"]);
    let out = run(&["--output", "json", "version"]);
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["meta"]["environment"], "production");
}

/// Same group, opposite category: a token cannot exist without credentials.
#[test]
fn token_without_credentials_exits_2() {
    support::bin_without_credentials()
        .args(["auth", "token"])
        .assert()
        .code(2);
}

#[tokio::test]
async fn token_prints_the_bearer_and_nothing_else() {
    let server = support::mock_with_token().await;
    let out = support::bin(&server)
        .args(["auth", "token"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(String::from_utf8(out).unwrap().trim(), "tok-xyz");
}

/// **The token exchange, asserted where the other operations are.**
///
/// Every authenticated command begins with it, and the mock that serves it
/// matches on method and path alone — so a dropped, duplicated or misspelled
/// form parameter reaches production with the whole suite green.
#[tokio::test]
async fn the_token_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::contracts::exchange("get-oauth-token", "client credentials");
    support::bin(&server)
        .args(["auth", "token"])
        .assert()
        .success();
    support::assert_form_exchange_observed(&server, &ex).await;
}

#[test]
fn switch_rejects_an_unknown_profile() {
    support::bin_without_credentials()
        .args(["auth", "switch", "staging"])
        .assert()
        .code(3);
}

/// An explicit flag still beats the stored default.
#[test]
fn profile_flag_overrides_the_stored_default() {
    let home = tempfile::tempdir().unwrap();
    let base = || {
        let mut c = support::bin_without_credentials_in(home.path());
        // The stored default is what the flag has to beat, so nothing may pin
        // the profile ahead of either.
        c.env_remove("FLUTE2_PROFILE");
        c
    };
    base()
        .args(["auth", "switch", "production"])
        .assert()
        .success();
    let out = base()
        .args(["--profile", "sandbox", "--output", "json", "version"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["meta"]["environment"], "sandbox");
}

/// `auth logout` manages the keychain itself, so it is never routed down a
/// path that resolves credentials first — and when the keychain is off limits
/// it refuses in the same shape `auth login` does. Reporting a removal it
/// could not perform tells a user a secret is gone while it is still on the
/// machine.
#[test]
fn logout_without_a_keychain_refuses_rather_than_reporting_removal() {
    let out = support::bin_without_credentials()
        .args(["auth", "logout"])
        .assert()
        .code(3)
        .get_output()
        .clone();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.contains("removed"), "{stdout}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("FLUTE2_NO_KEYCHAIN"), "{stderr}");
}

/// `auth login` prompts for a secret through `rpassword`, so the prompt itself
/// cannot be driven by `assert_cmd`. What *is* checkable is that the command
/// exists and says where the secret goes — which is the parity evidence that
/// the capability was carried over at all. Its storage path is covered by the
/// keychain unit tests and its round trip by the live pass.
#[test]
fn login_is_reachable_and_documents_where_the_secret_goes() {
    let out = support::bin_without_credentials()
        .args(["auth", "login", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let help = String::from_utf8(out).unwrap();
    assert!(help.to_lowercase().contains("keychain"), "{help}");
}

/// A credential lookup that *fails* is not an unauthenticated answer: half an
/// environment pair set is a configuration error, and reporting
/// `authenticated: false` would send the user looking at their account.
#[test]
fn status_propagates_a_credential_lookup_failure() {
    let out = support::bin_without_credentials()
        .env("FLUTE2_CLIENT_ID", "only-the-id")
        .env_remove("FLUTE2_CLIENT_SECRET")
        .args(["auth", "status"])
        .assert()
        .code(3)
        .get_output()
        .clone();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("FLUTE2_CLIENT_SECRET"), "{stderr}");
}

/// A client that cannot be built is a configuration fault, not an
/// unauthenticated answer. The production override refusal has to reach the
/// user, who would otherwise go looking at their account for a setting on
/// their own machine.
#[test]
fn status_propagates_a_client_that_cannot_be_built() {
    let out = support::bin_without_credentials()
        .env("FLUTE2_CLIENT_ID", "test-id")
        .env("FLUTE2_CLIENT_SECRET", "test-secret")
        .args(["--profile", "production", "auth", "status"])
        .assert()
        .code(3)
        .get_output()
        .clone();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.contains("Authenticated"), "{stdout}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("FLUTE2_API_BASE_URL"), "{stderr}");
}

/// `auth switch` warns about the profile it switches **to**. The banner is
/// about the environment the next command will act on, which is the one named
/// on the command line rather than the one currently in force.
#[test]
fn switch_to_production_warns_about_the_target_profile() {
    let home = tempfile::tempdir().unwrap();
    let out = support::bin_without_credentials_in(home.path())
        .args(["auth", "switch", "production"])
        .assert()
        .success()
        .get_output()
        .clone();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("⚠ Operating on PRODUCTION"), "{stderr}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.contains("PRODUCTION"), "{stdout}");
}
