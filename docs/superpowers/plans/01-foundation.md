# Phase 0 — Foundation

> Part of the [flute-cli-v2 implementation plan](00-overview.md). Read [`00-overview.md`](00-overview.md) first — its global constraints and testing contract apply to every task here.

# Phase 0 — Foundation

Nothing in this phase calls the API. It exists so that Phase 1 can be written test-first.

### Task 1: Scaffold the crate and lock the toolchain

**Files:**
- Create: `Cargo.toml`, `src/main.rs`, `src/lib.rs`, `rust-toolchain.toml`, `.gitignore`, `LICENSE`
- Test: `tests/smoke.rs`

**Interfaces:**
- Produces: `flute_cli2::run() -> anyhow::Result<()>`, binary `flute2`.

- [ ] **Step 1: Write the failing test**

```rust
// tests/smoke.rs
use assert_cmd::Command;

#[test]
fn version_flag_prints_crate_version() {
    Command::cargo_bin("flute2")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicates::str::contains("2.0.0"));
}

/// The exit-code boundary is reachable without spawning a binary, which is
/// what makes every later exit-code assertion cheap.
#[tokio::test]
async fn main_entry_returns_success_for_version() {
    assert_eq!(flute_cli2::main_entry(["flute2", "--version"]).await,
               std::process::ExitCode::SUCCESS);
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test --test smoke`
Expected: FAIL — no such binary `flute2`.

- [ ] **Step 3: Write `Cargo.toml`**

```toml
[package]
name = "flute2"
version = "2.0.0"
edition = "2024"
rust-version = "1.85"
license = "MIT"
description = "CLI for the Flute payments platform, v2 API"
repository = "https://github.com/getflute/flute-cli-v2"

[[bin]]
name = "flute2"
path = "src/main.rs"

[lib]
name = "flute_cli2"
path = "src/lib.rs"

[dependencies]
anyhow = "1"
async-trait = "0.1"
axoupdater = { version = "0.10.0", default-features = false, features = ["tokio", "github_releases"] }
chrono = { version = "0.4", features = ["serde"] }
clap = { version = "4", features = ["derive", "env"] }
clap_complete = "4"
dirs = "5"
keyring = { version = "3", features = ["apple-native", "windows-native", "linux-native-sync-persistent"] }
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
rpassword = "7"
rust_decimal = "1"
serde = { version = "1", features = ["derive"] }
serde_json = { version = "1", features = ["arbitrary_precision"] }
thiserror = "2"
tokio = { version = "1", features = ["macros", "rt-multi-thread", "sync", "time", "signal"] }
toml = "0.8"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
url = "2"

[dev-dependencies]
assert_cmd = "2"
insta = { version = "1", features = ["json"] }
jsonschema = "0.26"
predicates = "3"
pretty_assertions = "1"
sha2 = "0.10"
temp-env = "0.3"
tempfile = "3"
tokio = { version = "1", features = ["macros", "rt-multi-thread", "sync", "time", "test-util"] }
wiremock = "0.6"

[profile.dist]
inherits = "release"
lto = "thin"
```

This is v1's dependency set with three additions, each justified by a test layer: `insta` (layer 7 snapshots), `jsonschema` (layer 2 conformance), and `sha2` (the vendored-bundle hash gate). Nothing v1 relies on is dropped — v1's dev-dependency block also re-declares `reqwest`, `serde_json`, `tracing`, and `tracing-subscriber`, which are already normal dependencies, so repeating them there buys nothing. `async-trait` is required by the transplanted `token.rs`; `rpassword` by `auth login`; `axoupdater` by `update` and the update check; `url` and `chrono` by query building and date handling.

Two dependency choices are load-bearing and must not be "tidied":

- **`serde_json`'s `arbitrary_precision`** is what makes an exact amount possible. v1 parses `Decimal::to_string()` into a `serde_json::Number`, so `"100.00"` serializes back as `100.00` — not `100`, not `100.0`, and never a float artifact like `0.10000000000000001`.
- **`rust_decimal` carries no serde feature, deliberately.** Amounts never go through `#[derive(Serialize)]` on a `Decimal`; they go through the `to_amount_number` helper. Enabling `serde-with-float` would route them through `f64` and silently destroy the precision the whole design protects.

- [ ] **Step 4: Write the entry points**

```rust
// src/main.rs
#![forbid(unsafe_code)]

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    flute_cli2::main_entry(std::env::args_os()).await
}
```

```rust
// src/lib.rs
#![forbid(unsafe_code)]

use clap::Parser;
use std::ffi::OsString;
use std::process::ExitCode;

#[derive(Parser, Debug)]
#[command(name = "flute2", version, about = "CLI for the Flute payments platform")]
pub struct Cli {}

/// The single place an error becomes output and an exit code.
///
/// Library code returns `Result` and never calls `process::exit`: an exit
/// buried in a dispatch arm cannot be unit-tested and skips destructors.
/// `run` decides *what happened*; `main_entry` decides *how to say it and what
/// to exit with*, which is also why a parse failure can be rendered as a JSON
/// envelope — the output mode is known here.
pub async fn main_entry<I, T>(args: I) -> ExitCode
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    match Cli::try_parse_from(args) {
        Ok(cli) => match run(cli).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => report(&e),
        },
        Err(e) => report_clap(&e),
    }
}

pub async fn run(_cli: Cli) -> anyhow::Result<()> {
    Ok(())
}
```

Three consequences the rest of the plan depends on:

- **`ExitCode` carries every code except 130.** `ExitCode::from(u8)` covers 0–4. Ctrl-C during a `pos create --wait` poll is the one case still needing `process::exit(130)`, because the value must escape a signal handler; that exception is documented where it happens rather than becoming a general licence.
- **`try_parse_from`, not `parse`.** `parse` exits the process itself, which would put a clap error outside the one place that knows the output mode — and rendering usage errors as a `{"kind":"client"}` envelope on stdout is part of the contract.
- **`main_entry` takes its arguments**, so tests drive it with a vector and reach the exit-code and envelope logic without spawning a binary.

- [ ] **Step 5: Write `rust-toolchain.toml` and `.gitignore`**

```toml
# rust-toolchain.toml
[toolchain]
channel = "1.85"
components = ["rustfmt", "clippy"]
```

```
# .gitignore
/target
CLAUDE.md
NOTES.local.md
.flute2-live.env
```

**`Cargo.lock` is committed.** This crate ships a binary, and a binary's build should be reproducible: without the lockfile, CI and a release build can resolve different versions of `reqwest`, `keyring`, or `rust_decimal` than anyone tested. v1 commits its lockfile too.

The live tests are **committed** — only the file holding sandbox identifiers is ignored. Uncommitted tests rot, and the record of what the API actually accepts would live on one machine.

- [ ] **Step 6: Confirm the documentation filenames are lowercase**

```bash
git ls-files | grep -i '^readme'
```

Expected: exactly one entry, `readme.md`. The branch already tracks the lowercase name, so there is nothing to rename.

The rule this checks still applies to `agents.md` in Task 28: documentation filenames are lowercase, matching v1 and the design's document table. Two spellings of one name collide silently on a case-insensitive filesystem and produce two files on a case-sensitive one. Should a capitalised name ever reappear, git on macOS will not see a case-only change as a rename, so it takes two steps through a temporary name.

- [ ] **Step 7: Add the MIT `LICENSE` file**

v1 claims MIT in its readme but ships no LICENSE file, so its licence is not detected and the claim is unenforceable. Use the standard MIT text with `Copyright (c) 2026 Flute`.

- [ ] **Step 8: Run the test and the full gate**

Run: `cargo test && cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "ARISE-4928 build(cli): scaffold flute2 crate"
```

---

### Task 2: Transplant money, and prove decimals never become floats

**Files:**
- Create: `src/cli/mod.rs`, `src/cli/money.rs`
- Source: **two** v1 locations — `src/cli/money.rs` (the parsers) and `to_amount_number` from `src/api/models/mod.rs`. v2 has no `api/models`, so the helper joins the parsers it belongs with.

**Interfaces:**
- Produces: `parse_amount(&str) -> anyhow::Result<Decimal>`, `parse_rate(&str) -> anyhow::Result<Decimal>`, `to_amount_number(Decimal) -> serde_json::Result<serde_json::Value>`.

- [ ] **Step 1: Write the failing tests**

These are v1's rules, not a fresh invention. v1 rejects each case explicitly so the policy is visible in code rather than an accident of the library's parsing.

```rust
// in src/cli/money.rs
#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    #[test]
    fn rejects_more_than_two_decimal_places() {
        assert!(parse_amount("10.001").is_err());
        assert!(parse_amount("10.00").is_ok());
    }

    #[test]
    fn accepts_an_integer_amount() {
        assert_eq!(parse_amount("100").unwrap(), Decimal::from_str("100").unwrap());
    }

    #[test]
    fn rejects_negative_amounts() {
        assert!(parse_amount("-5.00").is_err());
    }

    /// The API expects a plain decimal string, so these are refused up front
    /// rather than left to whatever Decimal's parser happens to accept.
    #[test]
    fn rejects_scientific_notation_and_leading_plus() {
        assert!(parse_amount("1e2").is_err());
        assert!(parse_amount("1E2").is_err());
        assert!(parse_amount("+5.00").is_err());
    }

    #[test]
    fn rejects_non_numeric_and_empty() {
        assert!(parse_amount("abc").is_err());
        assert!(parse_amount("").is_err());
    }

    /// Rates are not money: sub-cent precision is legitimate, so they allow
    /// four decimal places where amounts allow two.
    #[test]
    fn rate_allows_four_decimal_places_but_not_five() {
        assert!(parse_rate("0.1850").is_ok());
        assert!(parse_rate("0.18505").is_err());
        assert!(parse_rate("-0.1").is_err());
    }

    /// The API declares baseAmount as number/double. Going through f64 would
    /// make 1234567.89 unrepresentable; it must stay exact.
    #[test]
    fn serializes_as_exact_json_number_not_float() {
        let d = Decimal::from_str("1234567.89").unwrap();
        let v = to_amount_number(d).unwrap();
        assert!(v.is_number());
        assert_eq!(serde_json::to_string(&v).unwrap(), "1234567.89");
    }

    #[test]
    fn preserves_trailing_zero_scale() {
        let d = parse_amount("10.50").unwrap();
        assert_eq!(serde_json::to_string(&to_amount_number(d).unwrap()).unwrap(), "10.50");
    }

    /// The classic float artifact: 0.10 must not become 0.10000000000000001.
    #[test]
    fn no_float_artifact_on_one_tenth() {
        let d = parse_amount("0.10").unwrap();
        assert_eq!(serde_json::to_string(&to_amount_number(d).unwrap()).unwrap(), "0.10");
    }
}
```

- [ ] **Step 2: Run and watch them fail**

Run: `cargo test money`
Expected: FAIL — `parse_amount` not found.

- [ ] **Step 3: Transplant the implementation from v1**

Copy v1's `src/cli/money.rs` (both parsers) and `to_amount_number` from v1's `src/api/models/mod.rs` into one module. Rewrite every comment to house style — strip ticket references and change history, keep only the *why* (the precision invariant and why each rejected form is rejected). Add `pub mod money;` to `src/cli/mod.rs`.

- [ ] **Step 4: Run and watch them pass**

Run: `cargo test money`
Expected: PASS, 9 tests.

- [ ] **Step 5: Commit**

```bash
git add src/cli
git commit -m "ARISE-4928 feat(cli): transplant decimal money parsing"
```

---

### Task 3: Transplant redaction as its own module

Security-critical, and in v1 it shares a file with HTTP transport. It gets its own module here.

**Files:**
- Create: `src/api/mod.rs`, `src/api/redact.rs`
- Source: the masking half of v1 `src/api/client/mod.rs`

**Interfaces:**
- Produces: `redact(&str) -> String`.

- [ ] **Step 1: Write the failing tests**

```rust
// in src/api/redact.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_pan_to_last_four() {
        let out = redact(r#"{"cardNumber":"4111111111111111"}"#);
        assert!(!out.contains("4111111111111111"));
        assert!(out.contains("1111"));
    }

    #[test]
    fn removes_security_code_entirely() {
        let out = redact(r#"{"securityCode":"123"}"#);
        assert!(!out.contains("123"));
    }

    #[test]
    fn masks_bearer_token() {
        let out = redact("authorization: Bearer abc.def.ghi");
        assert!(!out.contains("abc.def.ghi"));
    }

    #[test]
    fn masks_client_secret_in_form_body() {
        let out = redact("grant_type=client_credentials&client_secret=s3cr3t");
        assert!(!out.contains("s3cr3t"));
    }

    #[test]
    fn masks_ach_account_number() {
        let out = redact(r#"{"accountNumber":"000123456789"}"#);
        assert!(!out.contains("000123456789"));
    }
}
```

Bearer tokens and client secrets are new here — v1 masks card and account numbers only.

- [ ] **Step 2: Run and watch them fail**

Run: `cargo test redact`
Expected: FAIL — `redact` not found.

