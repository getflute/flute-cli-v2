mod support;

use predicates::prelude::*;

/// A usage error is raised before clap has produced a `Cli`, so this one path
/// resolves the output format for itself.
#[test]
fn usage_error_under_output_json_prints_an_envelope_to_stdout_and_exits_3() {
    let out = support::bin_without_credentials()
        .args(["--output", "json", "nosuchcommand"])
        .assert()
        .code(3)
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["kind"], "client");
    assert!(v["message"].is_string());
}

#[test]
fn usage_error_without_json_writes_to_stderr_and_leaves_stdout_empty() {
    support::bin_without_credentials()
        .args(["nosuchcommand"])
        .assert()
        .code(3)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::is_empty().not());
}

/// Same decision, reached through the environment rather than argv.
#[test]
fn flute2_output_env_selects_json_for_usage_errors() {
    support::bin_without_credentials()
        .env("FLUTE2_OUTPUT", "json")
        .args(["nosuchcommand"])
        .assert()
        .code(3)
        .stdout(predicate::str::starts_with("{"));
}

/// `--help` and `--version` are clap "errors" that are not failures: they go
/// to stdout and the process succeeds.
#[test]
fn help_and_version_succeed_on_stdout() {
    for arg in ["--help", "--version"] {
        support::bin_without_credentials()
            .arg(arg)
            .assert()
            .success()
            .stdout(predicate::str::is_empty().not());
    }
}

/// The global flags are advertised under their own heading, so they do not
/// interleave with a flag-heavy leaf command's own options.
#[test]
fn the_global_flags_are_grouped_under_their_own_heading() {
    let out = support::bin_without_credentials()
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let help = String::from_utf8(out).unwrap();
    assert!(help.contains("Global options"), "{help}");
    for flag in ["--profile", "--output", "--debug"] {
        assert!(help.contains(flag), "{flag} missing from --help:\n{help}");
    }
}

/// An unknown profile is a client error, not a request aimed at nowhere.
#[test]
fn an_unknown_profile_exits_3_and_names_the_value() {
    let out = support::bin_without_credentials()
        .args(["--profile", "staging", "--output", "json", "version"])
        .assert()
        .code(3)
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["kind"], "client");
    assert!(
        v["message"].as_str().unwrap().contains("staging"),
        "{:?}",
        v["message"]
    );
}

/// The production banner is stderr-only, so `--output json` stays parseable.
///
/// `auth status` is the subject because it is the one command that prints a
/// data document while tolerating absent credentials, so it is where a banner
/// on stdout would corrupt a machine consumer's parse.
#[test]
fn the_production_banner_never_reaches_stdout() {
    let out = support::bin_without_credentials()
        .args([
            "--profile",
            "production",
            "--output",
            "json",
            "auth",
            "status",
        ])
        .assert()
        .get_output()
        .clone();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stdout.contains("PRODUCTION"),
        "banner leaked to stdout:\n{stdout}"
    );
    assert!(
        stderr.contains("PRODUCTION"),
        "banner missing from stderr:\n{stderr}"
    );
}

/// **402, 409, 429 and a server 503 exit 1**, and `agents.md` tells a caller
/// so.
///
/// The three 4xx deliberately claim no code of their own:
/// nothing can already depend on a code the CLI has never emitted, so naming
/// one later stays backwards compatible. A 5xx reaches the same arm from the
/// other end of the range. The exit-code map is unit-tested in `cli::output`;
/// this asserts it through a real invocation, which is the only layer a caller
/// can observe.
#[tokio::test]
async fn the_statuses_with_no_code_of_their_own_exit_one_through_the_general_arm() {
    for status in [402u16, 409, 429, 503] {
        let server = support::mock_with_token().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/v2/ping"))
            .respond_with(wiremock::ResponseTemplate::new(status))
            .mount(&server)
            .await;

        let out = support::bin(&server)
            .args(["--output", "json", "ping"])
            .assert()
            .code(1)
            .get_output()
            .stdout
            .clone();
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["kind"], "api", "{status}");
        assert_eq!(v["status"], status, "{status}");
    }
}

