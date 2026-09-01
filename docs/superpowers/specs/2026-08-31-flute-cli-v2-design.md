# Flute CLI v2 — Design

**Ticket:** ARISE-4928
**Repository:** `getflute/flute-cli-v2`
**Status:** Final — open questions resolved

---

## 1. Summary

`flute2` is a cross-platform CLI for the Flute payments platform, MIT licensed, written in Rust. This document designs the v2 CLI: a new project targeting the v2 API, succeeding the v1 CLI (`flute`) at `getflute/flute-cli`.

The v2 API exposes 61 operations. Eleven are webhooks and are out of scope; the CLI covers the remaining 50, spread across 12 groups, one of which is the OAuth token endpoint.

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

`[T]` marks modules transplanted from the v1 CLI and adapted. These are the non-API-shaped components — roughly 1,900 lines against ~13,000 written new. They are carried over rather than rewritten because a fresh implementation of PCI log masking, decimal precision, or the public output contract risks reintroducing solved problems with no test able to see the regression. Their comments are rewritten to house style; v1's carry ticket references and change history that this repo's conventions exclude.

`redact.rs` is split out of the v1 client module, where masking and HTTP transport shared one file. It is security-critical and belongs on its own.

### The injectable client

v1 called `build_client(profile)` inside every dispatch arm, which resolved credentials from the OS keychain before any request could be constructed, and v1 has no base-URL override anywhere. There was therefore no seam at which a test could point the compiled CLI at a mock server: **no v1 test reaches a request body through argv.** v1 does assert request bodies against wiremock, but only where it constructs an `ApiClient` directly, which skips the whole argument-parsing and body-building path that the tests most need to cover.

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

### Bodyless responses

**Thirteen** of the fifty operations answer success with no JSON body — a quarter of the surface, not a handful of special cases:


| Operation | Status |
| --------- | ------ |
| `GET /v2/ping` | 200 |
| `DELETE /v2/customers/{customerId}` | 200 |
| `DELETE /v2/payment-methods/{paymentMethodId}` | 200 |
| `DELETE /v2/api-keys/{clientId}` | 200 |
| `DELETE /v2/payment-links/{paymentLinkId}` | 204 |
| `PATCH /v2/customers/{customerId}` | 200 |
| `PATCH /v2/payment-methods/{paymentMethodId}` | 200 |
| `PATCH /v2/settings/transaction-autofill` | 200 |
| `POST /v2/payment-methods/{paymentMethodId}/set-default` | 200 |
| `POST /v2/payment-sessions/{paymentSessionId}/cancel` | 200 |
| `POST /v2/payment-links/{paymentLinkId}/share` | 204 |
| `POST /v2/pos/transactions/{posTransactionId}/print-receipt` | 200 |
| `POST /v2/transactions/{transactionId}/share-receipt` | 200 |


This set is **derived from the bundle by test**, not maintained by hand, so an operation that gains or loses a body upstream fails the build.

Two consequences worth stating because they are easy to get wrong. **`update` renders nothing:** `PATCH /v2/customers/{id}` and `PATCH /v2/payment-methods/{id}` return no body, so those commands confirm rather than print the updated resource — there is no updated resource to print. And a `quiet`-mode id must come from the request, since the response cannot supply one.

The transport therefore returns

```rust
pub struct Response {
    pub status: u16,
    pub body: Option<serde_json::Value>,
    pub correlation_id: Option<String>,
}
```

rather than a bare `Value`. A signature that always yields a body would make an empty 200 a decode error inside transport, where no renderer can rescue it — and `ping`, the first command built, is one of these. The renderer decides what an absent body means; transport only reports it faithfully.

### Offline/online split

Commands fall into three categories, not two:

- **No credential resolution** — `completion`, `version`, `update`, and `auth login/logout/switch` are handled before any `Ctx` exists. None of them resolves credentials or builds an `ApiClient`.

  They are not all offline, and not all keychain-free. `auth login` writes the keychain and `auth logout` deletes from it — that is their entire purpose — and `update` reaches GitHub. What unites them is that **nothing resolves a credential on their behalf before dispatch**, so none can fail for want of one. A command that manages the keychain is doing its own work on it; a command that is handed credentials it did not ask for is the thing this split removes.