- [ ] **Step 3: Transplant and extend**

Lift the masking functions out of v1's client module into `src/api/redact.rs`, then add the two new cases. Prefer explicit parsing over regex; where v1 uses one, keep it only if it is the clearest option and keep it anchored.

- [ ] **Step 4: Run and watch them pass**

Run: `cargo test redact`
Expected: PASS, 5 tests.

- [ ] **Step 5: Commit**

```bash
git add src/api
git commit -m "ARISE-4928 feat(api): split PCI redaction into its own module"
```

---

### Task 4: Transplant the error parser and teach it the v2 envelope

The API returns four distinct shapes against one documented envelope.

**Files:**
- Create: `src/api/error.rs`
- Modify: `src/api/mod.rs`
- Source: v1 `src/api/error.rs` (254 lines)

**Interfaces:**
- Produces: `ApiError` (`Api { status, message, correlation_id }`, `Auth(String)`, `Transport(String)`, `Decode(String)`), `parse_error_body(status: u16, body: &str, www_authenticate: Option<&str>) -> ApiError`.

- [ ] **Step 1: Write the failing tests**

```rust
// in src/api/error.rs
#[cfg(test)]
mod tests {
    use super::*;

    /// v2 replaced v1's `Details` with `Cause` + `Resolution`.
    #[test]
    fn reads_cause_and_resolution_from_pascal_case_envelope() {
        let body = r#"{"StatusCode":400,"Title":"Validation failed",
            "Cause":"baseAmount must be greater than zero",
            "Resolution":"Supply a positive amount",
            "CorrelationId":"c-1","ExceptionType":"ValidationException"}"#;
        let e = parse_error_body(400, body, None);
        let ApiError::Api { message, correlation_id, .. } = e else { panic!() };
        assert!(message.contains("baseAmount must be greater than zero"));
        assert!(message.contains("Supply a positive amount"));
        assert_eq!(correlation_id.as_deref(), Some("c-1"));
    }

    #[test]
    fn flattens_field_error_map_with_camel_case_paths() {
        let body = r#"{"Title":"Validation failed","Errors":{
            "transactionDetails.cardData.paymentMethodDetails.cardNumber":["is required"]}}"#;
        let e = parse_error_body(400, body, None);
        let ApiError::Api { message, .. } = e else { panic!() };
        assert!(message.contains("transactionDetails.cardData.paymentMethodDetails.cardNumber"));
        assert!(message.contains("is required"));
    }

    /// Edge model-validation returns RFC 9110 ProblemDetails with traceId and no correlationId.
    #[test]
    fn falls_back_to_trace_id_when_correlation_id_absent() {
        let body = r#"{"type":"https://tools.ietf.org/html/rfc9110","title":"One or more validation errors occurred.","status":400,"traceId":"00-abc-def-01","errors":{"BaseAmount":["invalid"]}}"#;
        let e = parse_error_body(400, body, None);
        let ApiError::Api { correlation_id, .. } = e else { panic!() };
        assert_eq!(correlation_id.as_deref(), Some("00-abc-def-01"));
    }

    /// The token endpoint speaks OpenIddict, not the API envelope.
    #[test]
    fn parses_openiddict_token_error() {
        let body = r#"{"error":"invalid_client","error_description":"Client authentication failed"}"#;
        let e = parse_error_body(400, body, None);
        let ApiError::Api { message, .. } = e else { panic!() };
        assert!(message.contains("invalid_client"));
        assert!(message.contains("Client authentication failed"));
    }

    /// Some 401s carry no body at all.
    #[test]
    fn uses_www_authenticate_when_body_is_empty() {
        let e = parse_error_body(401, "", Some(r#"Bearer error="invalid_token""#));
        let ApiError::Api { message, .. } = e else { panic!() };
        assert!(message.contains("invalid_token"));
    }
}
```

- [ ] **Step 2: Run and watch them fail**

Run: `cargo test error`
Expected: FAIL — `parse_error_body` not found.

- [ ] **Step 3: Transplant and extend**

v1's parser already tolerates both casings and flattens the `Errors` map, so it covers the first two shapes with edits: read `Cause`/`Resolution` instead of `Details`, fall back to `traceId`, and add the OpenIddict and empty-body arms. Keep v1's `serde(alias = ...)` dual-casing approach.

- [ ] **Step 4: Run and watch them pass**

Run: `cargo test error`
Expected: PASS, 5 tests.

- [ ] **Step 5: Commit**

```bash
git add src/api
git commit -m "ARISE-4928 feat(api): parse the four v2 error shapes"
```

---

### Task 5: Output contract — envelope, error JSON, exit codes

**Files:**
- Create: `src/cli/output.rs`
- Source: v1 `src/cli/output.rs` (329 lines)

**Interfaces:**
- Produces: `OutputFormat {Table, Json, Quiet}`, `OutputFormat::from_config_str`, `Envelope::new(object, data, environment, correlation_id, page_info)`, `ErrorJson::from_anyhow`, `exit_code_for_api(u16) -> i32`, `exit_code_for(&anyhow::Error) -> i32`, `treat_404_as_ok(Result<(), ApiError>) -> anyhow::Result<()>`.

- [ ] **Step 1: Write the failing tests**

```rust
// in src/cli/output.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_map_status_classes() {
        assert_eq!(exit_code_for_api(401), 2);
        assert_eq!(exit_code_for_api(403), 2);
        assert_eq!(exit_code_for_api(400), 3);
        assert_eq!(exit_code_for_api(422), 3);
        assert_eq!(exit_code_for_api(404), 4);
        assert_eq!(exit_code_for_api(500), 1);
    }

    /// v2 introduces these three; they fall through the general arm rather
    /// than claiming new codes.
    #[test]
    fn new_v2_statuses_fall_through_to_general() {
        assert_eq!(exit_code_for_api(402), 1);
        assert_eq!(exit_code_for_api(409), 1);
        assert_eq!(exit_code_for_api(429), 1);
    }

    /// Deleting an already-deleted resource is success: the intent holds.
    #[test]
    fn treat_404_as_ok_maps_404_to_success_and_passes_others() {
        let missing = crate::api::ApiError::Api {
            status: 404, message: "gone".into(), correlation_id: None };
        assert!(treat_404_as_ok(Err(missing)).is_ok());

        let bad = crate::api::ApiError::Api {
            status: 400, message: "bad".into(), correlation_id: None };
        assert!(treat_404_as_ok(Err(bad)).is_err());
    }

    #[test]
    fn envelope_carries_meta_and_page_info() {
        let page = serde_json::json!({
            "pageIndex": 0, "pageSize": 20, "totalItems": 17,
            "totalPages": 1, "hasMore": false });
        let env = Envelope::new("customer", serde_json::json!({"id": "c1"}),
            "sandbox", Some("corr-1".into()), Some(page));
        let v = serde_json::to_value(&env).unwrap();
        assert_eq!(v["object"], "customer");
        assert_eq!(v["meta"]["environment"], "sandbox");
        assert_eq!(v["meta"]["correlation_id"], "corr-1");
        assert_eq!(v["meta"]["page_info"]["hasMore"], false);
    }

    #[test]
    fn error_json_classifies_kinds() {
        let api = anyhow::Error::from(crate::api::ApiError::Api {
            status: 422, message: "bad".into(), correlation_id: Some("x".into()) });
        let e = ErrorJson::from_anyhow(&api);
        assert_eq!(e.kind, "api");
        assert_eq!(e.status, Some(422));
        assert_eq!(e.correlation_id.as_deref(), Some("x"));

        let plain = anyhow::anyhow!("garbage argument");
        assert_eq!(ErrorJson::from_anyhow(&plain).kind, "client");
    }
}
```

- [ ] **Step 2: Run and watch them fail**

Run: `cargo test output`
Expected: FAIL — items not found.

- [ ] **Step 3: Transplant and extend**

Carry v1's `output.rs`, keeping `Envelope<T: Serialize>` generic over its data rather than fixing it to `serde_json::Value`. Three changes:

- `Meta` gains `page_info: Option<Value>`, carrying `#[serde(skip_serializing_if = "Option::is_none")]` exactly as v1's `correlation_id` already does — this is what makes the "writes carry no `page_info`" assertion in Task 11 hold.
- `Envelope::new` grows from v1's four parameters to five, the new one being `page_info`.
- `treat_404_as_ok` moves here from v1's `lib.rs`, because it is part of the exit-code contract rather than part of dispatch.

- [ ] **Step 4: Run and watch them pass**

Run: `cargo test output`
Expected: PASS, 5 tests.

- [ ] **Step 5: Commit**

```bash
git add src/cli
git commit -m "ARISE-4928 feat(cli): output envelope and exit-code contract"
```

---

### Task 6: Config, keychain, token, and the injectable client

The seam the whole test strategy depends on.

**Files:**
- Create: `src/config.rs`, `src/auth/mod.rs`, `src/auth/keychain.rs`, `src/auth/token.rs`, `src/api/client.rs`
- Source: v1 `src/config.rs`, `src/auth/*`, transport half of `src/api/client/mod.rs`

**Interfaces:**
- Produces:
  - `Profile { name, api_base_url, oauth_url }`, `Profile::by_name(&str) -> Option<Profile>`, `Profile::is_production(&self) -> bool`, with these exact constants:

| Profile | `api_base_url` | `oauth_url` |
|---|---|---|
| `sandbox` | `https://sandbox.api.flute.com` | `https://sandbox.oauth.api.flute.com/oauth2/token` |
| `production` | `https://api.flute.com` | `https://oauth.api.flute.com/oauth2/token` |

    The OAuth host is a **different hostname**, not a path on the API host. The bundle's `servers` block says otherwise and is wrong (D7): posting to `https://sandbox.api.flute.com/oauth2/token` returns 404 with an empty body. Do not derive these from the bundle.
  - `Config { default_profile, output, auto_update_check }`, `load_or_default()`, `save(&Config)`
  - `keychain::load_with_env_fallback(profile) -> Result<Option<(String, String)>>`
  - `ApiClient::new(profile: &Profile, creds: (String, String)) -> ApiClient`
  - `ApiClient::request(method, path, query: &[(&str, String)], body: Option<Value>) -> Result<Response, ApiError>`
  - `Response { status: u16, body: Option<Value>, correlation_id: Option<String> }` — `body` is `None` for a bodyless success, which is a valid outcome and not a decode error
  - `Ctx { api: ApiClient, profile: Profile, output: OutputFormat }`

- [ ] **Step 1: Write the failing tests**

```rust
// in src/config.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_dir_is_namespaced_away_from_v1() {
        assert!(config_dir().ends_with(".flute2"));
    }

    #[test]
    fn by_name_resolves_aliases_and_rejects_unknown() {
        assert_eq!(Profile::by_name("prod").unwrap().name, "production");
        assert!(Profile::by_name("staging").is_none());
        assert!(!Profile::by_name("sandbox").unwrap().is_production());
    }

    /// A wrong constant here aims payment traffic at the wrong environment,
    /// and every request would still be perfectly well-formed. Nothing else
    /// in the suite would notice.
    #[test]
    fn hosts_are_exactly_these_four() {
        let s = Profile::by_name("sandbox").unwrap();
        assert_eq!(s.api_base_url, "https://sandbox.api.flute.com");
        assert_eq!(s.oauth_url, "https://sandbox.oauth.api.flute.com/oauth2/token");

        let p = Profile::by_name("production").unwrap();
        assert_eq!(p.api_base_url, "https://api.flute.com");
        assert_eq!(p.oauth_url, "https://oauth.api.flute.com/oauth2/token");
    }

    /// The OAuth host is a separate hostname, not a path on the API host.
    #[test]
    fn oauth_host_is_not_the_api_host() {
        for name in ["sandbox", "production"] {
            let p = Profile::by_name(name).unwrap();
            assert!(!p.oauth_url.starts_with(&p.api_base_url),
                "{name}: oauth_url must not sit under api_base_url");
        }
    }
}
```

```rust
// in src/auth/keychain.rs
#[cfg(test)]
mod tests {
    use super::*;

    /// Env wins over the keychain — the supported path for CI and agents.
    #[test]
    fn env_credentials_take_precedence() {
        temp_env::with_vars(
            [("FLUTE2_CLIENT_ID", Some("env-id")),
             ("FLUTE2_CLIENT_SECRET", Some("env-secret"))],
            || {
                let (id, secret) = creds_from_env().unwrap().unwrap();
                assert_eq!(id, "env-id");
                assert_eq!(secret, "env-secret");
            });
    }

    /// A half-set pair is a misconfiguration, and v1 answers it by silently
    /// falling through to the keychain — so a CI job with a typo'd secret name
    /// authenticates as whoever last logged in on that machine. v2 reports it.
    /// Filed against v1 as V7.
    #[test]
    fn partial_env_credentials_are_an_error_not_a_fallthrough() {
        temp_env::with_vars(
            [("FLUTE2_CLIENT_ID", Some("env-id")),
             ("FLUTE2_CLIENT_SECRET", None::<&str>)],
            || {
                let err = creds_from_env().unwrap_err();
                assert!(err.to_string().contains("FLUTE2_CLIENT_SECRET"));
            });
    }

    /// Neither set is not a misconfiguration; it just means "use the keychain".
    #[test]
    fn absent_env_credentials_fall_through_to_the_keychain() {
        temp_env::with_vars(
            [("FLUTE2_CLIENT_ID", None::<&str>),
             ("FLUTE2_CLIENT_SECRET", None::<&str>)],
            || assert!(creds_from_env().unwrap().is_none()));
    }

    /// v1's names must never be read; two binaries, two credential sets.
    #[test]
    fn v1_env_names_are_not_read() {
        temp_env::with_vars(
            [("FLUTE_CLIENT_ID", Some("v1-id")),
             ("FLUTE_CLIENT_SECRET", Some("v1-secret")),
             ("FLUTE2_CLIENT_ID", None::<&str>),
             ("FLUTE2_CLIENT_SECRET", None::<&str>)],
            || assert!(creds_from_env().unwrap().is_none()));
    }
}
```

