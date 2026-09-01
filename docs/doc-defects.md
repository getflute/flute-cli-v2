# API v2 — documentation and behaviour defects

Divergences between the published v2 API reference and observed behaviour, found while building this CLI.

Two kinds, because they are fixed in different places:

- **Documentation defects** — the API behaves sensibly, the reference is wrong or silent about it.
- **Behaviour defects** — the reference describes the intended contract, the implementation diverges from it.

Every confirmed entry was verified against the sandbox API, not by reading the specification. That distinction matters: three earlier findings were withdrawn after they turned out to be errors in how the specification was read rather than errors in the specification.

**Status** is tracked per entry:

- **Reported** — presented to the API team; awaiting their feedback. Still open here until they respond.
- **To raise** — confirmed, not yet presented.
- **Unconfirmed** — observed, not yet reproduced well enough to raise.

Nothing is removed from this file when it is reported. An entry leaves only when the API team resolves it, and then it leaves with a note saying so. **Where the documentation and the running implementation disagree, this CLI is built against the implementation** — a defect is a thing to raise, not a thing to wait on.

## Verifying

```sh
export API=https://sandbox.api.flute.com
export SPEC=https://developer.flute.com/_bundle/api-reference/@v2/index.json
export TOKEN=$(flute auth token)
```

`$SPEC` is the OpenAPI bundle the documentation site renders from, so it is the published source rather than a paraphrase of it.

Two habits when checking responses. Pipe through `jq -S` so keys sort and two payloads can be compared line by line. And never project fields blindly — `jq '{transactionId, status}'` renders an error envelope as a wall of nulls. Prefer:

```sh
jq -S 'if has("transactionId") then {transactionId, transactionStatus} else . end'
```

Transaction requests need a fresh `referenceId` each run — it is part of the duplicate-check key, and repeating a card/amount pair without one trips the `IsDuplicate` rejection in D12. Where no reference id is accepted, vary the amount instead.

## Documentation defects

| ID | Defect | Status |
|---|---|---|
| D2 | Conditional requirements for new ACH payments are undocumented | Reported |
| D4 | `captureMethod` missing from a schema that forbids unknown fields | Reported |
| D5 | `customerId` missing from the create-transaction request schema | Reported |
| D11 | Create-transaction success documented as a paged collection | Reported |
| D12 | Duplicate-check window undocumented, and its message is wrong | Reported |
| D7 | `servers` block points at a host that does not serve `/oauth2/token` | To raise |
| D8 | `merchantId` documented as a filter, silently ignored | To raise |
| D13 | `CardDataDto.paymentMethodId` description is ACH text | To raise |
| D14 | `cardData` and `achData` carry no descriptions | To raise |
| D15 | Capture request schema and its own example disagree on the field name | To raise |
| D16 | Delete operations are inconsistent in status code and declared `404` | To raise |
| D17 | Every request body is declared optional, including twelve with required fields | To raise |

### D2 — Conditional ACH requirements undocumented

**Status:** Reported.

`billingAddress` and `contactInfo` are absent from the schema's `required` list, and the referenced schemas describe them only as "Identifies the address information." and "Contact information." Both are mandatory when supplying a new ACH account inline, as is `contactInfo.mobilePhoneNumber`, plus either `firstName` and `lastName` or `companyName`.

The requirement is conditional — it does not apply to card payments, or to ACH against a saved `paymentMethodId` — which OpenAPI's `required` array cannot express. The condition therefore has to live in the field descriptions.

Posting a new ACH account without them returns `400`:

```json
{"contactInfo":["ContactInfo is required for new ACH payments."],
 "billingAddress":["BillingAddress is required for new ACH payments."]}
```

### D4 — `captureMethod` missing from `CardDataDto`

**Status:** Reported. Still present in the current bundle.

`components.schemas.CardDataDto` lists only `paymentMethodId` and `paymentMethodDetails`, with `additionalProperties: false`. By the schema, sending `captureMethod` is invalid.

It is in fact accepted, and selects the transaction type:

