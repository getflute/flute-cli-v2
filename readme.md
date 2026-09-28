# flute2

[![CI](https://github.com/getflute/flute-cli-v2/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/getflute/flute-cli-v2/actions/workflows/ci.yml)

`flute2` — a cross-platform CLI for the Flute payments platform, targeting the
**v2 API**. MIT licensed, built in Rust.

Driving it from a program rather than a terminal? Read
[`agents.md`](agents.md) instead: it is the machine-readable contract.

**`flute2` installs alongside `flute`.** It does not replace it, and the two do
not conflict: they are separate binaries, with separate config files and
keychain entries, authenticating against different hosts with separate
credentials. `devices` and `subscriptions` exist only in v1.

---

## Install

**Homebrew (macOS / Linux)**

```sh
brew install getflute/flute-cli-v2/flute2
```

**Shell script (macOS / Linux)**

```sh
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/getflute/flute-cli-v2/releases/latest/download/flute2-installer.sh | sh
```

**PowerShell (Windows)**

```powershell
irm https://github.com/getflute/flute-cli-v2/releases/latest/download/flute2-installer.ps1 | iex
```

A release builds `aarch64-apple-darwin` (Apple silicon macOS),
`x86_64-pc-windows-msvc` (Windows), and `x86_64-unknown-linux-gnu` and
`aarch64-unknown-linux-gnu` (Linux). Every other platform builds from source,
Intel macOS among them.

**From source**

```sh
cargo install --path .        # installs `flute2`
cargo build --release         # or just build: target/release/flute2
```

On Linux the keychain is the Secret Service, reached over D-Bus: building needs
`libdbus-1-dev` and `pkg-config`, and the released binary needs `libdbus-1` at
runtime plus an unlocked Secret Service. Where there is none — a container, a
CI runner — set `FLUTE2_CLIENT_ID` and `FLUTE2_CLIENT_SECRET` instead.

---

## Quick start

**Authenticate.** Interactive: prompts for the client id and secret and stores
them in the OS keychain, per profile. Client credentials are issued for your
Flute account, and an existing credential can create more with
`flute2 api-keys create`.

```sh
flute2 auth login
```

For CI, a container, or anything unattended, use the environment instead. It
takes precedence over the keychain, so nothing is stored on the machine:

```sh
export FLUTE2_CLIENT_ID=<client-id>
export FLUTE2_CLIENT_SECRET=<client-secret>
```

Setting exactly one of the two is an error naming the other, rather than a
silent fall-through to whoever last logged in on that machine.

**Check it works, and find your processor ids** — every transaction endpoint
requires one, and there is a separate processor per payment method:

```sh
flute2 ping
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

**Send a billing address.** When address verification is on for the merchant
account, a charge without one — or with one the card's issuer does not match —
comes back `Declined` with an AVS reason. On the sandbox, `123 Test St`,
`10001`, `US` is the address that verifies.

**Give every charge its own `--reference-id`.** The API refuses a second charge
of the same card for the same amount as a possible duplicate unless the
reference ids differ.

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

The API voids a transaction that has not settled yet, and a void is always for
the whole amount. So `--amount` is accepted only on a settled card transaction;
on anything else the CLI refuses it before sending the reversal.

**Vault a customer and charge the stored card.** A saved card carries no
address, so the charge needs one: save it on the customer and pass
`--customer-id`, or pass the `--billing-*` flags on the charge itself.

```sh
flute2 customers create --first-name Ada --last-name Lovelace --email ada@example.com \
  --billing-line1 "123 Test St" --billing-postal-code 10001 --billing-country US
flute2 payment-methods add-card --customer-id <customer-id> \
  --card 4111111111111111 --exp 12/2032
flute2 transactions create --payment-processor-id <card-processor-id> \
  --amount 12.00 --reference-id order-1003 \
  --payment-method-id <method-id> --instrument card --customer-id <customer-id>
```

**Debit a bank account (ACH).** ACH uses its own processor id, and a new bank
account needs a billing address and contact details:

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

**A declined charge is not an error.** It exits 0, because the request
succeeded — the payment did not. Read `transactionStatus`:

```sh
flute2 --output json transactions create … | jq -r '.data.transactionStatus'
```

**See why, in one screen:** `transactions inspect <transaction-id>` shows the
status, the amounts, the decline reason and the address-verification result
together, where `get` prints every field the API returns.

---

## Profiles and environments

| Profile | API base |
|---|---|
| `sandbox` (default) | `https://sandbox.api.flute.com` |
| `production`, `prod` | `https://api.flute.com` |

Resolved `--profile` → `FLUTE2_PROFILE` → `default_profile` in
`~/.flute2/config.toml` → `sandbox`.

```sh
flute2 auth switch production     # sets default_profile
flute2 --profile sandbox ping     # overrides it for one command
flute2 auth status                # which profile, and do the credentials work
```

`auth status` is a live check: it calls the API, so `authenticated` is true only
when the stored credentials actually round-trip.

Every command that resolves a profile prints a warning banner to **stderr**
when that profile is production. `completion` and `update` resolve none, so
they are the two commands that never print it.
Credentials are stored per profile, so a sandbox login and a production login
coexist.

---

## Command overview

| Group | What it does |
|---|---|
| `auth` | `login`, `status`, `switch`, `logout`, `token` — credential and profile management |
| `ping` | API health check |
| `version` | Print the CLI version and the active profile |
| `transactions` | `create`, `capture`, `reversal`, `credit`, `tip-adjust`, `ach-hold`, `ach-release`, `share-receipt`, `calculate-amount`, `get`, `list`, `inspect` — the card and ACH payment lifecycle |
| `customers` | `create`, `get`, `list`, `update`, `delete` — the customer record |
| `payment-methods` | `list`, `get`, `add-card`, `add-ach`, `update`, `delete`, `set-default` — the vault of stored cards and bank accounts |
| `payment-links` | `create`, `get`, `list`, `update`, `delete`, `share` — hosted links a payer opens and pays |
| `payment-sessions` | `create`, `get`, `cancel` — hosted checkout sessions |
| `terminals` | `list`, `status` — card-present terminal inventory and readiness |
| `pos` | `create` (with `--wait` long-poll), `get`, `list`, `cancel`, `print-receipt` — card-present transactions |
| `settlements` | `list`, `get`, `close` — settlement batch queries, and closing a processor's open batch |
| `settings` | `payment-config`, `contact-info`, `autofill`, `update-autofill` — account settings, including the processor ids |
| `api-keys` | `create`, `list`, `revoke` — merchant API key management |
| `completion` | Print a shell completion script |
| `update` | Self-update to the latest GitHub Release |

`flute2 <group> --help` lists a group's commands; `flute2 <group> <command>
--help` documents every flag, with its bounds and its reasons.

### Renamed from v1, with no aliases

A v1 command or flag name breaks loudly under `flute2` rather than charging
something else.

| v1 | v2 |
|---|---|
| `transactions sale`, `transactions auth` | `transactions create` (`--capture-method manual` for an authorization) |
| `transactions void`, `transactions refund` | `transactions reversal` |
| `transactions settle` | `settlements close` |
| `ach debit` | `transactions create` with the ACH flags |
| `ach credit` | `transactions credit` |
| `ach void`, `ach refund` | `transactions reversal` |
| `customers add-card`, `add-ach`, `methods`, `remove-method` | the `payment-methods` group |
| `keys` | `api-keys` |
| `devices`, `subscriptions` | **not in v2** — keep `flute` installed for these |
| `transactions list --unsettled` | **no equivalent** — `transactionStatus` filters by equality and the API declares no parameter for the negation |
| `--tip-rate`, `--l2-tax-rate` and the other rate flags as decimal fractions (`0.18` for 18%) | the same flags as percentages (`18.5` for 18.5%), with a value between 0 and 1 noted on stderr |
| `--page` (1-based) and `--limit` (default 25) | `--page-index` (0-based) and `--page-size` (the server's default when omitted) — the page numbering differs by one, so the names differ too rather than letting `--page 1` read a different page under each CLI |
| `--currency-id` (an integer) | `--currency-code` (an ISO 4217 code, `USD`) — the v2 schema declares the code |
| `--billing-country-id`, `--billing-state-id` | `--billing-country`, `--billing-state` — codes, not ids |
| `--faster` | `--same-day` — it names the ACH settlement window it asks for |
| `pos create --transaction-type`, `--target-transaction-id` | `pos create --capture-method` — the terminal command starts the flow, and `transactions capture` or `transactions reversal` finishes it against the returned transaction id |
| `keys revoke --merchant-id` | **dropped** — the client id alone identifies the key |
| `ach credit --reference-id` optional | `--reference-id` required — v2 declares it on the credit request |

### JSON fields renamed from v1

The envelope is unchanged — same `object` names, same `data`, same `meta` — so
`jq` parses a v2 response without complaint and a v1 path inside `data` returns
`null` instead of failing. A gate such as
`[ "$(… | jq -r '.data.status')" = "Approved" ]` takes the failure branch on a
transaction that was approved. The transaction paths that moved:

| v1 | v2 |
|---|---|
| `.data.status` | `.data.transactionStatus` |
| `.data[].id` | `.data[].transactionId` |
| `.data[].status`, `.data[].type`, `.data[].date` | `.data[].transactionStatus`, `.transactionType`, `.transactionDateTime` |
| `.data.amount.totalAmount`, `.data.totalAmount` | `.data.processedAmount`, `.data.amountBreakdown.*` |
| `.data.authCode` | `.data.processorDetails.authCode` |
| `.data.responseDescription`, `.data.responseCode` | `.data.declineDetails.message`, `.data.declineDetails.code` |

### Destructive commands need `--yes`

`customers delete`, `payment-methods delete`, `payment-links delete`,
`payment-sessions cancel`, `pos cancel` and `api-keys revoke` refuse without
it, and refuse **before** issuing a request. Repeating one that already
happened is success for the four deletes and revokes: a 404 exits 0, with
`found: false` in the confirmation because the server cannot say whether the
resource ever existed. The two cancels differ, because a cancelled resource
still exists: a second cancel is a 400 state error and exits 3, and a 404 is an
unknown id and exits 4.

### Pagination

`--page-index` is **0-based**, mirroring the API's own `pageIndex`, and
`--page-size` is bounded 1–100. Both are omitted when you do not pass them, so
the server's defaults govern.

```sh
flute2 transactions list --page-index 2 --page-size 50
flute2 transactions list --all            # every page, no page_info
```

In `table` mode, a page that is not the last ends with a line on stderr naming
the next `--page-index` and `--all`.

`--all` with `--page-index` is refused: a walk of everything and a starting page
contradict each other. `--page-size` stays legal with `--all` — it is a batch
size, not a position. A walk that repeats a page, or runs past 10000 of them,
ends as a `decode` error rather than continuing. A page is a JSON object whose
`items` is nullable and optional, so an absent one, a null one and an empty
array are all an empty page; a body that is not an object, or an `items` that is
neither an array nor null, is a `decode` error rather than a collection reported
as empty. `api-keys list` is
not paginated at all and rejects the flags.

### Amounts

Plain decimals: `--amount 10.50`. Two decimal places, validated locally, and
sent as an exact JSON number. **No amount passes through a float** at any point
between argv and the wire. `--exp` is `MM/YY` or `MM/YYYY`.

---

## Output and scripting

Three modes, resolved `--output` → `FLUTE2_OUTPUT` → the config file's `output`
→ `table`.

**`table`** — the default, for reading. Declared fields first, in the order
that matters when a payment goes wrong; nothing is hidden, so a field the API
adds still appears.

**`json`** — for a program:

```json
{ "object": "transaction",
  "data": { },
  "meta": { "environment": "sandbox", "correlation_id": "…", "page_info": { } } }
```

`data` is what the resource reports, with its object keys emitted in sorted
order at every depth. On a collection read it is the array itself. `page_info`
reproduces the API's `pageInfo` and is absent — not null — where there are no
pages.

**`quiet`** — the identifier alone, for chaining:

```sh
TXN=$(flute2 --output quiet transactions create --payment-processor-id <id> \
        --amount 10.50 --reference-id order-1005 --capture-method manual \
        --card 4111111111111111 --exp 12/2032 --cvv 123 \
        --billing-line1 "123 Test St" --billing-postal-code 10001 --billing-country US)
flute2 transactions capture --transaction-id "$TXN"
```

**stdout is data. stderr is everything else** — tracing, the production banner,
the update notice, the `--wait` warnings. That holds under `--debug` too, so
`flute2 --debug --output json …` still emits parseable JSON on stdout. Parse one
stream, never both.

### Exit codes

| Code | Meaning |
|---|---|
| `0` | success, **including a declined payment** and a 404 on a delete or revoke |
| `1` | general: transport, an unreadable response — including a `pos create --wait` response with no `posTransactionStatus` — a server 5xx, an expired `--wait-timeout` |
| `2` | authorisation: 401, 403, or no credentials |
| `3` | validation: a server 400 or 422, a client-side refusal, or a usage error |
| `4` | not found: a 404 that was not mapped to success |
| `130` | interrupted — Ctrl-C during a `pos create --wait` poll |
| `141` | the consumer of stdout stopped reading, so the output is a prefix — `128 + SIGPIPE`, what a `pipefail` pipeline expects when `jq -e` or `head` closes the pipe early |

Under `--output json` a failure is a JSON envelope on stdout —
`{"kind": …, "message": …, "status": …, "correlation_id": …}` — so a program
never has to guess at an empty stream. `kind` is one of `api`, `transport`,
`auth`, `decode` or `client`. [`agents.md`](agents.md) has the retry table and
the four exits that carry no envelope.

---

## Terminals and POS

A card-present sale runs on a terminal in **semi-integrated** mode that is
online:

```sh
flute2 terminals list
flute2 pos create --terminal-id <id> --pos-device-id <device> \
  --amount 10.50 --currency-code USD --wait
```

`--wait` holds the create until the terminal accepts, then polls until the
transaction leaves `InProgress`. `--wait-timeout` requires `--wait`, defaults
to 120 seconds and accepts 0 to 86400; it expires while a poll is open as well
as between polls, and on expiry the last-known status is printed and the exit
code is 1. Ctrl-C leaves stdout empty and exits 130.

A terminal takes one in-progress transaction at a time.

---

## Logging & debugging

`flute2` writes every log line to **stderr**; command output goes to stdout.
That split holds with `--debug` on, so `flute2 --debug --output json …` still
emits a clean, parseable JSON document on stdout.

By default only warnings and a few brief notices are logged — for example, the
one-line note when a stale token triggers the automatic single 401 retry.
Successful commands are otherwise quiet on stderr apart from the production
banner.

### `--debug`

The global `--debug` flag prints full HTTP request and response traces — method,
URL, status, and body — to stderr:

```sh
flute2 --debug ping
flute2 --debug transactions get <txn-id>
```

**Sensitive fields are masked before anything is logged:**

| Field | How it appears in logs |
|---|---|
| Card and bank account numbers (`cardNumber`, `accountNumber`, `routingNumber`, `pan`) | masked to the last 4 — e.g. `************1111` |
| Security codes (`securityCode`, `cvv`, `cvc`) | removed entirely — `***` |
| Client secrets, API keys and tokens (`clientSecret`, `authorization`, and any field name containing `secret`, `token` or `apikey`) | removed entirely — `***` |

The bearer token is sent as a header and is never part of a body trace. Masking
applies to every trace rather than only to `--debug`, and an error body is
redacted structurally before it is parsed.

> Masking lowers the risk but does not eliminate it: a `--debug` trace still
> reveals amounts, the last 4 digits, and request metadata. Don't capture
> `--debug` output into shared or long-lived logs when operating on
> `production`.

### `RUST_LOG` — fine-grained control

Setting `RUST_LOG` overrides the built-in filters entirely and accepts the
standard [`EnvFilter` syntax](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html).
The same field masking is applied to `flute2`'s own traces no matter how the
level is set:

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
| Confirm which environment and URL is being hit | `flute2 --debug ping` (the request URL is in the trace) |

Command errors are always printed to stderr — and to stdout as JSON under
`--output json` — regardless of the log level.

For a support conversation, `meta.correlation_id` from `--output json` is worth
more than a trace: it is what identifies the request server-side.

---

## Environment variables

| Variable | Effect |
|---|---|
| `FLUTE2_CLIENT_ID`, `FLUTE2_CLIENT_SECRET` | Credentials, checked before the keychain. Setting exactly one is an error. |
| `FLUTE2_PROFILE` | Active profile, below `--profile`. |
| `FLUTE2_OUTPUT` | Output mode, below `--output`. |
| `FLUTE2_NO_KEYCHAIN` | Any value makes the keychain unavailable, fail-closed. |
| `FLUTE2_NO_UPDATE_CHECK` | Any value suppresses the update notice; `CI` does too. |
| `FLUTE2_API_BASE_URL`, `FLUTE2_OAUTH_URL` | Point at another host. **Refused on production.** |
| `FLUTE2_GITHUB_TOKEN` | A token for `update`'s release lookup. |

No bare `FLUTE_` name is ever read: v1's variables, keychain entry and config
file are separate. One that is set while its `FLUTE2_` counterpart is not gets a
line on stderr at startup, naming the variable read instead.

---

## Shell completions

```sh
flute2 completion bash > /etc/bash_completion.d/flute2
flute2 completion zsh  > "${fpath[1]}/_flute2"
flute2 completion fish > ~/.config/fish/completions/flute2.fish
flute2 completion powershell | Out-String | Invoke-Expression
flute2 completion elvish > ~/.config/elvish/lib/flute2.elv
```

---

## Self-update

```sh
flute2 update
```

Replaces the binary in place from GitHub Releases. A build from source says so
rather than pretending. After any successful command, a newer release produces a
one-line notice on stderr. It is skipped on `update`, `completion` and every
`auth` command, under `--output json`, whenever stderr is not a terminal, and
when `FLUTE2_NO_UPDATE_CHECK` or `CI` is set or the config file sets
`auto_update_check = false`.

---

## For AI agents / MCP

See [`agents.md`](agents.md) for the machine-readable contract: the success and
error envelope shapes, the exit-code table, the idempotency table, the
pagination rules, and copy-pasteable commands for common agent intents.

---

## Development

```sh
cargo test                                     # hermetic and offline
cargo clippy --all-targets -- -D warnings
cargo fmt --check
python3 docs/reference/group-facts.py --check   # the generated API facts
```

All four pass at every commit. The suite never touches the network or the OS
keychain, and every binary-level test runs against a mock server through a
base-URL override that production refuses.

A release build:

```sh
cargo build --release                          # binary at target/release/flute2
```

The codebase enforces `#![forbid(unsafe_code)]` — there is zero `unsafe` Rust.

The live sandbox suite is committed, `#[ignore]`d, and opted into by hand:

```sh
source .flute2-live.env      # not committed; see .flute2-live.env.example
cargo test --test live -- --ignored --test-threads=1 \
    --skip attended --skip irreversible --skip needs_terminal
```

Read the module documentation at the top of `tests/live.rs` first. The
scenarios share one sandbox account, which is why the thread count is fixed and
why the three name suffixes carry the tiers.

Where the API's published reference and its behaviour disagree, the conformance
layer carries a narrow, named exemption rather than relaxing the check: each one
is scoped to the operations it covers, names the live scenario that is its only
oracle, and states the condition that deletes it.

---

MIT licensed. See [`LICENSE`](LICENSE).
