# flute2

[![CI](https://github.com/getflute/flute-cli-v2/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/getflute/flute-cli-v2/actions/workflows/ci.yml)

`flute2` — a cross-platform CLI for the Flute payments platform, targeting the **v2 API**. MIT licensed, built in Rust.

---

## Install

**Shell script (macOS / Linux — installs from GitHub Releases)**

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/getflute/flute-cli-v2/releases/latest/download/flute2-installer.sh | sh
```

**PowerShell (Windows — installs from GitHub Releases)**

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/getflute/flute-cli-v2/releases/latest/download/flute2-installer.ps1 | iex"
```

The installer drops `flute2.exe` in `%USERPROFILE%\.cargo\bin` and adds that directory to your user `PATH`. Open a new terminal afterwards so the updated `PATH` takes effect, then verify with `flute2 version`.

A release builds `aarch64-apple-darwin` (Apple silicon macOS), `x86_64-pc-windows-msvc` (Windows), and `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu` (Linux). Every other platform builds from source, Intel macOS among them.

The Homebrew formula is attached to each release page rather than published to a tap, so install it from the page's `.rb` asset.

**From source**

```sh
# Installs `flute2`
cargo install --locked --path .

# Or just build
cargo build --release
# Binary lands at target/release/flute2
```

On Linux the keychain is the Secret Service, reached over D-Bus: building needs `libdbus-1-dev` and `pkg-config`, and the released binary needs `libdbus-1` at runtime plus an unlocked Secret Service. Where there is none — a container, a CI runner — set `FLUTE2_CLIENT_ID` and `FLUTE2_CLIENT_SECRET` instead.

---

## Quick start

**Authenticate** (interactive — prompts for client ID and secret, stored in the OS keychain per profile):

```sh
flute2 auth login
```

Client credentials are issued for your Flute account. Creating more with `flute2 api-keys create` needs partner credentials: a merchant's own client id and secret are refused with a 403.

For CI, a container, or any non-interactive environment use env vars instead of the keychain. They take precedence over it, so nothing is stored on the machine:

```sh
export FLUTE2_CLIENT_ID=<your-client-id>
export FLUTE2_CLIENT_SECRET=<your-client-secret>
```

Setting exactly one of the two is an error naming the other, rather than a silent fall-through to whoever last logged in on that machine.

**Health check, version and processor ids** — `transactions create`, `transactions credit` and `settlements close` require a processor id, and there is a separate processor per payment method:

```sh
flute2 ping
flute2 version
flute2 settings payment-config
```

**Charge a card:**

```sh
flute2 transactions create \
  --payment-processor-id <card-processor-id> \
  --amount 10.50 --reference-id order-1001 \
  --card 4111111111111111 --exp 12/2032 --cvv 123 \
  --billing-line1 "123 Test St" --billing-postal-code 10001 --billing-country US
```

**Send a billing address.** When address verification is on for the merchant account, a charge without one — or with one the card's issuer does not match — comes back `Declined` with an AVS reason. On the sandbox, `123 Test St`, `10001`, `US` is the address that verifies.

**Give every charge its own `--reference-id`.** The API refuses a second charge of the same card for the same amount as a possible duplicate unless the reference ids differ.

**Authorize now, capture later:**

```sh
flute2 transactions create --payment-processor-id <card-processor-id> \
  --amount 50.00 --reference-id order-1002 --capture-method manual \
  --card 4111111111111111 --exp 12/2032 --cvv 123 \
  --billing-line1 "123 Test St" --billing-postal-code 10001 --billing-country US
flute2 transactions capture --transaction-id <transaction-id> --amount 20.00
```

**Refund or void** — one command; the API works out which applies:

```sh
flute2 transactions reversal --transaction-id <transaction-id> [--amount 5.00]
```

The API voids a transaction that has not settled yet, and a void is always for the whole amount. So `--amount` is accepted only on a settled card transaction; on anything else the CLI refuses it before sending the reversal.

**Vault a customer and charge the stored card.** A saved card carries no address, so the charge needs one: save it on the customer and pass `--customer-id`, or pass the `--billing-*` flags on the charge itself.

```sh
flute2 customers create --first-name Ada --last-name Lovelace --email ada@example.com \
  --billing-line1 "123 Test St" --billing-postal-code 10001 --billing-country US
flute2 payment-methods add-card --customer-id <customer-id> \
  --card 4111111111111111 --exp 12/2032
flute2 transactions create --payment-processor-id <card-processor-id> \
  --amount 12.00 --reference-id order-1003 \
  --payment-method-id <method-id> --instrument card --customer-id <customer-id>
```

**Debit a bank account (ACH).** ACH uses its own processor id, and a new bank account needs a billing address and contact details:

```sh
flute2 transactions create \
  --payment-processor-id <ach-processor-id> \
  --amount 20.00 --reference-id order-1004 \
  --ach-account-number 123456789 --ach-routing-number 021000021 \
  --ach-account-type checking --ach-account-holder-type personal \
  --sec-code web --requester-ip 203.0.113.10 \
  --contact-first-name Ada --contact-last-name Lovelace \
  --contact-email ada@example.com --contact-phone +14155552309 \
  --billing-line1 "123 Test St" --billing-postal-code 10001 --billing-country US
```

**A declined charge is not an error.** It exits 0, because the request succeeded — the payment did not. Read `transactionStatus`:

```sh
flute2 --output json transactions create … | jq -r '.data.transactionStatus'
```

**See why, in one screen:** `transactions inspect <transaction-id>` shows the status, the amounts, the decline reason and the address-verification result together, where `get` prints every field the API returns.

**Output modes** are controlled by `--output table|json|quiet` (or the `FLUTE2_OUTPUT` env var / `output` key in `~/.flute2/config.toml`):

- `table` — human-readable (default)
- `json` — structured envelope, suitable for scripts and agents
- `quiet` — resource id only, one per line; ideal for shell capture: `TXN=$(flute2 --output quiet transactions create --payment-processor-id <id> --amount 10.50 …)`

---

## Profiles / environments

| Profile | Alias | API |
|---|---|---|
| `sandbox` | — | `https://sandbox.api.flute.com` |
| `production` | `prod` | `https://api.flute.com` |

`sandbox` is the default. **Every command that resolves a profile prints a warning banner to stderr when that profile is production.** `completion` and `update` resolve none, so they are the two commands that never print it.

**Select a profile** (three ways, highest precedence first):

1. Flag: `--profile production`
2. Env var: `FLUTE2_PROFILE=production`
3. Config file default: `~/.flute2/config.toml` → `default_profile = "production"`

With none of them set, the profile is `sandbox`.

```sh
flute2 auth switch production     # sets default_profile
flute2 --profile sandbox ping     # overrides it for one command
flute2 auth status                # which profile, and do the credentials work
```

`auth status` is a live check: it calls the API, so `authenticated` is true only when the stored credentials actually round-trip.

Credentials are kept in the OS keychain, keyed per profile, so a sandbox login and a production login coexist. The config file (`~/.flute2/config.toml`) stores only non-secret settings: `default_profile`, `output` and `auto_update_check`.

**Precedence for all settings:** flag > env var > `config.toml` > built-in default.

### Environment variables

| Variable | Effect |
|---|---|
| `FLUTE2_CLIENT_ID`, `FLUTE2_CLIENT_SECRET` | Credentials, checked before the keychain. Setting exactly one is an error. |
| `FLUTE2_PROFILE` | Active profile, below `--profile`. |
| `FLUTE2_OUTPUT` | Output mode, below `--output`. |
| `FLUTE2_NO_KEYCHAIN` | Any value makes the keychain unavailable, fail-closed. |
| `FLUTE2_NO_UPDATE_CHECK` | Any value suppresses the update notice; `CI` does too. |
| `FLUTE2_API_BASE_URL`, `FLUTE2_OAUTH_URL` | Point at another host. **Refused on production.** |
| `FLUTE2_GITHUB_TOKEN` | A token for `update`'s release lookup. |
| `FLUTE2_INSTALLER_GITHUB_BASE_URL`, `FLUTE2_INSTALLER_GHE_BASE_URL` | Point `update`'s release lookup at another GitHub host, or at a GitHub Enterprise server. Setting both is an error. |

No bare `FLUTE_` name is ever read: v1's variables, keychain entry and config file are separate. One that is set while its `FLUTE2_` counterpart is not gets a line on stderr at startup, naming the variable read instead.

---

## Command overview

| Group | What it does |
|---|---|
| `auth` | `login`, `status`, `switch`, `logout`, `token` — credential and profile management |
| `transactions` | `create`, `capture`, `reversal`, `credit`, `tip-adjust`, `ach-hold`, `ach-release`, `share-receipt`, `calculate-amount`, `get`, `list`, `inspect` — the card and ACH payment lifecycle. `create` accepts AVS `--billing-*` flags |
| `customers` | `create`, `get`, `list`, `update`, `delete` — the customer record. `create`/`update` accept AVS `--billing-*` flags |
| `payment-methods` | `list`, `get`, `add-card`, `add-ach`, `update`, `delete`, `set-default` — the vault of stored cards and bank accounts |
| `payment-links` | `create`, `get`, `list`, `update`, `delete`, `share` — hosted links a payer opens and pays |
| `payment-sessions` | `create`, `get`, `cancel` — hosted checkout sessions |
| `terminals` | `list`, `status` — card-present terminal inventory and readiness |
| `pos` | `create` (with `--wait` long-poll), `get`, `list`, `cancel`, `print-receipt` — card-present transactions |
| `settlements` | `list`, `get`, `close` — settlement batch queries, and closing a processor's open batch |
| `settings` | `payment-config`, `contact-info`, `autofill`, `update-autofill` — account settings, including the processor ids |
| `api-keys` | `create`, `list`, `revoke` — merchant API key management |
| `ping` | API health check |
| `version` | Print CLI version and active profile |
| `update` | Self-update to the latest GitHub Release |
| `completion` | Print shell completion script |

Run `flute2 <group> --help` for a group's commands, and `flute2 <group> <command> --help` for every flag, with its bounds and its reasons.

### Destructive commands need `--yes`

`customers delete`, `payment-methods delete`, `payment-links delete`, `payment-sessions cancel`, `pos cancel` and `api-keys revoke` refuse without it, and refuse **before** issuing a request. Repeating one that already happened is success for the four deletes and revokes: a 404 exits 0, with `found: false` in the confirmation because the server cannot say whether the resource ever existed. The two cancels differ, because a cancelled resource still exists: a second cancel is a 400 state error and exits 3, and a 404 is an unknown id and exits 4.

### Pagination

`--page-index` is **0-based**, mirroring the API's own `pageIndex`, and `--page-size` is bounded 1–100. Both are omitted when you do not pass them, so the server's defaults govern.

```sh
flute2 transactions list --page-index 2 --page-size 50
flute2 transactions list --all            # every page, no page_info
```

In `table` mode, a page that is not the last ends with a line on stderr naming the next `--page-index` and `--all`.

`--all` with `--page-index` is refused: a walk of everything and a starting page contradict each other. `--page-size` stays legal with `--all` — it is a batch size, not a position. A walk that repeats a page, or runs past 10000 of them, ends as a `decode` error rather than continuing. A page is a JSON object whose `items` is nullable and optional, so an absent one, a null one and an empty array are all an empty page; a body that is not an object, or an `items` that is neither an array nor null, is a `decode` error rather than a collection reported as empty. `api-keys list` is not paginated at all and rejects the flags.

### Amounts

Plain decimals: `--amount 10.50`. Two decimal places, validated locally, and sent as an exact JSON number. **No amount passes through a float** at any point between argv and the wire. `--exp` is `MM/YY` or `MM/YYYY`.

### Terminals and POS

A card-present sale runs on a terminal in **semi-integrated** mode that is online:

```sh
flute2 terminals list
flute2 pos create --terminal-id <id> --pos-device-id <device> \
  --amount 10.50 --currency-code USD --wait
```

`--wait` holds the create until the terminal accepts, then polls until the transaction leaves `InProgress`. `--wait-timeout` requires `--wait`, defaults to 120 seconds and accepts 0 to 86400; it expires while a poll is open as well as between polls, and on expiry the last-known status is printed and the exit code is 1. Ctrl-C leaves stdout empty and exits 130.

A terminal takes one in-progress transaction at a time.

---

## Output and scripting

`--output json` wraps every success response in a consistent envelope:

```json
{ "object": "transaction", "data": { }, "meta": { "environment": "sandbox", "correlation_id": "…", "page_info": { } } }
```

`data` is what the resource reports, with its object keys emitted in sorted order at every depth. On a collection read it is the array itself. `page_info` reproduces the API's `pageInfo` and is absent — not null — where there are no pages.

`table`, the default, is for reading: declared fields first, in the order that matters when a payment goes wrong. Nothing is hidden, so a field the API adds still appears.

`quiet` prints the identifier alone, for chaining:

```sh
TXN=$(flute2 --output quiet transactions create --payment-processor-id <id> \
        --amount 10.50 --reference-id order-1005 --capture-method manual \
        --card 4111111111111111 --exp 12/2032 --cvv 123 \
        --billing-line1 "123 Test St" --billing-postal-code 10001 --billing-country US)
flute2 transactions capture --transaction-id "$TXN"
```

Errors (non-zero exit) are also written to **stdout** as structured JSON when `--output json` is active:

```json
{ "kind": "api"|"transport"|"auth"|"decode"|"client", "message": "…", "status": 422, "correlation_id": "…" }
```

**Parse one stream — never both.** Data always goes to stdout; tracing, the production banner, the update notice and the `--wait` warnings always go to stderr.

### Semantic exit codes

| Code | Meaning |
|---|---|
| `0` | Success, **including a declined payment** and a 404 on a delete or revoke |
| `1` | General: transport, an unreadable response — including a `pos create --wait` response with no `posTransactionStatus` — a server 5xx, an expired `--wait-timeout` |
| `2` | Auth failure (401/403 or missing credentials) |
| `3` | Validation / bad input — server 400/422, client-side refusals, or CLI usage/parse errors |
| `4` | Not found: a 404 that was not mapped to success |
| `130` | Interrupted — Ctrl-C during a `pos create --wait` poll |
| `141` | The consumer of stdout stopped reading, so the output is a prefix — `128 + SIGPIPE`, what a `pipefail` pipeline expects when `jq -e` or `head` closes the pipe early |

Under `--output json`, a failure is a JSON envelope on stdout, CLI usage/parse errors included, so a machine consumer never has to guess at an empty stdout. [`agents.md`](agents.md) has the retry table and the five exits that carry no error envelope.

---

## Logging & debugging

`flute2` writes all logs to **stderr**; command output goes to stdout. That split holds even with `--debug` on, so `flute2 --debug --output json …` still emits a clean, parseable JSON document on stdout.

By default only warnings and a few brief notices are logged (for example, a one-line note when a stale token triggers the automatic single 401 retry). Successful commands are otherwise quiet on stderr apart from the production banner.

### `--debug`

The global `--debug` flag prints full HTTP request/response traces — method, URL, status, and body — to stderr:

```sh
flute2 --debug ping
flute2 --debug transactions get <txn-id>
```

**Sensitive fields are masked before anything is logged:**

| Field | How it appears in logs |
|---|---|
| Card / bank account numbers (`cardNumber`, `accountNumber`, `routingNumber`, `pan`) | masked to the last 4 — for example, `************1111` |
| CVV / security code (`securityCode`, `cvv`, `cvc`) | removed entirely — `***` |
| Client secrets, API keys and tokens (`clientSecret`, `authorization`, and any field name containing `secret`, `token` or `apikey`) | removed entirely — `***` |
| Bearer token | never logged (it is sent as a header, never part of the body trace) |

Masking applies to every trace rather than only to `--debug`, and an error body is redacted structurally before it is parsed.

> Masking lowers the risk but does not eliminate it: a `--debug` trace still reveals amounts, the last 4 digits, and request metadata. Don't capture `--debug` output into shared or long-lived logs when operating on `production`.

### `RUST_LOG` — fine-grained control

Setting `RUST_LOG` overrides the built-in filters entirely and accepts the standard [`EnvFilter` syntax](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html). The same field masking is applied to `flute2`'s own traces no matter how the level is set:

```sh
# Only flute2's own HTTP traces, nothing from dependency crates
RUST_LOG=flute_cli2=debug flute2 ping

# Add connection / TLS / DNS detail from the HTTP stack
RUST_LOG=flute_cli2=debug,reqwest=debug,hyper=debug flute2 ping
```

### Filter presets

| Mode | Effective filter |
|---|---|
| default | `warn,flute_cli2=info` |
| `--debug` | `debug,flute_cli2=debug,reqwest=debug,hyper=info` |
| `RUST_LOG` set | your value (overrides both of the above) |

### Quick reference

| Goal | Command |
|---|---|
| See the API's response when a command fails | `flute2 --debug <cmd>` → read the `HTTP response` line on stderr |
| Get a structured error for a script | `--output json` → error envelope (kind / message / status / correlation_id) on stdout |
| Diagnose TLS / DNS / connection problems | `RUST_LOG=flute_cli2=debug,reqwest=debug,hyper=debug flute2 ping` |
| Confirm which environment/URL is being hit | `flute2 --debug ping` (the request URL is in the trace) |

Under `--output json` a command error is a JSON envelope on stdout, and nothing on stderr. In the other modes it is printed to stderr. Either way it appears whatever the log level.

For a support conversation, `meta.correlation_id` from `--output json` is worth more than a trace: it is what identifies the request server-side.

---

## Shell completions

Supported shells: `bash`, `zsh`, `fish`, `powershell`, `elvish`.

Generate and install a completion script:

```sh
# Bash
flute2 completion bash > /etc/bash_completion.d/flute2

# Zsh (into the first directory on $fpath)
flute2 completion zsh > "${fpath[1]}/_flute2"

# Fish
flute2 completion fish > ~/.config/fish/completions/flute2.fish

# PowerShell
flute2 completion powershell | Out-String | Invoke-Expression

# Elvish
flute2 completion elvish > ~/.config/elvish/lib/flute2.elv
```

---

## Self-update

```sh
flute2 update
```

Downloads and installs the latest GitHub Release binary in-place. A build from source says so rather than pretending.

After any successful command, a newer release produces a one-line notice on stderr. It is skipped on `update`, `completion` and every `auth` command, under `--output json`, whenever stderr is not a terminal, and when `FLUTE2_NO_UPDATE_CHECK` or `CI` is set or the config file sets `auto_update_check = false`.

---

## For AI agents / MCP

See [`agents.md`](agents.md) for the machine-readable contract: the success and error envelope shapes, the exit-code table, the idempotency table, the pagination rules, and copy-pasteable commands for common agent intents.

---

## Development

```sh
# Everything CI runs, in the order it runs it: format, lint, tests, the live
# suite's build and the generated facts on stable, then the tests on the
# rust-version toolchain (install it with
# `rustup toolchain install "$(scripts/ci.sh msrv-toolchain)"`)
scripts/ci.sh

# One CI job at a time
scripts/ci.sh check
scripts/ci.sh msrv

# Run all tests (hermetic and offline)
cargo test

# Lint (zero warnings enforced)
cargo clippy --all-targets -- -D warnings

# Format check
cargo fmt --check

# The generated API facts
python3 docs/reference/group-facts.py --check

# Release build
cargo build --release
# Binary lands at target/release/flute2
```

The four checks pass at every commit. The suite never touches the network or the OS keychain, and every binary-level test runs against a mock server through a base-URL override that production refuses.

The codebase enforces `#![forbid(unsafe_code)]` — there is zero `unsafe` Rust.

The live sandbox suite is committed, `#[ignore]`d, and opted into by hand:

```sh
source .flute2-live.env      # not committed; see .flute2-live.env.example
cargo test --features live --test live -- --ignored --test-threads=1 \
    --skip attended --skip irreversible --skip needs_terminal --skip needs_partner
```

Read the module documentation at the top of `tests/live.rs` first. The scenarios share one sandbox account, which is why the thread count is fixed and why the four name suffixes carry the tiers.

Where the API's published reference and its behaviour disagree, the conformance layer carries a narrow, named exemption rather than relaxing the check: each one is scoped to the operations it covers, names the live scenario that is its only oracle, and states the condition that deletes it.

---

## Coming from `flute` (v1)

`flute2` installs alongside `flute` and does not replace it. The v1 commands,
flags and JSON paths, and what each becomes in v2, are in
[`docs/migrating-from-v1.md`](docs/migrating-from-v1.md).

---

MIT licensed. See [`LICENSE`](LICENSE).