| Sent | Result |
|---|---|
| `"Auto"` | `transactionType: Sale`, `transactionStatus: Captured` |
| `"Manual"` | `transactionType: Authorization`, `transactionStatus: Authorized` |
| omitted | defaults to `Auto` |

`additionalProperties: false` is enforced — an unknown field returns `The JSON property 'zzzNotAField' could not be mapped to any .NET member contained in type 'CardDataDto'` — so `captureMethod` is a genuine member the published schema omits. Both card examples on the endpoint use it, and the description of `POST /v2/transactions/{transactionId}/capture` refers to it by name.

Undocumented, there is no discoverable way to create an authorization rather than an immediate charge.

```sh
curl -s $SPEC | jq -S '.components.schemas.CardDataDto | {props:(.properties|keys), additionalProperties}'
```

### D5 — `customerId` missing from `CreateTransactionRequestDto`

**Status:** Reported. Still present in the current bundle.

The schema lists sixteen properties with `additionalProperties: false`; `customerId` is not among them. It is accepted, links the transaction to a customer record, and is echoed back on the response. The same unknown-field control as D4 confirms it is a real member.

```sh
curl -s $SPEC | jq -S '.components.schemas.CreateTransactionRequestDto.properties | keys'
```

### D11 — Create-transaction success documented as a paged collection

**Status:** Reported. Still present in the current bundle, and broader than first filed.

`POST /v2/transactions` documents its `200` as `PageOfGetTransactionResponseDto`, whose properties are `items` and `pageInfo`. It returns a single transaction object. The endpoint's own examples show the single-object form, so the schema contradicts the examples beside it, and a generated client fails to deserialise every successful create.

Seven single-transaction writes are affected, not one:

```sh
curl -s $SPEC | jq -r '.paths | to_entries[] | .key as $p | .value | to_entries[]
  | select(.value.responses."200".content."application/json".schema."$ref"? // ""
           | test("PageOfGetTransactionResponseDto"))
  | "\(.key|ascii_upcase) \($p)"'
```

returns `POST /v2/transactions`, `capture`, `reversal`, `credit`, `tip-adjustment`, `ach-hold`, and `ach-release`, alongside the one endpoint for which the paged shape is correct, `GET /v2/transactions`.

**No schema in the bundle describes what these seven return.** The response carries `amountDetails`, `processorResponse`, `receipt`, and `responseDetails`; the two transaction response schemas — `GetTransactionResponseDtoShort`, which the page's `items` reference, and `GetTransactionResponseDtoFull`, which `GET /v2/transactions/{id}` returns — declare none of those fields and both set `additionalProperties: false`. So the fix is not re-pointing the `$ref` at an existing schema: the write response shape has to be added.

The endpoints' own response examples do show it, and all seven agree, which is what makes the shape recoverable at all:

```sh
curl -s $SPEC | jq -S '[.components.schemas
  | (.GetTransactionResponseDtoShort, .GetTransactionResponseDtoFull)
  | {additionalProperties, props:(.properties|keys)}]'
curl -s $SPEC | jq -S '.paths."/v2/transactions".post.responses."200".content
  ."application/json".examples | to_entries[0].value.value | keys'
```

The practical consequence is worse than a deserialisation failure: because the declared schema forbids additional properties, a validating client rejects every successful charge, and a strict conformance suite cannot express the divergence as a field-level exemption at all.

### D12 — Duplicate-transaction rejection: undocumented window, inaccurate message

**Status:** Reported. **Narrowed** — the mechanism is now documented; two parts of the original finding are not.

Posting the same `baseAmount` twice within a short window returns `400` with an `IsDuplicate` error.

The bundle now documents the mechanism, under `referenceId`: "This is included in the duplicate-check key. It allows the same card and amount combination to be charged multiple times when the reference identifiers are different." The original claim that the behaviour is wholly undocumented no longer holds and is withdrawn.

Two parts stand:

- **The window is still unspecified.** Nothing states how long two transactions must be apart for the key to stop matching, and merchants who legitimately bill identical amounts need that number.
- **The error message is inaccurate.** It cites "the same transaction amount, customer details, and a close timestamp", but the match ignores the customer — reproduced with two different customers and with no customer at all. It also does not mention `referenceId`, which is the one field a caller can actually use to avoid the collision.

