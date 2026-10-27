#!/usr/bin/env bash
# Run the unattended live suite once, each scenario as the account it names.
#
#   scripts/live.sh [DIR]
#
# DIR holds .flute2-live-merchant.env, .flute2-live-partner.env,
# .flute2-live-partner-uat.env and .flute2-live-terminal.env (the repository
# root when omitted), each like .flute2-live.env.example. A scenario whose
# file is missing fails naming it.
#
# `_attended`, `_irreversible` and `_needs_configured_user` scenarios are left
# out: they need a person, change what later scenarios see, or need a partner
# user with Schedule As and Split Profiles set up. Run those by name.
set -euo pipefail

case "${1:-}" in
  -h|--help) sed -n '2,13p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
  "") ;;
  *)
    [ -d "$1" ] || { echo "not a directory: $1 (see --help)" >&2; exit 3; }
    FLUTE2_LIVE_DIR=$(cd "$1" && pwd)
    export FLUTE2_LIVE_DIR
    ;;
esac

cd "$(dirname "$0")/.."
exec cargo test --locked --features live --test live -- \
  --ignored --test-threads=1 --skip attended --skip irreversible \
  --skip needs_configured_user