- **Credentials optional** — `auth status` alone. It resolves credentials but tolerates their absence, reporting `authenticated: false` and exiting 0, as in v1. It is the command a user runs precisely *because* authentication is not working, so failing it for want of credentials would defeat its purpose. `auth token` is **not** in this category: its whole output is a bearer token, which cannot be obtained without credentials, and v1 fails it the same way.
- **Credentials required** — everything else receives a `Ctx`. Missing credentials is exit 2 before any request is built.

One credential resolution, one production banner, one place to reason about.

### Hosts

The four endpoint constants are fixed in `config.rs` and asserted by unit test. They are **not** derived from the bundle's `servers` block, which places the token endpoint on the API host where it returns 404 (D7).


| Profile      | API                                | OAuth                                                |
| ------------ | ---------------------------------- | ---------------------------------------------------- |
| `sandbox`    | `https://sandbox.api.flute.com`    | `https://sandbox.oauth.api.flute.com/oauth2/token`   |
| `production` | `https://api.flute.com`            | `https://oauth.api.flute.com/oauth2/token`           |


A wrong constant here points payment traffic at the wrong environment, which no other test would catch — the requests would be perfectly well-formed.

### Test seams

1. **In-process** — construct `Ctx` with `base_url` set to a mock server and call a group's `dispatch` directly.
2. **Binary-level** — `FLUTE2_API_BASE_URL` and `FLUTE2_OAUTH_URL` aim the compiled binary at a mock server, driven through `assert_cmd`.

Both assert on the recorded request: exact method, path, and JSON body. Credentials come from `FLUTE2_CLIENT_ID` / `FLUTE2_CLIENT_SECRET`, which take precedence over the keychain, so no test touches the OS keychain.

`FLUTE2_NO_KEYCHAIN` makes credential resolution skip the OS keychain. Without it a "no credentials" test is only valid on a machine nobody has logged in on, because removing the credential variables deliberately falls through to the keychain — so the test would read a developer's real credentials and reach the network. It is safe under production, since it can only remove access, never redirect it.

**The base-URL overrides are refused when `--profile production` is active.** A test hook able to silently redirect production payment traffic is a liability; combining them is a hard error.

## 4. Command surface


| Group              | Commands                                                                                                                                          |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| `auth`             | `login`, `logout`, `status`, `switch`, `token`                                                                                                    |
| `ping`             | —                                                                                                                                                 |
| `customers`        | `create`, `get`, `list`, `update`, `delete`                                                                                                       |
| `payment-methods`  | `list`, `get`, `add-card`, `add-ach`, `update`, `delete`, `set-default`                                                                           |
| `transactions`     | `create`, `get`, `list`, `capture`, `reversal`, `credit`, `tip-adjust`, `ach-hold`, `ach-release`, `calculate-amount`, `share-receipt`, `inspect` |
| `pos`              | `create` (`--wait`), `get`, `list`, `cancel`, `print-receipt`                                                                                     |
| `terminals`        | `list`, `status`                                                                                                                                  |
| `settlements`      | `list`, `close`, `get`                                                                                                                            |
| `payment-links`    | `create`, `get`, `list`, `update`, `delete`, `share`                                                                                              |
| `payment-sessions` | `create`, `get`, `cancel`                                                                                                                         |
| `settings`         | `payment-config`, `contact-info`, `autofill`, `update-autofill`                                                                                   |
| `api-keys`         | `create`, `list`, `revoke`                                                                                                                        |
| utility            | `version`, `update`, `completion`                                                                                                                 |


Global flags are unchanged from v1: `--profile`, `--output table|json|quiet`, `--debug`. `--profile` and `--output` each read an environment variable when the flag is absent, as in v1.