```
amount 242.46, no referenceId, first attempt   ->  200 Captured
amount 242.46, no referenceId, second attempt  ->  400 IsDuplicate
```

```sh
curl -s $SPEC | jq -r '.components.schemas.CreateTransactionRequestDto.properties.referenceId.description'
```

### D7 — `servers` block points at the wrong host for the token endpoint

**Status:** To raise.

The spec declares `servers: [https://sandbox.api.flute.com, https://api.flute.com]`, and `/oauth2/token` declares no path- or operation-level override, placing the token endpoint on the API host. That returns **404 with an empty body**. The prose names a different host, which works.

```sh
curl -s $SPEC | jq -S '.servers'
curl -s $SPEC | jq -S '{path:(.paths."/oauth2/token".servers // "none"), op:(.paths."/oauth2/token".post.servers // "none")}'

curl -s -w "\n%{http_code}\n" -X POST 'https://sandbox.api.flute.com/oauth2/token' \
  -d 'grant_type=client_credentials&scope=offline_access'          # 404, empty body
curl -s -w "\n%{http_code}\n" -X POST 'https://sandbox.oauth.api.flute.com/oauth2/token' \
  -d 'grant_type=client_credentials&scope=offline_access'          # 400, asks for client_id
```

Highest impact of the set: authentication is the first call of every integration, and a client generated from the spec cannot obtain a token at all. Fixed by adding a `servers` override on the `/oauth2/token` path. Needs no credentials to reproduce.

### D8 — `merchantId` is documented but ignored

**Status:** To raise.

Documented as a `GET /v2/transactions` query parameter; not bound. A malformed value passes, where a malformed value on a genuinely bound parameter is rejected.

```sh
curl -s -o /dev/null -w "merchantId=notauuid -> %{http_code}\n" \
  "$API/v2/transactions?pageSize=1&merchantId=notauuid" -H "Authorization: Bearer $TOKEN"   # 200, ignored
curl -s -o /dev/null -w "customerId=notauuid -> %{http_code}\n" \
  "$API/v2/transactions?pageSize=1&customerId=notauuid" -H "Authorization: Bearer $TOKEN"   # 400, bound
```

A documented filter that accepts anything and changes nothing fails silently, which is worse than an absent one.

### D13 — `CardDataDto.paymentMethodId` description is ACH text

**Status:** To raise.

The card schema's `paymentMethodId` description is character-for-character identical to the ACH one and describes the wrong payment method: "a previously saved **ACH account** to charge", and "use `paymentMethodId` instead of `**accountNumber**`". For a card it should reference a saved card and `cardNumber`.

```sh
curl -s $SPEC | jq -r '.components.schemas.CardDataDto.properties.paymentMethodId.description'
curl -s $SPEC | jq -r '.components.schemas.AchDataDto.properties.paymentMethodId.description'
```

### D14 — `cardData` and `achData` carry no descriptions

**Status:** To raise.

`transactionDetails.cardData` and `transactionDetails.achData` are mutually exclusive — exactly one must be supplied — and neither property has a description, so the rule is never stated where a reader would look. It can only be inferred from the request examples or from the nested `paymentMethodId` text.

```sh
curl -s $SPEC | jq -S '.components.schemas.TransactionDetailsDto.properties
  | {cardData:(.cardData.description // "NONE"), achData:(.achData.description // "NONE")}'
```

Related, as an improvement rather than a defect: the four payload shapes are modelled as one flat object with no `oneOf` or `discriminator`, so none of the exactly-one-of rules are machine-readable and schema validation accepts payloads the API rejects.

### D15 — Capture request schema and its own example disagree on the field name

**Status:** To raise. Found while planning the transactions group.

`CaptureRequestDto` declares exactly one property, `captureAmount`, and sets `additionalProperties: false`. The capture operation's own request example sends `{"amount": 50}`. Under the schema that example is invalid, and — because unknown fields are rejected rather than ignored (see D4) — a client that follows the example cannot capture at all, while a client that follows the schema contradicts the documentation beside it.

