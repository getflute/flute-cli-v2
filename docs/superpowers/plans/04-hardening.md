# Phase 3 — Hardening and documentation

> Part of the [flute-cli-v2 implementation plan](00-overview.md). Read [`00-overview.md`](00-overview.md) first — its global constraints and testing contract apply to every task here.

# Phase 3 — Hardening and documentation

### Task 25: Contract snapshots across the whole command tree

Layer 4. Deferred to here deliberately: snapshotting a tree that is still growing produces churn, not signal.

**Files:** Create `tests/snapshots.rs`, `tests/snapshots/` (insta files)

- [ ] **Step 1: Write the snapshot tests**

```rust
// tests/snapshots.rs
fn help_for(args: &[&str]) -> String {
    let out = assert_cmd::Command::cargo_bin("flute2").unwrap()
        .args(args).arg("--help").output().unwrap();
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn root_help_is_stable() {
    insta::assert_snapshot!(help_for(&[]));
}

#[test]
fn every_group_help_is_stable() {
    for group in ["auth", "customers", "payment-methods", "transactions", "pos",
                  "terminals", "settlements", "settings", "payment-links",
                  "payment-sessions", "api-keys"] {
        insta::assert_snapshot!(group, help_for(&[group]));
    }
}

/// Pagination help must state that the index is zero-based, because the
/// obvious reading of "page" is one-based and would be off by one.
#[test]
fn list_help_states_page_index_is_zero_based() {
    assert!(help_for(&["customers", "list"]).contains("0-based"));
}

/// Dropped v1 commands must fail as unrecognised, never silently succeed.
#[test]
fn removed_v1_commands_are_not_accepted() {
    for args in [vec!["transactions", "sale"], vec!["transactions", "void"],
                 vec!["ach", "debit"], vec!["keys", "list"],
                 vec!["devices", "list"], vec!["subscriptions", "list"]] {
        assert_cmd::Command::cargo_bin("flute2").unwrap().args(&args).assert().code(3);
    }
}
```

- [ ] **Step 2: Review and accept the snapshots**

Run: `cargo insta review`
Read every one. A snapshot accepted without reading proves nothing.

- [ ] **Step 3: Add shell-completion tests, ported from v1**

- [ ] **Step 4: Run the full gate and commit**

```bash
cargo test && cargo fmt --check && cargo clippy --all-targets -- -D warnings
git add -A && git commit -m "ARISE-4928 test(cli): contract snapshots across the command tree"
```

---

### Task 26: CI and release

**Files:** Create `.github/workflows/ci.yml`, `dist-workspace.toml`

- [ ] **Step 1: Write the CI workflow**

Runs `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and the spec hash gate on push and pull request. **Third-party actions are SHA-pinned**, not tag-pinned. `cargo test` must run with no credentials in the environment, proving the suite is hermetic.

- [ ] **Step 2: Add the upstream spec-drift workflow**

`cargo test` is hermetic and offline, so the hash test can only prove the vendored file has not been edited locally. Detecting that the *published* bundle has moved needs a fetch, which belongs in its own scheduled workflow rather than in the test suite.

```yaml
# .github/workflows/spec-drift.yml
name: spec-drift
on:
  schedule: [{ cron: "0 7 * * 1" }]
  workflow_dispatch:
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@<sha>
      - name: Compare published bundle against the vendored copy
        run: |
          curl -sSf -o /tmp/upstream.json \
            'https://developer.flute.com/_bundle/api-reference/@v2/index.json'
          upstream=$(shasum -a 256 /tmp/upstream.json | awk '{print $1}')
          vendored=$(cat docs/reference/openapi-v2.sha256)
          if [ "$upstream" != "$vendored" ]; then
            echo "::error::published v2 bundle has changed; re-vendor and read the diff"
            diff <(jq -S . docs/reference/openapi-v2.json) <(jq -S . /tmp/upstream.json) || true
            exit 1
          fi