`--profile` resolves `--profile` → `FLUTE2_PROFILE` → the config file's `default_profile` → `sandbox`. **This is a deliberate v2 change: the config step does not work in v1.** v1's `auth switch` writes `default_profile` to the config file, but its `--profile` flag carries a hard-coded `default_value`, so the flag is never absent and the stored value is never read — `auth switch production` reports success and changes nothing. v2 parses the flag as an `Option<String>` so "absent" is distinguishable from "sandbox", which is what makes `auth switch` mean something.

### Environment

Every name v1 owns is carried over under the `FLUTE2_` prefix, so the two binaries never contend for one variable:


| Variable                                 | Effect                                                      |
| ---------------------------------------- | ----------------------------------------------------------- |
| `FLUTE2_PROFILE`                         | Default profile when `--profile` is absent                  |
| `FLUTE2_OUTPUT`                          | Default output format when `--output` is absent             |
| `FLUTE2_CLIENT_ID`, `FLUTE2_CLIENT_SECRET` | Credentials; take precedence over the keychain             |
| `FLUTE2_NO_UPDATE_CHECK`                 | Suppresses the post-command update notice                   |
| `FLUTE2_GITHUB_TOKEN`                    | Lifts the unauthenticated rate limit on `update`            |
| `FLUTE2_API_BASE_URL`, `FLUTE2_OAUTH_URL` | Test seams; refused under `--profile production`           |
| `FLUTE2_NO_KEYCHAIN`                     | Skip the OS keychain; keeps `cargo test` hermetic           |


A bare `CI` also suppresses the update check, as in v1 — it is not namespaced because it is not ours.

### Pagination

`--page-index` (0-based) and `--page-size` mirror the API's `pageIndex` / `pageSize` exactly. v1 used a 1-based `--page` with `--limit`; translating would be friendlier in isolation but introduces a silent off-by-one against every example in the API documentation. The help text states that the index is 0-based.

The API bounds `pageSize` to 1–100 and defaults it to 20. The CLI rejects an out-of-range `--page-size` client-side rather than spending a round trip on a 400, and omits the parameter entirely when the flag is absent so the server's default governs. v1 differed on both counts: it defaulted `--limit` to 25 and always sent it.

Every **paginated** `list` command also accepts `--all`, which follows `pageInfo.hasMore` to exhaust the collection. `api-keys list` is not paginated — it returns a bare `apiKeys` array with no `pageInfo` — so it takes neither the pagination flags nor `--all`. This is the one place the CLI adds something the API does not have, and it is cheap precisely because the API's response shape provides `hasMore`. No field of `pageInfo` is declared required, so `--all` stops when `hasMore` is absent or false, or when a page returns no items — it never infers a further page from `totalPages` alone.

### Client-side operations

Three commands have no corresponding endpoint:

- `transactions inspect` — a detailed single-transaction view, carried over from v1.
- `settlements get` — v2 has no single-batch endpoint, but `GET /v2/settlements/batches` documents a `batchIds` query filter, so the command sends `batchIds=<id>` and returns the single match. It does not fetch a page and filter client-side: a batch past the first page would report not-found. An empty result is exit 4. The help text says the command is a filtered list rather than a dedicated read.
- `auth login`, `auth logout`, `auth switch` — credential and profile management, entirely local. The other two are not: `auth token` fetches from the OAuth host, and `auth status` performs an authenticated `GET /v2/ping` to decide whether to report `authenticated: true`. Only the first three run without a network.

### Client-side validation

`transactions create` mirrors the server's four documented rules before issuing a request, so a malformed invocation exits 3 without a round trip: base amount greater than zero, payment processor id present, transaction details present, and exactly one of card or ACH data. v1 sent requests with no payment instrument at all, which the v1 API answered with a 500.

`transactions create` takes an instrument in one of **four** shapes, and validation differs per shape. Exactly one must be supplied:


| Shape      | Flags                                                            | Wire                                                        |
| ---------- | ---------------------------------------------------------------- | ----------------------------------------------------------- |
| New card   | `--card`, `--cvv`, `--exp`                                       | `cardData.paymentMethodDetails`                             |
| Saved card | `--payment-method-id` with `--instrument card`                   | `cardData.paymentMethodId`                                  |
| New ACH    | `--ach-account-number`, `--ach-routing-number`, `--ach-*`        | `achData.paymentMethodDetails`                              |
| Saved ACH  | `--payment-method-id` with `--instrument ach`                    | `achData.paymentMethodId`                                   |


The saved routes are not optional extras: v1 accepts `--payment-method-id` on four transaction commands, so omitting them would regress it. `cardData` and `achData` each declare `paymentMethodId` as mutually exclusive with `paymentMethodDetails`.

ACH carries requirements card does not. `achData` requires `requesterIpAddress` and `secCode` by schema, and a **new** ACH account additionally requires `billingAddress` and `contactInfo` including `mobilePhoneNumber` — a conditional requirement OpenAPI cannot express, filed as D2. The saved-ACH route needs none of them.

`pos create` carries one rule of its own: v2 exposes **two** independent waiting controls, and `--wait` drives both because a caller waiting for a terminal wants both halves:

- `waitForAcceptanceByTerminal` on the **create** body — holds the create response until the terminal accepts or declines the request.
- `waitForTransactionProcessing` on the **get** query — holds each poll open until the transaction state changes.

Using only the second would leave the create returning before the terminal had even acknowledged; using only the first would busy-poll for the result. `--wait` sets `waitForAcceptanceByTerminal: true` on create and then polls with `waitForTransactionProcessing=true`, so each request blocks server-side rather than spinning. Without `--wait`, both are false or absent and the command returns immediately.

The API requires `waitForAcceptanceByTerminal` to be false when `initiationChannel` is `Deeplink`, so combining `--wait` with that channel exits 3 without a round trip.

`--payment-processor-id` is always required. v2 requires `paymentProcessorId` on every transaction and v1 did not, so this is the one flag every existing charge invocation must gain. The CLI could default it when an account has exactly one processor configured, which would remove the friction for most users; it does not, because that makes the required arguments a function of account configuration — a script written against a single-processor account would fail on a multi-processor one, at the point of taking a payment. The missing-flag error names `settings payment-config` as the way to find the value.

### Destructive commands require `--yes`

v1 refuses `customers delete`, `customers remove-method`, `keys revoke`, and `subscriptions terminate` unless `--yes` is passed, and answers with a message naming the full invocation. v2 keeps the gate and extends it to every destructive verb it owns: `customers delete`, `payment-methods delete`, `payment-links delete`, `api-keys revoke`, `payment-sessions cancel`, and `pos cancel`.

The refusal is client-side and exits 3 **without issuing a request** — a confirmation gate that still reaches the API has already failed. Tests assert the absence of the HTTP call, not merely the exit code.

This sits alongside the idempotent-404 rule rather than against it: `--yes` governs whether the CLI is willing to ask, and the 404 rule governs how it reads the answer.

### Behaviour worth surfacing

`api-keys` sits behind a server feature flag; with the flag disabled the endpoint returns 404, which reads as a wrong URL rather than a disabled feature. A 404 from this group specifically produces an error message saying so.

## 5. Output and error contract

### Modes and streams

Three modes — `table` (default), `json`, `quiet` — resolved `--output` → `FLUTE2_OUTPUT` → config file → default, as in v1. A clap usage error is raised before the config file is read, so that one path decides between a JSON envelope and text by scanning argv for `--output json` and falling back to `FLUTE2_OUTPUT`, again as in v1. **Data goes to stdout; everything else goes to stderr**: tracing, the production banner, update notices. This holds under `--debug`, so `flute2 --debug --output json` still emits parseable JSON on stdout.

### Success envelope

```json
{ "object": "transaction",
  "data": { },
  "meta": { "environment": "sandbox",
            "correlation_id": "…",
            "page_info": { "pageIndex": 0, "pageSize": 20, "totalItems": 17,
                           "totalPages": 1, "hasMore": false } } }
```

Two changes from v1. `page_info` is carried on collection responses so an agent can paginate without parsing table output; it reproduces the API's `pageInfo` field for field, and omits any the response leaves out. `correlation_id` is populated from the response on success, not only on failure, so a user can quote an identifier when something looks wrong.