/// **A base-URL override is refused on the production profile**, and the
/// refusal is a client error rather than a redirected request.
///
/// The seam that lets a test point the binary at a mock server is the same
/// seam that could point a real charge somewhere else. `client.rs` unit-tests
/// the resolver; this asserts the whole invocation, because the distinction
/// that matters is exit 3 — refused before anything was built — rather than
/// exit 1, which is what a request to the override would produce.
#[test]
fn a_base_url_override_is_refused_on_the_production_profile() {
    let out = support::bin_without_credentials()
        .env("FLUTE2_PROFILE", "production")
        // Credentials, so the exit 3 under test is the refusal rather than
        // the absence of a way to authenticate.
        .env("FLUTE2_CLIENT_ID", "test-id")
        .env("FLUTE2_CLIENT_SECRET", "test-secret")
        .args(["--output", "json", "ping"])
        .assert()
        .code(3)
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["kind"], "client");
    assert!(
        v["message"]
            .as_str()
            .unwrap()
            .contains("FLUTE2_API_BASE_URL"),
        "the refusal must name the variable: {v}"
    );
}

/// A bodyless 401 has no body to carry a correlation id, so the response
/// header is the only identifier that ever existed. Losing it costs the user
/// the one value support asks for.
#[tokio::test]
async fn a_bodyless_failure_reports_the_correlation_id_from_the_header() {
    let server = support::mock_with_token().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/v2/ping"))
        .respond_with(
            wiremock::ResponseTemplate::new(401)
                .insert_header("x-correlation-id", "corr-from-header"),
        )
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args(["--output", "json", "ping"])
        .assert()
        .code(2)
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["kind"], "api");
    assert_eq!(v["status"], 401);
    assert_eq!(v["correlation_id"], "corr-from-header");
}

/// A bare `flute2` is a request for help, not a usage error: nothing was
/// asked for wrongly, so help goes to stdout and the process succeeds.
#[test]
fn a_bare_invocation_prints_help_to_stdout_and_succeeds() {
    let out = support::bin_without_credentials()
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8(out).unwrap();
    assert!(
        stdout.contains("Usage: flute2"),
        "a bare invocation must print the root help:\n{stdout}"
    );
}

/// The `--help` screen for one path through the tree.
///
/// The env-backed globals are cleared because clap prints an `env`-backed
/// argument's current value into `--help`, and what these assert must not
/// depend on the environment the suite runs in.
fn help_for(args: &[&str]) -> String {
    let out = support::bin_without_credentials()
        .env_remove("FLUTE2_PROFILE")
        .env_remove("FLUTE2_OUTPUT")
        .args(args)
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    String::from_utf8(out).unwrap()
}

/// The whole `Commands:` block of a `--help` screen, in the order it lists.
fn commands_in(args: &[&str]) -> Vec<String> {
    help_for(args)
        .lines()
        .skip_while(|l| !l.starts_with("Commands:"))
        .skip(1)
        .take_while(|l| !l.trim().is_empty())
        .filter_map(|l| l.strip_prefix("  "))
        .filter(|l| !l.starts_with(' '))
        .map(|l| l.split_whitespace().next().unwrap().to_string())
        .collect()
}

/// The root screen is the one every caller sees first, and it must read as
/// prose written for them: no attribute names, no rationale for how the flags
/// are declared.
///
/// The rendering is compact — one line per flag — because that is what clap
/// produces while no argument carries a multi-paragraph help, and a paragraph
/// of reasoning is what turns the whole screen into the verbose form.
#[test]
fn the_root_help_is_compact_and_explains_no_implementation() {
    let help = help_for(&[]);
    for needle in ["default_value", "auth switch a no-op", "v1"] {
        assert!(
            !help.contains(needle),
            "`flute2 --help` contains {needle:?}:\n{help}"
        );
    }
    let lines: Vec<&str> = help.lines().collect();
    let profile = lines
        .iter()
        .position(|l| l.contains("--profile <PROFILE>"))
        .unwrap_or_else(|| panic!("no --profile line in:\n{help}"));
    assert!(
        lines[profile + 1].contains("--output <OUTPUT>"),
        "the global options are not rendered compactly:\n{help}"
    );
}