```sh
curl -s $SPEC | jq -S '.components.schemas.CaptureRequestDto | {props:(.properties|keys), additionalProperties}'
curl -s $SPEC | jq -S '.paths."/v2/transactions/{transactionId}/capture".post.requestBody.content."application/json".example'
```

The schema description compounds it by using the example's wording: "Omit amount or send empty body for full capture; set `captureAmount` for partial capture" — naming both fields in one sentence.

Needs a live capture against a real authorization to settle which the implementation accepts.

### D16 — Delete operations are inconsistent in status code and declared `404`

**Status:** To raise.

The four delete operations disagree with each other in ways nothing explains:

| Operation | Success | Declares `404`? |
|---|---|---|
| `DELETE /v2/customers/{customerId}` | `200` | yes |
| `DELETE /v2/payment-methods/{paymentMethodId}` | `200` | yes |
| `DELETE /v2/api-keys/{clientId}` | `200` | **no** |
| `DELETE /v2/payment-links/{paymentLinkId}` | **`204`** | yes |

```sh
curl -s $SPEC | jq -r '.paths | to_entries[] | .key as $p | .value | to_entries[]
  | select(.key=="delete") | "\($p) -> \(.value.responses|keys|join(","))"'
```

A client cannot write one delete path: one of the four returns no body, and one of the four gives no documented meaning to a missing resource. Either is defensible alone; together they read as drift rather than design.

### D17 — Every request body is declared optional

**Status:** To raise. Found while building the conformance harness.

No operation in the bundle sets `requestBody.required: true`. All 23 operations that accept an `application/json` body leave it optional, while 13 of their schemas — 12 excluding the webhook endpoint — declare required properties:

| Operation | Schema requires |
|---|---|
| `POST /v2/transactions` | `baseAmount`, `paymentProcessorId`, `transactionDetails` |
| `POST /v2/transactions/credit` | `baseAmount`, `creditDetails`, `paymentProcessorId`, `referenceId` |
| `POST /v2/transactions/{transactionId}/share-receipt` | `hasCustomerConsent`, `recipient`, `shareBy` |
| `POST /v2/customers` | `firstName`, `lastName` |
| `POST /v2/payment-methods/cards` | `cardNumber`, `expirationMonth`, `expirationYear` |
| `POST /v2/payment-methods/ach` | `accountHolderType`, `accountNumber`, `accountType`, `routingNumber` |
| `POST /v2/payment-links` | `paymentMethods` |
| `POST /v2/payment-links/{paymentLinkId}/share` | `hasCustomerConsent`, `recipient`, `shareBy` |
| `POST /v2/pos/transactions` | `baseAmount`, `currencyCode`, `posDeviceId`, `terminalId` |
| `POST /v2/pos/transactions/{posTransactionId}/print-receipt` | `terminalId` |
| `POST /v2/api-keys` | `apiKeyName`, `merchantId` |
| `POST /v2/settlements/batches/close` | `paymentProcessorId` |

All twelve in-scope operations are listed. The thirteenth, `POST /v2/webhooks/endpoints`, has the same defect and is outside this CLI's scope.

The two statements cannot both hold: a body whose schema requires `baseAmount` cannot be satisfied by no body at all. The reading a generated client takes is the permissive one, so it will happily offer an empty `POST /v2/transactions` — a request to take a payment with no amount, no processor, and no instrument.

```sh
curl -s $SPEC | jq -r '[.paths[][] | select(type=="object") | .requestBody
  | select(. != null) | .required // false] | group_by(.) | map({(.[0]|tostring): length}) | add'
```

Distinct from D2, which is about a requirement OpenAPI cannot express. This one OpenAPI expresses fine and the bundle states wrongly.

## Behaviour defects

| ID | Defect | Status |
|---|---|---|
| D3a | Two incompatible `400` shapes | Reported |
| D3b | Error responses are PascalCase where every documented example is camelCase | Reported |
| D3c | Internal type names in error text | Reported |
| D10 | Nothing returns `201` | To raise |

