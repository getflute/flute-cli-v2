#!/usr/bin/env bash
# The checks CI runs, in the order it runs them. `.github/workflows/ci.yml`
# calls this script for each job, so a local run and a CI run execute the
# same commands.
#
#   scripts/ci.sh            both jobs: check, then msrv
#   scripts/ci.sh check      format, lint, test (stable)
#   scripts/ci.sh msrv       the test suite on rust-version
#   scripts/ci.sh msrv-toolchain
#                            print rust-version, for the toolchain install
#
# The workflow installs the toolchains and system packages; this script
# assumes them. Locally that means `rustup toolchain install stable` and
# `rustup toolchain install "$(scripts/ci.sh msrv-toolchain)"`.
set -euo pipefail

cd "$(dirname "$0")/.."

msrv_toolchain() {
  sed -n 's/^rust-version = "\(.*\)"$/\1/p' Cargo.toml
}

step() {
  printf '\n==> %s\n' "$*"
  "$@"
}

check() {
  step cargo fmt --all --check
  step cargo clippy --locked --all-targets -- -D warnings

  # **The suite must be hermetic, and this is what proves the run is not
  # quietly supplying credentials.** `cargo test` never reaches the network
  # and never reads the keychain; an environment that had a client id in it
  # could hide a test that depended on one.
  local leaked
  leaked=$(env | grep -E '^FLUTE2_' | cut -d= -f1 || true)
  if [ -n "$leaked" ]; then
    echo "error: the suite is hermetic and the environment sets: $leaked" >&2
    exit 1
  fi

  step cargo test --locked

  # The live suite needs a sandbox account, so it is compiled here and never
  # run: a scenario that does not build fails the check.
  step cargo test --locked --features live --test live --no-run

  # The vendored bundle's hash is asserted by the suite, which can only see a
  # local edit. Drift in the *published* bundle is spec-drift.yml's job,
  # because detecting it needs a fetch.
  step python3 docs/reference/group-facts.py --check
}

msrv() {
  local toolchain
  toolchain=$(msrv_toolchain)
  # **`--locked` is load-bearing.** Without it the run resolves a different
  # dependency tree from the one under review, and the rust-version claim in
  # Cargo.toml stops applying to what ships.
  step cargo "+$toolchain" test --locked
}

case "${1:-all}" in
  check) check ;;
  msrv) msrv ;;
  msrv-toolchain) msrv_toolchain ;;
  all) check; msrv ;;
  *)
    echo "usage: scripts/ci.sh [check|msrv|msrv-toolchain]" >&2
    exit 2
    ;;
esac