#[test]
fn the_root_lists_its_commands_in_the_documented_order() {
    assert_eq!(
        commands_in(&[]),
        [
            "auth",
            "ping",
            "version",
            "transactions",
            "customers",
            "payment-methods",
            "payment-links",
            "payment-sessions",
            "terminals",
            "pos",
            "settlements",
            "settings",
            "api-keys",
            "completion",
            "update",
            "help",
        ]
    );
}

/// Within a group, the operations that move money come before the ones that
/// read it back.
#[test]
fn each_group_lists_its_commands_in_the_documented_order() {
    assert_eq!(
        commands_in(&["auth"]),
        ["login", "status", "switch", "logout", "token", "help"]
    );
    assert_eq!(
        commands_in(&["transactions"]),
        [
            "create",
            "capture",
            "reversal",
            "credit",
            "tip-adjust",
            "ach-hold",
            "ach-release",
            "share-receipt",
            "calculate-amount",
            "get",
            "list",
            "inspect",
            "help",
        ]
    );
}

/// `--output` and `--debug` state the whole rule they obey: where the format
/// comes from when the flag is absent, and what a trace does not carry.
#[test]
fn the_global_flags_state_their_fallbacks_and_their_redaction() {
    let help = help_for(&[]);
    let flat = help.split_whitespace().collect::<Vec<_>>().join(" ");
    for sentence in [
        "When omitted, falls back to the `FLUTE2_OUTPUT` env var, then to the \
         `output` key in ~/.flute2/config.toml, then to `table`",
        "CVV/security codes are removed",
    ] {
        assert!(
            flat.contains(sentence),
            "`flute2 --help` is missing {sentence:?}:\n{help}"
        );
    }
}

/// The failure envelope follows the **resolved** output format, so a config
/// file asking for JSON gets JSON on the way out of a failure too.
#[test]
fn a_runtime_failure_honours_the_config_files_output() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join(".flute2")).unwrap();
    std::fs::write(
        home.path().join(".flute2").join("config.toml"),
        "output = \"json\"\n",
    )
    .unwrap();

    let out = support::bin_without_credentials_in(home.path())
        .args(["ping"])
        .assert()
        .code(2)
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["kind"], "auth");
}

/// The banner names the environment and the host it is about to charge.
#[test]
fn the_production_banner_names_the_environment_and_the_url() {
    // The banner names the profile's own base URL, not the override the
    // helper sets.
    let out = support::bin_without_credentials()
        .args(["--profile", "production", "auth", "status"])
        .assert()
        .get_output()
        .clone();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("⚠ Operating on PRODUCTION (https://api.flute.com)"),
        "{stderr}"
    );
}

/// A runtime failure is a sentence on stderr, and it names what is missing
/// rather than the layer that noticed.
#[test]
fn a_runtime_failure_is_prefixed_and_names_the_missing_credentials() {
    let out = support::bin_without_credentials()
        .args(["ping"])
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .get_output()
        .clone();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        stderr.trim_end(),
        "Error: auth error: no credentials for [sandbox]; set FLUTE2_CLIENT_ID and \
         FLUTE2_CLIENT_SECRET, or run `flute2 auth login`",
        "{stderr}"
    );
}

/// Both error paths print the same shape as every success envelope, so a
/// reader of a failed run sees the layout a successful one has.
#[test]
fn the_json_error_envelope_is_pretty_printed_on_both_paths() {
    let usage = support::bin_without_credentials()
        .args(["--output", "json", "nosuchcommand"])
        .assert()
        .code(3)
        .get_output()
        .stdout
        .clone();
    assert!(
        String::from_utf8_lossy(&usage).starts_with("{\n  \"kind\": \"client\""),
        "{}",
        String::from_utf8_lossy(&usage)
    );

    let runtime = support::bin_without_credentials()
        .args(["--output", "json", "ping"])
        .assert()
        .code(2)
        .get_output()
        .stdout
        .clone();
    assert!(
        String::from_utf8_lossy(&runtime).starts_with("{\n  \"kind\": \"auth\""),
        "{}",
        String::from_utf8_lossy(&runtime)
    );
}