```rust
// in src/api/client.rs
#[cfg(test)]
mod tests {
    use super::*;

    /// A test hook able to silently redirect production payment traffic is a
    /// liability, so combining them is a hard error rather than a warning.
    #[test]
    fn base_url_override_is_refused_on_production() {
        temp_env::with_var("FLUTE2_API_BASE_URL", Some("http://127.0.0.1:1"), || {
            let prod = crate::config::Profile::by_name("production").unwrap();
            let err = resolve_base_url(&prod).unwrap_err();
            assert!(err.to_string().contains("production"));
        });
    }

    #[test]
    fn base_url_override_applies_on_sandbox() {
        temp_env::with_var("FLUTE2_API_BASE_URL", Some("http://127.0.0.1:9"), || {
            let sandbox = crate::config::Profile::by_name("sandbox").unwrap();
            assert_eq!(resolve_base_url(&sandbox).unwrap(), "http://127.0.0.1:9");
        });
    }
}
```

- [ ] **Step 2: Run and watch them fail**

Run: `cargo test config auth client`
Expected: FAIL — items not found.

- [ ] **Step 3: Transplant and adapt**

Carry v1's `config.rs`, `auth/keychain.rs`, `auth/token.rs`, and the transport half of the client.

**Renames, which preserve v1's behaviour:**
- Every `FLUTE_` name becomes `FLUTE2_`. v1's keychain service constant is `"flute-cli"`; v2's becomes `"flute-cli-v2"`. The invariant is that it **differs** from v1's — two binaries must not read each other's credentials — so assert the constant in a test rather than trusting the string.
- `Config` keeps v1's three keys under v1's names: `default_profile`, `output`, `auto_update_check`. The key is `default_profile`, not `profile`.
- Keep v1's 401-retry-once behaviour.
- **`OAuth2Fetcher` gains `scope=offline_access`.** v2's token endpoint declares `required: [client_id, client_secret, grant_type, scope]` with `scope` an enum of exactly `offline_access`; v1 (`auth/token.rs:97-100`) sends the first three only. Transplanted unchanged, every token request would be malformed.
- **A keychain-disable seam is required, not optional.** `FLUTE2_NO_KEYCHAIN=1` makes `load_with_env_fallback` skip the OS keychain entirely and report "no credentials". Without it, a "no credentials" test is only valid on a machine nobody has logged in on: `env_remove` of the two credential variables deliberately falls through to the keychain, so on a developer's laptop the test would find real credentials, hit the network, and pass or fail for reasons unrelated to the code. The seam is also what keeps `cargo test` hermetic — a suite that reads an OS keychain is neither offline nor reproducible.

  Unlike the base-URL overrides, this one is safe under `--profile production`: it can only ever remove access, never redirect it.

- **`OAuth2Fetcher`'s error path must call `parse_error_body`.** v1 uses `.error_for_status()?` (`auth/token.rs:104`), which discards the response body — so the OpenIddict shape from Task 4 would never be parsed and `invalid_client` would surface as a bare status code.

**The sanctioned divergence, which deliberately does not match v1:**
- `ApiClient::new` takes an already-resolved `Profile` and credentials rather than resolving them itself. v1 calls `build_client(profile)` inside every dispatch arm, hitting the keychain before a request can be constructed. Moving resolution out is what lets `run()` build one client and a test hand it a mock server's URL.
- Add `resolve_base_url` / `resolve_oauth_url`, honouring `FLUTE2_API_BASE_URL` / `FLUTE2_OAUTH_URL`. **v1 has no equivalent — this is new surface, not a transplant.** Both are refused under `production`: a test hook able to silently redirect production payment traffic is a liability, so combining them is a hard error rather than a warning.

Without these two, every layer-2 test in Tasks 9–24 is unwritable.

- [ ] **Step 4: Write the OAuth round-trip test against a mock server**

```rust
// tests/support/mod.rs
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Mounts the token endpoint so any test can obtain a bearer without the keychain.
///
/// The form body is matched in full. v2 requires `scope=offline_access` on the
/// client-credentials grant and v1 does not send it, so a mock that matched only
/// the path would let a transplanted fetcher through with a request the real
/// token endpoint rejects.
pub async fn mock_with_token() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .and(body_string_contains("grant_type=client_credentials"))
        .and(body_string_contains("scope=offline_access"))
        .and(body_string_contains("client_id=test-id"))
        .and(body_string_contains("client_secret=test-secret"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "tok-xyz", "expires_in": 3600, "token_type": "Bearer"
        })))
        .mount(&server)
        .await;
    server
}

/// A `flute2` invocation aimed at `server`, authenticated from the environment.
pub fn bin(server: &MockServer) -> assert_cmd::Command {
    let mut c = assert_cmd::Command::cargo_bin("flute2").unwrap();
    isolate(&mut c);
    c.env("FLUTE2_API_BASE_URL", server.uri())
     .env("FLUTE2_OAUTH_URL", format!("{}/oauth2/token", server.uri()))
     .env("FLUTE2_CLIENT_ID", "test-id")
     .env("FLUTE2_CLIENT_SECRET", "test-secret")
     .env("FLUTE2_NO_UPDATE_CHECK", "1")
     .env("FLUTE2_NO_KEYCHAIN", "1")
     .env("CI", "1");
    c
}

/// Cut every binary test off from the developer's machine.
///
/// Without this a test reads `~/.flute2/config.toml`: a contributor whose
/// config sets `default_profile = "production"` trips the deliberate
/// production-plus-override refusal and every test in the suite fails on
/// their machine alone, and a config `output = "json"` silently changes what
/// table-mode assertions see. The keychain is cut off for the same reason —
/// a "no credentials" test is only meaningful when there are none.
fn isolate(c: &mut assert_cmd::Command) {
    static HOME: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
    let home = HOME.get_or_init(|| tempfile::tempdir().unwrap());
    c.env("HOME", home.path())
     .env("XDG_CONFIG_HOME", home.path())     // dirs uses this on Linux
     .env("USERPROFILE", home.path())         // and this on Windows
     .env("FLUTE2_PROFILE", "sandbox")
     .env("FLUTE2_NO_KEYCHAIN", "1")
     .env("FLUTE2_NO_UPDATE_CHECK", "1")
     .env_remove("FLUTE2_OUTPUT")
     .env("CI", "1");
}

/// Assert that the server actually observed the exchange the contract
/// describes — method, path, query, and body — rather than merely that the
/// command exited zero.
///
/// `mount` proves a *matching* request would be answered; without this a
/// command that sent nothing at all, or sent to a second mounted route, still
/// passes. The pair is the canonical pattern:
///
/// ```ignore
/// let ex = support::mount(&server, OPERATION_ID, VARIANT).await;
/// support::bin(&server).args([...]).assert().success();
/// support::assert_exchange_observed(&server, &ex).await;
/// ```
pub async fn assert_exchange_observed(server: &MockServer, ex: &Exchange) {
    let reqs = server.received_requests().await.unwrap();
    let hit = reqs.iter().find(|r| {
        r.method.as_str().eq_ignore_ascii_case(&ex.request.method)
            && r.url.path() == ex.request.path
    }).unwrap_or_else(|| panic!(
        "no {} {} was received; got {:?}", ex.request.method, ex.request.path,
        reqs.iter().map(|r| format!("{} {}", r.method, r.url.path())).collect::<Vec<_>>()));

    for (k, v) in &ex.request.query {
        assert_eq!(hit.url.query_pairs().find(|(qk, _)| qk == k).map(|(_, qv)| qv.to_string()),
            Some(v.clone()), "query parameter {k}");
    }
    if let Some(expected) = ex.request.body.as_ref() {
        let actual: serde_json::Value = serde_json::from_slice(&hit.body)
            .expect("request body was not JSON");
        assert_eq!(&actual, expected, "request body");
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
/// Mounting alone is not the assertion. wiremock's matchers are existential —
/// `query_param` checks that a pair is present, not that nothing else is — so
/// a command sending a misspelled or unsupported extra parameter still matches.
/// Pair every `mount` with `assert_exchange_observed`.
pub async fn mount(server: &MockServer, operation_id: &str, variant: &str) -> Exchange {
    let ex = contracts::exchange(operation_id, variant);
    let mut m = Mock::given(method(ex.request.method.as_str()))
        .and(path(ex.request.path.as_str()));
    for (k, v) in &ex.request.query {
        m = m.and(wiremock::matchers::query_param(k.as_str(), v.as_str()));
    }
    if let Some(body) = ex.request.body.as_ref() {
        m = m.and(wiremock::matchers::body_json(body.clone()));
    }
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
/// Layer 3 claims to catch a wrong query and a wrong content type, and mounting
/// cannot: an extra `?merchantId=` that the API silently ignores (D8), a
/// duplicated `pageSize`, or a form-encoded body where JSON was declared would
/// all pass the mock and reach production. The recorded request is the only
/// place those are visible.
///
/// The token request is excluded — it is the harness's, not the command's.
pub async fn assert_exchange_observed(server: &MockServer, ex: &Exchange) {
    let reqs = server.received_requests().await.unwrap();
    let sent: Vec<_> = reqs.iter()
        .filter(|r| r.url.path() != "/oauth2/token")
        .filter(|r| r.url.path() == ex.request.path && r.method.as_str()
            .eq_ignore_ascii_case(&ex.request.method))
        .collect();
    assert_eq!(sent.len(), 1,
        "expected exactly one {} {}, saw {}", ex.request.method, ex.request.path, sent.len());
    let req = sent[0];

    // Multisets, sorted: this rejects an extra parameter and a duplicated one,
    // which a per-pair matcher accepts.
    let mut observed: Vec<(String, String)> = req.url.query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned())).collect();
    let mut expected = ex.request.query.clone();
    observed.sort();
    expected.sort();
    assert_eq!(observed, expected, "query differs from the contract fixture");

    let observed_ct = req.headers.get("content-type").map(|v| v.to_str().unwrap()
        .split(';').next().unwrap().trim().to_string());
    let expected_ct = ex.request.body.as_ref()
        .map(|_| ex.request.content_type.clone().unwrap_or("application/json".into()));
    assert_eq!(observed_ct, expected_ct, "content type differs from the contract fixture");
}

/// A binary with no credentials at all — and, crucially, no access to the
/// developer's keychain either. Without the keychain seam this would pass on
/// a clean machine and behave differently on a logged-in one.
pub fn bin_without_credentials() -> assert_cmd::Command {
    let mut c = assert_cmd::Command::cargo_bin("flute2").unwrap();
    isolate(&mut c);
    c.env_remove("FLUTE2_CLIENT_ID")
     .env_remove("FLUTE2_CLIENT_SECRET")
     .env("FLUTE2_NO_KEYCHAIN", "1")
     .env("FLUTE2_NO_UPDATE_CHECK", "1")
     .env("CI", "1");
    c
}
```

- [ ] **Step 5: Run the whole gate**

Run: `cargo test && cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "ARISE-4928 feat(auth): injectable client, namespaced config and credentials"
```

---

### Task 7: Vendor the spec, and build the contract matrix and exchange conformance

The layer that makes the other layers checkable. Everything after this registers into it.

**Files:**
- Create: `docs/reference/openapi-v2.json`, `docs/reference/openapi-v2.sha256`, `tests/support/spec.rs`, `tests/support/contracts.rs`, `tests/conformance.rs`, `tests/coverage.rs`

**Interfaces:**
- Produces: `assert_exchange_conforms(operation_id, RequestFixture, ResponseFixture)`, `Contract`, `Variant`, `CONTRACTS`, `PENDING`, `Divergence`, `Rule`, `test_fn_exists(name) -> bool`, `test_fn_cites(name, operation_id) -> bool`.

- [ ] **Step 1: Vendor the bundle and record its hash**

```bash
curl -sS -o docs/reference/openapi-v2.json \
  'https://developer.flute.com/_bundle/api-reference/@v2/index.json'
shasum -a 256 docs/reference/openapi-v2.json | awk '{print $1}' \
  > docs/reference/openapi-v2.sha256
```

- [ ] **Step 2: Write the exchange-conformance harness**

Validating a body against a schema name cannot catch a fixture that is internally valid but attached to the wrong operation. Everything is keyed on `operationId` — all 61 operations declare one.

```rust
// tests/support/spec.rs
use serde_json::Value;
use std::sync::LazyLock;

pub static SPEC: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../docs/reference/openapi-v2.json")).unwrap()
});

