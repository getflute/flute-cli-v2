# agents.md — driving `flute2` from an autonomous caller

The machine-readable contract for a program calling this CLI: an agent, a
function-calling loop, an MCP server, a CI job. Humans should read
[`readme.md`](readme.md). This file describes the **v2** API surface.

## TL;DR

```bash
FLUTE2_CLIENT_ID=… FLUTE2_CLIENT_SECRET=… FLUTE2_PROFILE=sandbox \
  flute2 --output json transactions list --page-size 5
```

Never call `auth login` from an agent: it prompts for a secret on a terminal.
Set the two environment variables instead — they take precedence over the OS
keychain, so nothing has to be stored on the machine.

Three rules that shape every parser you write against this CLI:

1. **stdout is data; stderr is everything else.** Tracing, the production
   banner, the update notice and the `--wait` warnings are all stderr, under
   `--debug` too. Parse one stream, never both.
2. **A failure under `--output json` is a JSON envelope on stdout**, so a
   machine consumer never sees an empty stdout it has to guess about — with
   five named exceptions, below.
3. **A declined payment is a success.** Exit 0, HTTP 200, and the decline is in
   the body. Read `data.transactionStatus`, never the exit code, to find out
   whether money moved.

## Global flags

| Flag | Meaning |
|---|---|
| `--profile <name>` | `sandbox` (default), or `production`/`prod`. Env: `FLUTE2_PROFILE`; then `~/.flute2/config.toml`'s `default_profile`; then `sandbox`. |
| `--output <fmt>` | `json` (use this), `table` (human, the default), `quiet` (identifier only). Env: `FLUTE2_OUTPUT`; then the config file's `output`; then `table`. |
| `--debug` | HTTP request and response traces to **stderr**. Card numbers, bank account numbers, security codes, bearer tokens and client secrets are masked. Prefer `--output json` plus `meta.correlation_id`; reach for `--debug` only when an operator is investigating. |

There are no short flags beyond `-h` and `-V`. Spell it `--output quiet`.

The update notice is a single line on stderr, after a blank line, once the
command itself has finished. It is suppressed under `--output json`, whenever
stderr is not a terminal, when `FLUTE2_NO_UPDATE_CHECK` or `CI` is set, and on
`update`, `completion` and every `auth` command. JSON stdout stays pure and a
CI log stays quiet.

## Output contract

### Success

```json
{
  "object": "transaction",
  "data": { },
  "meta": {
    "environment": "sandbox",
    "correlation_id": "…",
    "page_info": { }
  }
}
```

- `object` names the resource. It is **stable**, and it is deliberately not
  always regular: `payment_method` and `payment_methods` sit beside
  `customer_list`, and the `api-keys` group's envelope is `api_token`. An
  envelope name does not move when a command is renamed, so anything branching
  on `object` keeps working.
- `data` is what the resource reports, and its object keys are emitted in
  **sorted order** at every depth rather than the order the API sent them.
  **On a collection read `data` is the array itself**, not a `{items, total}`
  wrapper: the count is `meta.page_info.totalItems`.
- `meta.correlation_id` is present when the API returned one. It is what
  support asks for. Observed present on a 4xx and absent on ordinary reads, so
  treat it as optional.
- `meta.page_info` reproduces the API's `pageInfo` field for field on a
  collection read, and is **absent** — not null — on a write, on a single-object
  read, and under `--all`. Under `--all` the data spans every page, so there is
  no one `pageInfo` to report.