**`data` is always the resource, never a transport wrapper.** Seven single-transaction writes — create, capture, reversal, credit, tip-adjust, ach-hold, ach-release — are *documented* as returning `PageOfGetTransactionResponseDto`, a paged collection. **They return a single transaction object.** The endpoint's own three response examples show the single-object form, and the sandbox confirms it; the schema is wrong, and this is filed as D11.

The CLI decodes what the API sends, not what the reference claims: a single transaction object goes straight into `data`, and these responses carry no `page_info`. Building to the documented schema instead would turn every successful charge into a decode error.

Because the API is in beta and the schema states the paged form, the decoder is explicitly **transitional**: it accepts a bare transaction object, and also accepts a one-item page by unwrapping it. Both paths are tested. A page with any other item count is a decode error rather than a silent choice of the first element. The page arm exists solely so that the server converging on its own published schema is not an outage, and it is deleted when D11 resolves in either direction.

Transaction responses come in more than one shape, so **one renderer prints the fields that are present** rather than assuming a schema. `transactions get` returns the full form; `transactions list` returns the short form, which omits `addressVerificationServiceResponse`, `declineDetails`, `originalTransactionId`, `refundDetails`, and `transactionEvents`; the write responses are a third shape again, carrying `amountDetails`, `processorResponse`, `receipt`, and `responseDetails` where the read forms carry `amountBreakdown` and `processorDetails`. `transactions inspect` exists because the full form repays a dedicated view.

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
| 130  | Interrupted — Ctrl-C during `pos create --wait`                              |


Identical to v1, including 130, which v1 emits but omits from its own exit-code table — it is documented only in the prose of v1's `agents.md`. On Ctrl-C the last-known status goes to stderr and **stdout stays empty in every output mode**. The other `pos create --wait` outcome, an expired `--wait-timeout`, prints the last-known success envelope to stdout, a warning to stderr, and exits 1 through the general arm. v2 introduces 402, 409, and 429, which fall through the existing general-error arm rather than claiming new codes.

**A 404 on a delete or revoke exits 0, not 4.** Deleting an already-deleted resource is success: the caller's intent holds. v1 applies this to `customers delete`, `customers remove-method`, and `keys revoke`; v2 applies it to every delete, revoke, and cancel. It is confined to those verbs — a 404 from a `get` or `list` still exits 4.

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

One documented envelope against four observed shapes is a divergence from the reference, not a quirk of this client. The parser accommodates all four because it must, and the divergence belongs in `docs/doc-defects.md` rather than living only as a comment in the parser.

## 6. Testing strategy

No test suite proves a payments CLI correct. The goal is that **every externally meaningful rule has an oracle independent of the implementation.** A test that restates the implementation in Rust proves only that the code agrees with itself, which is the failure mode that matters here: the dominant risk is not logic error but sending wrong bytes to a beta API.

Independence is the whole design. Each layer's oracle comes from somewhere the implementation cannot influence:

| Layer | Oracle | Catches |
|---|---|---|
| 1. Contract matrix | A checked-in row per operation *and variant* | An operation or variant with no test at all |
| 2. Exchange conformance | The vendored OpenAPI bundle | Wrong path, method, query, content type, body, status, or response shape |
| 3. Command → HTTP | A mock server and real argv | Wrong bytes from a real invocation |
| 4a. API surface | The bundle's request fields | A v2 capability the CLI cannot reach |
| 4b. Capability parity | The v1 CLI's flag surface | Silently dropped features |
| 5. Live sandbox | The running API | Everything the documentation gets wrong |
| 6. Invariants | Policy stated once, checked everywhere | `--yes`, redaction, streams, exit codes |
| 7. Contract snapshots | Committed golden files | Accidental drift in the command tree |

No layer is trusted as the oracle for everything. Unit tests inside modules remain, but they are not counted as coverage of an operation — they cannot be, because they share their author's assumptions.

### The harness is checked the same way