/// Owned, because the contract matrix constructs these and the command tests
/// consume the same values. A borrowed fixture would force every group's test
/// to rebuild the body by hand, which is how a mock and its conformance check
/// drift apart.
#[derive(Clone, Debug)]
pub struct RequestFixture {
    /// The method and path the CLI actually used. Without these the harness
    /// cannot tell a right body on the wrong endpoint from a right exchange.
    pub method: String,
    pub path: String,                       // rendered, e.g. /v2/customers/cus_1
    pub path_params: Vec<(String, String)>,
    pub query: Vec<(String, String)>,
    pub content_type: Option<String>,
    pub body: Option<Value>,
}

#[derive(Clone, Debug)]
pub struct ResponseFixture {
    pub status: u16,
    pub body: Option<Value>,
}

/// One request paired with the response it is expected to produce.
#[derive(Clone, Debug)]
pub struct Exchange {
    pub request: RequestFixture,
    pub response: ResponseFixture,
}

/// Locate an operation by id, returning its method, templated path, and a
/// pointer into SPEC. No `unsafe`: the crate forbids it, and SPEC is a
/// `LazyLock` whose contents live for the process, so an index is enough.
pub fn operation(operation_id: &str) -> (String, String, &'static Value) {
    let spec: &'static Value = &SPEC;
    for (path, item) in spec["paths"].as_object().unwrap() {
        for (method, op) in item.as_object().unwrap() {
            if op.is_object() && op["operationId"] == operation_id {
                return (method.clone(), path.clone(), op);
            }
        }
    }
    panic!("no operation with id {operation_id} in the vendored spec");
}

/// Validate a whole exchange against one operation.
pub fn assert_exchange_conforms(operation_id: &str, req: &RequestFixture, resp: &ResponseFixture) {
    let (method, template, op) = operation(operation_id);

    // 1. Method and path match the operation. This is the check that makes
    //    the rest meaningful — a valid body on the wrong endpoint fails here.
    assert_eq!(req.method.to_lowercase(), method,
        "{operation_id}: expected {method}, fixture used {}", req.method);
    assert!(path_matches(&template, &req.path),
        "{operation_id}: path {} does not match template {template}", req.path);

    // 2. Every {placeholder} is supplied, and the rendered path agrees.
    for name in placeholders(&template) {
        let v = req.path_params.iter().find(|(k, _)| k == &name)
            .unwrap_or_else(|| panic!("{operation_id}: path parameter {name} not supplied"));
        assert!(req.path.contains(&v.1),
            "{operation_id}: rendered path is missing {name} = {}", v.1);
    }

    let declared = resolved_parameters(op);

    // `assert_validates` takes `&Value`, so nothing is moved out of the borrow.
    // 3. Required parameters are present; every supplied one is declared and
    //    validates against its own schema. Undeclared query parameters are a
    //    silent-ignore risk (see D8), so they fail rather than pass.
    for p in &declared {
        let name = p["name"].as_str().unwrap();
        if p["in"] == "query" && p["required"] == true {
            assert!(req.query.iter().any(|(k, _)| k == name),
                "{operation_id}: required query parameter {name} not supplied");
        }
    }
    for (k, v) in &req.query {
        let p = declared.iter().find(|p| p["name"] == k.as_str() && p["in"] == "query")
            .unwrap_or_else(|| panic!("{operation_id}: query parameter {k} is not declared"));
        assert_scalar_validates(operation_id, k, &p["schema"], v);
    }

    // 4. Content type is one the operation declares.
    if let Some(content) = op["requestBody"]["content"].as_object() {
        let used = req.content_type.as_deref().unwrap_or("application/json");
        assert!(content.contains_key(used),
            "{operation_id}: content type {used} not declared; spec has {:?}", content.keys());
    }

    // 5. Request body: presence is decided by the schema, not by the
    //    `requestBody.required` flag, which is false on all 23 operations that
    //    take a JSON body while twelve of their schemas declare required
    //    properties (D17). Trusting the flag would accept an empty-bodied
    //    POST /v2/transactions, which is the single most important request in
    //    this CLI. A schema with required properties cannot be satisfied by an
    //    absent body, so that is the rule.
    let schema_requires_fields = !resolved_request_schema(op)
        .map(|s| s["required"].as_array().is_none_or(|r| r.is_empty()))
        .unwrap_or(true);
    let body_required = op["requestBody"]["required"] == true || schema_requires_fields;
    match (op["requestBody"]["content"].as_object(), req.body.as_ref()) {
        (Some(content), Some(body)) => {
            let used = req.content_type.as_deref().unwrap_or("application/json");
            assert_validates(operation_id, "request", &content[used]["schema"], body);
        }
        (Some(_), None) => assert!(!body_required,
            "{operation_id}: requestBody is required, fixture sends none"),
        (None, Some(_)) => panic!("{operation_id}: sent a body, spec declares none"),
        (None, None) => {}
    }

    // 6. The response status is declared for THIS operation.
    let status = resp.status.to_string();
    let responses = op["responses"].as_object().unwrap();
    assert!(responses.contains_key(&status),
        "{operation_id}: status {status} not declared; spec has {:?}", responses.keys());

    // 7. Empty-versus-JSON matches the declared status, in both directions.
    //    The body itself validates against the declared schema, unless a
    //    `ResponseShape` divergence says the declared schema is wrong for this
    //    operation — in which case its own declared examples are the oracle.
    let declares_body = responses[&status]["content"]["application/json"].is_object();
    match (declares_body, resp.body.as_ref()) {
        (true, None) => panic!("{operation_id}: {status} declares a JSON body, fixture has none"),
        (false, Some(_)) => panic!("{operation_id}: {status} declares no body, fixture invents one"),
        (true, Some(body)) => match response_shape_divergence(operation_id) {
            Some(d) => assert_matches_declared_examples(operation_id, &status, body, d),
            None => assert_validates(operation_id, "response",
                &responses[&status]["content"]["application/json"]["schema"], body),
        },
        (false, None) => {}
    }
}

/// A rendered path matches a template when their segment counts agree and
/// every non-placeholder segment is equal.
fn path_matches(template: &str, rendered: &str) -> bool {
    let (t, r): (Vec<_>, Vec<_>) = (template.split('/').collect(), rendered.split('/').collect());
    t.len() == r.len()
        && t.iter().zip(&r).all(|(t, r)| t.starts_with('{') || t == r)
}
```

`assert_validates` compiles the subschema with the whole bundle as its document so internal `$ref`s resolve, and applies the `RequestField` divergence rules from Step 5.

`resolved_request_schema` follows the `application/json` schema's `$ref` so the `required` array is read from the schema itself rather than from the reference. Operations whose schema declares no required properties keep an optional body — a full `capture` legitimately sends `{}` or nothing.

`assert_matches_declared_examples` reads the operation's `responses[status].content["application/json"].examples`, asserts at least one exists — a divergence with no examples to fall back on is a rule with no oracle and must fail loudly rather than pass silently — and then asserts that every key in the fixture appears in the union of those examples' keys, and that the fixture is not itself a page (no `items`/`pageInfo` pair). It is deliberately weaker than schema validation: it is a stopgap for operations whose published schema is known wrong, and its strength comes from the live test the divergence names.

- [ ] **Step 3: Write the contract matrix**

Data, not prose. A markdown table drifts silently; this one fails the build.

**At this task every command operation is pending.** `CONTRACTS` starts holding one row, the token endpoint (`get-oauth-token`) marked `Internal`, because it needs no command; `PENDING` holds the other **forty-nine**. The two must sum to fifty, not overlap and not exceed it — the token endpoint is one of the fifty, not an extra beside them, so "fifty pending" would make the operation-set invariant see fifty-one and fail on the first run.

A row moves into `CONTRACTS` in the same commit as the command and tests it names, never before: a row written here would name a `cmd_*` test that Tasks 9 onward have not written, which is the sequencing trap this staging exists to avoid.

The rows below are therefore a **template for what each owning task adds**, not content for this commit. `ping` lands its row in Task 9, `transactions create` in Task 11, `payment-methods set-default` in Task 16.

```rust
// tests/support/contracts.rs
use super::spec::{Exchange, RequestFixture, ResponseFixture};
use serde_json::json;

