mod support;

use predicates::prelude::*;

/// **The shells come from clap's own possible-value list.** A hand-written
/// list is a list nobody updates, and a shell clap starts supporting is a
/// shell whose script nothing here would generate.
///
/// Each script must also name the binary it completes, or a user's shell
/// completes the wrong `flute`.
#[test]
fn completion_emits_a_script_for_each_supported_shell() {
    let help = String::from_utf8(
        support::bin_without_credentials()
            .args(["completion", "--help"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    let (_, rest) = help
        .split_once("[possible values: ")
        .expect("the completion help lists no possible values");
    let (values, _) = rest.split_once(']').expect("unterminated value list");
    let shells: Vec<&str> = values.split(", ").map(str::trim).collect();
    assert!(
        shells.len() >= 5,
        "parsed only {shells:?} out of the completion help"
    );

    for shell in shells {
        support::bin_without_credentials()
            .args(["completion", shell])
            .assert()
            .success()
            .stdout(predicate::str::contains("flute2"));
    }
}

/// Offline commands must not touch the keychain or the network.
#[test]
fn completion_and_version_work_without_credentials() {
    for args in [vec!["completion", "bash"], vec!["version"]] {
        support::bin_without_credentials()
            .args(&args)
            .assert()
            .success();
    }
}

#[test]
fn version_reports_the_crate_version_in_every_output_mode() {
    for mode in ["table", "json", "quiet"] {
        support::bin_without_credentials()
            .args(["--output", mode, "version"])
            .assert()
            .success()
            .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
    }
}

/// Tracing must never contaminate the data stream.
#[tokio::test]
async fn debug_traces_go_to_stderr_leaving_stdout_parseable() {
    let server = support::mock_with_token().await;
    let out = support::bin(&server)
        .args(["--debug", "--output", "json", "version"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice::<serde_json::Value>(&out)
        .expect("stdout stayed parseable under --debug");
}

/// The update notice must never contaminate stdout, and must never fail the
/// command it followed. Under `assert_cmd` stderr is a pipe, so the check is
/// skipped at the call site — this asserts the *outcome* holds either way.
#[test]
fn a_command_succeeds_and_prints_no_notice_to_stdout() {
    let out = support::bin_without_credentials()
        .args(["--output", "json", "version"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    // Parseable, so nothing was appended to the data stream.
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["object"], "version");
}

/// `update` reaches GitHub, so the install itself is not driven here. What is
/// checkable without a network is that the command exists and that its
/// user-facing text names v2 rather than v1 — the failure mode being a
/// well-formed request to the wrong project.
#[test]
fn update_is_reachable_and_names_the_v2_binary() {
    let out = support::bin_without_credentials()
        .args(["update", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let help = String::from_utf8(out).unwrap();
    assert!(help.contains("flute2"), "{help}");
    assert!(!help.contains("flute-cli\""), "{help}");
}

/// A deterministic 401-then-200 sequence, so the retry is observable from
/// outside the process.
struct UnauthorizedOnce {
    calls: std::sync::atomic::AtomicUsize,
}

impl wiremock::Respond for UnauthorizedOnce {
    fn respond(&self, _: &wiremock::Request) -> wiremock::ResponseTemplate {
        if self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
            wiremock::ResponseTemplate::new(401)
        } else {
            wiremock::ResponseTemplate::new(200)
        }
    }
}

fn unauthorized_once() -> UnauthorizedOnce {
    UnauthorizedOnce {
        calls: std::sync::atomic::AtomicUsize::new(0),
    }
}

/// Tracing is installed for every invocation, so `RUST_LOG` selects what an
/// operator sees without `--debug` — and the token refresh a 401 triggers is
/// an `info`, not a silent retry.
#[tokio::test]
async fn rust_log_surfaces_the_401_retry_without_debug() {
    let server = support::mock_with_token().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/v2/ping"))
        .respond_with(unauthorized_once())
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .env("RUST_LOG", "flute_cli2=info")
        .args(["ping"])
        .assert()
        .success()
        .get_output()
        .clone();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("HTTP 401 — invalidating cached token and retrying once"),
        "{stderr}"
    );
    assert!(
        !stderr.contains("HTTP request"),
        "a debug trace surfaced at info level:\n{stderr}"
    );
}

/// `RUST_LOG` beats the preset `--debug` would otherwise choose, so an
/// operator can raise the level of a command already in flight without
/// changing the command.
#[tokio::test]
async fn rust_log_debug_surfaces_http_traces_without_debug() {
    let server = support::mock_with_token().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/v2/ping"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .env("RUST_LOG", "flute_cli2=debug")
        .args(["ping"])
        .assert()
        .success()
        .get_output()
        .clone();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("HTTP response"),
        "RUST_LOG=flute_cli2=debug did not raise the level:\n{stderr}"
    );
}

/// Both halves of a retried request are traced — a trace that shows only the
/// second response hides the reason there was one — and a trace carries no
/// ANSI escapes, because `--debug` output is read from a file at least as
/// often as from a terminal.
#[tokio::test]
async fn a_retried_request_traces_both_responses_and_no_ansi() {
    let server = support::mock_with_token().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/v2/ping"))
        .respond_with(unauthorized_once())
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .args(["--debug", "ping"])
        .assert()
        .success()
        .get_output()
        .clone();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        stderr.matches("HTTP response").count(),
        2,
        "both the 401 and the retry must be traced:\n{stderr}"
    );
    assert!(
        !stderr.contains('\u{1b}'),
        "ANSI escapes in a trace:\n{stderr}"
    );
}

/// `completion` output is piped straight into a shell, so nothing may follow
/// it — the update notice is skipped for the command outright.
#[test]
fn completion_prints_a_script_and_nothing_beside_it() {
    let out = support::bin_without_credentials()
        .args(["completion", "bash"])
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(
        String::from_utf8_lossy(&out.stderr).is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// **Colour is spent only where something can show it.** A log line is read
/// from a file at least as often as from a terminal, and escape sequences in
/// a redirected log are noise a grep has to work around.
#[tokio::test]
async fn a_log_line_on_a_redirected_stderr_carries_no_colour() {
    let server = support::mock_with_token().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/v2/ping"))
        .respond_with(unauthorized_once())
        .mount(&server)
        .await;

    let out = support::bin(&server)
        .env("RUST_LOG", "flute_cli2=info")
        .args(["ping"])
        .assert()
        .success()
        .get_output()
        .clone();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("invalidating cached token"),
        "the scenario logged nothing, so it proves nothing:\n{stderr}"
    );
    assert!(
        !stderr.contains('\u{1b}'),
        "an escape sequence reached a stderr nobody is looking at: {stderr:?}"
    );
}
