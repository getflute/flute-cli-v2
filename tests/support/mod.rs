//! Shared harness for the binary-level tests.
//!
//! Each test crate compiles its own copy of this module and uses a different
//! subset of it, so unused items here are expected rather than dead.
#![allow(dead_code)]

pub mod checks;
pub mod contracts;
pub mod spec;
pub mod surface;

use spec::Exchange;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Mount the token endpoint so any test can obtain a bearer without the
/// keychain.
///
/// The form body is not matched here on purpose: a matcher that rejects the
/// request answers 404, and every command test would then report a wrong
/// bearer request as a failure of its own operation.
/// `the_token_exchange_matches_the_contract` is where the exchange is asserted
/// as an exact multiset, against the same fixture this serves.
pub async fn mock_with_token() -> MockServer {
    let server = MockServer::start().await;
    let ex = contracts::exchange("get-oauth-token", "client credentials");
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .respond_with(
            ResponseTemplate::new(ex.response.status).set_body_json(
                ex.response
                    .body
                    .expect("the token fixture answers with a body"),
            ),
        )
        .mount(&server)
        .await;
    server
}

/// A `flute2` invocation aimed at `server`, authenticated from the environment.
pub fn bin(server: &MockServer) -> assert_cmd::Command {
    let mut c = raw_bin();
    c.env("FLUTE2_API_BASE_URL", server.uri())
        .env("FLUTE2_OAUTH_URL", format!("{}/oauth2/token", server.uri()))
        .env("FLUTE2_CLIENT_ID", "test-id")
        .env("FLUTE2_CLIENT_SECRET", "test-secret");
    assert_cmd::Command::from_std(c)
}

/// A binary with no credentials at all — and no access to the developer's
/// keychain either. Without the keychain seam this would pass on a clean
/// machine and behave differently on a logged-in one.
pub fn bin_without_credentials() -> assert_cmd::Command {
    assert_cmd::Command::from_std(raw_bin_without_credentials())
}

/// The same, over a config directory the caller owns, for a test whose second
/// invocation has to read what its first one wrote.
pub fn bin_without_credentials_in(home: &std::path::Path) -> assert_cmd::Command {
    let mut c = raw_bin_without_credentials();
    point_at_home(&mut c, home);
    assert_cmd::Command::from_std(c)
}

/// The same isolated invocation as a plain `std::process::Command`, for a test
/// that drives the child's pipes itself rather than waiting for its output.
pub fn raw_bin_without_credentials() -> std::process::Command {
    let mut c = raw_bin();
    c.env_remove("FLUTE2_CLIENT_ID")
        .env_remove("FLUTE2_CLIENT_SECRET");
    c
}

fn raw_bin() -> std::process::Command {
    let mut c = std::process::Command::new(assert_cmd::cargo::cargo_bin("flute2"));
    isolate(&mut c);
    c
}

/// The three names a config directory is looked up under, set together: one
/// left pointing at the developer's own directory undoes the other two.
fn point_at_home(c: &mut std::process::Command, home: &std::path::Path) {
    c.env("HOME", home)
        .env("XDG_CONFIG_HOME", home)
        .env("USERPROFILE", home);
}