pub enum Mapping { Command(&'static str), Internal(&'static str), Excluded(&'static str) }
pub enum Live { Test(&'static str), Skip(&'static str) }

/// Operations whose group has not been built yet, each naming the task that
/// owns it. A row moves from here into `CONTRACTS` as part of that task, never
/// separately — the move and the tests it names land in one commit.
///
/// This is what lets Phase 0 pass its own gate. `CONTRACTS ∪ PENDING` is
/// asserted equal to the bundle's non-webhook operation set, so an operation
/// added upstream still fails the build; but the existence checks — mock test,
/// live test, `--help` flag — only run over rows that claim to be built.
pub static PENDING: &[(&str, &str)] = &[
    ("flute-v2-get-customers", "Task 15"),
    ("flute-v2-patch-customers-customerId", "Task 15"),
    ("flute-v2-delete-customers-customerId", "Task 15"),
    ("flute-v2-get-payment-methods", "Task 16"),
    // … every operation not yet built, removed as its task lands.
];

pub struct Variant {
    pub name: &'static str,
    /// The exchange this variant asserts. It is the single source of truth:
    /// conformance validates it, and the group's command test drives its
    /// mock from the same value.
    ///
    /// Nothing here restates what the fixture already says. An earlier draft
    /// carried `required_params`, `body_schema`, and a `success` enum
    /// alongside a test *name*, and none of them were ever read — a matrix
    /// that describes tests instead of feeding them is decoration that goes
    /// stale silently.
    pub exchange: fn() -> Exchange,
    pub mock_test: &'static str,
    pub live: Live,
    pub divergence: Option<&'static str>,
}

pub struct Contract {
    pub operation_id: &'static str,
    pub mapping: Mapping,
    pub variants: &'static [Variant],
}

/// Look up one variant's exchange. Command tests call this rather than
/// rebuilding the body, so a mock and its conformance check cannot disagree.
pub fn exchange(operation_id: &str, variant: &str) -> Exchange {
    let c = CONTRACTS.iter().find(|c| c.operation_id == operation_id)
        .unwrap_or_else(|| panic!("no contract row for {operation_id}"));
    let v = c.variants.iter().find(|v| v.name == variant)
        .unwrap_or_else(|| panic!("{operation_id} has no variant {variant}"));
    (v.exchange)()
}

fn req(method: &str, path: &str) -> RequestFixture {
    RequestFixture {
        method: method.into(), path: path.into(),
        path_params: vec![], query: vec![], content_type: None, body: None,
    }
}

pub static CONTRACTS: &[Contract] = &[
    // At Task 7 this holds only the token endpoint. Everything below arrives
    // with its owning task, and each row's shape is the template.
    Contract {
        operation_id: "flute-v2-get-ping",
        mapping: Mapping::Command("ping"),
        variants: &[Variant {
            name: "default",
            exchange: || Exchange {
                request: req("GET", "/v2/ping"),
                // 200 with no body; the harness rejects an invented one.
                response: ResponseFixture { status: 200, body: None },
            },
            mock_test: "ping_sends_bearer_to_v2_ping_and_reports_reachable",
            live: Live::Test("live_ping_succeeds"),
            divergence: None,
        }],
    },
    Contract {
        operation_id: "flute-v2-post-transactions",
        mapping: Mapping::Command("transactions create"),
        variants: &[
            Variant {
                name: "new card, automatic capture",
                exchange: || Exchange {
                    request: RequestFixture {
                        body: Some(json!({
                            "paymentProcessorId": "pp-1",
                            "baseAmount": 10.50,
                            "transactionDetails": {"cardData": {
                                "captureMethod": "Auto",
                                "paymentMethodDetails": {
                                    "cardNumber": "4111111111111111",
                                    "securityCode": "123",
                                    "expirationMonth": 12,
                                    "expirationYear": 2032}}}
                        })),
                        ..req("POST", "/v2/transactions")
                    },
                    response: ResponseFixture { status: 200, body: Some(json!({
                        "transactionId": "txn_1", "transactionStatus": "Captured"})) },
                },
                mock_test: "create_posts_the_documented_and_undocumented_fields",
                live: Live::Test("live_card_sale_auto_capture"),
                divergence: Some("D4"),
            },
            Variant {
                name: "saved card",
                exchange: || Exchange {
                    request: RequestFixture {
                        body: Some(json!({
                            "paymentProcessorId": "pp-1", "baseAmount": 1.00,
                            "transactionDetails": {"cardData": {"paymentMethodId": "pm_1"}}
                        })),
                        ..req("POST", "/v2/transactions")
                    },
                    response: ResponseFixture { status: 200, body: Some(json!({
                        "transactionId": "txn_1", "transactionStatus": "Captured"})) },
                },
                mock_test: "saved_card_sends_payment_method_id_and_no_card_details",
                live: Live::Test("live_saved_card_sale"),
                divergence: None,
            },
            // … new card manual capture, new ACH, saved ACH.
        ],
    },
    Contract {
        operation_id: "flute-v2-post-payment-methods-paymentMethodId-set-default",
        mapping: Mapping::Command("payment-methods set-default"),
        variants: &[Variant {
            name: "default",
            exchange: || Exchange {
                request: RequestFixture {
                    path_params: vec![("paymentMethodId".into(), "pm_1".into())],
                    // Required by the operation; omitting it fails conformance.
                    query: vec![("customerId".into(), "cus_1".into())],
                    ..req("POST", "/v2/payment-methods/pm_1/set-default")
                },
                response: ResponseFixture { status: 200, body: None },
            },
            mock_test: "set_default_sends_the_customer_id_query_and_tolerates_no_body",
            live: Live::Test("live_set_default_payment_method"),
            divergence: None,
        }],
    },
    // … one Contract per non-webhook operation; capture carries "full" and "partial".
];
```

- [ ] **Step 4: Write the coverage invariants**

```rust
// tests/coverage.rs
mod support;
use support::{contracts::*, spec::{self, assert_exchange_conforms, SPEC}};

/// Every non-webhook operation is accounted for — built, or pending with the
/// task that owns it. Adding one upstream fails the build until somebody
/// decides what it means for the CLI.
#[test]
fn every_non_webhook_operation_is_built_or_pending_exactly_once() {
    let mut spec_ids = spec::non_webhook_operation_ids();
    spec_ids.sort();
    assert_eq!(spec_ids.len(), 50, "50 non-webhook operations expected");

    let mut accounted: Vec<String> = CONTRACTS.iter()
        .map(|c| c.operation_id.to_string())
        .chain(PENDING.iter().map(|(id, _)| id.to_string()))
        .collect();
    accounted.sort();
    let before = accounted.len();
    accounted.dedup();
    assert_eq!(before, accounted.len(), "an operation is both built and pending");
    assert_eq!(spec_ids, accounted, "matrix and spec disagree on the operation set");
}

/// A deferral must name the task that ends it, or it is just a hole.
#[test]
fn every_pending_operation_names_its_owning_task() {
    for (id, task) in PENDING {
        assert!(!task.is_empty(), "{id}: pending with no owning task");
    }
}

/// **The invariant that makes the matrix mean something.** Every variant's
/// fixture is validated against its own operation: method, rendered path,
/// path parameters, required and undeclared query parameters, content type,
/// request body, declared status, empty-versus-JSON, and response body.
///
/// A matrix that only recorded parameter names and a schema name next to a
/// test *name* proved nothing: the names were never read, and the named test
/// might exercise a different operation entirely.
#[test]
fn every_variant_conforms_to_its_own_operation() {
    for c in CONTRACTS {
        let Mapping::Command(cmd) = &c.mapping else { continue };
        assert!(!c.variants.is_empty(), "{} ({cmd}): no variants", c.operation_id);
        for v in c.variants {
            let ex = (v.exchange)();
            assert_exchange_conforms(c.operation_id, &ex.request, &ex.response);
        }
    }
}

/// A named test that does not exist is an orphaned row, not coverage — and
/// a test that never mentions its operation id is not demonstrably testing it.
#[test]
fn every_variant_names_a_mock_test_that_exists_and_cites_its_operation() {
    for c in CONTRACTS {
        let Mapping::Command(_) = &c.mapping else { continue };
        for v in c.variants {
            assert!(support::test_fn_exists(v.mock_test),
                "{} / {}: mock test `{}` does not exist", c.operation_id, v.name, v.mock_test);
            assert!(support::test_fn_cites(v.mock_test, c.operation_id),
                "{} / {}: `{}` never references its operation id; drive its mock from \
                 contracts::exchange(\"{}\", \"{}\")",
                c.operation_id, v.name, v.mock_test, c.operation_id, v.name);
        }
    }
}

/// Live coverage is opt-out with a stated reason, never silent.
#[test]
fn every_variant_has_live_coverage_or_a_reason() {
    for c in CONTRACTS {
        let Mapping::Command(_) = &c.mapping else { continue };
        for v in c.variants {
            match &v.live {
                Live::Test(name) => assert!(support::test_fn_exists(name),
                    "{} / {}: live test `{}` does not exist", c.operation_id, v.name, name),
                Live::Skip(reason) => assert!(!reason.is_empty(),
                    "{} / {}: live skip needs a reason", c.operation_id, v.name),
            }
        }
    }
}

/// Both directions. An operation that returns JSON must have at least one
/// variant carrying a body, and one that returns none must have none — an
/// earlier one-way check would have accepted a fixture claiming a bodyless
/// success for an endpoint that answers with a resource.
#[test]
fn body_expectations_match_the_spec_in_both_directions() {
    let bodyless = spec::bodyless_successes();
    for c in CONTRACTS {
        let Mapping::Command(_) = &c.mapping else { continue };
        for v in c.variants {
            let ex = (v.exchange)();
            let spec_says_empty = bodyless.iter()
                .any(|(id, st)| id == c.operation_id && *st == ex.response.status);
            assert_eq!(ex.response.body.is_none(), spec_says_empty,
                "{} / {}: fixture says body={}, spec says body={} for status {}",
                c.operation_id, v.name, ex.response.body.is_some(),
                !spec_says_empty, ex.response.status);
        }
    }
}

/// Both sets are derived, never typed by hand. A hard-coded count is how
/// "every write endpoint -- nineteen" came to omit eleven of them.
///
/// Pending operations are skipped here and caught by the Task 27 release gate;
/// they are already known to the suite through `PENDING`, so skipping them
/// hides nothing.
#[test]
fn every_built_write_and_bodyless_success_is_covered() {
    let pending = |id: &str| PENDING.iter().any(|(p, _)| *p == id);

    for op_id in spec::write_operation_ids() {
        if pending(&op_id) { continue }
        let c = CONTRACTS.iter().find(|c| c.operation_id == op_id)
            .unwrap_or_else(|| panic!("{op_id} is a write with no contract row"));
        assert!(c.variants.iter().any(|v| matches!(v.live, Live::Test(_))),
            "{op_id} is a write and needs live coverage or an explicit Skip reason");
    }
    for (op_id, status) in spec::bodyless_successes() {
        if pending(&op_id) { continue }
        assert!(CONTRACTS.iter().any(|c| c.operation_id == op_id
                && c.variants.iter().any(|v| {
                    let ex = (v.exchange)();
                    ex.response.status == status && ex.response.body.is_none()
                })),
            "{op_id} answers {status} with no body; no variant exercises that");
    }
}
```

`spec::write_operation_ids()` enumerates non-webhook, non-OAuth `POST`/`PATCH`/`DELETE`; `bodyless_successes()` enumerates 2xx responses with no `application/json` content. Neither takes an expected count.

**Every checker body lives in `tests/support/checks.rs`; the integration tests are one-line wrappers.** `tests/coverage.rs`, `tests/surface.rs`, and `tests/parity.rs` are *separate crates* — a function defined in one is not importable from another, and `mod support;` compiles a fresh copy into each. Anything that needs to be called twice (notably the negative controls that verify the checkers themselves fail when they should) has to live in the shared module:

```rust
// tests/coverage.rs — the whole file
mod support;
#[test] fn operation_set_matches_spec() { support::checks::operation_set_matches_spec(); }
#[test] fn variants_conform()           { support::checks::variants_conform(); }
#[test] fn variants_name_real_tests()   { support::checks::variants_name_real_tests(); }
```

The negative controls run the same functions against deliberately broken inputs and assert they panic — via `std::panic::catch_unwind`, leaving the global panic hook alone. Replacing the hook races with every other test in the binary, because tests share a process.

`test_fn_exists` scans the `tests/` sources for `fn <name>`; `test_fn_cites` checks that same function body for the operation id, which is satisfied for free when the test drives its mock from `contracts::exchange(...)`.

- [ ] **Step 5: Write the divergence rules**

Each defect gets a narrow, executable rule rather than a blanket schema skip:

Two kinds exist, because a divergence can be a field the schema omits or a response shape the schema gets wrong, and a pointer strip cannot express the second.

```rust
// tests/support/spec.rs
pub struct Divergence {
    pub id: &'static str,
    pub rule: Rule,
    pub evidence: &'static str,    // the live test proving the runtime behaves this way
    pub removal: &'static str,     // the condition under which this rule is deleted
}

pub enum Rule {
    /// Strip exactly this JSON pointer from the **request** body before
    /// validating. Everything else, including any *other* undocumented field,
    /// still has to validate.
    RequestField { operations: &'static [&'static str], pointer: &'static str },

    /// Validate the **response** body against the operation's own declared
    /// response examples instead of its declared response schema.
    ResponseShape { operations: &'static [&'static str] },
}

pub static DIVERGENCES: &[Divergence] = &[
    Divergence { id: "D4",
        rule: Rule::RequestField {
            operations: &["flute-v2-post-transactions"],
            pointer: "/transactionDetails/cardData/captureMethod" },
        evidence: "live_card_auth_manual_capture",
        removal: "published CardDataDto includes captureMethod" },
    Divergence { id: "D5",
        rule: Rule::RequestField {
            operations: &["flute-v2-post-transactions"],
            pointer: "/customerId" },
        evidence: "live_card_sale_with_customer",
        removal: "published CreateTransactionRequestDto includes customerId" },
    Divergence { id: "D11",
        rule: Rule::ResponseShape { operations: &[
            "flute-v2-post-transactions",
            "flute-v2-post-transactions-transactionId-capture",
            "flute-v2-post-transactions-transactionId-reversal",
            "flute-v2-post-transactions-credit",
            "flute-v2-post-transactions-transactionId-tip-adjustment",
            "flute-v2-post-transactions-transactionId-ach-hold",
            "flute-v2-post-transactions-transactionId-ach-release",
        ]},
        evidence: "live_card_sale_auto_capture",
        removal: "the seven writes declare a single-transaction response schema" },
];
```

Every rule is scoped to the operations it applies to. An unscoped pointer would strip `/customerId` from operations that genuinely declare it, which turns a divergence rule into a blanket hole.

**`RequestField` strips only the named pointer** and validates everything else, so a *second* undocumented field still fails. "Validate the documented subset" would hide it.

**`ResponseShape` exists because D11 cannot be expressed any other way, and cannot be fixed by naming a different schema.** The seven writes declare `PageOfGetTransactionResponseDto`, which sets `additionalProperties: false` and declares only `items` and `pageInfo` — so the single-object fixture these operations actually return is a hard validation failure, not a permissive pass. Nor is there a correct schema to point at instead: the write responses carry `amountDetails`, `processorResponse`, `receipt`, and `responseDetails`, and **no schema in the bundle declares those fields** — `GetTransactionResponseDtoShort` and `GetTransactionResponseDtoFull` both set `additionalProperties: false` and have none of them.

What the bundle *does* ship is the operations' own **response examples**, and all seven carry the single-object form. Those examples are written by the API team, live in the checked-in bundle, and are not something this implementation can influence — so they satisfy the independence requirement that a schema would otherwise have provided. For these seven, conformance asserts the response fixture's field set against the union of the operation's declared examples, and asserts that the fixture is not a page.

The rule is self-limiting in the direction that matters: `GET /v2/transactions` declares the same paged schema and its example *is* a page, so applying `ResponseShape` to it would fail. The genuinely paged operation cannot be swept in by mistake.

Its real oracle stays the live test named in `evidence`; the examples are what keep a fixture honest between live runs. When D11 resolves in either direction the rule is deleted, along with the transitional page arm in the decoder.

- [ ] **Step 6: Write the vendored-spec hash gate**

The bundle is the oracle for layers 1, 2, and 4a, so an undetected local edit to it silently rewrites what every one of those layers proves. The gate is one test, and it needs no `build.rs`: the conformance suite already has the file contents through `include_str!`, so it can hash them directly.

The comparison takes its inputs as arguments rather than reading them, so Task 7c can hand it a mutated copy and prove the gate rejects one. A gate nobody has watched reject anything is not known to be a gate.

```rust
// tests/support/spec.rs
pub fn assert_bundle_hash(bytes: &[u8], recorded: &str) {
    use sha2::{Digest, Sha256};
    assert_eq!(format!("{:x}", Sha256::digest(bytes)), recorded,
        "vendored bundle does not match openapi-v2.sha256. Re-vendoring is a \
         deliberate act: read the diff, update docs/doc-defects.md for anything \
         that moved, then re-record the hash.");
}
```

```rust
// tests/conformance.rs
/// The vendored bundle is the oracle three layers validate against. Editing it
/// to make a test pass would be indistinguishable from fixing the code, so the
/// file is pinned to a hash recorded beside it.
#[test]
fn vendored_bundle_matches_its_recorded_hash() {
    support::spec::assert_bundle_hash(
        include_bytes!("../docs/reference/openapi-v2.json"),
        include_str!("../docs/reference/openapi-v2.sha256").trim());
}
```

This proves only that the vendored file has not been edited locally. Detecting that the *published* bundle has moved needs a fetch, which `cargo test` must not do — that is the separate scheduled workflow in Task 26, and neither substitutes for the other.

- [ ] **Step 7: Run and watch them pass**

Run: `cargo test --test conformance --test coverage`
Expected: PASS. Every command operation sits in `PENDING`, so the operation-set invariant is satisfied while the existence checks have nothing yet to check. A missing or miscounted operation fails here, which is the invariant this task exists to install.

The conformance harness itself is not exercised by real fixtures until Task 9 adds the first row. Task 7c is what proves it works in the meantime — without those controls this task ships a checker nobody has watched reject anything.

- [ ] **Step 8: Commit**

```bash
git add docs/reference tests/
git commit -m "ARISE-4928 test(cli): contract matrix and exchange conformance"
```

---

### Task 7a: The API surface matrix

The contract matrix proves every *operation* is reachable. It says nothing about whether every *field* is. `customers list` can hold a passing contract row while `fullName`, `email`, `companyName`, `mobilePhoneNumber`, `createdFrom`, `createdTo`, `sortBy`, and `asc` are all unreachable from the CLI — the endpoint is covered and most of its capability is not.

This is the same blind spot the v1 parity matrix closes, pointed at v2 instead of v1.

**The field set is leaf-level, and a container never satisfies its descendants.** The argument that endpoint coverage hides field coverage applies again one level down: a single row for `/transactionDetails` would mark the entire card and ACH surface accounted for, which is exactly the shape of the miss that lost the saved-instrument routes. Measured against the bundle, `POST /v2/transactions` has 16 top-level properties and **60 leaves**, 14 of them under `transactionDetails` and 13 under `transactionEnhancedData`; across the 21 operations with a JSON request body it is 131 top-level against **250 leaves**. Nearly half the request surface is invisible to a top-level matrix.

**Files:**
- Create: `tests/support/surface.rs`, `tests/surface.rs`

- [ ] **Step 1: Write the matrix**

```rust
// tests/support/surface.rs
pub enum Exposure {
    /// Reachable from the CLI through this flag.
    Flag(&'static str),
    /// Always sent with a value the CLI chooses, never user-supplied.
    Fixed(&'static str),
    /// Deliberately not exposed, with a reason.
    Excluded(&'static str),
}

pub struct Field {
    pub operation_id: &'static str,
    /// A **leaf** request-body JSON pointer, or `?name` for a query parameter.
    /// Array element steps are written `/[]`, so a product's name is
    /// `/transactionEnhancedData/products/[]/name`.
    ///
    /// Containers are not rows. A row for `/transactionDetails` would account
    /// for the whole card and ACH surface at once, which is the miss this
    /// matrix exists to catch.
    pub field: &'static str,
    pub exposure: Exposure,
}

pub static SURFACE: &[Field] = &[
    Field { operation_id: "flute-v2-get-customers", field: "?fullName",
        exposure: Exposure::Flag("--full-name") },
    Field { operation_id: "flute-v2-get-customers", field: "?email",
        exposure: Exposure::Flag("--email") },
    Field { operation_id: "flute-v2-get-customers", field: "?companyName",
        exposure: Exposure::Flag("--company-name") },
    Field { operation_id: "flute-v2-get-customers", field: "?mobilePhoneNumber",
        exposure: Exposure::Flag("--mobile") },
    Field { operation_id: "flute-v2-get-customers", field: "?createdFrom",
        exposure: Exposure::Flag("--created-from") },
    Field { operation_id: "flute-v2-get-customers", field: "?createdTo",
        exposure: Exposure::Flag("--created-to") },
    Field { operation_id: "flute-v2-get-customers", field: "?sortBy",
        exposure: Exposure::Flag("--sort-by") },
    Field { operation_id: "flute-v2-get-customers", field: "?asc",
        exposure: Exposure::Flag("--asc") },
    Field { operation_id: "flute-v2-post-transactions", field: "/appVersion",
        exposure: Exposure::Excluded("SDK telemetry; meaningless from a CLI") },
    Field { operation_id: "flute-v2-post-transactions", field: "/platform",
        exposure: Exposure::Fixed("cli") },
    // One row per leaf, never one for the object. Each of these is separately
    // reachable or separately missing, and only the leaf row can tell.
    Field { operation_id: "flute-v2-post-transactions",
        field: "/transactionEnhancedData/salesTaxRate",
        exposure: Exposure::Flag("--tax-rate") },
    Field { operation_id: "flute-v2-post-transactions",
        field: "/transactionEnhancedData/invoiceNumber",
        exposure: Exposure::Flag("--invoice") },
    Field { operation_id: "flute-v2-post-transactions",
        field: "/transactionEnhancedData/products/[]/name",
        exposure: Exposure::Flag("--product") },
    Field { operation_id: "flute-v2-post-transactions",
        field: "/transactionDetails/cardData/paymentMethodDetails/cardNumber",
        exposure: Exposure::Flag("--card") },
    Field { operation_id: "flute-v2-post-transactions",
        field: "/transactionDetails/cardData/paymentMethodId",
        exposure: Exposure::Flag("--payment-method-id") },
    // … every leaf and query parameter of every mapped operation: 250 request
    // leaves across the 21 operations that take a JSON body, plus query.
];
```

- [ ] **Step 2: Write the invariant**

```rust
// tests/surface.rs
mod support;
use support::{spec, surface::*};

/// Every request property and query parameter of every mapped operation is
/// reachable, fixed deliberately, or excluded with a reason. Endpoint
/// coverage cannot see this: an operation stays "covered" while most of its
/// fields are unreachable.
#[test]
fn every_request_field_is_exposed_fixed_or_excluded() {
    for (op_id, field) in spec::request_fields_of_mapped_operations() {
        let row = SURFACE.iter()
            .find(|f| f.operation_id == op_id && f.field == field)
            .unwrap_or_else(|| panic!(
                "{op_id}: {field} is in the spec and unaccounted for. Add a flag, \
                 fix it deliberately, or exclude it with a reason."));
        match &row.exposure {
            Exposure::Flag(f) => assert!(f.starts_with("--"), "{op_id}/{field}: not a flag"),
            Exposure::Fixed(v) => assert!(!v.is_empty(), "{op_id}/{field}: fixed to nothing"),
            Exposure::Excluded(r) => assert!(!r.is_empty(), "{op_id}/{field}: needs a reason"),
        }
    }
}

/// The reverse: a row naming a field the spec no longer has is stale.
#[test]
fn no_surface_row_names_a_field_that_does_not_exist() {
    let known = spec::request_fields_of_mapped_operations();
    for f in SURFACE {
        assert!(known.iter().any(|(o, x)| o == f.operation_id && x == f.field),
            "{}: {} is not in the spec", f.operation_id, f.field);
    }
}

/// Every flag named here must appear in that command's --help.
///
/// Only built operations have a command tree to ask, so this follows the
/// contract matrix: `request_fields_of_mapped_operations` covers `CONTRACTS`
/// and not `PENDING`, and a row for a pending operation is itself an error.
#[test]
fn every_exposed_flag_appears_in_help() {
    for f in SURFACE {
        assert!(!contracts::PENDING.iter().any(|(id, _)| *id == f.operation_id),
            "{}: surface row for an operation that is still pending", f.operation_id);
        if let Exposure::Flag(flags) = &f.exposure {
            for flag in flags.split('/') {
                assert!(support::help_for_operation(f.operation_id).contains(flag),
                    "{}: {flag} is claimed but absent from --help", f.operation_id);
            }
        }
    }
}
```

`spec::request_fields_of_mapped_operations()` derives the field set rather than taking it typed, and it resolves to **leaves**:

- `$ref` is followed, with a visited set so a self-referential schema terminates instead of recursing forever.
- `type: array` descends into `items`, contributing a `/[]` step.
- `allOf`, `oneOf`, and `anyOf` contribute the union of their branches' leaves. A leaf reachable through more than one branch is one row, keyed by its pointer — the CLI either reaches that field or it does not, regardless of which branch documents it.
- A property with no properties of its own is a leaf. An object that resolves to no leaves at all (an empty or free-form schema) is itself a leaf, so it cannot disappear from the matrix.

"Mapped" means present in `CONTRACTS` with `Mapping::Command`. Pending operations contribute no rows, so this matrix grows with the command tree rather than demanding 250 rows before the first command exists.

- [ ] **Step 3: Complete the matrix for the built operations, then run**

Run: `cargo test --test surface`
Expected: PASS. The invariant covers only operations already in `CONTRACTS`, so at this point that is the slice — completing its rows is the work, and each later group adds its own under definition-of-done point 7.

Running it before the rows are written fails with a list of unaccounted leaves. That list is the CLI's real capability gap for that group, and closing it is the point; it is not a state to commit.

- [ ] **Step 4: Commit**

```bash
git add tests/
git commit -m "ARISE-4928 test(cli): API surface matrix"
```

---

### Task 7b: The v1 capability-parity matrix

Operation coverage cannot see a dropped feature: every endpoint stays covered while the flag that reached it disappears. This is the only layer that catches that, and it has already found three.

**Files:**
- Create: `tests/support/parity.rs`, `tests/parity.rs`

- [ ] **Step 1: Enumerate v1's capability surface**

Read the flags off v1's clap tree rather than recalling them:

```bash
grep -nE '^\s+[a-z_0-9]+:' "${FLUTE_CLI_V1:-../flute-cli}/src/cli/mod.rs"
```

- [ ] **Step 2: Write the matrix**

```rust
// tests/support/parity.rs
pub enum Parity {
    Preserved(&'static str),              // v2 flag, same meaning
    Replaced(&'static str, &'static str), // v2 flag, why it differs
    Removed(&'static str),                // v2 reason
    Pending(&'static str),                // the task that will carry it over
}

pub struct Capability {
    pub v1: &'static str,
    pub parity: Parity,
    pub test: Option<&'static str>,
}

pub static CAPABILITIES: &[Capability] = &[
    Capability { v1: "transactions sale --card/--exp/--cvv",
        parity: Parity::Preserved("transactions create --card/--exp/--cvv"),
        test: Some("create_posts_the_documented_and_undocumented_fields") },
    Capability { v1: "transactions sale --payment-method-id",
        parity: Parity::Preserved("transactions create --payment-method-id --instrument card"),
        test: Some("create_with_saved_card_sends_payment_method_id") },
    Capability { v1: "ach debit --payment-method-id",
        parity: Parity::Preserved("transactions create --payment-method-id --instrument ach"),
        test: Some("create_with_saved_ach_sends_payment_method_id") },
    Capability { v1: "transactions auth (manual capture)",
        parity: Parity::Replaced("transactions create --capture-method manual",
            "one endpoint; sale and authorization differ only by captureMethod"),
        test: Some("create_manual_capture_sets_authorization") },
    Capability { v1: "transactions sale --l2-tax-rate/--l3-invoice/--l3-po/--l3-product",
        parity: Parity::Replaced("transactions create --tax-rate/--invoice/--purchase-order/--product",
            "v2 nests these under transactionEnhancedData with salesTaxRate/invoiceNumber/purchaseOrder/products"),
        test: Some("create_maps_enhanced_data_fields") },
    Capability { v1: "ach debit --faster (same-day)",
        parity: Parity::Preserved("transactions create --ach-same-day"),
        test: Some("ach_same_day_sets_is_same_day_processing") },
    Capability { v1: "customers list --search",
        parity: Parity::Replaced("customers list --full-name/--email/--company-name/--mobile",
            "v2 has no single search parameter; it exposes four discrete filters"),
        test: Some("customers_list_sends_discrete_filters") },
    Capability { v1: "devices *",
        parity: Parity::Removed("no v2 endpoints; run the v1 CLI, which installs alongside"),
        test: None },
    Capability { v1: "subscriptions *",
        parity: Parity::Removed("no v2 endpoints; run the v1 CLI, which installs alongside"),
        test: None },
    // … one row per v1 capability.
];
```

- [ ] **Step 3: Write the invariants**

```rust
// tests/parity.rs
mod support;
use support::parity::*;

/// Preserved and replaced capabilities must be proven by a test that exists.
/// A pending one names the task that will carry it over — the whole v1 surface
/// is enumerated here from Task 7b, but the groups that replace it are not
/// built until Phase 2, and a row cannot name a test that does not exist yet.
#[test]
fn every_carried_capability_names_a_test_that_exists() {
    for c in CAPABILITIES {
        match &c.parity {
            Parity::Removed(reason) => {
                assert!(!reason.is_empty(), "{}: removal needs a v2 reason", c.v1);
            }
            Parity::Pending(task) => {
                assert!(!task.is_empty(), "{}: pending with no owning task", c.v1);
                assert!(c.test.is_none(), "{}: pending but already names a test", c.v1);
            }
            _ => {
                let t = c.test.unwrap_or_else(|| panic!("{}: carried but names no test", c.v1));
                assert!(support::test_fn_exists(t), "{}: test `{t}` does not exist", c.v1);
            }
        }
    }
}

/// A removal must be a decision, not an oversight.
#[test]
fn removals_name_a_reason_and_no_test() {
    for c in CAPABILITIES {
        if let Parity::Removed(_) = &c.parity {
            assert!(c.test.is_none(), "{}: removed but names a test", c.v1);
        }
    }
}
```

- [ ] **Step 4: Complete the matrix and run**

Run: `cargo test --test parity`
Expected: PASS. Every v1 capability is enumerated now — that enumeration is the point of doing this in Phase 0 — with the ones whose group is not yet built marked `Pending` and naming their task.

- [ ] **Step 5: Commit**

```bash
git add tests/
git commit -m "ARISE-4928 test(cli): v1 capability-parity matrix"
```

---

### Task 7c: Negative controls — prove the harness can see

Tasks 7, 7a, and 7b built the checkers the rest of the plan trusts. Nothing yet proves any of them rejects anything. Every "Catches" cell in the layer table is currently prose, and a checker that passes everything looks exactly like a suite with no defects.

Each control feeds a deliberately broken input to a checker and asserts rejection **with the expected reason**. A panic for an unrelated reason is not a pass — that is the same rule the plan applies to production tests in step 2 of every task, turned on the harness.

**Files:**
- Create: `tests/harness_controls.rs`
- Modify: `tests/coverage.rs`, `tests/surface.rs`, `tests/parity.rs` — extract each invariant's body into a function over its inputs

**Interfaces:**
- Produces: `assert_rejects(reason_fragment, f)`.

- [ ] **Step 1: Make the invariants callable on supplied inputs**

The invariants currently read the `CONTRACTS`, `SURFACE`, and `CAPABILITIES` statics directly, so no control can hand them a broken row. Split each into a function taking its input and a thin `#[test]` that passes the static:

```rust
// tests/coverage.rs
pub fn check_variants_name_existing_tests(contracts: &[Contract]) { /* the body */ }

#[test]
fn every_variant_names_a_mock_test_that_exists_and_cites_its_operation() {
    check_variants_name_existing_tests(CONTRACTS);
}
```

This is the whole reason the controls are cheap. Without it they would have to mutate committed data.

- [ ] **Step 2: Write the rejection helper**

```rust
// tests/harness_controls.rs
mod support;

/// Assert that `f` rejects its input, and rejects it for the stated reason.
///
/// Reason matching is the point. A control that only asserts "something
/// panicked" passes when the checker panics on an unrelated line, which is how
/// a checker that has stopped checking still looks healthy.
fn assert_rejects(reason_fragment: &str, f: impl FnOnce()) {
    // The checkers panic on rejection, and a rejection here is the expected
    // outcome, so the default hook's backtraces would be pure noise.
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    std::panic::set_hook(prev);

    let payload = outcome.expect_err(
        &format!("checker accepted an input it claims to reject ({reason_fragment})"));
    let msg = payload.downcast_ref::<String>().map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("<non-string panic>");
    assert!(msg.contains(reason_fragment),
        "rejected for the wrong reason.\n  expected to mention: {reason_fragment}\n  \
         actual: {msg}");
}
```

- [ ] **Step 3: Write the controls**

**Every control builds its own fixture.** None reads `contracts::exchange`, because at this point the matrix holds no command rows — and even later it must not, since a control that consumed a real row would start failing when that row legitimately changed, turning a harness check into a maintenance tax. The operation ids are real, so the bundle is still the oracle; only the fixtures are synthetic.

Four in full, showing the four shapes the rest follow — a mutated exchange, a divergence scoping check, a mutated matrix row, and a mutated input file:

```rust
/// A minimal, valid customer-create request, built here rather than read from
/// the matrix so this file has no dependency on which groups exist yet.
fn valid_customer_create() -> (support::spec::RequestFixture, support::spec::ResponseFixture) {
    (support::spec::RequestFixture {
        method: "POST".into(),
        path: "/v2/customers".into(),
        path_params: vec![],
        query: vec![],
        content_type: None,
        body: Some(serde_json::json!({"firstName": "Ada", "lastName": "Lovelace"})),
     },
     support::spec::ResponseFixture {
        status: 200,
        body: Some(serde_json::json!({"customerId": "cus_1"})),
     })
}

/// The claim that makes exchange conformance worth more than schema
/// validation: a body that is internally valid but attached to the wrong
/// operation must fail. If this control ever passes, the layer has silently
/// degraded to "some JSON validated against some schema".
#[test]
fn conformance_rejects_a_valid_body_on_the_wrong_operation() {
    let (req, resp) = valid_customer_create();
    assert_rejects("expected", || {
        support::spec::assert_exchange_conforms("flute-v2-post-payment-links", &req, &resp);
    });
}

/// D8 is a documented filter the server ignores. An undeclared query parameter
/// is therefore invisible at runtime, and conformance is the only layer that
/// can see it.
#[test]
fn conformance_rejects_an_undeclared_query_parameter() {
    let req = support::spec::RequestFixture {
        method: "GET".into(),
        path: "/v2/customers".into(),
        path_params: vec![],
        query: vec![("notAParameter".into(), "x".into())],
        content_type: None,
        body: None,
    };
    let resp = support::spec::ResponseFixture {
        status: 200,
        body: Some(serde_json::json!({"items": [], "pageInfo": {"hasMore": false}})),
    };
    assert_rejects("is not declared", || {
        support::spec::assert_exchange_conforms("flute-v2-get-customers", &req, &resp);
    });
}

/// The flag says every body is optional (D17), so the body-presence rule has
/// to come from the schema. This is the control that would have caught an
/// empty-bodied charge being accepted as conformant.
#[test]
fn conformance_rejects_a_missing_body_when_the_schema_requires_fields() {
    let (mut req, resp) = valid_customer_create();
    req.body = None;
    assert_rejects("requestBody is required", || {
        support::spec::assert_exchange_conforms("flute-v2-post-customers", &req, &resp);
    });
}

/// A divergence must not leak onto its neighbours. D4 exempts captureMethod on
/// create only; the same field elsewhere is still an undocumented field.
#[test]
fn the_d4_exemption_does_not_apply_to_a_neighbouring_operation() {
    let req = support::spec::RequestFixture {
        method: "POST".into(),
        path: "/v2/transactions/txn_1/capture".into(),
        path_params: vec![("transactionId".into(), "txn_1".into())],
        query: vec![],
        content_type: None,
        body: Some(serde_json::json!({"captureAmount": 5.25, "captureMethod": "Auto"})),
    };
    let resp = support::spec::ResponseFixture {
        status: 200,
        body: Some(serde_json::json!({"transactionId": "txn_1",
                                      "transactionStatus": "Captured"})),
    };
    assert_rejects("captureMethod", || {
        support::spec::assert_exchange_conforms(
            "flute-v2-post-transactions-transactionId-capture", &req, &resp);
    });
}

/// The gate is only a gate if a mismatch fails. Hashing a mutated copy proves
/// the comparison runs, without touching the vendored file.
#[test]
fn the_hash_gate_rejects_an_altered_bundle() {
    let mut bytes = include_bytes!("../docs/reference/openapi-v2.json").to_vec();
    bytes.extend_from_slice(b"\n");
    let recorded = include_str!("../docs/reference/openapi-v2.sha256").trim();
    assert_rejects("does not match", || {
        support::spec::assert_bundle_hash(&bytes, recorded);
    });
}
```

- [ ] **Step 4: Complete the set — one control per invariant**

Each row states the defect injected and the invariant that must reject it. A row with no control is an invariant nobody has seen fail.

| Injected defect | Must be rejected by |
|---|---|
| Valid body on the wrong operation | `assert_exchange_conforms` (Step 3) |
| Undeclared query parameter | `assert_exchange_conforms` (Step 3) |
| Required query parameter omitted | `assert_exchange_conforms` |
| Request field with wrong casing (`baseamount`) | `assert_exchange_conforms` |
| Content type the operation does not declare | `assert_exchange_conforms` |
| Invented JSON body on a bodyless success | `assert_exchange_conforms` |
| No body where the status declares one | `assert_exchange_conforms` |
| No request body on an operation whose schema has required fields | body-presence rule — **must not consult `requestBody.required`, which is false everywhere (D17)** |
| A *second* undocumented field beside an exempt one | divergence strip stays narrow |
| D4's pointer used on another operation | divergence scoping (Step 3) |
| `ResponseShape` applied to `flute-v2-get-transactions` | divergence cannot swallow the genuinely paged operation |
| Variant naming a test that does not exist | `test_fn_exists` |
| Test that never mentions its operation id | `test_fn_cites` |
| Operation in neither `CONTRACTS` nor `PENDING` | the operation-set invariant |
| Operation in both | the operation-set invariant |
| A leaf surface row deleted | surface coverage — **proves rows are leaves, not top-level** |
| Container row standing in for its descendants | surface coverage |
| Surface row naming a field the bundle lacks | the staleness invariant |
| Carried capability naming a test that does not exist | parity |
| Altered bundle bytes | the hash gate (Step 3) |

Two of these are load-bearing beyond their own line. The deleted-leaf control is the only mechanical proof that the surface matrix is leaf-level — the property added because a single `/transactionDetails` row would mark the whole card and ACH surface covered. And the `flute-v2-get-transactions` control is what stops the D11 response exemption from being widened, by accident or by a later edit, onto the one transaction endpoint whose paged schema is correct.

- [ ] **Step 5: Run the gate**

Run: `cargo test --test harness_controls && cargo test`
Expected: PASS. Each control fails loudly if its checker stops checking.

Sanity-check the helper itself once, by hand: point one control at an input that is *not* broken and confirm it fails with "checker accepted an input it claims to reject". A rejection helper that never rejects would make all nineteen controls vacuous, which is the failure mode this whole task exists to eliminate — one level up.

- [ ] **Step 6: Commit**

```bash
git add tests/
git commit -m "ARISE-4928 test(cli): negative controls for the coverage harness"
```

---


### Task 8: The clap root, output resolution, and the offline/online split

**Files:**
- Modify: `src/lib.rs`, `src/cli/mod.rs`
- Create: `tests/cmd_global.rs`

**Interfaces:**
- Produces: `Cli` with global `--profile`, `--output`, `--debug`; `resolve_output(flag, config_value) -> OutputFormat`; `wants_json_output() -> bool`.

- [ ] **Step 1: Write the failing tests**

```rust
// in src/lib.rs
#[cfg(test)]
mod tests {
    use super::*;

    /// Precedence is flag → FLUTE2_OUTPUT → config → table. clap fills the
    /// flag from the environment, so this function sees only flag and config.
    #[test]
    fn output_precedence_prefers_flag_then_config_then_table() {
        assert_eq!(resolve_output(Some(OutputFormat::Json), "quiet"), OutputFormat::Json);
        assert_eq!(resolve_output(None, "quiet"), OutputFormat::Quiet);
        assert_eq!(resolve_output(None, "nonsense"), OutputFormat::Table);
    }

    /// The config step is reachable only because the flag is optional. If
    /// this test can be made to pass with a `default_value` on the flag,
    /// the flag is not optional and `auth switch` has been broken again.
    #[test]
    fn profile_precedence_consults_the_config_when_the_flag_is_absent() {
        assert_eq!(resolve_profile(Some("prod".into()), "production"), "prod");
        assert_eq!(resolve_profile(None, "production"), "production");
        assert_eq!(resolve_profile(None, ""), "sandbox");
    }
}
```

```rust
// tests/cmd_global.rs
mod support;

/// A usage error is raised before the config file is read, so this one path
/// decides between a JSON envelope and text by scanning argv.
#[test]
fn usage_error_under_output_json_prints_an_envelope_to_stdout_and_exits_3() {
    let out = assert_cmd::Command::cargo_bin("flute2").unwrap()
        .args(["--output", "json", "customers", "nosuchsubcommand"])
        .assert().code(3).get_output().stdout.clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["kind"], "client");
}

#[test]
fn usage_error_without_json_writes_to_stderr_and_leaves_stdout_empty() {
    assert_cmd::Command::cargo_bin("flute2").unwrap()
        .args(["customers", "nosuchsubcommand"])
        .assert().code(3).stdout(predicates::str::is_empty());
}

/// Same decision, reached through the environment rather than argv.
#[test]
fn flute2_output_env_selects_json_for_usage_errors() {
    assert_cmd::Command::cargo_bin("flute2").unwrap()
        .env("FLUTE2_OUTPUT", "json")
        .args(["customers", "nosuchsubcommand"])
        .assert().code(3).stdout(predicates::str::starts_with("{"));
}

/// Tracing must never contaminate the data stream.
#[test]
fn debug_traces_go_to_stderr_leaving_stdout_parseable() {
    let server = tokio::runtime::Runtime::new().unwrap().block_on(support::mock_with_token());
    let out = support::bin(&server)
        .args(["--debug", "--output", "json", "version"])
        .assert().success().get_output().stdout.clone();
    serde_json::from_slice::<serde_json::Value>(&out)
        .expect("stdout stayed parseable under --debug");
}
```

- [ ] **Step 2: Run and watch them fail**

Run: `cargo test --test cmd_global`
Expected: FAIL — subcommands do not exist yet.

- [ ] **Step 3: Implement the clap root**

```rust
// src/cli/mod.rs
const GLOBAL_HEADING: &str = "Global options";

#[derive(clap::Parser, Debug)]
#[command(name = "flute2", version, about = "CLI for the Flute payments platform")]
pub struct Cli {
    /// Active profile (environment). `sandbox` (default) or `production`/`prod`.
    ///
    /// No `default_value`: a defaulted flag is never absent, and "absent" is
    /// the only state in which the config file's `default_profile` can be
    /// consulted. Defaulting here is exactly what makes `auth switch` a no-op
    /// in v1 (V7 in docs/v1-defects.md is the credential case; V1 is this one).
    #[arg(long, env = "FLUTE2_PROFILE", global = true, help_heading = GLOBAL_HEADING)]
    pub profile: Option<String>,

    /// Output format: table (default), json, or quiet (id only).
    #[arg(long, env = "FLUTE2_OUTPUT", global = true, value_enum,
          help_heading = GLOBAL_HEADING)]
    pub output: Option<OutputFormat>,

    /// Print HTTP request/response traces to stderr. Card and bank-account
    /// numbers are masked to the last 4 digits; security codes are removed.
    #[arg(long, global = true, help_heading = GLOBAL_HEADING)]
    pub debug: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}
```

Without an explicit heading clap files global args into every subcommand's own options block, where they interleave with real flags on a flag-heavy leaf.

`resolve_profile` mirrors `resolve_output`, and both run **after** parsing:

```rust
/// `--profile` → `FLUTE2_PROFILE` (via clap) → config `default_profile` → sandbox.
pub(crate) fn resolve_profile(flag: Option<String>, config_value: &str) -> String {
    flag.filter(|s| !s.is_empty())
        .or_else(|| Some(config_value).filter(|s| !s.is_empty()).map(str::to_string))
        .unwrap_or_else(|| "sandbox".into())
}
```

- [ ] **Step 4: Implement `run()` with the offline/online split**

`completion`, `version`, `update`, and `auth login/logout/switch` are handled before any `Ctx` exists. The invariant is **no credential pre-resolution**, not "no keychain access": `auth login` writes the keychain and `auth logout` deletes from it, and neither may be routed down a path that resolves credentials first — a login that fails because you are not logged in is the failure this split exists to prevent. `update` is likewise not offline; it reaches GitHub, and is here because it calls no Flute API.

Everything else resolves credentials once, builds one `ApiClient`, prints the production banner to stderr, and dispatches with a `Ctx`.

- [ ] **Step 5: Run and watch them pass**

Run: `cargo test --test cmd_global`
Expected: PASS, 4 tests.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "ARISE-4928 feat(cli): clap root, output precedence, offline/online split"
```

---

### Task 8b: `auth`, `version`, `update`, `completion`

The commands the scope promises and no other task builds. They come before the slice because `auth login` is how a human gets credentials in the first place, and `auth status` is what they run when it has gone wrong.

**Files:**
- Create: `src/groups/auth.rs`, `src/cli/auth.rs`, `src/update.rs`, `src/update_check.rs`, `tests/cmd_auth.rs`, `tests/cmd_utility.rs`
- Source: v1 `src/cli/auth.rs`, `src/update.rs`, `src/update_check.rs`, `src/cli/util.rs`

**Interfaces:**
- Consumes: `keychain::load_with_env_fallback`, `Profile::by_name`, `config::{load_or_default, save}`, `OutputFormat`.
- Produces: `auth::{login, logout, status, switch, token}`, `update::run()`, `update_check::{opt_out, check_for_update, notice_for}`.

**`auth status` must not require credentials** — and it is the *only* command in that category. It resolves credentials, and when there are none it reports `authenticated: false` and exits 0. Routing it down the credentials-required path would make it fail precisely when a user runs it to find out why things are failing.

`auth token` is **not** in that category, despite sitting in the same group. Its entire output is a bearer token, which cannot be obtained without credentials; v1 fails it through `build_client`, and v2 exits 2 the same way. Add a test asserting exactly that, so the two commands do not drift together by proximity.

- [ ] **Step 1: Write the failing tests**

```rust
// tests/cmd_auth.rs
mod support;

/// The whole point of the command: it works when auth does not.
#[test]
fn status_without_credentials_reports_false_and_exits_zero() {
    let out = support::bin_without_credentials()
        .args(["--output", "json", "auth", "status"])
        .assert().success().get_output().stdout.clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["data"]["authenticated"], false);
}

/// A command that does need credentials still fails cleanly without them.
#[test]
fn a_credentialed_command_without_credentials_exits_2() {
    support::bin_without_credentials()
        .args(["customers", "list"])
        .assert().code(2);
}

#[tokio::test]
async fn status_with_credentials_pings_and_reports_true() {
    let server = support::mock_with_token().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/v2/ping"))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .mount(&server).await;

    let out = support::bin(&server).args(["--output", "json", "auth", "status"])
        .assert().success().get_output().stdout.clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["data"]["authenticated"], true);
}

/// v1 writes default_profile and never reads it, so `auth switch` is a no-op
/// there. Here the write must actually change what the next command targets.
#[test]
fn switch_writes_default_profile_and_the_next_command_honours_it() {
    let home = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        assert_cmd::Command::cargo_bin("flute2").unwrap()
            .env("HOME", home.path()).env("FLUTE2_NO_UPDATE_CHECK", "1").env("CI", "1")
            .env_remove("FLUTE2_PROFILE")
            .args(args).assert().success().get_output().stdout.clone()
    };
    run(&["auth", "switch", "production"]);
    let out = run(&["--output", "json", "version"]);
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["meta"]["environment"], "production");
}

/// Same group, opposite category: a token cannot exist without credentials.
#[test]
fn token_without_credentials_exits_2() {
    support::bin_without_credentials().args(["auth", "token"]).assert().code(2);
}

#[test]
fn switch_rejects_an_unknown_profile() {
    assert_cmd::Command::cargo_bin("flute2").unwrap()
        .args(["auth", "switch", "staging"]).assert().code(3);
}

/// An explicit flag still beats the stored default.
#[test]
fn profile_flag_overrides_the_stored_default() {
    let home = tempfile::tempdir().unwrap();
    assert_cmd::Command::cargo_bin("flute2").unwrap()
        .env("HOME", home.path()).env("FLUTE2_NO_UPDATE_CHECK", "1").env("CI", "1")
        .args(["auth", "switch", "production"]).assert().success();
    let out = assert_cmd::Command::cargo_bin("flute2").unwrap()
        .env("HOME", home.path()).env("FLUTE2_NO_UPDATE_CHECK", "1").env("CI", "1")
        .args(["--profile", "sandbox", "--output", "json", "version"])
        .assert().success().get_output().stdout.clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["meta"]["environment"], "sandbox");
}
```

```rust
// tests/cmd_utility.rs
#[test]
fn completion_emits_a_script_for_each_supported_shell() {
    for shell in ["bash", "zsh", "fish", "powershell", "elvish"] {
        assert_cmd::Command::cargo_bin("flute2").unwrap()
            .args(["completion", shell])
            .assert().success().stdout(predicates::str::is_empty().not());
    }
}

/// Offline commands must not touch the keychain or the network.
#[test]
fn completion_and_version_work_without_credentials() {
    for args in [vec!["completion", "bash"], vec!["version"]] {
        support::bin_without_credentials().args(&args).assert().success();
    }
}

#[test]
fn version_reports_the_crate_version_in_every_output_mode() {
    for mode in ["table", "json", "quiet"] {
        assert_cmd::Command::cargo_bin("flute2").unwrap()
            .env("FLUTE2_NO_UPDATE_CHECK", "1").env("CI", "1")
            .args(["--output", mode, "version"])
            .assert().success().stdout(predicates::str::contains("2.0.0"));
    }
}
```

```rust
// in src/update.rs
#[cfg(test)]
mod tests {
    use super::*;