Most of that table is delivered by harness written for this repository, and nothing in the table checks the harness. Each "Catches" entry is a claim about roughly 1,500 lines of bespoke code, and an unchecked checker fails in the one direction that is invisible: a validator that accepts everything is indistinguishable from a clean suite. The `test_fn_exists` source scan is the clearest case — pointed at the wrong directory it reports full coverage indefinitely.

So every invariant carries a **negative control**: an input holding exactly the defect the invariant claims to catch, and an assertion that it is rejected *for that reason*, since a panic on an unrelated line would otherwise read as success. The controls mutate fixtures in process and never touch production code.

Two of them defend properties that no other test can reach. Deleting a nested surface row must fail, which is the only mechanical proof that the surface matrix is leaf-level rather than top-level. And applying the D11 response exemption to `GET /v2/transactions` must fail, which is what keeps that exemption off the one transaction endpoint whose paged schema is right.

### The contract matrix

`tests/support/contracts.rs` holds one row per non-webhook operation, keyed by the bundle's `operationId`, and one sub-row per **request variant**. A single row per operation is not enough: `transactions create` has five variants (new card automatic, new card manual, saved card, new ACH, saved ACH) and `capture` has two (full, partial), and a variant with no test is exactly how the saved-instrument routes went missing.

**Each variant carries the exchange itself** — method, rendered path, path parameters, query, content type, request body, expected status, expected response body — rather than describing it. Conformance validates that fixture against its operation, and the group's command test drives its mock from the same value, so the asserted bytes and the validated bytes are one value and cannot drift. A variant additionally names its mock test, its live test, and any divergence ID.

Recording parameter names and a schema name *beside* a test name would be decoration: nothing reads them, and a row could name a test exercising a different operation entirely.

Three invariants are enforced by test, not by review:

1. **Every non-webhook operation in the bundle appears exactly once**, mapped to a command, excluded with a reason, marked internal (the token endpoint), or listed as pending with the task that owns it. Enumerated from the vendored bundle, so adding an operation upstream fails the build until someone decides what it means.
2. **Every mapped variant names a mock test, and that test exists** — verified by scanning the test sources for the function name, so a renamed or deleted test fails rather than silently orphaning a row.
3. **Every variant names a live test or gives a reason it cannot safely be exercised.**

This turns "50 operations" from a sentence in this document into a failing build.

**Structural and existential enforcement are staged.** Invariant 1 holds from the moment the matrix exists; invariants 2 and 3 cannot, because a row written before its group is built would name tests that do not exist, and the suite is required to pass at every commit. So the matrix accounts for the whole operation set immediately and splits it: built operations carry full rows, the rest are pending and name their task. Existence checks run over built rows only, and a release gate asserts nothing is still pending. The same staging applies to the surface and parity matrices. Without it the enumeration could only begin once the last group landed, which is the point at which it has nothing left to catch.

### Exchange conformance

Conformance validates a **whole exchange against one operation**, not a body against a schema:

```rust
assert_exchange_conforms(
    "flute-v2-post-payment-links-paymentLinkId-share",
    RequestFixture { path_params, query, content_type, body },
    ResponseFixture { status: 204, body: None },
);
```

Schema-oriented validation cannot catch a fixture that is internally valid but attached to the wrong endpoint — which is how a required `customerId` query parameter went missing and how a bodyless 204 acquired an invented JSON resource. Operation-oriented validation checks, in one call: the operation exists; path parameters are supplied; required query parameters are supplied; the content type matches; the request body validates; the response status is declared for *that* operation; empty-versus-JSON matches that status; and the response body validates against the declared response schema.

Mock response fixtures are validated too. Validating only requests is what let fixtures drift onto v1 field names that the API never returns.

### API surface coverage

`tests/support/surface.rs` maps **every leaf request field and query parameter of every mapped operation** to a CLI flag, a value the CLI fixes deliberately, or an exclusion with a reason — derived from the bundle, so an unaccounted field fails the build.

