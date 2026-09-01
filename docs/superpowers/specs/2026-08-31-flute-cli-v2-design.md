# Flute CLI v2 — Design

**Ticket:** [ARISE-4928](https://rwaapps.atlassian.net/browse/ARISE-4928)
**Repository:** `getflute/flute-cli-v2`
**Status:** Final — open questions resolved

---

## 1. Summary

`flute2` is a cross-platform CLI for the Flute payments platform, MIT licensed, written in Rust. This document designs the v2 CLI: a new project targeting the v2 API, succeeding the v1 CLI (`flute`) at `getflute/flute-cli`.

The v2 API exposes 50 non-webhook operations across 11 resource groups plus an OAuth token endpoint. The CLI covers all of them. Webhooks are explicitly out of scope.

The v2 API is published as **V2 Beta** — "functionality, request and response formats, field names, and behavior may change before the stable release." That shapes several decisions below, most visibly the spec-conformance test layer and the documentation-defect log.

## 2. Scope

### In scope

All 50 non-webhook v2 operations:


| Group            | Ops | Group                   | Ops |
| ---------------- | --- | ----------------------- | --- |
| Transactions     | 11  | Merchant API Keys       | 3   |
| Payment Methods  | 7   | Payment Sessions        | 3   |
| Payment Links    | 6   | Settlements             | 2   |
| Customers        | 5   | Terminals               | 2   |
| POS Transactions | 5   | Ping                    | 1   |
| Settings         | 4   | API Token Authorization | 1   |


Plus client-side commands carried over from v1: `auth` (login/logout/status/switch/token), `version`, `update`, `completion`, and `transactions inspect`.

### Out of scope

- **Webhooks** (11 operations) — excluded by the ticket.
- **Devices / Tap-to-Pay** (5 v1 commands) — no v2 endpoints exist.
- **Subscriptions** (5 v1 commands) — no v2 endpoints exist.

Devices and subscriptions are dropped rather than stubbed or pointed at v1 endpoints, so the binary is honestly a v2 client. The readme and `agents.md` record them as unavailable in v2. A feature with no v2 endpoint stays unimplemented: it is not stubbed, not aliased to v1, and not registered as a command that exits with an explanation. It arrives when the endpoint does. Users needing devices or subscriptions run the v1 CLI, which is why the two binaries install side by side.

### v1 → v2 command changes

**Renamed or removed.** Each v1 command in this table is replaced. The v1 name is not accepted — invoking it is an unrecognised-subcommand error.


| v1                                                 | v2                                                                                    | Reason                                                       |
| -------------------------------------------------- | ------------------------------------------------------------------------------------- | ------------------------------------------------------------ |
| `transactions sale` / `auth`                       | `transactions create --capture-method auto|manual`                                    | One endpoint; sale and authorization differ only by a field  |
| `transactions void` / `refund`                     | `transactions reversal`                                                               | One endpoint; payment method and settled state auto-detected |
| `transactions settle`                              | `settlements close`                                                                   | Operation relocated to the settlements group                 |
| `ach debit/credit/void/refund`                     | `transactions create --ach-*`, `transactions credit`, `transactions reversal`         | ACH folded into the unified transaction endpoints            |
| `customers add-card/add-ach/methods/remove-method` | `payment-methods add-card/add-ach/list/delete`                                        | Payment methods are a top-level resource in v2               |
| `keys`                                             | `api-keys`                                                                            | Endpoint is `/v2/api-keys`                                   |
| `devices *`, `subscriptions *`                     | —                                                                                     | No v2 endpoints                                              |
| —                                                  | `payment-links`, `payment-sessions`, `settings`, and 8 new transaction/POS operations | New in v2                                                    |


**Unchanged.** `auth` (`login`, `logout`, `status`, `switch`, `token`), `version`, `update`, and `completion` keep their v1 names and behaviour. None of them changed in v2; `auth token` and `ping` call v2 endpoints but their command shape is identical.

The naming principle: **every command that calls the API is named after the v2 operation it invokes, with no v1 aliases.** A 2.0 release in a new repository is the point at which legacy verbs are dropped, and the choice is reversible in the safe direction — an alias can be added in a later minor version without breaking anyone, whereas removing one later would.

## 3. Architecture

### Module layout

Organised by group cohesion, so one pull request touches one group module and its tests.

```
src/
  main.rs                     thin entry
  lib.rs                      run(): tracing, output resolution, offline/online split, dispatch
  config.rs               [T] profiles, ~/.flute2/config.toml
  auth/
    keychain.rs           [T] OS keychain + env fallback
    token.rs              [T] TokenStore/Fetcher; v2 OAuth2Fetcher
  api/
    client.rs             [T] send/issue, 401 retry
    redact.rs             [T] PCI masking
    error.rs              [T] error envelope parser
    endpoints/*.rs            method + path wrappers, one file per group
  cli/
    output.rs             [T] envelope, ErrorJson, exit codes
    money.rs              [T] decimal parsing
    address.rs                v2 address shape
    common.rs                 shared args: pagination, billing, contact
  groups/*.rs                 clap enum + body builders + dispatch + renderers
```

`[T]` marks modules transplanted from the v1 CLI and adapted. These are the non-API-shaped components — roughly 1,700 lines against ~13,000 written new. They are carried over rather than rewritten because a fresh implementation of PCI log masking, decimal precision, or the public output contract risks reintroducing solved problems with no test able to see the regression. Their comments are rewritten to house style; v1's carry ticket references and change history that this repo's conventions exclude.

`redact.rs` is split out of the v1 client module, where masking and HTTP transport shared one file. It is security-critical and belongs on its own.

### The injectable client

v1 called `build_client(profile)` inside every dispatch arm, which resolved credentials from the OS keychain before any request could be constructed. There was therefore no seam at which a test could point the CLI at a mock server, and v1's integration suite contains **no request-body assertions** as a direct result.

v2 builds the client once in `run()` and passes it down:

```rust
pub struct Ctx {
    pub api: ApiClient,
    pub profile: Profile,
    pub output: OutputFormat,
}
```

Each group exposes `dispatch(&Ctx, Args) -> anyhow::Result<()>`.

No transport trait and no mock object: wiremock provides a real HTTP boundary, and a `base_url` pointed at it is a truer test than a hand-written fake. The trade-off is that tests use real HTTP to localhost rather than an in-memory double — marginally slower, and simulating exotic transport failures takes extra work. Accepted because sending wrong bytes is this project's dominant risk, and only a real socket proves the bytes.

### Offline/online split

`completion`, `version`, `update`, and `auth login/logout/switch` are handled before any `Ctx` exists and never touch the keychain. Everything else receives a `Ctx`. One credential resolution, one production banner, one place to reason about.

### Test seams

1. **In-process** — construct `Ctx` with `base_url` set to a mock server and call a group's `dispatch` directly.
2. **Binary-level** — `FLUTE2_API_BASE_URL` and `FLUTE2_OAUTH_URL` aim the compiled binary at a mock server, driven through `assert_cmd`.

Both assert on the recorded request: exact method, path, and JSON body. Credentials come from `FLUTE2_CLIENT_ID` / `FLUTE2_CLIENT_SECRET`, which take precedence over the keychain, so no test touches the OS keychain.

**The base-URL overrides are refused when `--profile production` is active.** A test hook able to silently redirect production payment traffic is a liability; combining them is a hard error.

## 4. Command surface


| Group              | Commands                                                                                                                                          |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| `auth`             | `login`, `logout`, `status`, `switch`, `token`                                                                                                    |
| `ping`             | —                                                                                                                                                 |
| `customers`        | `create`, `get`, `list`, `update`, `delete`                                                                                                       |
| `payment-methods`  | `list`, `get`, `add-card`, `add-ach`, `update`, `delete`, `set-default`                                                                           |
| `transactions`     | `create`, `get`, `list`, `capture`, `reversal`, `credit`, `tip-adjust`, `ach-hold`, `ach-release`, `calculate-amount`, `share-receipt`, `inspect` |
| `pos`              | `create`, `get` (`--wait`), `list`, `cancel`, `print-receipt`                                                                                     |
| `terminals`        | `list`, `status`                                                                                                                                  |
| `settlements`      | `list`, `close`, `get`                                                                                                                            |
| `payment-links`    | `create`, `get`, `list`, `update`, `delete`, `share`                                                                                              |
| `payment-sessions` | `create`, `get`, `cancel`                                                                                                                         |
| `settings`         | `payment-config`, `contact-info`, `autofill`, `update-autofill`                                                                                   |
| `api-keys`         | `create`, `list`, `revoke`                                                                                                                        |
| utility            | `version`, `update`, `completion`                                                                                                                 |


Global flags are unchanged from v1: `--profile`, `--output table|json|quiet`, `--debug`.

### Pagination

`--page-index` (0-based) and `--page-size` mirror the API's `pageIndex` / `pageSize` exactly. v1 used a 1-based `--page` with `--limit`; translating would be friendlier in isolation but introduces a silent off-by-one against every example in the API documentation. The help text states that the index is 0-based.

Every `list` command also accepts `--all`, which follows `pageInfo.hasMore` to exhaust the collection. This is the one place the CLI adds something the API does not have, and it is cheap precisely because the API's response shape provides `hasMore`.

### Client-side operations

Three commands have no corresponding endpoint:

- `transactions inspect` — a detailed single-transaction view, carried over from v1.
- `settlements get` — v2 still has no single-batch endpoint; the command fetches a page and filters, and says so in its help text.
- `auth *` — credential and profile management, entirely local.

### Client-side validation

`transactions create` mirrors the server's four documented rules before issuing a request, so a malformed invocation exits 3 without a round trip: base amount greater than zero, payment processor id present, transaction details present, and exactly one of card or ACH data. v1 sent requests with no payment instrument at all, which the v1 API answered with a 500.

`--payment-processor-id` is always required. v2 requires `paymentProcessorId` on every transaction and v1 did not, so this is the one flag every existing charge invocation must gain. The CLI could default it when an account has exactly one processor configured, which would remove the friction for most users; it does not, because that makes the required arguments a function of account configuration — a script written against a single-processor account would fail on a multi-processor one, at the point of taking a payment. The missing-flag error names `settings payment-config` as the way to find the value.

### Behaviour worth surfacing

`api-keys` sits behind a server feature flag; with the flag disabled the endpoint returns 404, which reads as a wrong URL rather than a disabled feature. A 404 from this group specifically produces an error message saying so.

## 5. Output and error contract

### Modes and streams

Three modes — `table` (default), `json`, `quiet` — resolved `--output` → `FLUTE2_OUTPUT` → config file → default. The environment variable is new: v1 resolves flag → config → default, and reads `FLUTE_OUTPUT` only in the pre-parse path that decides whether a clap usage error prints as JSON, where it recognises `json` and nothing else. Putting it in the chain makes one variable mean the same thing before and after argument parsing. **Data goes to stdout; everything else goes to stderr**: tracing, the production banner, update notices. This holds under `--debug`, so `flute2 --debug --output json` still emits parseable JSON on stdout.

### Success envelope

```json
{ "object": "transaction",
  "data": { },
  "meta": { "environment": "sandbox",
            "correlation_id": "…",
            "page_info": { "pageIndex": 0, "pageSize": 20, "totalItems": 17, "hasMore": true } } }
```

Two changes from v1. `page_info` is carried on collection responses so an agent can paginate without parsing table output. `correlation_id` is populated from the response on success, not only on failure, so a user can quote an identifier when something looks wrong.

### Error envelope

```json
{ "kind": "api|transport|auth|decode|client", "message": "…", "status": 422, "correlation_id": "…" }
```

Written to **stdout** under `--output json` so a machine consumer never sees an empty stdout, and to stderr otherwise. CLI usage and parse errors are included, as `kind: "client"` with exit 3.

### Exit codes


| Code | Meaning                                                                      |
| ---- | ---------------------------------------------------------------------------- |
| 0    | Success, **including a declined transaction**                                |
| 1    | General / unexpected — transport, decode, 5xx, and 402 / 409 / 429           |
| 2    | Auth — 401 / 403                                                             |
| 3    | Validation / bad input — 400 / 422, client-side validation, CLI usage errors |
| 4    | Not found — 404                                                              |
| 130  | Interrupted — Ctrl-C during `pos get --wait`                                 |


Identical to v1, including 130, which v1 emits but omits from its own exit-code table — it is documented only in the prose of v1's `agents.md`. The other `pos get --wait` outcome, an expired `--wait-timeout`, prints the last-known success envelope to stdout, a warning to stderr, and exits 1 through the general arm. v2 introduces 402, 409, and 429, which fall through the existing general-error arm rather than claiming new codes.

A decline exits 0, and the caller reads `transactionStatus` from the response to learn whether payment succeeded. This holds because a declined card is answered with HTTP 200 and a declined status rather than 402 — observed on the sandbox against v1 endpoints, and re-verified against v2 during the live sandbox pass before release. If v2 turns out to answer some declines with 402, the same decline would exit 1 through one path and 0 through the other, and a dedicated decline code becomes worth adding; that addition is backwards compatible, because nothing can depend on an exit code the CLI has never emitted.

### Error parsing

The specification documents one error envelope, shared across every operation via `components.responses`. The API returns **four distinct shapes**, so the parser handles all four:


| Source                                | Shape                                                                                                                                   |
| ------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| Downstream validation and most errors | The documented envelope, PascalCase: `StatusCode`, `Source`, `ExceptionType`, `CorrelationId`, `Errors`, `Title`, `Cause`, `Resolution` |
| Edge model-validation                 | RFC 9110 ProblemDetails: `type`, `title`, `status`, `errors`, `traceId` — no correlation id                                             |
| Token endpoint                        | OpenIddict: `error`, `error_description`, `error_uri`                                                                                   |
| Some 401s                             | Empty body; `www-authenticate` header only                                                                                              |


The v1 parser is carried over and already tolerates both casings and flattens a field-error map, so it handles the first two with minor additions: read `Cause` and `Resolution` (the envelope has no `Details`-equivalent that v1 looked for), and fall back to `traceId` when `correlationId` is absent.

Field-level validation messages are keyed by public camelCase field paths (`baseAmount`, `transactionDetails.cardData.paymentMethodDetails.cardNumber`), which the existing flattening renders directly.

One documented envelope against four observed shapes is a divergence from the reference, not a quirk of this client, and is raised as a defect rather than silently accommodated.

## 6. Testing strategy

No test suite proves a payments CLI correct. The goal is that every failure mode has a layer able to see it. The dominant risk is not logic error but sending wrong bytes to a beta API, which unit tests cannot detect because they only prove the code agrees with itself.


| Layer                  | Proves                                                                          | Cannot prove                         |
| ---------------------- | ------------------------------------------------------------------------------- | ------------------------------------ |
| 1. Unit                | Body builders, renderers, money parsing, redaction, error parsing, exit codes   | That the API wants those field names |
| 2. Command → HTTP      | Exact method, path, and JSON body from a real argv invocation                   | That the API accepts them            |
| 3. Spec conformance    | Every request validates against the published schema; every path we call exists | Anything the spec gets wrong         |
| 4. Contract snapshots  | The clap tree and rendered output do not change accidentally                    | Correctness, only stability          |
| 5. Live sandbox        | The API accepts our bytes                                                       | Production behaviour                 |
| 6. Security regression | Card numbers, CVVs, bearer tokens, and secrets never reach the log sink         | —                                    |


Layer 2 is the layer v1 lacks entirely, and it is enabled by the injectable client.

Layer 3 vendors the OpenAPI bundle into `docs/reference/`. A **hash gate** fails CI when the vendored spec changes, forcing a human to read the diff rather than drift silently — necessary because the API is in beta. Known divergences live in `docs/doc-defects.md`; an unlisted divergence fails the build.

Layer 5 lives in `tests/live/`, which is **gitignored**. It never runs in CI and is not published. Sandbox verification is a local pass, run by hand before a release.

The reason it is uncommitted rather than merely skipped: a live test has to name real account identifiers — payment processor ids, merchant id, customer ids — and this repository is public. Keeping the tests out of version control keeps sandbox account data out of it too.

The tests stay `#[ignore]`d even locally, so a plain `cargo test` remains hermetic and offline; `cargo test -- --ignored` opts in. Each run needs a unique transaction amount, because the API rejects a repeated amount within a short window.

The trade-off, accepted deliberately: the live tests are not reviewable, not shared with teammates, and not recoverable if the working copy is lost. The executable record of what the API accepts lives on one machine. Group-level request-shape tests (layer 2) are committed and carry no account data, so what is verifiable in CI stays verifiable.

Tests are written before implementation. Every group's definition of done is mechanical: builder unit tests, a command→HTTP test, spec conformance or a filed defect, renderer golden files, a help snapshot, and a live sandbox call. `cargo test`, `cargo clippy --all-targets -- -D warnings`, and `cargo fmt --check` pass at every commit, with the command and its pass/fail totals recorded in the summary.

### Tests carried over from v1

The redaction end-to-end guard ports near-verbatim and is extended to cover bearer tokens and client secrets. Unit tests inside the transplanted modules come with them. The shell-completion and exit-code contract tests port with edits. v1's remaining integration tests are predominantly `--help` assertions against v1 response shapes and do not carry over; the devices and subscriptions suites are deleted with their groups.

## 7. Repository, build, and release

- Binary `flute2`, version `2.0.0`, `rust-version = "1.85"` (edition 2024's floor).
- Release targets via cargo-dist: `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`, `x86_64-unknown-linux-gnu`, with shell and PowerShell installers and a Homebrew formula.
- CI runs `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and the spec hash gate on push and pull request. Third-party actions are SHA-pinned.
- `#![forbid(unsafe_code)]` as in v1.

### Name

The crate, the binary, and the Homebrew formula are all `flute2`. `brew install getflute/tap/flute2` installs `bin/flute2`, so v1 and v2 coexist: no `conflicts_with`, no migration deadline, and v1's formula is left alone. Coexistence is required rather than merely convenient, because devices and subscriptions exist only in v1 and this binary does not implement them.

The cost is a permanent name. `flute2` cannot be renamed to `flute` once v1 retires without breaking every script written against it. Accepted: the alternative — one binary name and a `conflicts_with` — makes reaching v2 conditional on giving up two feature groups.

Two binaries on one machine must not share mutable state, so v2 namespaces everything it owns: `~/.flute2/config.toml`, its own OS keychain service, and `FLUTE2_`-prefixed environment variables. Credentials are not shared with v1 even where the values would be identical; the two binaries authenticate against different token hosts, and one variable meaning two things is the failure this avoids. CI that already exports the v1 names sets the `FLUTE2_` names alongside them.

### Documents


| Path                          | Purpose                                                                                                                   |
| ----------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `readme.md`                   | User-facing documentation, v1's structure                                                                                 |
| `agents.md`                   | Machine-readable contract for agents driving the CLI                                                                      |
| `LICENSE`                     | MIT. v1 claims MIT in its readme but ships no LICENSE file, so the licence is not detected and the claim is unenforceable |
| `docs/superpowers/specs/`     | This document                                                                                                             |
| `docs/superpowers/plans/`     | Ordered build plan; its checkboxes are the project status                                                                 |
| `docs/doc-defects.md`         | Divergences between the published API documentation and observed behaviour, with a reproduction for each                  |
| `docs/reference/`             | Vendored OpenAPI bundle, hash-gated                                                                                       |
| `CLAUDE.md`, `NOTES.local.md` | Contributor working notes. Not committed                                                                                  |
| `tests/live/`                 | Live sandbox verification. Not committed — carries real account identifiers                                               |


There is deliberately no separate status document. Status lives in the plan's checkboxes, which are updated as part of doing the work, with merged pull requests and git history as the ground truth beneath.

## 8. Documentation defects

The published API reference is the primary source for this implementation. Where it diverges from observed behaviour, the divergence is a defect to be filed, not a quirk to be silently accommodated.

The full list, with a reproduction command for each, lives in [`docs/doc-defects.md`](../../doc-defects.md).

## 9. Build order

Work proceeds as a vertical slice, then one pull request per group.

**Slice** — `ping`, `customers create/get`, and `transactions` card create/get, taken end to end: scaffolding, transplanted core, v2 authentication, the spec-conformance harness, command→HTTP tests, renderers, and a live sandbox call. This selection is deliberately risk-loaded: it exercises OAuth against the real API, money precision, PCI redaction of a real card number, error parsing, all three output modes, and the two transaction fields the specification omits. It is roughly 600 lines and proves the architecture before the remaining operations are built on it.

**Then, one group per pull request**, in dependency order: `payment-methods`, the remaining `transactions` operations, `pos`, `terminals`, `settlements`, `settings`, `payment-links`, `payment-sessions`, `api-keys`.

**Then** contract snapshots across the whole command tree, the extended redaction guard, a live sandbox verification pass, and finally `readme.md` and `agents.md`.

Each group is reviewed against the same checklist, so reviewing the eleventh is a matter of confirming it matches the first.