/// The envelope's `message` is the complaint alone: clap's styling is gone and
/// the `Usage:` block, which restates the help, is cut off.
#[test]
fn a_usage_error_message_carries_no_styling_and_no_usage_block() {
    let out = support::bin_without_credentials()
        .args(["--output", "json", "customers", "delete"])
        .assert()
        .code(3)
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    let message = v["message"].as_str().unwrap();
    assert!(!message.contains('\u{1b}'), "{message}");
    assert!(!message.contains("Usage:"), "{message}");
    assert_eq!(message, message.trim());
}

/// A token exchange that never reached the server is a **transport** failure,
/// exit 1: the invocation and the credentials are both fine, so the caller
/// should retry with backoff rather than send an operator after the secret.
#[test]
fn a_token_endpoint_that_refuses_the_connection_is_a_transport_failure() {
    let out = support::bin_without_credentials()
        .env("FLUTE2_CLIENT_ID", "test-id")
        .env("FLUTE2_CLIENT_SECRET", "test-secret")
        .args(["--output", "json", "ping"])
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["kind"], "transport", "{v}");
    // Both endpoints point at the refusing port, so the message is what says
    // the token request is the one that failed.
    assert!(
        v["message"]
            .as_str()
            .unwrap()
            .contains("token request failed"),
        "{v}"
    );
    // The cause is part of the message: a refused connection reads
    // differently from a DNS or TLS failure.
    assert!(
        v["message"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("connect"),
        "{v}"
    );
}

/// An API request that never reached the server names its cause the same way.
#[tokio::test]
async fn an_api_request_that_is_refused_names_the_connection() {
    let server = support::mock_with_token().await;
    let out = support::bin(&server)
        .env("FLUTE2_API_BASE_URL", "http://127.0.0.1:1")
        .args(["--output", "json", "ping"])
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["kind"], "transport", "{v}");
    assert!(
        v["message"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("connect"),
        "{v}"
    );
}

/// `update` reaches GitHub over the network, so a network failure there is
/// a transport failure that names its cause.
#[test]
fn an_update_that_cannot_reach_github_is_a_transport_failure() {
    let out = support::bin_without_credentials()
        .env("HTTPS_PROXY", "http://127.0.0.1:1")
        .env("https_proxy", "http://127.0.0.1:1")
        .env("ALL_PROXY", "http://127.0.0.1:1")
        .env_remove("NO_PROXY")
        .env_remove("no_proxy")
        .args(["--output", "json", "update"])
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["kind"], "transport", "{v}");
    assert!(
        v["message"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("connect"),
        "{v}"
    );
}

/// The token endpoint answering 401 is the other half of the split: the
/// credentials reached it and were refused, which is exit 2 and an operator's
/// problem.
#[tokio::test]
async fn a_token_endpoint_that_rejects_the_credentials_is_an_auth_failure() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/oauth2/token"))
        .respond_with(
            wiremock::ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "error": "invalid_client", "error_description": "Client authentication failed"})),
        )
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args(["--output", "json", "ping"])
        .assert()
        .code(2)
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["kind"], "auth", "{v}");
}

/// A token endpoint answering 5xx or 429 says nothing about the credentials,
/// so it is an **api** failure with its status, exit 1, and the caller retries
/// with backoff instead of sending an operator after a secret that may be
/// fine. Its message names the token request as its source.
#[tokio::test]
async fn a_token_endpoint_that_fails_is_an_api_failure_not_an_auth_one() {
    for status in [503u16, 429] {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/oauth2/token"))
            .respond_with(
                wiremock::ResponseTemplate::new(status).set_body_string("upstream unavailable"),
            )
            .mount(&server)
            .await;

        let out = support::bin(&server)
            .args(["--output", "json", "ping"])
            .assert()
            .code(1)
            .get_output()
            .stdout
            .clone();
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["kind"], "api", "{v}");
        assert_eq!(v["status"], status, "{v}");
        let message = v["message"].as_str().unwrap();
        assert!(message.starts_with("token request failed: "), "{v}");
    }
}