    /// A wrong constant here is a well-formed request to the wrong project:
    /// `flute2 update` would read v1's releases and, on an installer-managed
    /// machine, hand v1's installer a v2 update. Nothing else in the suite
    /// would see it.
    #[test]
    fn update_targets_v2_everywhere_and_never_v1() {
        assert_eq!(APP_NAME, "flute2");
        assert_eq!(REPO_OWNER, "getflute");
        assert_eq!(REPO_NAME, "flute-cli-v2");

        let hint = reinstall_hint();
        assert!(hint.contains("flute2-installer.sh"));
        assert!(hint.contains("flute2-installer.ps1"));
        assert!(hint.contains("getflute/tap/flute2"));

        // Two binaries sharing one install receipt is how an update swaps the
        // wrong one. The path is derived from APP_NAME; assert it anyway.
        assert!(receipt_path().to_string_lossy().contains("flute2"));

        // Nothing may name v1. `flute2` contains `flute`, so the check is for
        // the v1 names specifically, not for the substring.
        for s in [hint.as_str(), REPO_NAME, APP_NAME] {
            assert!(!s.contains("getflute/tap/flute\""), "{s}: v1 formula");
            assert!(!s.contains("flute-installer"), "{s}: v1 installer");
        }
        assert_ne!(REPO_NAME, "flute-cli");
    }
}
```

```rust
// in src/update_check.rs
#[cfg(test)]
mod tests {
    use super::*;

