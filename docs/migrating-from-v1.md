# Coming from `flute` (v1)

**`flute2` installs alongside `flute`.** It does not replace it, and the two do not conflict: they are separate binaries, with separate config files and keychain entries, calling different versions of the API on the same hosts. `devices` and `subscriptions` exist only in v1, so keep `flute` installed for those.

### v1 commands and flags, and their v2 equivalents

There are no aliases. A v1 command or flag name breaks loudly under `flute2` rather than charging something else. The exceptions are flags that keep their names with a changed meaning, the rate flags among them: they read their values as percentages.

| v1 | v2 |
|---|---|
| `transactions sale`, `transactions auth` | `transactions create` (`--capture-method manual` for an authorization) |
| `transactions void`, `transactions refund` | `transactions reversal` |
| `transactions settle` | `settlements close` |
| `transactions sale` with no processor id | `transactions create --payment-processor-id` — required; `settings payment-config` lists the ids |
| `--card-data-source` | **dropped** |
| `ach debit` | `transactions create` with the ACH flags |
| `ach credit` | `transactions credit` |
| `ach void <id>`, `ach refund <id>` | `transactions reversal --transaction-id <id>` |
| `--sec-code 1` to `4` (integers, `4` = Telephone) | `--sec-code web`, `ppd` or `ccd` — there is no Telephone value |
| `--l3-product "Description,SKU,UnitPrice,UnitOfMeasure,Quantity"` | `--l3-product` as comma-separated `key=value` pairs; `--help` lists the keys |
| `customers add-card <customer-id>`, `customers add-ach <customer-id>` | `payment-methods add-card`, `payment-methods add-ach` with `--customer-id <customer-id>` — optional, so pass it to attach the method to a customer |
| `customers methods <customer-id>` | `payment-methods list --customer-id <customer-id>` |
| `customers remove-method <customer-id> <method-id>` | `payment-methods delete <method-id> --yes` |
| `customers add-ach` with `--account-holder-type` optional | `payment-methods add-ach --account-holder-type` — required |
| `customers create` with optional names | `customers create --first-name --last-name` — both required |
| `customers list --search` | `--full-name`, `--email`, `--company-name` or `--mobile` |
| `pos create --reading-method 1` or `2` | `--reading-method keyed-entry` or `regular` |
| `pos create` with `--reference-id` required, `--amount` optional | `--amount` and `--currency-code` required, `--reference-id` optional |
| `keys`, `tokens` | `api-keys` |
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

### v1 JSON paths, and their v2 equivalents

The envelope keeps its `object` names and its `data` and `meta` keys. Two things differ. A collection's `data` is the array itself, where v1 wrapped it in an object with `items` and `total`, so a v1 path such as `.data.items[]` fails with `Cannot index array`. And each resource names its own fields, so a v1 path inside a single object's `data` returns `null` rather than failing. A gate such as `[ "$(… | jq -r '.data.status')" = "Approved" ]` takes the failure branch on a transaction that was approved.

| v1 | v2 |
|---|---|
| `.data.items[]`, `.data.total` on a list | `.data[]`, `.meta.page_info.totalItems` |
| `.data.id` on a customer, terminal, POS transaction or settlement batch | `.data.customerId`, `.data.terminalId`, `.data.posTransactionId`, `.data.batchId` |
| `.data.status` on a transaction | `.data.transactionStatus` |
| `.data.status` on a settlement batch | `.data.batchStatus` |
| `.data.items[].id` on a transaction list | `.data[].transactionId` |
| `.data.items[].status`, `.type`, `.date` | `.data[].transactionStatus`, `.transactionType`, `.transactionDateTime` |
| `.data.amount.totalAmount`, `.data.totalAmount` | `.data.processedAmount`, `.data.amountBreakdown.*` |
| `.data.authCode` | `.data.processorDetails.authCode` |
| `.data.responseDescription`, `.data.responseCode` | `.data.declineDetails.message`, `.data.declineDetails.code` |
| `.data.avsResponse` | `.data.addressVerificationServiceResponse` |
| `.data.availableOperations` | not declared on a `transactions get` response |
| `.data.merchant_id` from `auth status` | not reported |

A delete or revoke under `--output json` prints a confirmation envelope, where v1 printed nothing.
