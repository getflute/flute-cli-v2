#!/usr/bin/env bash
# Run the live suite in the passes its tiers call for, each on the
# credentials it needs, so a failure is one the scenario earned rather than
# one the wrong account produced.
#
#   scripts/live.sh --sandbox FILE [--partner FILE] [--terminal FILE]
#
#   --sandbox FILE   merchant credentials and FLUTE2_LIVE_* ids for the
#                    sandbox: every unattended, reversible scenario that
#                    needs neither a partner nor a terminal
#   --partner FILE   partner credentials, sourced over the sandbox file: the
#                    `_needs_partner` scenarios. Set FLUTE2_LIVE_MERCHANT_ID
#                    here to a merchant under that partner.
#   --terminal FILE  credentials and ids for an account with a terminal,
#                    sourced alone: the `_needs_terminal` scenarios. Set
#                    FLUTE2_LIVE_API_BASE_URL and FLUTE2_LIVE_OAUTH_URL here
#                    when that account is not on the sandbox.
#
# A pass whose file is not given is reported as skipped. `_attended` and
# `_irreversible` scenarios are never run here: they need a person, or change
# what later scenarios see. Run those by name.
#
# Each file is a shell file of `export` lines, like .flute2-live.env.example.
# Every pass starts from an environment with no FLUTE2_ variable set, so
# nothing leaks from the shell or from the previous pass.
set -euo pipefail

cd "$(dirname "$0")/.."

sandbox="" partner="" terminal=""
while [ $# -gt 0 ]; do
  case "$1" in
    --sandbox) sandbox="$2"; shift 2 ;;
    --partner) partner="$2"; shift 2 ;;
    --terminal) terminal="$2"; shift 2 ;;
    -h|--help) sed -n '2,25p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown argument: $1 (see --help)" >&2; exit 3 ;;
  esac
done
[ -n "$sandbox" ] || { echo "--sandbox FILE is required (see --help)" >&2; exit 3; }
for f in "$sandbox" "$partner" "$terminal"; do
  [ -z "$f" ] || [ -r "$f" ] || { echo "cannot read $f" >&2; exit 3; }
done

cargo test --locked --features live --test live --no-run --quiet

results=()
failed=0

# pass NAME COUNT FILE... TEST-FILTER-ARG...
pass() {
  local name="$1"
  shift
  echo "==> $name"
  # shellcheck disable=SC2016,SC2046 # expanded by the inner shell; one -u per name
  if env $(env | sed -n 's/^\(FLUTE2_[A-Za-z0-9_]*\)=.*/-u \1/p') \
    bash -c 'n="$1"; shift; set -a
             while [ "$n" -gt 0 ]; do . "$1"; shift; n=$((n - 1)); done
             set +a
             exec cargo test --locked --quiet --features live --test live -- \
               --ignored --test-threads=1 "$@"' \
    _ "$@"; then
    results+=("$name: passed")
  else
    results+=("$name: FAILED")
    failed=1
  fi
}

pass sandbox 1 "$sandbox" \
  --skip attended --skip irreversible --skip needs_terminal --skip needs_partner

if [ -n "$partner" ]; then
  pass partner 2 "$sandbox" "$partner" needs_partner
else
  results+=("partner: skipped (no --partner)")
fi

if [ -n "$terminal" ]; then
  pass terminal 1 "$terminal" needs_terminal
else
  results+=("terminal: skipped (no --terminal)")
fi

echo
printf '%s\n' "${results[@]}"
exit "$failed"
