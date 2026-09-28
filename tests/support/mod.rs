//! Shared harness for the binary-level tests.
//!
//! Each test crate compiles its own copy of this module and uses a different
//! subset of it, so unused items here are expected rather than dead.
#![allow(dead_code)]

pub mod checks;
pub mod contracts;
pub mod parity;
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
/// what conformance checked. It also satisfies the coverage gate, which
/// requires each command test to name its operation id.
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

/// Every `.rs` source under `tests/`, concatenated.
///
/// Cargo runs a test binary with the package root as its working directory, so
/// the relative path resolves. Scanning the wrong directory would report full
/// coverage forever, which is why a negative control asserts a made-up name is
/// *not* found.
fn test_sources() -> &'static str {
    static SOURCES: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    SOURCES.get_or_init(|| {
        let mut buf = String::new();
        collect_rs(std::path::Path::new("tests"), &mut buf);
        assert!(
            !buf.is_empty(),
            "no test sources found under tests/; the scan is looking in the \
             wrong place and every coverage claim built on it is vacuous"
        );
        buf
    })
}

fn collect_rs(dir: &std::path::Path, buf: &mut String) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs(&path, buf);
        } else if path.extension().is_some_and(|e| e == "rs") {
            if let Ok(text) = std::fs::read_to_string(&path) {
                buf.push_str(&text);
                buf.push('\n');
            }
        }
    }
}

/// The same text with every comment, string literal and char literal blanked
/// to spaces.
///
/// A definition never lives inside a comment or a quoted string, so searching
/// the raw text lets a name that is merely *mentioned* satisfy a row claiming
/// a test. Blanking rather than deleting keeps the two texts byte-for-byte
/// aligned, which is what lets a body located here be read back in full from
/// the original — the citation scan needs the string literals it finds there.
fn scannable_sources() -> &'static str {
    static BLANKED: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    BLANKED.get_or_init(|| blank_comments_and_literals(test_sources()))
}

/// Rust's four literal forms plus its two comment forms, replaced by spaces.
///
/// Newlines survive so the attribute window above a `fn` is still line-shaped,
/// and the byte length is unchanged so offsets stay usable against the
/// original. Block comments nest, as they do in the language.
fn blank_comments_and_literals(src: &str) -> String {
    let b = src.as_bytes();
    let mut out = b.to_vec();
    fn blank(out: &mut [u8], at: usize) {
        if out[at] != b'\n' {
            out[at] = b' ';
        }
    }
    let ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let mut i = 0usize;
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    blank(&mut out, i);
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                let mut depth = 0usize;
                while i < b.len() {
                    if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
                        depth += 1;
                    } else if b[i] == b'*' && b.get(i + 1) == Some(&b'/') {
                        depth -= 1;
                        blank(&mut out, i);
                        blank(&mut out, i + 1);
                        i += 2;
                        if depth == 0 {
                            break;
                        }
                        continue;
                    }
                    blank(&mut out, i);
                    i += 1;
                }
            }
            b'"' => {
                // A raw string is `r`, then any number of `#`, then the quote;
                // counting the hashes backwards is what tells the closer how
                // many to expect. `b` may prefix either form.
                let mut hash_start = i;
                while hash_start > 0 && b[hash_start - 1] == b'#' {
                    hash_start -= 1;
                }
                let mut raw_start = None;
                if hash_start > 0 && b[hash_start - 1] == b'r' {
                    let before = hash_start - 1;
                    let byte_prefixed = before > 0 && b[before - 1] == b'b';
                    let outside = if byte_prefixed { before - 1 } else { before };
                    if outside == 0 || !ident(b[outside - 1]) {
                        raw_start = Some(outside);
                    }
                }
                let hashes = i - hash_start;
                match raw_start {
                    Some(from) => {
                        for at in from..=i {
                            blank(&mut out, at);
                        }
                        i += 1;
                        loop {
                            if i >= b.len() {
                                break;
                            }
                            let closes = b[i] == b'"'
                                && b[i + 1..]
                                    .iter()
                                    .take(hashes)
                                    .filter(|c| **c == b'#')
                                    .count()
                                    == hashes;
                            blank(&mut out, i);
                            i += 1;
                            if closes {
                                for _ in 0..hashes {
                                    blank(&mut out, i);
                                    i += 1;
                                }
                                break;
                            }
                        }
                    }
                    None => {
                        blank(&mut out, i);
                        i += 1;
                        while i < b.len() && b[i] != b'"' {
                            if b[i] == b'\\' {
                                blank(&mut out, i);
                                i += 1;
                            }
                            blank(&mut out, i);
                            i += 1;
                        }
                        if i < b.len() {
                            blank(&mut out, i);
                        }
                        i += 1;
                    }
                }
            }
            // A char literal, not a lifetime: `'{'` must not open a block. An
            // escape puts the closing quote past the escaped character, so
            // `'\''` is one literal rather than an empty one and a stray quote.
            b'\'' if b.get(i + 1) == Some(&b'\\') => {
                let end = b
                    .get(i + 3..)
                    .and_then(|rest| rest.iter().position(|c| *c == b'\''))
                    .map_or(b.len() - 1, |n| i + 3 + n);
                for at in i..=end {
                    blank(&mut out, at);
                }
                i = end + 1;
            }
            b'\'' if b.get(i + 2) == Some(&b'\'') => {
                for at in i..=i + 2 {
                    blank(&mut out, at);
                }
                i += 3;
            }
            _ => i += 1,
        }
    }
    String::from_utf8(out).expect("blanking replaces whole bytes, never parts of one")
}