    /// v1 asserts its notice says "flute update"; ours must say "flute2".
    #[test]
    fn the_update_notice_names_the_v2_binary() {
        let n = notice_for("2.1.0");
        assert!(n.contains("flute2 update"));
    }

    #[test]
    fn opt_outs_are_honoured() {
        let cfg = crate::config::Config { auto_update_check: false, ..Default::default() };
        assert!(opt_out(&cfg));

        let on = crate::config::Config::default();
        temp_env::with_var("FLUTE2_NO_UPDATE_CHECK", Some("1"), || assert!(opt_out(&on)));
        temp_env::with_var("CI", Some("1"), || assert!(opt_out(&on)));
        temp_env::with_vars(
            [("FLUTE2_NO_UPDATE_CHECK", None::<&str>), ("CI", None::<&str>)],
            || assert!(!opt_out(&on)));
    }
}
```

- [ ] **Step 2: Run and watch them fail**

Run: `cargo test --test cmd_auth --test cmd_utility`
Expected: FAIL — subcommands do not exist.

- [ ] **Step 3: Transplant the five auth verbs and the utility commands**

Carry v1's `cli/auth.rs`, `update.rs`, `update_check.rs`, and the `version` half of `cli/util.rs`. Adaptations:

**`update.rs` carries v1's identity in constants, and a transplant that misses them makes `flute2 update` operate on v1.** This is the one transplanted module whose bug is not a wrong result but a wrong *target*: it would query v1's releases, and on an installer-managed machine hand v1's installer a v2 update, overwriting the wrong binary. Every one of these must change, and each gets an asserted constant rather than trust:

| v1 | v2 |
|---|---|
| `APP_NAME = "flute"` | `"flute2"` |
| `REPO_NAME = "flute-cli"` (owner `getflute` unchanged) | `"flute-cli-v2"` |
| `flute-installer.sh` / `flute-installer.ps1` in the reinstall hint | `flute2-installer.sh` / `flute2-installer.ps1` |
| Homebrew `getflute/tap/flute` | `getflute/tap/flute2` |
| Install receipt `~/.config/flute/flute-receipt.json` | the `flute2` receipt |

The receipt path is derived from `APP_NAME`, so fixing that constant moves it — but assert the resolved path anyway, because two binaries sharing one receipt is precisely the failure that silently swaps them. `update_check.rs` also asserts its notice text mentions `flute update`; that string becomes `flute2 update`.

Test the constants directly. A wrong value here produces a perfectly well-formed request to the wrong repository, which no other test in the suite would notice — the same argument that puts the four host constants under test in Task 6.

- `auth login` keeps v1's `rpassword` prompt so the secret never reaches the shell history or the process table.
- `auth status` keeps v1's structure: resolve credentials, and only if present attempt an authenticated `GET /v2/ping`; any failure leaves `authenticated: false` rather than erroring.
- `update_check::opt_out` keeps v1's three gates — the config key, `FLUTE2_NO_UPDATE_CHECK`, and a bare `CI`.
- The update notice goes to **stderr**, is skipped under `--output json`, and never fails the command that carried it.

- [ ] **Step 4: Run and watch them pass**

Run: `cargo test --test cmd_auth --test cmd_utility && cargo test update`
Expected: PASS — 10 command tests, plus the update-identity and notice unit tests.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "ARISE-4928 feat(auth): add auth verbs and utility commands"
```

---

