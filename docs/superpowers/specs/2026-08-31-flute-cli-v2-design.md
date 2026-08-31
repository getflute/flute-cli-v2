# Flute CLI v2 — Design

**Ticket:** [ARISE-4928](https://rwaapps.atlassian.net/browse/ARISE-4928)
**Repository:** `getflute/flute-cli-v2`
**Status:** Draft — awaiting review

---

## 1. Summary

`flute` is a cross-platform CLI for the Flute payments platform, MIT licensed, written in Rust. This document designs the v2 CLI: a new project targeting the v2 API, replacing the v1 CLI at `getflute/flute-cli`.

The v2 API exposes 50 non-webhook operations across 11 resource groups plus an OAuth token endpoint. The CLI covers all of them. Webhooks are explicitly out of scope.

The v2 API is published as **V2 Beta** — "functionality, request and response formats, field names, and behavior may change before the stable release." That shapes several decisions below, most visibly the spec-conformance test layer and the documentation-defect log.

## 2. Scope

### In scope

All 50 non-webhook v2 operations:

| Group | Ops | Group | Ops |
|---|---|---|---|
| Transactions | 11 | Merchant API Keys | 3 |
| Payment Methods | 7 | Payment Sessions | 3 |
| Payment Links | 6 | Settlements | 2 |
| Customers | 5 | Terminals | 2 |
| POS Transactions | 5 | Ping | 1 |
| Settings | 4 | API Token Authorization | 1 |

Plus client-side commands carried over from v1: `auth` (login/logout/status/switch/token), `version`, `update`, `completion`, and `transactions inspect`.

### Out of scope

- **Webhooks** (11 operations) — excluded by the ticket.
- **Devices / Tap-to-Pay** (5 v1 commands) — no v2 endpoints exist.
- **Subscriptions** (5 v1 commands) — no v2 endpoints exist.

Devices and subscriptions are dropped rather than stubbed or pointed at v1 endpoints, so the binary is honestly a v2 client. The readme and `agents.md` record them as unavailable in v2. See [Open question 1](#q1-confirm-dropping-devices-and-subscriptions).

### v1 → v2 command changes

| v1 | v2 | Reason |
|---|---|---|
| `transactions sale` / `auth` | `transactions create --capture-method auto\|manual` | One endpoint; `sale`/`auth` retained as aliases |
| `transactions void` / `refund` | `transactions reversal` | One endpoint, payment method auto-detected; both retained as aliases |
| `transactions settle` | `settlements close` | Operation relocated to the settlements group |
| `ach debit/credit/void/refund` | `transactions create --ach-*`, `credit`, `reversal` | ACH folded into the unified transaction endpoints |
| `customers add-card/add-ach/methods/remove-method` | `payment-methods add-card/add-ach/list/delete` | Payment methods are a top-level resource in v2 |
| `keys` | `api-keys` (alias `keys`) | Endpoint is `/v2/api-keys` |
| `devices *`, `subscriptions *` | — | No v2 endpoints |
| — | `payment-links`, `payment-sessions`, `settings`, and 8 new transaction/POS operations | New in v2 |

The naming principle: **the canonical command mirrors the API; a v1 verb survives as an alias where it maps onto exactly one canonical invocation.** Aliases preset a flag; they are not a second code path.

## 3. Architecture

### Module layout

Organised by group cohesion, so one pull request touches one group module and its tests.

```
src/
  main.rs                     thin entry
  lib.rs                      run(): tracing, output resolution, offline/online split, dispatch
  config.rs               [T] profiles, ~/.flute/config.toml
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
2. **Binary-level** — `FLUTE_API_BASE_URL` and `FLUTE_OAUTH_URL` aim the compiled binary at a mock server, driven through `assert_cmd`.

Both assert on the recorded request: exact method, path, and JSON body. Credentials come from `FLUTE_CLIENT_ID` / `FLUTE_CLIENT_SECRET`, which take precedence over the keychain, so no test touches the OS keychain.

**The base-URL overrides are refused when `--profile production` is active.** A test hook able to silently redirect production payment traffic is a liability; combining them is a hard error.

## 4. Command surface

| Group | Commands |
|---|---|
| `auth` | `login`, `logout`, `status`, `switch`, `token` |
| `ping` | — |
| `customers` | `create`, `get`, `list`, `update`, `delete` |
| `payment-methods` | `list`, `get`, `add-card`, `add-ach`, `update`, `delete`, `set-default` |
| `transactions` | `create` (aliases `sale`, `auth`), `get`, `list`, `capture`, `reversal` (aliases `void`, `refund`), `credit`, `tip-adjust`, `ach-hold`, `ach-release`, `calculate-amount`, `share-receipt`, `inspect` |
| `pos` | `create`, `get` (`--wait`), `list`, `cancel`, `print-receipt` |
| `terminals` | `list`, `status` |
| `settlements` | `list`, `close`, `get` |
| `payment-links` | `create`, `get`, `list`, `update`, `delete`, `share` |
| `payment-sessions` | `create`, `get`, `cancel` |
| `settings` | `payment-config`, `contact-info`, `autofill`, `update-autofill` |
| `api-keys` (alias `keys`) | `create`, `list`, `revoke` |
| utility | `version`, `update`, `completion` |

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

### Behaviour worth surfacing

`api-keys` sits behind a server feature flag; with the flag disabled the endpoint returns 404, which reads as a wrong URL rather than a disabled feature. A 404 from this group specifically produces an error message saying so.

## 5. Output and error contract

### Modes and streams

Three modes — `table` (default), `json`, `quiet` — resolved flag → environment variable → config file → default. **Data goes to stdout; everything else goes to stderr**: tracing, the production banner, update notices. This holds under `--debug`, so `flute --debug --output json` still emits parseable JSON on stdout.

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

| Code | Meaning |
|---|---|
| 0 | Success, **including a declined transaction** |
| 1 | General / unexpected — transport, decode, 5xx, and 402 / 409 / 429 |
| 2 | Auth — 401 / 403 |
| 3 | Validation / bad input — 400 / 422, client-side validation, CLI usage errors |
| 4 | Not found — 404 |

Identical to v1. v2 introduces 402, 409, and 429, which fall through the existing general-error arm rather than claiming new codes. See [Open question 3](#q3-decline-signalling).

### Error parsing

The v2 specification defines **no schema and no description** for any 4xx or 5xx response across all 46 paths, so error handling cannot be built from the primary source. The v1 parser is carried over: it handles both casings of the platform's error envelope, combines title with details, appends the exception type, and flattens field-level validation errors into a readable string.

v2 keys validation messages by public camelCase field paths (`baseAmount`, `transactionDetails.cardData.paymentMethodDetails.cardNumber`) rather than v1's PascalCase field names, which the existing flattening handles unchanged and renders more usefully.

**The error envelope is treated as unverified until the live sandbox pass.**

## 6. Testing strategy

No test suite proves a payments CLI correct. The goal is that every failure mode has a layer able to see it. The dominant risk is not logic error but sending wrong bytes to a beta API, which unit tests cannot detect because they only prove the code agrees with itself.

| Layer | Proves | Cannot prove |
|---|---|---|
| 1. Unit | Body builders, renderers, money parsing, redaction, error parsing, exit codes | That the API wants those field names |
| 2. Command → HTTP | Exact method, path, and JSON body from a real argv invocation | That the API accepts them |
| 3. Spec conformance | Every request validates against the published schema; every path we call exists | Anything the spec gets wrong |
| 4. Contract snapshots | The clap tree and rendered output do not change accidentally | Correctness, only stability |
| 5. Live sandbox | The API accepts our bytes | Production behaviour |
| 6. Security regression | Card numbers, CVVs, bearer tokens, and secrets never reach the log sink | — |

Layer 2 is the layer v1 lacks entirely, and it is enabled by the injectable client.

Layer 3 vendors the OpenAPI bundle into `docs/reference/`. A **hash gate** fails CI when the vendored spec changes, forcing a human to read the diff rather than drift silently — necessary because the API is in beta. Known divergences live in `docs/doc-defects.md` with a filed ticket each; an unlisted divergence fails the build.

Layer 5 runs `#[ignore]`d by default against sandbox credentials, invoked by hand before a release rather than in CI. See [Open question 5](#q5-sandbox-credentials-in-ci).

Tests are written before implementation. Every group's definition of done is mechanical: builder unit tests, a command→HTTP test, spec conformance or a filed defect, renderer golden files, a help snapshot, and a live sandbox call. `cargo test`, `cargo clippy --all-targets -- -D warnings`, and `cargo fmt --check` pass at every commit, with the command and its pass/fail totals recorded in the summary.

### Tests carried over from v1

The redaction end-to-end guard ports near-verbatim and is extended to cover bearer tokens and client secrets. Unit tests inside the transplanted modules come with them. The shell-completion and exit-code contract tests port with edits. v1's remaining integration tests are predominantly `--help` assertions against v1 response shapes and do not carry over; the devices and subscriptions suites are deleted with their groups.

## 7. Repository, build, and release

- Binary `flute`, version `2.0.0`, `rust-version = "1.85"` (edition 2024's floor).
- Release targets via cargo-dist: `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`, `x86_64-unknown-linux-gnu`, with shell and PowerShell installers and a Homebrew formula.
- CI runs `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and the spec hash gate on push and pull request. Third-party actions are SHA-pinned.
- `#![forbid(unsafe_code)]` as in v1.

The Homebrew formula installs the same binary name as v1 and declares `conflicts_with` against it. See [Open question 4](#q4-binary-name-and-homebrew-conflict).

### Documents

| Path | Purpose |
|---|---|
| `readme.md` | User-facing documentation, v1's structure |
| `agents.md` | Machine-readable contract for agents driving the CLI |
| `LICENSE` | MIT. v1 claims MIT in its readme but ships no LICENSE file, so the licence is not detected and the claim is unenforceable |
| `docs/superpowers/specs/` | This document |
| `docs/superpowers/plans/` | Ordered build plan; its checkboxes are the project status |
| `docs/doc-defects.md` | Divergences between the published API documentation and observed behaviour, one filed ticket each |
| `docs/reference/` | Vendored OpenAPI bundle, hash-gated |
| `CLAUDE.md`, `NOTES.local.md` | Contributor working notes. Not committed |

There is deliberately no separate status document. Status lives in the plan's checkboxes, which are updated as part of doing the work, with merged pull requests and git history as the ground truth beneath.

## 8. Documentation defects

The published API reference is the primary source for this implementation. Where it diverges from observed behaviour, the divergence is a documentation defect to be filed, not a quirk to be silently accommodated. Each entry below needs a ticket.

| # | Defect | Impact |
|---|---|---|
| 1 | `pageIndex` / `pageSize` are absent from the query parameters of **every** collection endpoint | Pagination cannot be implemented from the documentation |
| 2 | `billingAddress` and `contactInfo` are shown as optional; both are required for a new ACH payment method, along with specific sub-fields | Runtime 400s |
| 3 | **No schema and no description on any 4xx or 5xx response**, across all 46 paths | Error handling cannot be implemented from the documentation |
| 4 | `transactionDetails.cardData.captureMethod` appears in every card request example but is missing from the `CardDataDto` schema | The distinction between a sale and an authorization is undiscoverable |
| 5 | `customerId` is missing from the `CreateTransactionRequestDto` schema | The capability is invisible |
| 6 | `transactionStatus` and `sortOrder` are missing from the `GET /v2/transactions` query parameters | Server-side filtering appears unsupported |
| 7 | The `servers` block implies `/oauth2/token` is served from the API host; the prose specifies separate OAuth hosts | Authentication fails on a first integration attempt |
| 8 | `merchantId` is documented as a `GET /v2/transactions` query parameter and does not appear in the service's request model | Documented parameter may not exist — needs confirmation |
| 9 | `402` is declared on `POST /v2/payment-methods/{id}/set-default`, an operation that moves no money | Either an error or an undocumented behaviour |
| 10 | No endpoint returns `201`; the API design conventions specify `201 Created` for `POST /customers`, `/transactions`, and `/payment-methods` | The conventions document and the published specification contradict each other |

Defect 3 is the highest priority: it is the only one where the CLI is built blind rather than around a known gap.

## 9. Open questions

Each question is self-contained and can be forwarded independently.

### Q1: Confirm dropping devices and subscriptions

The v1 CLI has ten commands across two groups — `devices` (Tap-to-Pay device registration and activation) and `subscriptions` (recurring billing) — that have **no v2 API endpoints**. The design drops both.

The alternatives are to keep them calling v1 endpoints, making the binary a mixed v1/v2 client, or to register them as commands that exit with "not available in API v2". Dropping is recommended because the ticket specifies v2 support and a mixed client hides which API version is in use.

**Needs:** confirmation that no consumer depends on these ten commands, and whether v2 endpoints are planned.

### Q2: Ownership of the API documentation defects

Ten documentation defects are listed in section 8, including one — no error schema anywhere — that blocks writing error handling from the published documentation at all.

**Needs:** the team or project that owns `developer.flute.com/api-reference/v2`, and approval to file. No tickets have been created.

### Q3: Decline signalling

A declined card exits 0 today, consistent with v1, and callers read `transactionStatus` from the response to determine whether payment succeeded.

The complication is that a decline reported as HTTP 402 exits 1, while the same decline reported as a 200 with `transactionStatus: "Declined"` exits 0. The documentation does not state which the API does, and it may do both depending on the failure mode. A dedicated exit code for declines would let an agent distinguish "card refused, tell the customer" from "server failed, retry" without parsing a body.

**Needs:** confirmation after the live sandbox pass establishes what a declined transaction actually returns. Adding an exit code later is backwards compatible; v1 used only 0–4.

### Q4: Binary name and Homebrew conflict

The v2 binary is named `flute`, matching v1. Two Homebrew formulae installing the same binary name collide, expressed with `conflicts_with`, so a user runs one or the other. Naming it `flute2` would permit side-by-side installation during a transition, at the cost of a permanent name.

**Needs:** confirmation that users are expected to migrate rather than run both, and whether v1's Homebrew formula is eventually replaced.

### Q5: Sandbox credentials in CI

Live sandbox tests are the only layer proving the API accepts the CLI's requests, which matters more than usual against a beta API whose specification has ten known defects. They currently run locally by hand before a release; CI stops at layer 4.

**Needs:** a decision on whether sandbox credentials can live in repository secrets. If so, correctness is verified on every pull request rather than at release time.

### Q6: Payment processor id on every transaction

`paymentProcessorId` is required on every v2 transaction and was not required in v1, so every `transactions create` invocation must now supply `--payment-processor-id`. The value is obtainable from `settings payment-config`.

The CLI could default it when exactly one processor is configured, which removes the most common source of friction, at the cost of behaviour that varies with account configuration — a script working on a single-processor account would fail on a multi-processor one.

**Needs:** a preference. Recommended default is to require the flag explicitly, with the error message naming `settings payment-config` as the way to find the value.

## 10. Build order

Work proceeds as a vertical slice, then one pull request per group.

**Slice** — `ping`, `customers create/get`, and `transactions` card create/get, taken end to end: scaffolding, transplanted core, v2 authentication, the spec-conformance harness, command→HTTP tests, renderers, and a live sandbox call. This selection is deliberately risk-loaded: it exercises OAuth against the real API, money precision, PCI redaction of a real card number, error parsing, all three output modes, and the two transaction fields the specification omits. It is roughly 600 lines and proves the architecture before the remaining operations are built on it.

**Then, one group per pull request**, in dependency order: `payment-methods`, the remaining `transactions` operations, `pos`, `terminals`, `settlements`, `settings`, `payment-links`, `payment-sessions`, `api-keys`.

**Then** contract snapshots across the whole command tree, the extended redaction guard, a live sandbox verification pass, and finally `readme.md` and `agents.md`.

Each group is reviewed against the same checklist, so reviewing the eleventh is a matter of confirming it matches the first.