/// A token endpoint answering 200 with a body that is not a token is a
/// **decode** failure, exit 1: the credentials were accepted and the CLI
/// cannot read the answer, which is a contract change to investigate.
#[tokio::test]
async fn a_token_endpoint_answering_an_unreadable_body_is_a_decode_failure() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/oauth2/token"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("<html>ok</html>"))
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args(["--output", "json", "ping"])
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["kind"], "decode", "{v}");
}

/// A usage error resolves its output format the way every other failure does,
/// so a config file asking for JSON gets a parseable failure out of a
/// mistyped command too.
#[test]
fn a_usage_error_honours_the_config_files_output() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join(".flute2")).unwrap();
    std::fs::write(
        home.path().join(".flute2").join("config.toml"),
        "output = \"json\"\n",
    )
    .unwrap();

    let out = support::bin_without_credentials_in(home.path())
        .args(["customers", "nosuchverb"])
        .assert()
        .code(3)
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["kind"], "client", "{v}");
}

/// **An unparseable `--output` value leaves stdout empty**, whatever the
/// config file asks for. It is the one usage error that cannot honour the
/// resolved mode, because the mode is the thing being rejected — so it is
/// named in the contract rather than left for a caller to discover as a
/// silent stream.
#[test]
fn an_unparseable_output_value_leaves_stdout_empty() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join(".flute2")).unwrap();
    std::fs::write(
        home.path().join(".flute2").join("config.toml"),
        "output = \"json\"\n",
    )
    .unwrap();

    support::bin_without_credentials_in(home.path())
        .args(["--output", "nosuchmode", "ping"])
        .assert()
        .code(3)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("invalid value"));
    support::bin_without_credentials_in(home.path())
        .args(["--output=nosuchmode", "ping"])
        .assert()
        .code(3)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("invalid value"));
    support::bin_without_credentials_in(home.path())
        .env("FLUTE2_OUTPUT", "nosuchmode")
        .args(["ping"])
        .assert()
        .code(3)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("invalid value"));
}

/// **The output mode is case-insensitive** from the flag, from
/// `FLUTE2_OUTPUT` and from the config file, and a usage error renders in the
/// mode the parser would have chosen.
#[tokio::test]
async fn the_output_mode_is_case_insensitive() {
    let server = support::mock_with_token().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/v2/ping"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .mount(&server)
        .await;

    let spellings: [(&[&str], Option<&str>); 3] = [
        (&["--output", "JSON"], None),
        (&["--output=Json"], None),
        (&[], Some("JSON")),
    ];
    for (flag, env) in spellings {
        let mut cmd = support::bin(&server);
        if let Some(value) = env {
            cmd.env("FLUTE2_OUTPUT", value);
        }
        let out = cmd
            .args(flag)
            .arg("ping")
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["object"], "ping", "{flag:?} {env:?}: {v}");

        let mut cmd = support::bin_without_credentials();
        if let Some(value) = env {
            cmd.env("FLUTE2_OUTPUT", value);
        }
        let out = cmd
            .args(flag)
            .args(["customers", "nosuchverb"])
            .assert()
            .code(3)
            .get_output()
            .stdout
            .clone();
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["kind"], "client", "{flag:?} {env:?}: {v}");
    }
}

/// A group invoked with no subcommand is a usage error, and the envelope has
/// to say so: clap's payload for it is the group's help page, whose first
/// line is the group description and says nothing about what is wrong.
#[test]
fn a_group_without_a_subcommand_names_the_help_to_run() {
    for group in ["auth", "transactions"] {
        let out = support::bin_without_credentials()
            .args(["--output", "json", group])
            .assert()
            .code(3)
            .get_output()
            .stdout
            .clone();
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["kind"], "client", "{v}");
        let message = v["message"].as_str().unwrap();
        assert!(message.contains("subcommand"), "{message}");
        assert!(
            message.contains(&format!("flute2 {group} --help")),
            "{message}"
        );
    }
}

/// Without `--output json` the same failure is clap's own help on stderr, so
/// stdout stays empty for a caller that is reading it for data.
#[test]
fn a_group_without_a_subcommand_writes_the_help_to_stderr() {
    let out = support::bin_without_credentials()
        .args(["auth"])
        .assert()
        .code(3)
        .stdout(predicate::str::is_empty())
        .get_output()
        .clone();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("Usage: flute2 auth"), "{stderr}");
    assert!(stderr.contains("login"), "{stderr}");
}

