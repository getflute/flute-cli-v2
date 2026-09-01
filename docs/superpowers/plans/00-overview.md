# Flute CLI v2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship `flute2`, a cross-platform Rust CLI covering all 50 non-webhook operations of the Flute v2 API, with a test layer able to see every failure mode.

**Architecture:** One `ApiClient` is built once in `run()` and passed to each group's `dispatch(&Ctx, Args)`, so a test can point the compiled binary at a mock server — the seam v1 lacks. Non-API commands (`completion`, `version`, `update`, `auth login/logout/switch`) are handled before any `Ctx` exists, so nothing resolves credentials on their behalf — `auth login` and `auth logout` still manage the keychain themselves, which is their whole job. Work proceeds as one risk-loaded vertical slice, then one pull request per group.

**Tech Stack:** Rust edition 2024, clap 4 (derive), tokio, reqwest, serde/serde_json, rust_decimal, anyhow, tracing, keyring. Tests: wiremock, assert_cmd, insta, jsonschema.

**Spec:** [`docs/superpowers/specs/2026-08-31-flute-cli-v2-design.md`](../specs/2026-08-31-flute-cli-v2-design.md) — read it alongside this plan. Where they disagree, the spec wins and this plan is wrong.

## Global Constraints

Every task's requirements implicitly include this section.

- Crate, binary, and Homebrew formula are all `flute2`. Version `2.0.0`. `rust-version = "1.85"`, edition 2024.
- `#![forbid(unsafe_code)]` in `main.rs` and `lib.rs`.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` pass at **every** commit. Paste the command and its pass/fail totals into the commit body or PR description.
- **Test before implementation, always.** Add a failing test, confirm it fails for the right reason, then write the minimum code to pass it.
- `cargo test` stays hermetic and offline. Anything touching the network lives in `tests/live/`, is `#[ignore]`d, and is opted into with `cargo test --test live -- --ignored`. Those tests are **committed**; their account values come from `FLUTE2_LIVE_*` and the file supplying them is not.
- This repository is **public**. No internal hostnames, no sandbox account identifiers, no ticket contents beyond the ID, nothing from the internal monorepo.
- Commits: `ARISE-4928 <type>(<scope>): <description>`. No `Co-Authored-By` trailer.
- Comments explain **why**, never what. No ticket refs, no change history, no version stamps in source.
- Money is `rust_decimal::Decimal` end to end. **`f64` never touches an amount** — `baseAmount` is declared `number/double` in the API, so serialization must go through an exact-decimal path, not a float.
- Namespace everything: `~/.flute2/config.toml`, a `flute-cli-v2` keychain service, `FLUTE2_`-prefixed variables. Never read a bare `FLUTE_` name.
- **`cargo test` never touches the OS keychain.** Every binary-level test goes through `support::bin` or `support::bin_without_credentials`, both of which set `FLUTE2_NO_KEYCHAIN=1`. A test that reads a developer's keychain is neither hermetic nor reproducible, and "no credentials" tests are simply wrong without it.
- **No clap `default_value` on `--profile`.** A defaulted flag is never absent, and absent is the only state in which `config.default_profile` can be read. This is the v1 bug in V1; reintroducing the default reintroduces the bug.
- **Preserve v1 behaviour unless the design explicitly enumerates a v2-driven change.** The design enumerates many — renamed commands, the `flute2` namespace, `pageIndex`/`pageSize`, `page_info`, client-side validation, dropped groups, the extended 404 rule, and the `default_profile` fix. Anything *not* on that list that differs from v1 is a defect in this plan. The largest enumerated change is the injectable client and its `FLUTE2_API_BASE_URL` / `FLUTE2_OAUTH_URL` overrides: v1 resolves credentials inside every dispatch arm and has no base-URL override, which is why no v1 test reaches a request body through argv. Layer 2 does not exist without it, so **do not "restore v1 parity" there.**
- **Destructive commands require `--yes`,** refused client-side with no request issued. v1 gates four commands this way; v2 gates six. Tests assert that no HTTP call was made, not just the exit code.
- **`--profile` is `Option<String>`,** resolving `--profile` → `FLUTE2_PROFILE` → `config.default_profile` → `sandbox`. A clap `default_value` would make the flag never-absent and silently strand `auth switch`, as it does in v1.

---

## The Testing Contract

Testing is not a phase in this plan; it is the shape of every task. **Every externally meaningful rule gets an oracle independent of the implementation.** A test that restates the implementation in Rust proves only that the code agrees with itself — and the dominant risk here is not logic error but sending wrong bytes to a beta API.

