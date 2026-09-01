# v1 CLI — defects found while porting

Bugs in the v1 CLI (`getflute/flute-cli`) surfaced while building v2 against it as the behavioural reference.

They are recorded here rather than in [`doc-defects.md`](doc-defects.md), which is about the API's published reference. These are about our own prior client.

Each entry says what v2 does instead, because "v1 is the reference for behaviour" needs an explicit exception wherever v1 is wrong. **A v1 bug is not a behaviour to preserve.** Deliberately copying one would need a reason recorded here; none currently do.

**Status** is tracked per entry:

- **To raise** — confirmed, not yet filed against the v1 repository.
- **Filed** — an issue exists on `getflute/flute-cli`.

| ID | Defect | Severity | Status |
|---|---|---|---|
| V1 | `auth switch` has no effect | High — the command reports success and changes nothing | To raise |
| V2 | Debug traces do not mask bearer tokens or client secrets | High — credential disclosure in logs | To raise |
| V3 | Token-fetch errors discard the response body | Medium — authentication failures are unactionable | To raise |
| V4 | Transactions are sent with no payment instrument | Medium — a `500` where a local error belongs | To raise |
| V5 | The documented exit-code table omits `130` | Low — documentation | To raise |
| V6 | MIT licence is claimed but no `LICENSE` file ships | Low — licence is undetectable and the claim unenforceable | To raise |
| V7 | A half-set credential pair silently falls through to the keychain | Medium — a CI job can authenticate as the wrong account | To raise |

---

### V1 — `auth switch` has no effect

**Severity:** High. The command prints a success message and changes nothing.

`cli/auth.rs:99-106` writes the chosen profile to `config.default_profile` and saves the file:

```rust
cfg.default_profile = new_profile.to_string();
config::save(&cfg)?;
println!("Default profile set to [{new_profile}].");
```

But `default_profile` is never read. The only other references are its declaration and its default (`config.rs:8,16`) and one test (`config.rs:124`). Nothing consults it at startup, because `--profile` carries a clap `default_value`:

```rust
#[arg(long, env = "FLUTE_PROFILE", default_value = "sandbox", global = true, ...)]
pub profile: String,
```

A defaulted flag is never absent, so there is no state in which the stored value could be consulted. `flute auth switch production` therefore reports success, writes the file, and leaves every subsequent command on sandbox.

The failure is silent and points the wrong way: a user who runs `auth switch production` and sees the confirmation has every reason to believe later commands hit production. They hit sandbox. The inverse — believing you are on sandbox while pointed at production — is not reachable this way, which is the only reason this is not critical.

**v2:** `--profile` is `Option<String>` and resolves `--profile` → `FLUTE2_PROFILE` → `config.default_profile` → `sandbox`. Distinguishing "absent" from "sandbox" is what makes the config step reachable at all.

### V2 — Debug traces do not mask bearer tokens or client secrets

**Severity:** High. Credential material reaches the log sink.

v1's redaction masks card numbers to the last four and removes security codes, and its end-to-end guard (`tests/redaction_e2e.rs`) asserts exactly that. Neither the masking nor the guard covers the OAuth bearer token or the `client_secret` in the token-request form body, both of which pass through the same traced HTTP path under `--debug`.

The card-data masking is sound; the gap is that the credential material added by the auth layer was never brought under it.

**v2:** `redact.rs` covers PANs, security codes, ACH account numbers, bearer tokens, and client secrets, and the layer-6 guard asserts all five against a real `--debug` invocation.

### V3 — Token-fetch errors discard the response body

**Severity:** Medium.

`auth/token.rs:104` handles a failed token fetch with:

```rust
.error_for_status()?
```

which turns the response into a status-code error and drops the body. The token endpoint returns OpenIddict-shaped errors — `error`, `error_description`, `error_uri` — so the actionable part (`invalid_client`, "Client authentication failed") is thrown away and the user sees a bare status code.

This is the error users hit most often, since it fires on every mistyped or expired credential.

**v2:** the fetcher routes failures through the shared error parser, which has an OpenIddict arm.

### V4 — Transactions are sent with no payment instrument

**Severity:** Medium.

`build_sale_body` (`cli/transactions.rs:123`) inserts `accountNumber` only when `--card` is supplied, and nothing requires a card, a token, or a saved payment method:

```rust
if let Some(card) = &args.card {
    obj.insert("accountNumber".into(), Value::String(card.clone()));
}
```

`flute transactions sale --amount 10.00` therefore sends a well-formed request describing no way to pay, and the v1 API answers `500`. A local error naming the missing flag costs no round trip and does not look like a server fault.

**v2:** `transactions create` checks four rules before issuing a request — positive base amount, processor id present, transaction details present, exactly one of card or ACH data — and exits 3 locally.

### V5 — The documented exit-code table omits `130`

**Severity:** Low. Documentation only; the behaviour is correct.

`agents.md:124` lists `0`, `1`, `2`, `3`, and `4`. The CLI also exits `130` when Ctrl-C interrupts a `pos create --wait` poll (`lib.rs:454`), and that is described only in prose at `agents.md:71` — outside the table an agent would parse.

An agent reading the table treats `130` as unknown, when it has a precise meaning worth acting on.

**v2:** `130` is in the table.

### V6 — MIT licence claimed, no `LICENSE` file

**Severity:** Low.

`Cargo.toml:6` sets `license = "MIT"` and the readme states MIT, but no `LICENSE` file exists at the repository root. GitHub's licence detection finds nothing, and the claim has no accompanying licence text to enforce.

**v2:** ships a `LICENSE` file.

### V7 — A half-set credential pair silently falls through to the keychain

**Severity:** Medium.

`auth/keychain.rs:49-57` treats "both variables set" as the only success and returns `None` otherwise:

```rust
match (std::env::var("FLUTE_CLIENT_ID"), std::env::var("FLUTE_CLIENT_SECRET")) {
    (Ok(id), Ok(secret)) => Some((id, secret)),
    _ => None,
}
```

`load_with_env_fallback` then falls through to the keychain. Setting one variable and misspelling the other is therefore indistinguishable from setting neither: the CLI quietly authenticates as whoever last ran `auth login` on that machine.

The failure is worst where it is least visible. On a developer laptop the keychain holds sandbox credentials and the result is confusing but harmless. On a shared or long-lived runner, a typo'd secret name means a job intended to run as one account runs as another, and nothing in the output says so.

Half-configured is not the same as unconfigured, and only the latter is a request to use the keychain.

**v2:** `creds_from_env` returns `Result<Option<_>>`. Neither variable set is `Ok(None)` and falls through to the keychain; exactly one set is an error naming the missing variable.
