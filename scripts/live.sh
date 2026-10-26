#!/usr/bin/env bash
# Run the unattended live suite once, each scenario as the account it names.
#
#   scripts/live.sh [DIR]
#
# DIR holds .flute2-live-merchant.env, .flute2-live-partner.env and
# .flute2-live-terminal.env (the repository root when omitted), each like
# .flute2-live.env.example. A scenario whose file is missing fails naming it.
#
# `_attended` and `_irreversible` scenarios are left out: they need a person,
# or change what later scenarios see. Run those by name.
set -euo pipefail

dir="${1:-$PWD}"
[ -d "$dir" ] || { echo "not a directory: $dir" >&2; exit 3; }
dir=$(cd "$dir" && pwd)

cd "$(dirname "$0")/.."
FLUTE2_LIVE_DIR="$dir" exec cargo test --locked --features live --test live -- \
  --ignored --test-threads=1 --skip attended --skip irreversible