| Layer | Lives in | Oracle | Catches |
|---|---|---|---|
| 1. Contract matrix | `tests/support/contracts.rs`, `tests/coverage.rs` | A row per operation *and variant* | An operation or variant with no test at all |
| 2. Exchange conformance | `tests/conformance.rs` | The vendored bundle | Wrong path, method, query, content type, body, status, or response shape |
| 3. Command → HTTP | `tests/cmd_<group>.rs` | A mock server and real argv | Wrong bytes from a real invocation |
| 4a. API surface | `tests/surface.rs` | The bundle's request fields | v2 capability the CLI cannot reach |
| 4b. Capability parity | `tests/parity.rs` | v1's flag surface | Silently dropped features |
| 5. Live sandbox | `tests/live/` (committed) | The running API | Everything the documentation gets wrong |
| 6. Invariants | across the suite | Policy stated once | `--yes`, redaction, streams, exit codes |
| 7. Contract snapshots | `tests/snapshots.rs` | Golden files | Accidental drift in the command tree |

Unit tests inside modules stay, but **they do not count as coverage of an operation** — they share their author's assumptions, which is exactly what an oracle must not do.

### The harness itself needs an oracle

Most of the safety above comes from roughly 1,500 lines of harness written for this repository: exchange conformance, three matrices, the divergence rules, and a `test_fn_exists` that scans test sources for a function name. Nothing in the layer table checks *that* code. Every entry in the "Catches" column is a claim about it, and a claim about a checker is worth what a claim about the CLI is worth — nothing, until a test fails when it is false.

A checker that silently passes everything is the worst outcome available here, because it is indistinguishable from a clean suite. `test_fn_exists` scanning the wrong directory would report full coverage forever.

So each invariant gets a **negative control**: an input carrying exactly the defect the invariant claims to catch, and an assertion that it is rejected *for that reason*. Task 7c builds them. They mutate fixtures in-process and never touch production code, so they cost a few hundred lines once and then answer "would this class of error have been caught?" mechanically rather than by argument.

**Layer 3 is the layer v1 lacks entirely.** v1 has no base-URL override anywhere, so no v1 test reaches a request body through argv. Every group here gets tests that run the compiled binary and assert the bytes on the wire.

**Layers 1, 2, and 4 exist because of measured failure, not theory.** Three review rounds found roughly twenty-five real defects in this plan — wrong field name, wrong casing, missing required parameter, invented response body, a whole missing instrument route. Every one is a class these three layers mechanise. Finding them by hand does not scale to fifty operations.

### The matrices are enforced in two stages

A row cannot name a test that does not exist yet, so each matrix asserts two different things and only one of them can hold in Phase 0:

- **Structural** — the row set matches the bundle, and every fixture conforms to its own operation. This needs no command and no test, so it holds from Task 7 onward.
- **Existential** — the named mock test, the named live test, and the advertised `--help` flag all exist. This can only hold once the group is built.

Each matrix therefore accounts for the whole operation set from Task 7, split in two: `CONTRACTS` holds rows for operations already built, and `PENDING` names the rest alongside the task that owns each. **`CONTRACTS ∪ PENDING` is asserted equal to the bundle's non-webhook operation set**, so an operation added upstream still fails the build until somebody decides what it means. Existence checks run over `CONTRACTS` only. Task 27 asserts `PENDING` is empty, which is what stops a deferral from becoming a permanent hole.

Without the split, Phase 0 cannot pass its own gate. Task 7's invariants name `cmd_*` tests, Task 7a's `every_exposed_flag_appears_in_help` needs a command tree, and Task 7b's rows name group tests — none of which exist before Task 9, against a global constraint that `cargo test` passes at **every** commit.

**Definition of done, per group** — mechanical, identical for all eleven:

1. Every operation and **every request variant** has a row in the contract matrix, moved out of `PENDING` in this task's own commit, and the row carries the exchange rather than describing it. `transactions create` has five variants; `capture` has two. A row whose fields merely *name* a schema and a test proves nothing — the invariants read fixtures, not labels.
2. Builder unit tests covering every flag that reaches the wire, **including omission when the flag is absent**.
3. At least one command→HTTP test per command, asserting method, path, **the exact query**, content type, and full JSON body. Mounting a fixture is not the assertion — wiremock's matchers are existential, so every command test pairs `support::mount` with `support::assert_exchange_observed`, which compares the recorded request's whole query multiset and content type against the fixture.
3b. **The canonical command-test pattern, without exception:**