/// The banner precedes **every** command that resolves a profile, not only
/// the ones that reach the API: `version` reports which environment it is
/// pointed at, and `auth logout` acts on production credentials.
#[test]
fn the_production_banner_precedes_every_profile_bound_command() {
    for args in [
        vec!["--profile", "production", "version"],
        vec!["--profile", "production", "auth", "logout"],
    ] {
        let out = support::bin_without_credentials()
            .args(&args)
            .assert()
            .get_output()
            .clone();
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("⚠ Operating on PRODUCTION"),
            "{args:?} printed no banner to stderr:\n{stderr}"
        );
        assert!(
            !stdout.contains("PRODUCTION"),
            "{args:?} leaked the banner to stdout:\n{stdout}"
        );
    }
}

/// The two exceptions. `completion` writes a script a shell sources, and
/// neither it nor `update` resolves a profile — so neither has one to warn
/// about.
#[test]
fn completion_prints_no_production_banner() {
    let out = support::bin_without_credentials()
        .args(["--profile", "production", "completion", "bash"])
        .assert()
        .success()
        .get_output()
        .clone();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("PRODUCTION"), "{stderr}");
    assert!(!stdout.contains("PRODUCTION"), "{stdout}");
}

/// A `FLUTE_` name belongs to the other binary. Its value is never read, so
/// the only safe thing to do with one is say which variable is read instead —
/// silence leaves the caller believing they configured this CLI.
#[test]
fn a_v1_environment_variable_names_the_flute2_one_read_instead() {
    let out = support::bin_without_credentials()
        .env("FLUTE_OUTPUT", "json")
        .args(["version"])
        .assert()
        .success()
        .get_output()
        .clone();

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("FLUTE_OUTPUT"), "{stderr}");
    assert!(stderr.contains("FLUTE2_OUTPUT"), "{stderr}");
    // The value is not adopted: the output stays the table default.
    assert!(
        String::from_utf8_lossy(&out.stdout).starts_with("flute2  v"),
        "{:?}",
        String::from_utf8_lossy(&out.stdout)
    );

    // Presence is the whole test, so a variable set to nothing is still set.
    support::bin_without_credentials()
        .env("FLUTE_OUTPUT", "")
        .args(["version"])
        .assert()
        .success()
        .stderr(predicate::str::contains("FLUTE2_OUTPUT"));
}

/// Nothing to correct once the `FLUTE2_` name is set, so the note would be
/// noise on every invocation of a correctly configured machine.
#[test]
fn a_v1_variable_is_silent_when_the_flute2_one_is_set() {
    support::bin_without_credentials()
        .env("FLUTE_OUTPUT", "json")
        .env("FLUTE2_OUTPUT", "quiet")
        .args(["version"])
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
}

/// A refusal the CLI makes on its own is reported as itself, whether or not a
/// credential could be resolved. A caller told to fix its credentials when its
/// arguments are wrong is sent to the one place that cannot help, and an
/// unconfigured machine is exactly where a first invocation lands.
#[test]
fn an_invalid_argument_is_a_client_error_without_credentials() {
    let out = support::bin_without_credentials()
        .args(["--output", "json", "customers", "list", "--page-size", "0"])
        .assert()
        .code(3)
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["kind"], "client", "{v}");
    assert!(
        v["message"].as_str().unwrap().contains("--page-size"),
        "{v}"
    );
}

/// The same ordering for a refusal that reads the whole argument set rather
/// than one flag.
#[test]
fn an_empty_update_is_a_client_error_without_credentials() {
    let out = support::bin_without_credentials()
        .args(["--output", "json", "customers", "update", "cus_1"])
        .assert()
        .code(3)
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["kind"], "client", "{v}");
    assert!(
        v["message"].as_str().unwrap().contains("nothing to update"),
        "{v}"
    );
}