/// Cut every binary test off from the developer's machine.
///
/// Without this a test reads `~/.flute2/config.toml`: a contributor whose
/// config sets `default_profile = "production"` trips the deliberate
/// production-plus-override refusal and every test in the suite fails on their
/// machine alone, and a config `output = "json"` silently changes what
/// table-mode assertions see. The keychain is cut off for the same reason — a
/// "no credentials" test is only meaningful when there are none.
///
/// Both endpoints default to a port that refuses immediately. A command that
/// should not have issued a request then fails on connection refused instead
/// of reaching the real sandbox, which is the difference between a failing
/// test and a hermeticity hole.
fn isolate(c: &mut std::process::Command) {
    static HOME: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
    let home = HOME.get_or_init(|| tempfile::tempdir().unwrap());
    point_at_home(c, home.path());
    c.env("FLUTE2_PROFILE", "sandbox")
        .env("FLUTE2_API_BASE_URL", "http://127.0.0.1:1")
        .env("FLUTE2_OAUTH_URL", "http://127.0.0.1:1/oauth2/token")
        .env("FLUTE2_NO_KEYCHAIN", "1")
        .env("FLUTE2_NO_UPDATE_CHECK", "1")
        .env_remove("FLUTE2_OUTPUT")
        .env("CI", "1");
    // A `FLUTE_` name set with no `FLUTE2_` counterpart is noted on stderr, so
    // a contributor whose shell carries one would see that note inside every
    // stderr assertion in the suite.
    for suffix in [
        "PROFILE",
        "OUTPUT",
        "CLIENT_ID",
        "CLIENT_SECRET",
        "NO_UPDATE_CHECK",
        "GITHUB_TOKEN",
    ] {
        c.env_remove(format!("FLUTE_{suffix}"));
    }
}

/// Mount a contract exchange on the mock server and hand it back.
///
/// **Every command test uses this.** It is the only way the asserted bytes and
/// the conformance-validated bytes stay one value: the matrix supplies both the
/// request matcher and the response, so a hand-written mock cannot drift from
/// what conformance checked. An unknown operation or variant panics.
///
/// **Mounting is routing, not assertion.** It matches the method and the path
/// and nothing else, so that `assert_exchange_observed` is the one place a
/// request is judged. A matcher that also weighed the query and the body would
/// answer 404 on any mismatch, the command would exit 4, and the test would
/// fail on `.success()` reporting "API 404" — burying the differing bytes the
/// diff was written to print.
///
/// Pair every `mount` with `assert_exchange_observed`. Two exchanges in one
/// test that share a method and path would need a distinguishing matcher to be
/// told apart; no test mounts such a pair.
pub async fn mount(server: &MockServer, operation_id: &str, variant: &str) -> Exchange {
    let ex = contracts::exchange(operation_id, variant);
    let m = Mock::given(method(ex.request.method.as_str())).and(path(ex.request.path.as_str()));
    let mut tpl = ResponseTemplate::new(ex.response.status);
    if let Some(body) = ex.response.body.as_ref() {
        tpl = tpl.set_body_json(body.clone());
    }
    m.respond_with(tpl).mount(server).await;
    ex
}

/// Assert that what the binary *actually sent* is the whole exchange, not
/// merely a superset of it.
///
/// Layer 3 claims to catch a wrong query and a wrong content type, and
/// mounting cannot: an extra `?merchantId=` the API silently ignores, a
/// duplicated `pageSize`, or a form-encoded body where JSON was declared would
/// all pass the mock and reach production. The recorded request is the only
/// place those are visible.
///
/// The token request is excluded — it is the harness's, not the command's.
pub async fn assert_exchange_observed(server: &MockServer, ex: &Exchange) {
    let reqs = server.received_requests().await.unwrap();
    let sent: Vec<_> = reqs
        .iter()
        .filter(|r| r.url.path() != "/oauth2/token")
        .filter(|r| {
            r.url.path() == ex.request.path
                && r.method.as_str().eq_ignore_ascii_case(&ex.request.method)
        })
        .collect();
    assert_eq!(
        sent.len(),
        1,
        "expected exactly one {} {}, saw {}. All requests: {:?}",
        ex.request.method,
        ex.request.path,
        sent.len(),
        reqs.iter()
            .map(|r| format!("{} {}", r.method, r.url.path()))
            .collect::<Vec<_>>()
    );
    let req = sent[0];

    // Multisets, sorted: this rejects an extra parameter and a duplicated one,
    // which a per-pair matcher accepts.
    let mut observed: Vec<(String, String)> = req
        .url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    let mut expected = ex.request.query.clone();
    observed.sort();
    expected.sort();
    assert_eq!(
        observed, expected,
        "query differs from the contract fixture"
    );

    let observed_ct = req.headers.get("content-type").map(|v| {
        v.to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .trim()
            .to_string()
    });
    // A bodyless POST/PUT/PATCH still sends an empty frame, and the frame
    // declares its type: the API answers 415 to one that does not.
    let declares_a_frame = matches!(
        ex.request.method.to_ascii_uppercase().as_str(),
        "POST" | "PUT" | "PATCH"
    );
    let expected_ct = match (ex.request.body.as_ref(), declares_a_frame) {
        (Some(_), _) => Some(
            ex.request
                .content_type
                .clone()
                .unwrap_or("application/json".into()),
        ),
        (None, true) => Some("application/json".into()),
        (None, false) => None,
    };
    assert_eq!(
        observed_ct, expected_ct,
        "content type differs from the contract fixture"
    );

    assert_body_matches(&req.body, ex.request.body.as_ref());
}