```rust
let server = support::mock_with_token().await;
let ex = support::mount(&server, OPERATION_ID, VARIANT).await;
support::bin(&server).args([...]).assert().success();
support::assert_exchange_observed(&server, &ex).await;
```

    `mount` proves a *matching* request would be answered; `assert_exchange_observed` proves one was actually sent. Without the second line a command that sent nothing still passes. Hand-built mocks are not permitted — they are how a mock drifts from the fixture conformance validated, and the coverage gate rejects a test that never names its operation id.

4. **A contract row per variant, carrying the exchange fixture** — conformance validates it automatically, and the command test drives its mock from the same value via `contracts::exchange(...)`. There is no separate per-group conformance test to write, and a mock cannot drift from what conformance checked because they are one value. Divergences are scoped to named operations and are either a single stripped request pointer or, where the published response schema is wrong, the operation's own declared examples.
5. **Negative and omission tests for every conditional field.** A positive test cannot establish optionality; only the absent case can.
6. Renderer snapshots for `table` and `json`, plus a `quiet` assertion, and a `--help` snapshot per subcommand.
7. **Every request property and query parameter of the group's operations** has a surface row: a flag, a deliberately fixed value, or an exclusion with a reason. An operation is not done when only its required fields are reachable.
8. Every v1 capability the group replaces has a parity row naming an existing test.
9. Live coverage per the risk rules, or a stated reason it cannot safely be exercised.

Points 1, 4, 5, 7, 8, and 9 are enforced by test rather than by review: the coverage invariants in Task 7 fail the build when a row is missing, names a test that does not exist, or skips live coverage without a reason.

---

## File Structure

```
Cargo.toml
src/
  main.rs                         thin entry; forbid(unsafe_code)
  lib.rs                          run(): tracing, output resolution, offline/online split, dispatch
  config.rs                   [T] profiles, ~/.flute2/config.toml
  auth/
    mod.rs
    keychain.rs               [T] OS keychain + FLUTE2_ env fallback
    token.rs                  [T] TokenStore, OAuth2Fetcher
  api/
    mod.rs                        ApiError
    client.rs                 [T] send/issue, 401-retry, base_url override
    redact.rs                 [T] PCI masking (split out of v1's client)
    error.rs                  [T] four-shape error envelope parser
    endpoints/
      mod.rs
      ping.rs  customers.rs  transactions.rs  payment_methods.rs
      pos.rs  terminals.rs  settlements.rs  settings.rs
      payment_links.rs  payment_sessions.rs  api_keys.rs
  cli/
    mod.rs                        clap root, global flags
    output.rs                 [T] Envelope, ErrorJson, exit codes
    money.rs                  [T] decimal parsing, exact JSON number
    address.rs                    v2 address shape
    common.rs                     shared args: pagination, billing, contact
  groups/
    mod.rs
    ping.rs  customers.rs  transactions.rs  payment_methods.rs
    pos.rs  terminals.rs  settlements.rs  settings.rs
    payment_links.rs  payment_sessions.rs  api_keys.rs  auth.rs
tests/
  support/
    mod.rs                        mock server + binary harness + test_fn_exists
    spec.rs                       exchange conformance, divergence rules
    contracts.rs                  the contract matrix + PENDING
    surface.rs                    the API surface matrix
    parity.rs                     v1 capability-parity matrix
  coverage.rs                     layer 1 invariants
  conformance.rs                  layer 2 + vendored-spec hash gate
  cmd_<group>.rs                  layer 3, one per group
  surface.rs                      layer 4a invariants
  parity.rs                       layer 4b invariants
  harness_controls.rs             negative controls: what the harness can see
  live.rs                         layer 5 harness — COMMITTED
  live/                           layer 5 scenarios — COMMITTED
  redaction.rs                    layer 6
  snapshots.rs                    layer 7
.flute2-live.env           sandbox identifiers — NOT COMMITTED
.flute2-live.env.example   template with zero UUIDs — committed
docs/reference/
  openapi-v2.json                 vendored bundle
  openapi-v2.sha256               hash gate
```

`[T]` marks a module transplanted from the v1 CLI (`getflute/flute-cli`) and adapted. Roughly 1,900 lines carried over against ~13,000 written new. **Rewrite their comments to house style as part of the transplant** — v1's carry ticket references and change history this repo excludes.