The contract matrix proves each operation is reachable; it says nothing about how much of that operation is. `customers list` can hold a passing contract row while `fullName`, `email`, `companyName`, `mobilePhoneNumber`, `createdFrom`, `createdTo`, `sortBy`, and `asc` are all unreachable. Endpoint coverage and capability coverage are different claims, and only the second is what a user experiences.

**Leaf, not top level, and a container never accounts for its descendants** — the same argument one level down. Derivation follows `$ref` with a visited set, descends `items` for arrays, and unions the branches of `allOf`/`oneOf`/`anyOf`. A single row for `transactionDetails` would mark the entire card and ACH surface covered, which is the exact shape of the miss that lost the saved-instrument routes. `POST /v2/transactions` has 16 top-level properties and 60 leaves; across the request bodies it is 131 against 250. Almost half of the request surface is invisible to a top-level matrix.

### Capability parity

`tests/support/parity.rs` maps every v1 user-facing capability to one of three outcomes: **preserved**, **replaced** (naming the replacement), or **intentionally removed** (with a v2 reason). Each row names a test.

Operation coverage cannot detect feature regression, because a dropped flag leaves every endpoint still covered. v1's `--payment-method-id`, its L2/L3 enhanced data, and its customer `--search` are all cases where the endpoint is implemented and the capability is not.

### Divergences are narrow and executable

Each API defect gets a rule, not a blanket exemption, and every rule is scoped to the operations it applies to — an unscoped one would exempt a field on endpoints that genuinely declare it:

```
D4  captureMethod accepted at runtime, absent from CardDataDto
    Operations: flute-v2-post-transactions
    Evidence:   live test transaction_create_manual_capture
    Behaviour:  send "Auto" / "Manual"
    Removal:    published CardDataDto includes captureMethod
```

For a request field, conformance strips **only the named JSON pointer** and validates everything else, failing if any *other* undocumented field appears. "Validate the documented subset" is too loose — it hides the next undocumented field as effectively as the current one.

**A wrong response *shape* needs a second kind of rule, because no pointer strip can express it.** The seven single-transaction writes declare `PageOfGetTransactionResponseDto`, which sets `additionalProperties: false` and declares only `items` and `pageInfo`, so the single object they actually return fails validation outright. There is also no correct schema to substitute: the write responses carry `amountDetails`, `processorResponse`, `receipt`, and `responseDetails`, and both `GetTransactionResponseDtoShort` and `GetTransactionResponseDtoFull` forbid additional properties and declare none of them.

What the bundle does ship is those operations' own **response examples**, which all seven carry in the single-object form. For these seven only, conformance validates the response fixture against the union of the operation's declared examples rather than its declared schema. The examples are the API team's own statement of the shape and live in the checked-in bundle, so the oracle stays independent of this implementation; and the rule cannot be over-applied, because `GET /v2/transactions` declares the same schema and its example genuinely is a page. Their real oracle is the live test the divergence names, and both the rule and the transitional page arm in the decoder are deleted when D11 resolves.

### Live sandbox tests are committed

The test logic lives in `tests/live/` and **is committed**. Account-specific values are read from the environment — `FLUTE2_LIVE_PROCESSOR_ID`, `FLUTE2_LIVE_CUSTOMER_ID`, `FLUTE2_LIVE_TERMINAL_ID`, `FLUTE2_LIVE_MERCHANT_ID` — and the only ignored file is the local config that supplies them. The public repository therefore contains no secrets and no real identifiers, while the scenarios stay reviewable, shared, recoverable, and runnable by anyone with sandbox access.

Uncommitted tests cannot serve as a development guarantee: nothing stops them rotting, and the executable record of what the API actually accepts would live on one machine. Credentials already work exactly this way — if an environment variable is good enough for a client secret, it is good enough for a processor id.

They stay `#[ignore]`d, so a plain `cargo test` remains hermetic and offline; `cargo test --test live -- --ignored` opts in.

**Live coverage is selected by contract risk, not one call per group.** Fully covered: every write endpoint, every endpoint with an empty success body, every documented divergence, every polymorphic request variant, and every enum or casing a schema cannot check. Reads are sampled, with representative pagination and filter cases.