```

It fails loudly and on a schedule rather than blocking every pull request on a third-party fetch. Re-vendoring is a deliberate act: read the diff, update `docs/doc-defects.md` for anything that changed, re-record the hash, and note in the commit what moved.

- [ ] **Step 3: Configure cargo-dist**

Targets `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`, `x86_64-unknown-linux-gnu`, with shell and PowerShell installers and a Homebrew formula named `flute2`. **No `conflicts_with`** — v1 and v2 coexist because devices and subscriptions exist only in v1.

- [ ] **Step 4: Verify a dry-run build, then commit**

```bash
git add -A && git commit -m "ARISE-4928 ci(cli): add CI and release configuration"
```

---

### Task 27: Complete the live suite, selected by contract risk

Layer 5 across the whole surface. **Not one call per group** — that is too coarse to see the things live testing exists for.

- [ ] **Step 1: Cover every high-risk operation and variant**

Fully covered, no sampling:

- **Every write endpoint** — every non-webhook, non-OAuth `POST`, `PATCH`, or `DELETE`. There are **30**, not the 19 with a JSON request body; the eleven that take no body are still writes, and several are destructive. The set is derived from the bundle by test, never hard-coded, so the count cannot silently drift.
- **Every bodyless success** — all **13**, likewise derived. A 204 or an empty 200 is exactly what a mock is most likely to get wrong, and eight of the thirteen were missed on the first pass.
- **Every documented divergence.** Each row of `DIVERGENCES` names a live test as its evidence, and Task 7's coverage invariant fails if that test does not exist.
- **Every polymorphic request variant** — all five `transactions create` shapes, both `capture` shapes.
- **Every enum and casing a schema cannot check** — `captureMethod: Auto|Manual`, `secCode`, `accountHolderType`, `posTransactionStatus`, `initiationChannel`.

Sampled, not exhaustive: reads. One representative pagination walk (`--all` across a real multi-page collection), one filter case per list endpoint, and one `get` per resource.

- [ ] **Step 2: Create and destroy resources within each test**

No test depends on a long-lived identifier beyond the four `FLUTE2_LIVE_*` values, which name infrastructure (processor, merchant, terminal, POS device) rather than records. A test that needs a customer creates one and deletes it.

Two exceptions, stated because they cannot be engineered away:

- **Transactions cannot be torn down.** A reversal is a new transaction. Live payment tests accumulate sandbox state permanently, and each needs a unique amount because of D12. Budget for a growing sandbox rather than pretending otherwise.
- **`api-keys revoke` is only ever run against a key created inside the same test.** Revoking the authenticating key would break the rest of the suite, and on production would break the caller.

- [ ] **Step 3: Close the deferral gate**

Every matrix defers its existence checks for operations not yet built, which is what let Phase 0 commit a green suite. This is where that debt is called in:

```rust
// tests/coverage.rs
/// Nothing may ship pending. Until this passes, the coverage invariants are
/// running over a subset and the suite's headline claim is not yet true.
#[test]
fn nothing_is_still_pending() {
    assert!(contracts::PENDING.is_empty(),
        "still pending: {:?}", contracts::PENDING);
    let stalled: Vec<_> = parity::CAPABILITIES.iter()
        .filter(|c| matches!(c.parity, parity::Parity::Pending(_)))
        .map(|c| c.v1).collect();
    assert!(stalled.is_empty(), "v1 capabilities still pending: {stalled:?}");
}
```

Write it `#[ignore]`d from Task 7 so it is visible and runnable throughout, and remove the `#[ignore]` here, in the commit that empties the last list.

- [ ] **Step 4: Re-verify the two assumptions the exit-code contract rests on**

- A declined card still answers **HTTP 200 with a declined status**, not 402. If any decline returns 402, the same decline exits 1 through one path and 0 through the other — stop and add a dedicated decline code before release.
- Write responses are still **single objects**, not pages (D11). If the server has converged on its published schema, delete the transitional page arm and its test rather than leaving both.

- [ ] **Step 5: Run the full suite**

Run: `cargo test --test live -- --ignored`
Expected: PASS. Record totals and anything surprising in `NOTES.local.md`.

- [ ] **Step 6: File every divergence, then commit**

```bash
git add tests/ docs/doc-defects.md
git commit -m "ARISE-4928 test(cli): complete risk-selected live coverage"
```

---

### Task 28: `readme.md` and `agents.md`

Written last, when the surface is settled and every claim can be checked against a passing test.

- [ ] **Step 1: Write `readme.md`** following v1's structure: install, auth, per-group examples, exit codes, environment variables.

- [ ] **Step 2: Write `agents.md`** — the machine-readable contract. It must document, because v1's omits or buries them:
  - The complete exit-code table **including 130**, which v1's table omits and mentions only in prose.
  - That a decline is exit 0 with a status in the body.
  - That a 404 on delete, revoke, or cancel is exit 0.
  - That `page_info` rides on collection envelopes, and that single-transaction writes carry none.
  - That stdout is data and stderr is everything else, including under `--debug`.
  - That `pos create --wait` on Ctrl-C leaves stdout empty in every mode and exits 130.
  - The full `FLUTE2_` environment surface.
  - That devices and subscriptions are unavailable in v2, and v1 remains installed for them.

- [ ] **Step 3: Verify every documented claim against a test**

For each claim, name the test that proves it. A claim with no test is either wrong or an untested behaviour — fix one or the other.

- [ ] **Step 4: Final gate and commit**

```bash
cargo test && cargo fmt --check && cargo clippy --all-targets -- -D warnings
git add -A && git commit -m "ARISE-4928 docs(cli): add readme and agents contract"
```

---