The v1 checkout is located by `$FLUTE_CLI_V1`, defaulting to a sibling checkout at `../flute-cli`. This repository is public, so no contributor's absolute path belongs in it.

---

---

## The plan is five documents

| Document | Contents |
|---|---|
| [`00-overview.md`](00-overview.md) | Goal, global constraints, the testing contract, file structure — **read first, applies to every task** |
| [`01-foundation.md`](01-foundation.md) | Phase 0, Tasks 1–8b: the crate, transplants, client, matrices, clap root, auth |
| [`02-slice.md`](02-slice.md) | Phase 1, Tasks 9–14: the vertical slice and its review gate |
| [`03-groups.md`](03-groups.md) | Phase 2, Tasks 15–24: one group per pull request |
| [`04-hardening.md`](04-hardening.md) | Phase 3, Tasks 25–28: snapshots, CI, live pass, docs |

**Task details are hypotheses, not a script.** Where working code or a failing test contradicts a task, change the plan in the same commit that changes the code, and say so in the commit body. A plan that has to be obeyed after it is proven wrong is worse than no plan.

## Coverage check

Every spec section maps to at least one task:

| Spec section | Tasks |
|---|---|
| §2 Scope, 50 operations | 9–11, 15–24 |
| §2 v1→v2 renames, no aliases | 25 (removed-command test) |
| §3 Module layout, transplants | 2–6 |
| §3 Injectable client | 6 |
| §3 Offline/online split | 8 |
| §3 Test seams, production refusal | 6 |
| §4 Command surface | 9–11, 15–24 |
| §4 Environment | 6, 8, 28 |
| §4 Pagination, `--all`, bounds | 15 |
| §4 Client-side validation | 11, 18 |
| §4 api-keys feature flag | 24 |
| §5 Output modes and streams | 8 |
| §5 Success envelope, page_info, unwrapping | 5, 11 |
| §5 Error envelope | 5 |
| §5 Exit codes, 130, 404-idempotency | 5, 15, 18 |
| §5 Four error shapes | 4 |
| §6 Testing layers 1–7 | Every task; 7, 7b, 12, 13, 25, 27 |
| §6 Contract matrix and coverage invariants | 7 |
| §6 Exchange conformance | 7, then every group |
| §6 API surface coverage | 7a |
| §6 Capability parity | 7b |
| §6 Negative controls on the harness | 7c |
| §6 Narrow executable divergences | 7 |
| §6 Committed live tests, risk-selected | 13, 27 |
| §6 Negative and omission tests | 11, then every group |
| §7 Build, release, naming | 1, 26 |
| §7 Documents | 1, 28 |
| §3 Three-category command split | 8b |
| §4 auth, version, update, completion | 8b |
| §4 Destructive commands require `--yes` | 15, 16, 18, 22, 23, 24 — all six |
| §3 Bodyless responses (`Response.body: Option`) | 6, 9, 16, 22, 23 |
| §3 Host constants | 6 |
| §5 Write responses are single objects (D11) | 11, 17 |
| §4 Four instrument shapes | 11, 17 |
| §4 POS long-poll (both wait controls) | 18 |
| §4 `default_profile` resolution | 6, 8b |
| §8 Documentation defects | 7, 13, 27 |
| §9 Build order | Phase structure |

**Known gaps, stated rather than hidden:**
- Ctrl-C / exit 130 is not reachable from `assert_cmd`; it is covered only by the live pass (Tasks 18, 27).
- `auth login` prompts for a secret via `rpassword`, so the prompt itself is not driven by `assert_cmd`. Its storage path is covered by the keychain tests in Task 6 and the round trip by the live pass.
- OpenAPI's dialect is not JSON Schema 2020-12; conformance validates the overlap. Every exemption is scoped to named operations and carries a defect ID, an evidence test, and a removal condition, so a second undocumented field still fails (Task 7).
- The seven D11 write responses are validated against the operations' own declared response examples rather than a schema, because the bundle contains no schema matching the shape they return. That is weaker than schema validation, and their real oracle is the live test each divergence names (Tasks 7, 13, 27).
- The matrices defer their existence checks through `PENDING` so that every commit is green. The deferral is only as good as the Task 27 gate that empties it; until that gate passes, the coverage claims hold over a subset.
- Whether capture takes `captureAmount` or `amount` is settled by a live call in Task 17, not by this plan (D15).
- Documentation filenames are lowercase — `readme.md`, `agents.md` — matching v1. Task 1 checks it; `agents.md` is created lowercase in Task 28.