/// Judge the body a request carried against the one its fixture declares.
///
/// A fixture that declares none says the command sends none, so anything on
/// the wire there is bytes no layer has looked at.
pub fn assert_body_matches(body: &[u8], expected: Option<&serde_json::Value>) {
    match expected {
        Some(expected) => {
            let actual: serde_json::Value =
                serde_json::from_slice(body).expect("request body was not JSON");
            assert_eq!(&actual, expected, "request body");
        }
        None => assert!(
            body.is_empty(),
            "sent a body the contract fixture does not declare: {}",
            String::from_utf8_lossy(body)
        ),
    }
}

/// The same assertion for the one exchange whose body is form-encoded.
///
/// The pairs are compared as a sorted multiset, which is what rejects a
/// dropped, duplicated or extra parameter — a per-pair matcher accepts all
/// three. The fixture's body is a JSON object because that is what the matrix
/// holds; each value is a form field, so any non-string would not survive the
/// encoding and is refused here rather than compared as something else.
pub async fn assert_form_exchange_observed(server: &MockServer, ex: &Exchange) {
    let reqs = server.received_requests().await.unwrap();
    let sent: Vec<_> = reqs
        .iter()
        .filter(|r| {
            r.url.path() == ex.request.path
                && r.method.as_str().eq_ignore_ascii_case(&ex.request.method)
        })
        .collect();
    assert_eq!(
        sent.len(),
        1,
        "expected exactly one {} {}, saw {}",
        ex.request.method,
        ex.request.path,
        sent.len()
    );
    let req = sent[0];

    let observed_ct = req
        .headers
        .get("content-type")
        .map(|v| v.to_str().unwrap().split(';').next().unwrap().trim().into());
    assert_eq!(
        observed_ct, ex.request.content_type,
        "content type differs from the contract fixture"
    );

    let mut observed: Vec<(String, String)> = url::form_urlencoded::parse(&req.body)
        .into_owned()
        .collect();
    let mut expected: Vec<(String, String)> = ex
        .request
        .body
        .as_ref()
        .and_then(serde_json::Value::as_object)
        .expect("the form fixture is a JSON object")
        .iter()
        .map(|(k, v)| {
            let text = v.as_str().expect("a form field is a string");
            (k.clone(), text.to_string())
        })
        .collect();
    observed.sort();
    expected.sort();
    assert_eq!(observed, expected, "form body differs from the contract");
}

/// `--help` for the command a mapped operation is reached through.
///
/// The mapping is a command *line*, so `payment-methods set-default` has to be
/// split into arguments rather than passed as one.
pub fn help_for_operation(operation_id: &str) -> String {
    let contract = contracts::CONTRACTS
        .iter()
        .find(|c| c.operation_id == operation_id)
        .unwrap_or_else(|| panic!("no contract row for {operation_id}"));
    let contracts::Mapping::Command(command) = &contract.mapping else {
        panic!("{operation_id} is not mapped to a command")
    };
    let mut cmd = raw_bin();
    for word in command.split_whitespace() {
        cmd.arg(word);
    }
    let out = cmd.arg("--help").output().expect("running --help");
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}