/// A money flag's rules belong to the amount parser, so the value reaches it
/// instead of being read as a flag of its own — on every group that takes one,
/// not only the one a report happens to name.
#[test]
fn a_negative_amount_names_the_flag_and_the_rule() {
    for (args, expected) in [
        (
            vec!["transactions", "calculate-amount", "--amount", "-1.00"],
            "for '--amount <AMOUNT>': amount must not be negative: -1.00",
        ),
        (
            vec![
                "transactions",
                "capture",
                "--transaction-id",
                "txn_1",
                "--amount",
                "-1.00",
            ],
            "for '--amount <AMOUNT>': amount must not be negative: -1.00",
        ),
        (
            vec!["transactions", "list", "--min-amount", "-1.00"],
            "for '--min-amount <MIN_AMOUNT>': amount must not be negative: -1.00",
        ),
        (
            vec![
                "transactions",
                "tip-adjust",
                "--transaction-id",
                "txn_1",
                "--tip-rate",
                "-1.00",
            ],
            "for '--tip-rate <TIP_RATE>': rate must not be negative: -1.00",
        ),
        (
            vec!["payment-links", "create", "--amount", "-1.00"],
            "for '--amount <AMOUNT>': amount must not be negative: -1.00",
        ),
        (
            vec!["payment-links", "update", "pl_1", "--amount", "-1.00"],
            "for '--amount <AMOUNT>': amount must not be negative: -1.00",
        ),
        (
            vec!["payment-sessions", "create", "--amount", "-1.00"],
            "for '--amount <AMOUNT>': amount must not be negative: -1.00",
        ),
        (
            vec!["pos", "create", "--amount", "-1.00"],
            "for '--amount <AMOUNT>': amount must not be negative: -1.00",
        ),
        (
            vec![
                "settings",
                "update-autofill",
                "--product-unit-price",
                "-1.00",
            ],
            "for '--product-unit-price <UNIT_PRICE>': amount must not be negative: -1.00",
        ),
        (
            vec!["settings", "update-autofill", "--l2-tax-rate", "-1.00"],
            "for '--l2-tax-rate <TAX_RATE>': rate must not be negative: -1.00",
        ),
    ] {
        support::bin_without_credentials()
            .args(&args)
            .assert()
            .code(3)
            .stderr(predicate::str::contains(expected));
    }
}

/// The negative-number allowance is per money flag and only for numbers, so a
/// flag left without a value still reports the missing value rather than
/// eating the flag that follows it, and a hyphenated value on a flag that
/// takes text is still a stray token.
#[test]
fn a_money_flag_left_empty_does_not_swallow_the_next_token() {
    support::bin_without_credentials()
        .args([
            "transactions",
            "calculate-amount",
            "--amount",
            "--currency-code",
            "USD",
        ])
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "a value is required for '--amount <AMOUNT>'",
        ));

    support::bin_without_credentials()
        .args([
            "transactions",
            "credit",
            "--amount",
            "1.00",
            "--reference-id",
            "-1.00",
        ])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("unexpected argument"));
}