- `--output quiet` prints the identifier alone, one per line for a collection.
  Ideal for chaining. Four commands have no identifier to print and print
  nothing at all: see
  [Commands with no identifier](#commands-with-no-identifier).

Envelope object names, by group:

| Group | Single | Collection |
|---|---|---|
| `customers` | `customer` | `customer_list` |
| `payment-methods` | `payment_method` | `payment_methods` |
| `transactions` | `transaction` | `transaction_list` |
| `transactions calculate-amount` | `amount_calculation` | — |
| `pos` | `pos_transaction` | `pos_transaction_list` |
| `terminals list` | `terminal` | `terminal_list` |
| `terminals status` | `terminal_status` | — |
| `settlements` | `settlement` | `settlement_list` |
| `settlements close` | `batch_closure` | — |
| `settings payment-config` | `payment_config` | — |
| `settings contact-info` | `contact_info` | — |
| `settings autofill` | `transaction_autofill` | — |
| `payment-links` | `payment_link` | `payment_link_list` |
| `payment-sessions` | `payment_session` | — |
| `api-keys` | `api_token` | `api_token_list` |
| `ping` | `ping` | — |
| `version` | `version` | — |
| `auth status` | `auth_status` | — |

### Failure

```json
{ "kind": "api", "message": "…", "status": 403, "correlation_id": "…" }
```

`status` and `correlation_id` appear only for `kind: "api"`. Branch on `kind`
first, then on `status`.

| `kind` | Meaning | Retry? |
|---|---|---|
| `api`, status 500/502/503/504 | server error, which can arrive after the API carried the request out | only a command in the **Safe to retry** column of [Idempotency](#idempotency), with backoff; any other — reconcile with `list` or `get` first |
| `api`, status 401/403 | authorisation | no — refresh credentials, then retry once |
| `api`, status 400/422 | permanent for this request | no — surface `message` and `correlation_id` |
| `api`, status 404 | not found | no |
| `api`, status 402/409/429 | no exit code of their own — see the note under the exit-code table | 429 yes, with backoff; others no |
| `transport` | connection, DNS or TLS failure, or a **request timeout** — on the API request, the token request or `update`'s release lookup, with the cause named in `message`. A timeout can land after the API received the request and carried it out | only a command in the **Safe to retry** column of [Idempotency](#idempotency), with backoff; any other — reconcile with `list` or `get` first |
| `auth` | no credentials, one of `FLUTE2_CLIENT_ID` and `FLUTE2_CLIENT_SECRET` set without the other, or the token endpoint refused the ones it was sent | no — an operator must configure credentials |
| `decode` | the API sent something this CLI cannot read: a contract change, or a CLI bug | no — surface it for investigation |
| `client` | bad arguments, usage error, or client-side validation | no — fix the invocation |

A **usage error** is a `client` envelope on stdout under `--output json`, and
clap's own styled text on stderr otherwise — including a group invoked with no
subcommand, whose envelope names the `--help` that lists the choices. The JSON
shape follows `FLUTE2_OUTPUT` and the config file's `output` as well as the
flag, with one exception: an **unparseable `--output` value** is itself the
usage error, so there is no mode to render it in and stdout stays empty. `--help`
and `--version` are not failures: they print to stdout and exit 0.

### Exit codes

| Code | Meaning |
|---|---|
| `0` | success — **including a declined payment**, and including a 404 on a delete or revoke |
| `1` | general: transport, a body that could not be decoded — including a `pos create --wait` response carrying no `posTransactionStatus` — a server 5xx, and an expired `--wait-timeout` |
| `2` | authorisation: 401, 403, or no credentials at all |
| `3` | validation: server 400 or 422, client-side refusals, and CLI usage errors |
| `4` | not found: a 404 that was not mapped to success |
| `130` | interrupted — Ctrl-C during a `pos create` request or its `--wait` poll |
| `141` | the consumer of stdout stopped reading before the output was complete |

`130` and `141` are in the table rather than in prose alone, so an agent that
parses the table reads a precise, actionable outcome instead of an unknown one.

`141` is `128 + SIGPIPE`, the code a shell reports for a process a `SIGPIPE`
killed, so a `pipefail` pipeline reads the same value here as from every other
tool in it. It means the bytes the consumer did read are a **prefix**, not a
document: `… | jq -e '.data[0]'` and `… | head` close stdin as soon as they
have what they want, and everything after that point is dropped. Nothing is
written to stderr, and no error envelope is attempted — the stream it would go
to is the one that closed.

**A consumer that stops reading *stderr* is not that.** stderr carries no data,
so the command runs to its own end and exits with the code it earned; the
diagnostics written after the close are dropped. Read the exit code, not the
presence of a message.

**402, 409 and 429 exit 1**, through the general arm. They have no dedicated
exit code, and one may be assigned later: nothing can depend on a code the CLI
has never emitted, so adding one stays backwards compatible.

### A declined payment is exit 0

The API answers a decline with **HTTP 200**. The transaction exists, it has an
id, and it was refused:

```json
{ "object": "transaction",
  "data": { "transactionId": "…", "transactionStatus": "Declined",
            "declineDetails": { "message": "…" } },
  "meta": { "environment": "sandbox" } }
```

So the exit code cannot tell you whether money moved. **Read
`data.transactionStatus`.** An agent that treats exit 0 as "paid" will report a
declined charge as a completed one.

### A 404 on delete or revoke is exit 0, and says nothing was removed

Deleting something already deleted is the outcome the caller asked for, so the
four delete and revoke verbs map a 404 to exit 0: `customers delete`,
`payment-methods delete`, `payment-links delete` and `api-keys revoke`. A
retry after a timed-out delete is therefore safe.

The server answers a 404 alike for "already deleted" and "never existed", so
the confirmation does not claim a removal:

```json
{ "object": "customer",
  "data": { "customerId": "…", "deleted": false, "found": false },
  "meta": { "environment": "sandbox" } }
```

`revoke` reports `revoked: false, found: false`. The `table` line says nothing
was deleted; `quiet` prints the identifier, as it does on success. A caller
that must know the resource existed reads `found`.

**The cancels are not in this rule.** A cancelled `pos` transaction or payment
session still exists, and a second cancel answers 400, exit 3. The only 404 a
cancel meets is an id the server never had, which is exit 4 like any other
not-found. Reconcile with `get` before re-issuing a cancel.

Every other 404 is exit 4, on a write as much as on a read: a `capture`, an
`update` or a `share` against an id the server does not have exits 4, not 0.

### Writes that answer with a confirmation

Twelve commands answer every success with a confirmation built from the
identifier in the *request* rather than from the response, so no caller has to
read exit 0 plus an empty stdout as success:

```json
{ "object": "customer",
  "data": { "customerId": "…", "deleted": true },
  "meta": { "environment": "sandbox" } }
```

In `table` mode this is one confirmation line; in `quiet` mode, the identifier.
The confirmation replaces whatever the response carried, so a field the API
returned is not readable from the write — issue a `get` to read one back.

| Command | `data` |
|---|---|
| `customers update` | `customerId`, `updated: true` |
| `customers delete` | `customerId`, `deleted: true` |
| `payment-methods update` | `paymentMethodId`, `updated: true` |
| `payment-methods delete` | `paymentMethodId`, `deleted: true` |
| `payment-methods set-default` | `paymentMethodId`, `default: true` |
| `payment-links delete` | `paymentLinkId`, `deleted: true` |
| `payment-links share` | `paymentLinkId`, `shared: true` |
| `payment-sessions cancel` | `id`, `cancelled: true` — spelled `id`, not `paymentSessionId` |
| `pos print-receipt` | `posTransactionId`, `sent: true` — the API accepted the request; it does not report whether paper came out |
| `transactions share-receipt` | `transactionId`, `shared: true` |
| `settings update-autofill` | `updated: true`, and no identifier |
| `api-keys revoke` | `clientId`, `revoked: true` |

Every other operation declares a body, and a success carrying none is
`kind: "decode"`, exit 1 — never an envelope with `"data": null`. `pos cancel`
is one of those, which is why its success reports the transaction instead.

### Commands with no identifier

Four responses carry no identifier the CLI can print, and this is the schema's
shape rather than a choice:

- **`payment-sessions get`** — the read response declares no `id`, of any
  spelling, so a read cannot be correlated back to the session that was read.
  `create` does declare one. `quiet` prints nothing.
- **`settings autofill`** — the read of a singleton. The response carries the
  level 2 and level 3 rates and a product template, and nothing that names the
  resource, so `quiet` prints nothing.
- **`settings update-autofill`** — a bodyless write on a singleton. There is no
  identifier in the request either, so the envelope carries the verb alone,
  `quiet` prints nothing, and the table line names the resource.
- **`transactions calculate-amount`** — totals calculated from an amount the
  caller supplied. Nothing is created, so the response names no resource and
  `quiet` prints nothing. The currency code it echoes is not an identifier:
  nothing can be read back with it.

That is nothing at all — no line, no newline. Do not read the empty stdout as a
failure; check the exit code.

### Commands that print no envelope

These succeed with plain text or a script, in every output mode. Do not feed
their success output to a JSON parser. **Their failures are still JSON
envelopes**, so parse the failure path for every command.

| Command | stdout on success |
|---|---|
| `completion <shell>` | the raw completion script |
| `update` | plain text |
| `auth login` | interactive prompts — never call this from an agent |
| `auth logout` | plain text |
| `auth switch <profile>` | plain text |
| `auth token` | the bearer token, one line, no envelope |

`auth status` is the one `auth` verb that does emit an envelope.

**`auth switch` writes to the machine.** It stores `default_profile` in
`~/.flute2/config.toml`, creating the file if it is absent and writing `output`
and `auto_update_check` alongside it, and that default governs every later
process that passes no `--profile` — a switch to `production` included. An
agent should pass `--profile` on each command and leave the file alone.

### The non-zero exits that carry no error envelope

Three are `pos create --wait`, and they are why you must not infer the shape
of stdout from the exit code.

- **`--wait-timeout` expired** — the last-known **success** envelope is on
  stdout (`object: "pos_transaction"`, *not* an error envelope), a warning is on
  stderr, and the exit code is **1**. In `table` or `quiet` mode the table or
  the bare id is printed instead.
- **A poll failed after the create succeeded** — the transaction exists, so
  the last-known **success** envelope is on stdout with its `posTransactionId`
  to reconcile or cancel by. The failure is on stderr, and the exit code is the
  one that failure maps to in the table above: **1** for a `transport` failure,
  a 5xx or an unreadable poll response.
- **Ctrl-C** — the last-known status goes to stderr, **stdout stays empty in
  every output mode**, and the exit code is **130**.

The fourth is an **unparseable `--output` value**: the mode that would render
the envelope is the thing being rejected, so clap's text goes to stderr, stdout
stays empty, and the exit code is **3** — whatever `FLUTE2_OUTPUT` or the
config file says.

The fifth is a **closed stdout**, exit **141**: both streams are left as they
are, so stdout holds however much of the output the consumer read before it
stopped and stderr holds nothing.

If stdout is empty, go by the exit code. If it parses, check for a `kind` field
before treating it as an error.

## Environments

| Profile | API base |
|---|---|
| `sandbox` (default) | `https://sandbox.api.flute.com` |
| `production`, `prod` | `https://api.flute.com` |

A production command prints a warning banner to **stderr**, never stdout —
every command that resolves a profile, which is all of them except
`completion` and `update`. `auth switch <profile>` banners the profile it is
switching to, not the one that was active. A command refused before dispatch —
a destructive one without `--yes` — prints no banner at all, because the
refusal runs first. The base-URL overrides below are refused on production.

## Environment variables

| Variable | Effect |
|---|---|
| `FLUTE2_CLIENT_ID`, `FLUTE2_CLIENT_SECRET` | Credentials. Checked **before** the keychain, which is how an agent authenticates without storing anything. Setting exactly one is an error naming the missing variable — half-configured is not a request to use the keychain. |
| `FLUTE2_PROFILE` | The active profile, below `--profile` and above the config file. |
| `FLUTE2_OUTPUT` | The output mode, below `--output` and above the config file. |
| `FLUTE2_NO_KEYCHAIN` | Any value makes the OS keychain unavailable, fail-closed. For a CI runner, or any process that must not raise an authorisation dialog. |
| `FLUTE2_NO_UPDATE_CHECK` | Any value suppresses the update notice. `CI` does the same. |
| `FLUTE2_API_BASE_URL`, `FLUTE2_OAUTH_URL` | Point the CLI at another host. **Refused on the production profile**, so a test override cannot silently redirect a real charge. |
| `FLUTE2_GITHUB_TOKEN` | A token for `update`'s release lookup, for a rate-limited network. |
| `FLUTE2_INSTALLER_GITHUB_BASE_URL`, `FLUTE2_INSTALLER_GHE_BASE_URL` | Point `update`'s release lookup at another GitHub host, or at a GitHub Enterprise server. Setting both is an error. |

Nothing reads a bare `FLUTE_` name: v1's variables, keychain entry and config
file are all separate, and the two binaries call different versions of the API
on the same hosts. A `FLUTE_` name that is set while its `FLUTE2_` counterpart is not gets
one **stderr** line at startup naming the variable read instead, because an
inert variable is otherwise indistinguishable from a configured one.

## Commands

Each group opens with the usage line for every one of its verbs, exactly as
`--help` prints it: the positional arguments and the flags clap requires, with
`[OPTIONS]` standing for the verb's own optional flags and the global flags
listed above. The shapes are not regular — `transactions get` takes a
positional id and `transactions capture` takes `--transaction-id` — so read the
line rather than generalising from a sibling.

All amounts are plain decimals — `--amount 10.50` — validated to two decimal
places and sent as exact JSON numbers. **No amount passes through a float.**
A negative amount or rate is refused, and the refusal names the flag and the
rule it broke, as every other malformed value does. `--exp` is `MM/YY` or
`MM/YYYY`.

`--payment-processor-id` is required on `transactions create`, `transactions
credit` and `settlements close`. `capture`, `reversal`, `tip-adjust` and the
ACH actions address a transaction that already carries one and take none.
`flute2 settings payment-config` lists the account's processors, and there is
one per payment method: sending an ACH debit to a card processor fails in a way
that looks nothing like the cause.

**An identifier that goes into a request path is checked before the request is
built.** One that is empty after trimming, one that is exactly `.` or `..`, and
one that carries `/`, `\`, `?`, `#`, `%` or whitespace are each a `client`
envelope and exit 3 with nothing sent — those characters end a path segment, a
dot segment is resolved away, and `%` spells any of them as an escape the URL
parser decodes, so such an identifier would address an operation other than the
one named. An identifier sent in a query parameter or a body is held to the
empty rule alone — `settlements get`'s batch id, `settlements close
--payment-processor-id`, `pos print-receipt --terminal-id` and `payment-methods
set-default --customer-id` — so an empty filter cannot return the unfiltered list.

An identifier that passes those checks but is not the shape the server expects
— `not-a-real-id` where a UUID goes — is a **400, exit 3**, not a 404: the
server validates the shape before it looks anything up. Only a well-formed id
the server does not have is exit 4.

### `transactions`

```
flute2 transactions create [OPTIONS] --amount <AMOUNT> --payment-processor-id <PAYMENT_PROCESSOR_ID>
flute2 transactions get [OPTIONS] <TRANSACTION_ID>
flute2 transactions list [OPTIONS]
flute2 transactions inspect [OPTIONS] <TRANSACTION_ID>
flute2 transactions capture [OPTIONS] --transaction-id <TRANSACTION_ID>
flute2 transactions reversal [OPTIONS] --transaction-id <TRANSACTION_ID>
flute2 transactions tip-adjust [OPTIONS] --transaction-id <TRANSACTION_ID>
flute2 transactions ach-hold [OPTIONS] <TRANSACTION_ID>
flute2 transactions ach-release [OPTIONS] <TRANSACTION_ID>
flute2 transactions share-receipt [OPTIONS] --share-by <SHARE_BY> --recipient <RECIPIENT> <TRANSACTION_ID>
flute2 transactions calculate-amount [OPTIONS] --amount <AMOUNT>
flute2 transactions credit [OPTIONS] --amount <AMOUNT> --payment-processor-id <PAYMENT_PROCESSOR_ID> --reference-id <REFERENCE_ID>
```

- **`create` takes exactly one payment instrument**, and a missing or
  ambiguous one is refused before any request: a new card (`--card`, `--exp`,
  `--cvv`), a saved card (`--payment-method-id` with `--instrument card`), a new
  bank account (`--ach-account-number`, `--ach-routing-number`,
  `--ach-account-type`, `--ach-account-holder-type`), or a saved one
  (`--payment-method-id` with `--instrument ach`). `--instrument` is required
  alongside a stored id, because nothing in the id says which container it
  belongs in.
- **`--capture-method manual` creates an authorization** to capture later;
  `auto`, the default, charges immediately. It applies to a card only: on an
  ACH instrument, new or saved, `manual` is refused before the wire.
- **A new bank account needs more than the schema says.** `--sec-code`
  always, plus a billing address, `--contact-email`, `--contact-phone`, and
  either a name pair or `--contact-company`. Refused before the wire. The
  requirement *moves* rather than disappearing for a saved instrument:
  attached to a customer, the API reads the address and phone off the customer
  record, and a customer missing them is rejected server-side with a message
  naming the field. The CLI cannot see that half. `--requester-ip` is required
  on the wire too, and defaults to `127.0.0.1`.
- **`list` sends the direction you ask for.** `--asc` and `--desc` each send
  `sortOrder`; with neither, results come back newest first. `--sort-by`
  takes only the field names `--help` lists.
- **`reversal` is the one endpoint for a void and for a refund.** The payment
  method and the settled state are detected server-side. Without `--amount` it
  is full. The API honours `--amount` only on a **settled card** transaction:
  it voids an unsettled card transaction in full and reverses an ACH one in
  full whatever amount is sent. So with `--amount` the CLI first reads the
  transaction and refuses (exit 3, no reversal sent) unless it is a card
  transaction in `Settled` or `Refunded`. A partial amount on `capture` or `reversal` must be
  **greater than zero**: the whole of it is asked for by omitting the flag,
  not by naming nothing. `tip-adjust`'s `--tip-amount` and `--tip-rate` carry
  the same floor — zero moves no tip, so either is refused before the wire.
- **A tip or a discount is an amount or a rate, not both.** `transactions
  create` refuses `--tip-amount` with `--tip-rate`, and `--discount-amount`
  with `--discount-rate`; `pos create` and `tip-adjust` refuse the tip pair.
  The API rejects a charge that carries both.
- **The ACH actions answer with a `referenceId` the API assigned.** `ach-hold`,
  `ach-release` and a `reversal` of an ACH transaction each replace the
  merchant reference the transaction was created with, so
  `transactions list --reference-id <the value sent>` returns nothing once one
  of them has run. Each of the three writes one **stderr** line saying the
  reference in the answer is the API's; reconcile by transaction id. A card
  `reversal` keeps its reference and writes no line.
- **`capture` sends `captureAmount`**, not `amount` — the operation's own
  example is wrong about its own schema, and an unknown field is rejected
  rather than ignored.
- **`inspect` curates the `table` view, and only that.** It reads the same
  endpoint as `get` and lays out the fields that matter when a payment goes
  wrong, decline reason first, while `get`'s table hides nothing — including
  fields this CLI does not know about. Under `--output json` the two are
  byte-identical, so an agent gains nothing from `inspect` and spends a second
  round trip on it. Neither has an endpoint of its own beyond the read.
- `list` filters are all **server-side**. There is no client-side filtering
  anywhere in this CLI: a filter applied locally reports a wrong answer on any
  collection larger than one page.
- **Rate flags are percentages.** `--tip-rate`, `--discount-rate`,
  `--surcharge-rate` and `--l2-tax-rate` take `18.5` for 18.5%. A fraction is
  a valid rate, so `0.18` is accepted and means 0.18%, not 18% — and a value
  between 0 and 1 is noted on **stderr**, one line per flag, naming both
  readings. The note changes neither the request, the envelope, nor the exit
  code. `settings update-autofill`'s `--l2-tax-rate`, `--l3-shipping-rate`,
  `--l3-duty-rate` and `--product-discount` carry the same rule.
- **`--l3-invoice` accepts only letters, digits and spaces.** Undocumented, not
  enforced locally, and the API's own message is clear — a hyphen, as in the
  schema's own example, is rejected.
- **"Not settled" is not expressible server-side.** `transactionStatus` is an
  equality filter and the API declares no parameter for the negation, so there
  is no filter for "every transaction that has not settled".

### `customers`

```
flute2 customers create [OPTIONS] --first-name <FIRST_NAME> --last-name <LAST_NAME>
flute2 customers get [OPTIONS] <CUSTOMER_ID>
flute2 customers list [OPTIONS]
flute2 customers update [OPTIONS] <CUSTOMER_ID>
flute2 customers delete [OPTIONS] <CUSTOMER_ID>
```

- `update` is a **partial** update: omitted flags are left alone, and an empty
  value or `--clear <field>` removes one — see
  [Clearing a field](#clearing-a-field). An entirely empty update is refused
  before the wire.
- The two booleans **take a value on `update`** — `--sms-consent false` — and
  are bare switches on `create`. A patch has to be able to clear a flag.
- `delete` requires `--yes`.

### `payment-methods`

```
flute2 payment-methods list [OPTIONS]
flute2 payment-methods get [OPTIONS] <PAYMENT_METHOD_ID>
flute2 payment-methods add-card [OPTIONS] --card <CARD> --exp <EXP>
flute2 payment-methods add-ach [OPTIONS] --account <ACCOUNT_NUMBER> --routing <ROUTING_NUMBER> --account-holder-type <ACCOUNT_HOLDER_TYPE>
flute2 payment-methods update [OPTIONS] <PAYMENT_METHOD_ID>
flute2 payment-methods delete [OPTIONS] <PAYMENT_METHOD_ID>
flute2 payment-methods set-default [OPTIONS] --customer-id <CUSTOMER_ID> <PAYMENT_METHOD_ID>
```

- `update` changes the label and nothing else — that is all the endpoint takes.
  `--name ""` or `--clear name` removes it; see
  [Clearing a field](#clearing-a-field).
- `delete` requires `--yes`.

### `payment-links`

```
flute2 payment-links create [OPTIONS]
flute2 payment-links get [OPTIONS] <PAYMENT_LINK_ID>
flute2 payment-links list [OPTIONS]
flute2 payment-links update [OPTIONS] <PAYMENT_LINK_ID>
flute2 payment-links delete [OPTIONS] <PAYMENT_LINK_ID>
flute2 payment-links share [OPTIONS] --share-by <SHARE_BY> --recipient <RECIPIENT> <PAYMENT_LINK_ID>
```

- **`--currency-code` is required on `create`**, though the schema marks it
  optional. The API rejects a create without it, amount or not.
- **Omit `--amount` for a link the payer fills in.** `--amount 0` is refused,
  because a zero-amount link and a flexible one are different things.
- `update` is an RFC 7396 merge patch: an empty value or `--clear <field>`
  removes a value — see [Clearing a field](#clearing-a-field). Clearing
  `--amount` makes a fixed-price link one the payer fills in.
- `share` sends a real message. `--consent` is required by the API.
- **`list` sends the direction you ask for.** `--asc` and `--desc` each send
  `sortOrder`; with neither, results come back newest first. `terminals list`
  and `pos list` take the same pair.
- `delete` requires `--yes`, and it and `share` both answer **204**.

### `payment-sessions`

```
flute2 payment-sessions create [OPTIONS]
flute2 payment-sessions get [OPTIONS] <PAYMENT_SESSION_ID>
flute2 payment-sessions cancel [OPTIONS] <PAYMENT_SESSION_ID>
```

- **The amount carries three rules, and zero and absent mean opposite things.**
  Greater than zero for a `payment` or `payment-and-save` session; **exactly
  zero** for `--mode save-method`, which the CLI sends for you; **absent** for a
  flexible session the payer sets at checkout. All three are enforced before the
  wire, because OpenAPI can express none of them.
- `--metadata key=value` is repeatable and splits on the **first** `=`, so a URL
  or a query string survives as a value.
- `cancel` requires `--yes`. A second cancel is a 400, exit 3, and a 404 is exit 4.

### `pos`

```
flute2 pos create [OPTIONS] --terminal-id <TERMINAL_ID> --pos-device-id <POS_DEVICE_ID> --amount <AMOUNT> --currency-code <CURRENCY_CODE>
flute2 pos get [OPTIONS] <POS_TRANSACTION_ID>
flute2 pos list [OPTIONS]
flute2 pos cancel [OPTIONS] <POS_TRANSACTION_ID>
flute2 pos print-receipt [OPTIONS] --terminal-id <TERMINAL_ID> <POS_TRANSACTION_ID>
```

- **`--wait` drives two different API controls** — create-time acceptance by the
  terminal, and a get-time long poll — and both are needed. With only the first
  the CLI busy-polls; with only the second, create returns before the terminal
  has acknowledged anything. The poll ends on any `posTransactionStatus` other
  than `InProgress`. `--wait-timeout` defaults to 120 seconds and accepts 0 to
  86400; anything outside that range is a usage error, exit 3, and it requires
  `--wait`. One budget bounds the create and the poll together: it starts
  before the create, the create request is bounded by the budget plus a short
  margin, and the poll gets whatever the create left. It bounds the poll
  itself, not just the gaps between polls: it expires while the API is holding
  a response open, and `0` therefore waits no time at all. A create that
  outlasts it is a `transport` failure after which the transaction may exist,
  so reconcile with `pos list` before creating another.
- **`pos get --wait` is bounded by the default wait budget**, 120 seconds plus
  the same short margin, rather than by the client-wide 30-second request
  timeout. It takes no `--wait-timeout`. A long poll that outlasts the bound
  is a `transport` failure, exit 1.
- **Ctrl-C during the create request** exits 130 with stdout empty in every
  output mode and one stderr line: the transaction may exist on the terminal,
  so reconcile with `pos list` before creating another.
- `--wait` with `--initiation-channel deeplink` is refused before the wire: the
  API requires terminal acceptance to be false for that channel.
- A create response with no `posTransactionStatus` is a `decode` error, exit 1,
  rather than a state to wait out; a poll response without one ends the wait as
  a failed poll. Without the field the poll has no stop condition, so
  waiting on one could only end in the timeout.
- A terminal takes **one** in-progress transaction. Cancel or complete before
  starting another.
- `cancel` requires `--yes`.

### `terminals`

```
flute2 terminals list [OPTIONS]
flute2 terminals status [OPTIONS] <TERMINAL_ID>
```

`--status` filters on `ready`, `busy` or `offline`, which is what the query
parameter declares. The response reports a *different* vocabulary for the same
field, and the two cannot both be right. Do not assume a value you read back is
a value you can filter by.

### `settlements`

```
flute2 settlements list [OPTIONS]
flute2 settlements get [OPTIONS] <BATCH_ID>
flute2 settlements close [OPTIONS] --payment-processor-id <PAYMENT_PROCESSOR_ID>
```

- **`get` has no endpoint of its own**: it is the list endpoint with the
  documented `batchIds` filter applied **server-side**. An empty result is exit
  4, and two matches for one id is a refusal rather than a guess.
- **`--asc`, not `--desc`** — this list's `sortOrder` defaults to `desc`.
- `close` settles the named processor's whole open batch, not one
  transaction.

### `settings`

```
flute2 settings payment-config [OPTIONS]
flute2 settings contact-info [OPTIONS]
flute2 settings autofill [OPTIONS]
flute2 settings update-autofill [OPTIONS]
```

Every settings resource is a singleton: no pagination, no filters, no
identifiers. `payment-config` is where processor ids come from. An empty
`update-autofill` is refused before the wire.

`update-autofill` is a merge patch: clearing a stored default stops it applying
to later transactions, by either spelling — `--product-code ""` or
`--clear product-code`. The rates and prices clear the same way; only
`--l2-tax-rate` cannot — see [Clearing a field](#clearing-a-field).

### `api-keys`

```
flute2 api-keys create [OPTIONS] --merchant-id <MERCHANT_ID> --name <API_KEY_NAME>
flute2 api-keys list [OPTIONS]
flute2 api-keys revoke [OPTIONS] --client-id <CLIENT_ID>
```

- `create` returns the client secret **once**. Capture it from the envelope; the
  API never returns it again. It is the only response body in the API carrying a
  live credential, and it is not in the `--debug` trace. Under `quiet` output,
  from the flag, `FLUTE2_OUTPUT` or the config file, `create` is refused before
  any request, exit 3, because the identifier alone would discard the secret.
- **`list` is not paginated** — the response has no `pageInfo` — so it takes
  neither `--page-index`/`--page-size` nor `--all`, and its envelope carries no
  `page_info`. Its `apiKeys` may be absent or null — an empty collection — and
  any other non-array value is `kind: "decode"`, exit 1.
- `revoke` requires `--yes` and takes the client id alone — the endpoint
  accepts no merchant id.
- This group sits behind a server feature flag. **A 404 from `create` or `list`
  carries a message saying the feature may be off for the account** rather than
  reading as a wrong URL, while a 404 from `revoke` is the idempotent-revoke
  rule. A **403** keeps its own message and exit 2 — an authorisation failure is
  not a disabled feature.
- **The group needs partner credentials.** A merchant's own client id and
  secret answer 403 on every verb, with a message saying a merchant token is in
  use and a partner token is needed. A merchant key cannot manage keys.

### Utility

```
flute2 ping [OPTIONS]
flute2 version [OPTIONS]
flute2 update [OPTIONS]
flute2 completion [OPTIONS] <SHELL>
flute2 auth login [OPTIONS]
flute2 auth status [OPTIONS]
flute2 auth switch [OPTIONS] <PROFILE>
flute2 auth logout [OPTIONS]
flute2 auth token [OPTIONS]
```

`auth status` is a **live** check: it calls the API, so `authenticated` is true
only when the stored credentials actually round-trip. It is false when there
are no credentials or the server refuses them; a check that gets no answer
about them — the server is unreachable or fails — exits non-zero with the
usual failure envelope instead.

## Pagination

`--page-index` is **0-based**, mirroring the API's own `pageIndex`, and
`--page-size` is bounded 1–100. Both are **omitted entirely when absent**, so
the server's defaults govern; an out-of-range page size is refused locally,
spending no round trip.

`--all` follows `pageInfo.hasMore` until the collection is exhausted. It stops
when `hasMore` is absent or false, or when a page returns no items — it never
infers a further page from `totalPages` alone. A walk that cannot end is a
`decode` error, exit 1: a page repeating the one before it means the server
ignored `pageIndex`, and 10000 pages without exhaustion means the end is not
coming. It reports no `page_info`, and **`--all` with `--page-index` is
refused**: a walk of everything and a starting page contradict each other.

`--page-size` stays legal with `--all`. It is a batch size, not a position.

A page is a JSON object. Its `items` is declared nullable and optional, so an
absent one, a null one and an empty array are all an empty page. A body that is
not an object, or an `items` that is present and is neither an array nor null,
is `kind: "decode"`, exit 1, rather than a collection reported as empty.
`hasMore` is declared a boolean and is not nullable, so an absent one stops the
walk while a present value of any other type is `kind: "decode"`, exit 1.

## Clearing a field

The four `update` commands — `customers`, `payment-methods`, `payment-links`,
`settings update-autofill` — are RFC 7396 merge patches, where three states
exist and a flag has to reach all of them:

| You want | Spelling | On the wire |
|---|---|---|
| Leave it as it is | omit the flag | the key is absent |
| Set it | `--email ada@example.com` | `"email": "ada@example.com"` |
| Remove it | `--email ""` or `--clear email` | `"email": null` |

**Both removal spellings are one request.** `--email ""` and `--clear email`
produce the same body; use whichever reads better. `--clear` repeats for
several fields at once, and it is the only spelling that reads well in a script
where the value comes from a variable.

**An empty value is the removal, not a blank.** `--company ""` stores no company
rather than an empty one, and the two are different states downstream.

**Setting and clearing one field in one invocation is refused** — exit 3,
nothing sent.

**Not every field can be cleared.** The ones backed by a required column are
offered by neither spelling. `--clear` does not list them, and an empty value is
refused before the request:

```
$ flute2 customers update <id> --first-name ""
Error: --first-name cannot be cleared; pass a value.
```

| Command | Clearable | Refused |
|---|---|---|
| `customers update` | `company`, `email`, `mobile` | `first-name`, `last-name` |
| `payment-methods update` | `name` | — |
| `payment-links update` | `amount`, `customer-id`, `reference-id`, `description`, `expires-on` | `name`, `currency-code`, `card-processor-id`, `ach-processor-id` |
| `settings update-autofill` | the six product defaults, `l3-shipping-rate`, `l3-duty-rate` | `l2-tax-rate` |

**Numeric flags clear by both spellings too.** `--amount ""` and
`--clear amount` each send the null. Clearing an amount is not the same as
setting it to `0`: a zero rate is a configured value that still autofills, and
`0` is refused outright where the schema requires a positive number.

**`create` is unaffected.** It has nothing to remove, so an empty value there is
an absent key, never a null, and `create` takes no `--clear`. A processor id is
the exception: an empty one on any create is refused — exit 3, nothing sent —
because the request would otherwise run through the account's default
processor.

**`shouldUseBillingAsShippingAddress` and `hasSmsConsent` take a value on
`update`** — `--sms-consent false` — because a bare switch cannot express
`false`. They are not clearable.

**Address components are not individually clearable.** `--billing-*` flags build
one nested object shared with `create`; an empty component is omitted from it.

## Idempotency

| Safe to retry | Not safe to retry |
|---|---|
| every `get`, `list`, `status` and `inspect` | `transactions create`, `credit`, `capture`, `reversal`, `tip-adjust`; `pos create`; `customers create`; `payment-methods add-card`, `add-ach`; `payment-links create`; `payment-sessions create`; `api-keys create` |
| `customers delete`, `payment-methods delete`, `payment-links delete`, `api-keys revoke` — a repeat 404 is exit 0 with `found: false` | `pos cancel`, `payment-sessions cancel` — a repeat answers 400, exit 3; reconcile with `get` |

A `transport` failure or a 5xx on something not safe to retry is ambiguous:
the request may have been carried out. **`list` or `get` to reconcile before
re-issuing.**

**Use a unique `--reference-id`.** It is part of the API's duplicate-check key:
the same card and amount can be charged again when the reference ids differ,
and cannot when they do not. The window is not documented. Five of the
unsafe-to-retry commands take one — `transactions create`, `transactions
credit`, `pos create`, `payment-links create` and `payment-sessions create`.

The other seven have no de-duplication knob at all. `capture` and `reversal`
take an `--amount`, but it is the operation's meaning rather than a key to vary;
`transactions tip-adjust`, `customers create`, `payment-methods add-card`,
`payment-methods add-ach` and `api-keys create` take neither. **Reconcile with
`list` or `get` before re-issuing any of the seven.**

**Reconcile an ACH hold, release or reversal by transaction id.** Those three
answer with a `referenceId` the API assigned, so a `list --reference-id` by the
value the transaction was created with finds nothing and reads as a charge that
never happened. `transactions get <id>` is the lookup that survives them.

## Common intents

| Intent | Command |
|---|---|
| Health check | `flute2 --output json ping` |
| Find the processor ids | `flute2 --output json settings payment-config` |
| Charge a card | `flute2 --output json transactions create --payment-processor-id <id> --amount 10.50 --reference-id <unique> --card <pan> --exp MM/YY --cvv <cvv> --billing-line1 <street> --billing-postal-code <zip> --billing-country <cc>` |
| Authorize, then capture | the same with `--capture-method manual` — a card only — then `flute2 --output json transactions capture --transaction-id <id>` |
| Refund or void | `flute2 --output json transactions reversal --transaction-id <id> [--amount …]` |
| Read one transaction | `flute2 --output json transactions get <id>` |
| Walk every transaction | `flute2 --output json transactions list --all` |
| Vault a card for a customer | `flute2 … customers create --first-name … --last-name … --billing-line1 … --billing-postal-code … --billing-country …`, then `payment-methods add-card --customer-id <id> --card … --exp …` |
| Charge a vaulted card | `flute2 … transactions create --payment-method-id <id> --instrument card --customer-id <id> --payment-processor-id <id> --amount … --reference-id <unique>` |
| Start a terminal sale | `flute2 --output json pos create --terminal-id <id> --pos-device-id <dev> --amount 10.50 --currency-code USD --wait` |
| Take a payment on a hosted page | `flute2 --output json payment-sessions create --amount 10.50 --card-enabled` |
| Issue a merchant API key | `flute2 --output json api-keys create --merchant-id <id> --name "<name>"` — save the one-shot secret; needs partner credentials |

**A card charge needs a billing address.** When address verification is on for
the account, a charge without one — or with one the issuer does not match —
comes back `Declined` with an AVS reason, exit 0. A saved card carries no
address, so a vaulted charge reads it from the customer record via
`--customer-id`, or takes the `--billing-*` flags itself. On the sandbox,
`123 Test St`, `10001`, `US` verifies. **Give every charge its own
`--reference-id`**: the same card and amount under the same reference is
refused as a duplicate.

## Things to avoid

- **Don't treat exit 0 as "paid".** Read `data.transactionStatus`.
- **Don't parse stderr.** The structured error is on stdout under
  `--output json`; stderr carries tracing, the production banner, the update
  notice, the `--wait` warnings and a failed `--wait` poll, which leaves the
  created transaction's envelope on stdout.
- **Don't infer stdout's shape from the exit code.** The `pos create --wait`
  endings break that inference in both directions.
- **Don't parse stdout after exit 141.** It is a prefix of the document, and a
  prefix of valid JSON parses as nothing at all.
- **Don't call `auth login`** from an agent. Use the environment variables.
- **Don't call `auth switch`.** It rewrites `~/.flute2/config.toml` for every
  later process on the machine. Pass `--profile` per command instead.
- **Don't fire a second `pos create`** on a terminal that already has one in
  progress.
- **Don't retry a money-moving create** without reconciling first.
- **Don't rely on 402, 409 or 429 having a dedicated exit code.** They exit 1
  today and may not tomorrow.
- **Don't expect `devices` or `subscriptions`** — see
  [What this CLI does not have](#what-this-cli-does-not-have).

## What this CLI does not have

`devices` and `subscriptions` are absent because v2 declares no such
operations. `flute` (v1) still ships and still serves them; the two binaries do
not conflict, so both can be installed.

Webhook endpoints exist in the API and are out of scope here.