Two limits are stated rather than wished away:

- **Transactions cannot be torn down.** A reversal is a new transaction, not a deletion, so live payment tests accumulate sandbox state permanently — and D12 forces a unique amount per run on top of it. Tests that create customers, payment methods, payment links, or api-keys do delete what they create; transaction tests are append-only by nature.
- **`api-keys revoke` can destroy the credentials the suite is using.** It is exercised only against a key created within the same test, never against the authenticating key.

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

Two binaries on one machine must not share mutable state, so v2 namespaces everything it owns: `~/.flute2/config.toml`, its own OS keychain service, and `FLUTE2_`-prefixed environment variables. The config file carries v1's three keys unchanged — `default_profile`, `output`, and `auto_update_check`. Credentials are not shared with v1 even where the values would be identical; the two binaries authenticate against different token hosts, and one variable meaning two things is the failure this avoids. CI that already exports the v1 names sets the `FLUTE2_` names alongside them.

### Documents


| Path                          | Purpose                                                                                                                   |
| ----------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `readme.md`                   | User-facing documentation, v1's structure                                                                                 |
| `agents.md`                   | Machine-readable contract for agents driving the CLI                                                                      |
| `LICENSE`                     | MIT. v1 claims MIT in its readme but ships no LICENSE file, so the licence is not detected and the claim is unenforceable |
| `docs/superpowers/specs/`     | This document                                                                                                             |
| `docs/superpowers/plans/`     | Ordered build plan; its checkboxes are the project status                                                                 |
| `docs/doc-defects.md`         | Divergences between the published API documentation and observed behaviour, with a reproduction and a status for each     |
| `docs/v1-defects.md`          | Bugs found in the v1 CLI while porting, each naming what v2 does instead. A v1 bug is not a behaviour to preserve         |
| `docs/reference/`             | Vendored OpenAPI bundle, hash-gated                                                                                       |
| `CLAUDE.md`, `NOTES.local.md` | Contributor working notes. Not committed                                                                                  |
| `tests/live/`                 | Live sandbox verification. **Committed**; account values come from `FLUTE2_LIVE_*`                                        |
| `tests/support/contracts.rs`  | The contract matrix — one row per operation and request variant, enforced by test                                         |
| `tests/support/parity.rs`     | v1 capability-parity matrix — every v1 capability preserved, replaced, or removed with a reason                            |
| `.flute2-live.env`            | Local sandbox identifiers for the live suite. Not committed. A shell file, since the loader reads the environment         |


There is deliberately no separate status document. Status lives in the plan's checkboxes, which are updated as part of doing the work, with merged pull requests and git history as the ground truth beneath.

## 8. Documentation defects

The published API reference is the primary source for this implementation. Where it diverges from observed behaviour, the divergence is a defect to be filed, not a quirk to be silently accommodated.

The full list, with a reproduction command for each, lives in [`docs/doc-defects.md`](../../doc-defects.md).

## 9. Build order

Work proceeds as a vertical slice, then one pull request per group.

**Slice** — `ping`, `customers create/get`, and `transactions` card create/get, taken end to end: scaffolding, transplanted core, v2 authentication, the spec-conformance harness, command→HTTP tests, renderers, and a live sandbox call. This selection is deliberately risk-loaded: it exercises OAuth against the real API, money precision, PCI redaction of a real card number, error parsing, all three output modes, and the two request fields the specification omits — `captureMethod` on the card data and `customerId` on the create-transaction request. It is roughly 600 lines and proves the architecture before the remaining operations are built on it.

**Then, one group per pull request**, in dependency order: `payment-methods`, the remaining `transactions` operations, `pos`, `terminals`, `settlements`, `settings`, `payment-links`, `payment-sessions`, `api-keys`.

**Then** contract snapshots across the whole command tree, the extended redaction guard, a live sandbox verification pass, and finally `readme.md` and `agents.md`.

Each group is reviewed against the same checklist, so reviewing the eleventh is a matter of confirming it matches the first.