/// One `#[test]` or `#[tokio::test]` function the scan found.
struct TestFn {
    /// The brace-delimited body, in offsets both source texts share.
    body: std::ops::Range<usize>,
    /// `#[ignore]`d, so a plain `cargo test` skips it.
    ignored: bool,
}

/// Every test function under `tests/` carrying this name.
///
/// The attribute requirement is what separates a test from a helper: without
/// it any function in the tree — `sequence`, `client_for` — satisfies a row
/// that claims an operation is covered.
fn test_fns(name: &str) -> Vec<TestFn> {
    let haystack = scannable_sources();
    let needle = format!("fn {name}");
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = haystack[from..].find(&needle) {
        let at = from + rel;
        from = at + needle.len();
        // `fn foo_bar` must not match a request for `fn foo`.
        if haystack[from..]
            .chars()
            .next()
            .is_some_and(|c| c.is_alphanumeric() || c == '_')
        {
            continue;
        }
        let attributes = attribute_window(haystack, at);
        if !attributes.contains("#[test]") && !attributes.contains("#[tokio::test") {
            continue;
        }
        if let Some(body) = brace_body(haystack, at) {
            out.push(TestFn {
                body,
                ignored: attributes.contains("#[ignore"),
            });
        }
    }
    out
}

/// The attributes written above the line `at` sits on.
///
/// The window ends at the first line that is neither blank nor an attribute,
/// which is the previous item — so attributes belonging to one function can
/// never be read as another's. Doc comments arrive here already blanked, so
/// they read as the empty lines they are for this purpose.
fn attribute_window(text: &str, at: usize) -> &str {
    let end = text[..at].rfind('\n').map_or(0, |n| n + 1);
    let mut start = end;
    while start > 0 {
        let line_start = text[..start - 1].rfind('\n').map_or(0, |n| n + 1);
        let line = text[line_start..start - 1].trim();
        if !line.is_empty() && !line.starts_with('#') {
            break;
        }
        start = line_start;
    }
    &text[start..end]
}

/// Does a test function with this name exist anywhere under `tests/`?
pub fn test_fn_exists(name: &str) -> bool {
    test_fn_definitions(name) > 0
}

/// How many test functions under `tests/` carry this name.
///
/// A row naming a test defined twice is ambiguous, and the scan would
/// otherwise pick whichever came first in directory order — so a row could be
/// satisfied by a same-named test in another group's file that exercises a
/// different operation. Callers assert this is exactly 1.
pub fn test_fn_definitions(name: &str) -> usize {
    test_fns(name).len()
}

/// The same, counting only the ones a plain `cargo test` runs.
///
/// An `#[ignore]`d function is a scenario somebody opts into by hand. A row
/// that claims continuous coverage has to name one that runs; the live rows
/// are the deliberate exception and keep the counter above.
pub fn runnable_test_fn_definitions(name: &str) -> usize {
    test_fns(name).iter().filter(|f| !f.ignored).count()
}

/// Does that function drive a mock from `operation_id`?
///
/// The id has to sit in a `contracts::exchange(` or `support::mount(` argument
/// list. Anywhere else in the body — a comment, a string the harness never
/// reads, an id handed to a hand-built fixture — is a mention, and a mention
/// is not evidence that the test exercised the operation.
pub fn test_fn_cites(name: &str, operation_id: &str) -> bool {
    let source = test_sources();
    let found = test_fns(name);
    !found.is_empty()
        && found.iter().all(|f| {
            mock_call_arguments(&source[f.body.clone()])
                .iter()
                .any(|args| args.contains(operation_id))
        })
}

/// The argument list of every call that drives a mock from the matrix.
fn mock_call_arguments(body: &str) -> Vec<&str> {
    let mut out = Vec::new();
    for opener in ["contracts::exchange(", "support::mount("] {
        let mut from = 0usize;
        while let Some(rel) = body[from..].find(opener) {
            let open = from + rel + opener.len();
            from = open;
            let mut depth = 1usize;
            for (offset, c) in body[open..].char_indices() {
                match c {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            out.push(&body[open..open + offset]);
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    out
}

/// The brace-delimited block that opens after the function signature at `at`.
///
/// Its input is `scannable_sources`, where no brace can hide in a literal or a
/// comment. A `}` before the block opens means this occurrence is not a
/// definition, so it yields `None` rather than underflowing the depth counter.
fn brace_body(text: &str, at: usize) -> Option<std::ops::Range<usize>> {
    let mut depth = 0usize;
    let mut start = None;
    for (i, byte) in text.bytes().enumerate().skip(at) {
        match byte {
            b'{' => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            b'}' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
                if depth == 0 {
                    return Some(start?..i + 1);
                }
            }
            _ => {}
        }
    }
    None
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

/// Every `.rs` file under `src/groups/`, paired with the command group it
/// belongs to: its file stem at the top level, or its directory's name for a
/// group split into a module directory.
///
/// Recursive, because a scan of the top level alone silently skips a group
/// whose code lives in `src/groups/<group>/`.
pub fn group_sources() -> Vec<(String, std::path::PathBuf)> {
    fn walk(
        dir: &std::path::Path,
        group: Option<&str>,
        out: &mut Vec<(String, std::path::PathBuf)>,
    ) {
        for entry in std::fs::read_dir(dir).expect("src/groups is readable") {
            let path = entry.expect("a directory entry").path();
            let stem = path.file_stem().unwrap().to_string_lossy().to_string();
            if path.is_dir() {
                walk(&path, Some(group.unwrap_or(&stem)), out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push((group.unwrap_or(&stem).to_string(), path));
            }
        }
    }
    let mut out = Vec::new();
    walk(std::path::Path::new("src/groups"), None, &mut out);
    out
}