/// **A consumer that stops reading is not a failure the caller needs told.**
/// Rust ignores `SIGPIPE`, so the write returns `EPIPE` and the process lives
/// to choose its own ending: stderr stays empty and the exit code is 141, the
/// value a shell reports for a process a `SIGPIPE` killed and the one a
/// `pipefail` pipeline already expects from every other tool.
///
/// `completion` carries the payload because it reaches the write with no
/// network at all, and its script is far larger than any pipe buffer — so the
/// close lands mid-write rather than after the last byte.
#[test]
fn a_consumer_that_stops_reading_stdout_exits_141_in_silence() {
    let mut child = support::raw_bin_without_credentials()
        .args(["completion", "bash"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawning flute2");
    let mut stdout = child.stdout.take().expect("stdout was piped");
    let mut head = [0u8; 16];
    std::io::Read::read(&mut stdout, &mut head).expect("reading the first bytes");
    drop(stdout);

    let out = child.wait_with_output().expect("waiting for flute2");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.is_empty(), "stderr on a closed stdout:\n{stderr}");
    assert_eq!(out.status.code(), Some(141), "stderr was:\n{stderr}");
}

/// **An exported-but-empty variable holds no value, so it selects no format.**
/// clap fills the flag from the environment before anything else runs, so an
/// empty `FLUTE2_OUTPUT` would otherwise be a usage error on every invocation
/// — including one that names the format on the command line.
#[test]
fn an_empty_output_environment_value_is_read_as_unset() {
    support::bin_without_credentials()
        .env("FLUTE2_OUTPUT", "")
        .args(["--output", "json", "ping"])
        .assert()
        .code(2)
        .stdout(predicate::str::starts_with("{"));

    // Alone, it leaves the resolution where an unset variable leaves it: the
    // config file, then `table`, which reports a failure on stderr.
    support::bin_without_credentials()
        .env("FLUTE2_OUTPUT", "")
        .args(["ping"])
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("Error:"));

    // And the config file below it is reached, on the usage-error path that
    // walks the precedence by hand as much as on the parsed one.
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join(".flute2")).unwrap();
    std::fs::write(
        home.path().join(".flute2").join("config.toml"),
        "output = \"json\"\n",
    )
    .unwrap();
    support::bin_without_credentials()
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path())
        .env("USERPROFILE", home.path())
        .env("FLUTE2_OUTPUT", "")
        .args(["nosuchcommand"])
        .assert()
        .code(3)
        .stdout(predicate::str::starts_with("{"));
}

/// **A consumer that stops reading stderr costs the diagnostic and nothing
/// else.** The stream carries no data, so the caller's copy of the output is
/// whole and the command reports what it would have reported; only the line
/// explaining it is gone. 141 belongs to stdout, where a consumer walking away
/// does leave the caller holding a prefix.
///
/// The read end is closed before the child has resolved its credentials, so
/// every write it goes on to make lands on a pipe nobody holds.
#[test]
fn a_closed_stderr_loses_the_diagnostic_and_keeps_the_exit_code() {
    let mut child = support::raw_bin_without_credentials()
        .args(["ping"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawning flute2");
    drop(child.stderr.take().expect("stderr was piped"));

    let out = child.wait_with_output().expect("waiting for flute2");
    assert_eq!(
        out.status.code(),
        Some(2),
        "stdout was:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

/// **Every request identifies the CLI and its version.** The API's logs are
/// how a caller's traffic is told apart from a browser's, and the version is
/// what makes a report against an old build actionable.
#[tokio::test]
async fn every_request_carries_the_cli_user_agent() {
    let server = support::mock_with_token().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/v2/ping"))
        .and(wiremock::matchers::header(
            "user-agent",
            concat!("flute2/", env!("CARGO_PKG_VERSION")),
        ))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    support::bin(&server).args(["ping"]).assert().success();
}

/// An identifier sent in a query parameter or a body is held to the same
/// empty rule as one in a path: empty after trimming is a `client` refusal,
/// exit 3, and nothing is sent. An empty `batchIds` would otherwise filter
/// nothing and report the unfiltered list as the batch asked for.
#[tokio::test]
async fn an_empty_query_or_body_identifier_is_refused_before_the_wire() {
    let server = support::mock_with_token().await;
    for (flag, args) in [
        ("<BATCH_ID>", vec!["settlements", "get", ""]),
        ("<BATCH_ID>", vec!["settlements", "get", "  "]),
        (
            "--payment-processor-id",
            vec!["settlements", "close", "--payment-processor-id", "  "],
        ),
        (
            "--terminal-id",
            vec!["pos", "print-receipt", "--terminal-id", "  ", "ptx-1"],
        ),
        (
            "--customer-id",
            vec![
                "payment-methods",
                "set-default",
                "--customer-id",
                "",
                "pm-1",
            ],
        ),
    ] {
        let out = support::bin(&server)
            .args(["--output", "json"])
            .args(&args)
            .assert()
            .code(3)
            .get_output()
            .stdout
            .clone();
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["kind"], "client", "{args:?}: {v}");
        let message = v["message"].as_str().unwrap();
        assert!(message.contains(flag), "{args:?}: {message}");
    }
    assert!(
        server.received_requests().await.unwrap().is_empty(),
        "a refused identifier still reached the network"
    );
}