### D3a — Two incompatible `400` shapes

**Status:** Reported.

The reference documents one error envelope for `400`. Two are returned:

- **Edge validation** — RFC 9110 ProblemDetails: `type`, `title`, `status`, `errors`, `traceId`. Lowercase keys, no correlation id.
- **Downstream validation** — the documented envelope: `StatusCode`, `Source`, `ExceptionType`, `CorrelationId`, `Errors`, `Title`, `Cause`, `Resolution`. PascalCase.

Both come from `POST /v2/transactions`. A single deserialiser cannot be written from the documentation, and a `400` from the first path carries `traceId` rather than the `correlationId` support asks for.

A third shape exists on the token endpoint, which is not an ASP.NET service: `error`, `error_description`, `error_uri`. A fourth case returns no body at all — some `401`s carry only a `www-authenticate` header. **Four shapes against one documented envelope**, which is why this CLI's error parser has four arms.

### D3b — Error responses are PascalCase

**Status:** Reported.

Every documented example is camelCase, and success responses genuinely are camelCase. Error responses are PascalCase — `StatusCode`, `Cause`, `CorrelationId`. One API returns two casings depending on whether the call succeeded, and a case-sensitive client generated from the spec reads nothing from an error.

### D3c — Internal type names in error text

**Status:** Reported.

A `404` for a missing customer returns the internal entity type name — namespace and class — in `Details`, `Cause`, and `Resolution`, where the documented example shows "Entity with ID …" and "The requested resource does not exist or has been deleted." The exception type's own default cause matches the documented text, so the substitution overrides it.

`Resolution` pluralises the type name by appending an "s", which shows a type name being interpolated into a template expecting a friendly noun.

`entityId` is documented and populated in the `404` example but absent from the actual response — the one error where it is most useful, since it is the identifier just requested.

A component test in the shared error-handling package asserts the type-name form, so the current behaviour may be intentional. Raised as a contradiction between the documented contract and a tested behaviour rather than as an assumed bug.

Shared error handler, so this is not specific to customers.

### D10 — Nothing returns `201`

**Status:** To raise.

No endpoint in the specification declares `201`; fifty-nine declare `200` and two declare `204`. The API design conventions specify `201 Created` for `POST /customers`, `/transactions`, and `/payment-methods`. Either the conventions document or the implementation is wrong; the reference accurately reflects the implementation.

```sh
curl -s $SPEC | jq -r '[.paths[][] | select(type=="object") | .responses // {} | keys[]] | group_by(.) | map({(.[0]): length}) | add'
```

## Withdrawn

Three earlier findings were wrong. All three came from reading the specification without resolving `$ref`, which it uses heavily — 43 of 127 parameters and all 358 error responses are references.

| ID | Claim | Why it was wrong |
|---|---|---|
| D1 | Pagination parameters missing from list endpoints | `pageIndex` and `pageSize` are `$ref`s to `components.parameters` |
| D3 | No error responses documented anywhere | All error responses `$ref` `components.responses`, which carry schemas and worked examples |
| D6 | `transactionStatus` and `sortOrder` missing | Both are `$ref`s to shared parameters |

When a query returns `null` or a bare `{"$ref": …}`, that is a pointer, not an absence.

## Unconfirmed

**D9 — `402` on `POST /v2/payment-methods/{paymentMethodId}/set-default`.** `402` is declared on twelve operations, eleven of which move money. Setting a default payment method does not. Either an error in the declaration or a behaviour that needs documenting.

```sh
curl -s $SPEC | jq -r '.paths | to_entries[] | .key as $p | .value | to_entries[]
  | select(.value.responses."402") | "\(.key|ascii_upcase) \($p)"'
```

**Sandbox test cards.** `5555555555554444` is rejected at BIN lookup with `DeclineBinNotFound`, though `payment-config` advertises MasterCard under `availableCardTypes`. `4111111111111111` works. Unclear whether the sandbox BIN table is incomplete or that is simply the wrong test card; a documented list of sandbox test cards would resolve it.
