# API v2 — documentation and behaviour defects

Divergences between the published v2 API reference and observed behaviour, found while building this CLI.

Two kinds, because they are fixed in different places:

- **Documentation defects** — the API behaves sensibly, the reference is wrong or silent about it.
- **Behaviour defects** — the reference describes the intended contract, the implementation diverges from it.

Every confirmed entry was verified against the sandbox API, not by reading the specification. That distinction matters: three earlier findings were withdrawn after they turned out to be errors in how the specification was read rather than errors in the specification.

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

Transaction requests need a unique `baseAmount` each run; repeating one trips the undocumented duplicate check in D12.

## Documentation defects

| ID | Defect |
|---|---|
| D2 | Conditional requirements for new ACH payments are undocumented |
| D4 | `captureMethod` missing from a schema that forbids unknown fields |
| D5 | `customerId` missing from the create-transaction request schema |
| D7 | `servers` block points at a host that does not serve `/oauth2/token` |
| D8 | `merchantId` documented as a filter, silently ignored |
| D11 | Create-transaction success documented as a paged collection |
| D12 | Duplicate-transaction rejection undocumented, and its message is wrong |
| D13 | `CardDataDto.paymentMethodId` description is ACH text |
| D14 | `cardData` and `achData` carry no descriptions |

### D2 — Conditional ACH requirements undocumented

`billingAddress` and `contactInfo` are absent from the schema's `required` list, and the referenced schemas describe them only as "Identifies the address information." and "Contact information." Both are mandatory when supplying a new ACH account inline, as is `contactInfo.mobilePhoneNumber`, plus either `firstName` and `lastName` or `companyName`.

The requirement is conditional — it does not apply to card payments, or to ACH against a saved `paymentMethodId` — which OpenAPI's `required` array cannot express. The condition therefore has to live in the field descriptions.

Posting a new ACH account without them returns `400`:

```json
{"contactInfo":["ContactInfo is required for new ACH payments."],
 "billingAddress":["BillingAddress is required for new ACH payments."]}
```

### D4 — `captureMethod` missing from `CardDataDto`

`components.schemas.CardDataDto` lists only `paymentMethodId` and `paymentMethodDetails`, with `additionalProperties: false`. By the schema, sending `captureMethod` is invalid.

It is in fact accepted, and selects the transaction type:

| Sent | Result |
|---|---|
| `"Auto"` | `transactionType: Sale`, `transactionStatus: Captured` |
| `"Manual"` | `transactionType: Authorization`, `transactionStatus: Authorized` |
| omitted | defaults to `Auto` |

`additionalProperties: false` is enforced — an unknown field returns `The JSON property 'zzzNotAField' could not be mapped to any .NET member contained in type 'CardDataDto'` — so `captureMethod` is a genuine member the published schema omits. Both card examples on the endpoint use it, and the description of `POST /v2/transactions/{transactionId}/capture` refers to it by name.

Undocumented, there is no discoverable way to create an authorization rather than an immediate charge.

### D5 — `customerId` missing from `CreateTransactionRequestDto`

The schema lists sixteen properties with `additionalProperties: false`; `customerId` is not among them. It is accepted, links the transaction to a customer record, and is echoed back on the response. The same unknown-field control as D4 confirms it is a real member.

### D7 — `servers` block points at the wrong host for the token endpoint

The spec declares `servers: [https://sandbox.api.flute.com, https://api.flute.com]`, and `/oauth2/token` declares no path- or operation-level override. So the machine-readable spec places the token endpoint on the API host, where it returns **404 with an empty body**.

The prose names a different host — `https://sandbox.oauth.api.flute.com` and `https://oauth.api.flute.com` — which works, returning `400 {"error":"invalid_request","error_description":"The mandatory 'client_id' parameter is missing."}` for a request with no credentials.

Highest impact of the set: authentication is the first call of every integration, and a client generated from the spec cannot obtain a token at all. Fixed by adding a `servers` override on the `/oauth2/token` path.

### D8 — `merchantId` is documented but ignored

`merchantId` is documented as a `GET /v2/transactions` query parameter and is not bound. A malformed value passes; a malformed value on a genuinely bound parameter is rejected:

```
?merchantId=notauuid  ->  200   (ignored)
?customerId=notauuid  ->  400   (bound as a GUID)
```

A documented filter that accepts anything and changes nothing fails silently, which is worse than an absent one.

### D11 — Create-transaction success documented as a paged collection

`POST /v2/transactions` documents its `200` as `PageOfGetTransactionResponseDto`, whose properties are `items` and `pageInfo`. It returns a single transaction object. The endpoint's own examples show the single-object form, so the schema contradicts the examples beside it, and a generated client fails to deserialise every successful create.

### D12 — Undocumented duplicate-transaction rejection

Posting the same `baseAmount` twice within a short window returns `400` with an `IsDuplicate` error. The behaviour is not documented and the window is unspecified.

The message is also inaccurate. It cites "the same transaction amount, customer details, and a close timestamp", but the match ignores the customer — reproduced with two different customers and with no customer at all.

```
amount 242.46, first attempt   ->  200 Captured
amount 242.46, second attempt  ->  400 IsDuplicate
```

Legitimate repeat billing of identical amounts is affected, so the window length matters to merchants, not only to testing.

### D13 — `CardDataDto.paymentMethodId` description is ACH text

The card schema's `paymentMethodId` description is character-for-character identical to the ACH one, and describes the wrong payment method: "This is the identifier of a previously saved **ACH account** to charge. We recommend using `paymentMethodId` instead of **`accountNumber`**." For a card it should reference a saved card and `cardNumber`.

### D14 — `cardData` and `achData` have no descriptions

`transactionDetails.cardData` and `transactionDetails.achData` are mutually exclusive — exactly one must be supplied — and neither property carries a description, so the rule is never stated where a reader would look for it. It can only be inferred from the four request examples or from the nested `paymentMethodId` text.

Related: the schema models the four payload shapes as one flat object with no `oneOf` or `discriminator`, so none of the exactly-one-of rules are machine-readable. Schema validation accepts payloads the API rejects. Modelling the variants would make the constraints enforceable by generated clients; noted as an improvement rather than a defect, since the rules are stated in prose.

## Behaviour defects

| ID | Defect |
|---|---|
| D3a | `400` responses use two incompatible shapes |
| D3b | Error responses are PascalCase; success responses and the docs are camelCase |
| D3c | Internal type names in error text; `entityId` never populated |
| D10 | No endpoint returns `201` |

### D3a — Two incompatible `400` shapes

The reference documents one error envelope for `400`. Two are returned:

- **Edge validation** — RFC 9110 ProblemDetails: `type`, `title`, `status`, `errors`, `traceId`. Lowercase keys, no correlation id.
- **Downstream validation** — the documented envelope: `StatusCode`, `Source`, `ExceptionType`, `CorrelationId`, `Errors`, `Title`, `Cause`, `Resolution`. PascalCase.

Both come from `POST /v2/transactions`. A single deserialiser cannot be written from the documentation, and a `400` from the first path carries `traceId` rather than the `correlationId` support asks for.

A third shape exists on the token endpoint, which is not an ASP.NET service: `error`, `error_description`, `error_uri`.

### D3b — Error responses are PascalCase

Every documented example is camelCase, and success responses genuinely are camelCase. Error responses are PascalCase — `StatusCode`, `Cause`, `CorrelationId`. One API returns two casings depending on whether the call succeeded, and a case-sensitive client generated from the spec reads nothing from an error.

### D3c — Internal type names in error text

A `404` for a missing customer returns the internal entity type name — namespace and class — in `Details`, `Cause`, and `Resolution`, where the documented example shows "Entity with ID …" and "The requested resource does not exist or has been deleted." The exception type's own default cause matches the documented text, so the substitution overrides it.

`Resolution` pluralises the type name by appending an "s", which shows a type name being interpolated into a template expecting a friendly noun.

`entityId` is documented and populated in the `404` example but absent from the actual response — the one error where it is most useful, since it is the identifier just requested.

A component test in the shared error-handling package asserts the type-name form, so the current behaviour may be intentional. Raised as a contradiction between the documented contract and a tested behaviour rather than as an assumed bug.

Shared error handler, so this is not specific to customers.

### D10 — Nothing returns `201`

No endpoint in the specification declares `201`; fifty-nine declare `200` and two declare `204`. The API design conventions specify `201 Created` for `POST /customers`, `/transactions`, and `/payment-methods`. Either the conventions document or the implementation is wrong; the reference accurately reflects the implementation.

## Withdrawn

Three earlier findings were wrong. All three came from reading the specification without resolving `$ref`, which it uses heavily — 43 of 127 parameters and all 358 error responses are references.

| ID | Claim | Why it was wrong |
|---|---|---|
| D1 | Pagination parameters missing from list endpoints | `pageIndex` and `pageSize` are `$ref`s to `components.parameters` |
| D3 | No error responses documented anywhere | All error responses `$ref` `components.responses`, which carry schemas and worked examples |
| D6 | `transactionStatus` and `sortOrder` missing | Both are `$ref`s to shared parameters |

When a query returns `null` or a bare `{"$ref": …}`, that is a pointer, not an absence.

## Open, not yet confirmed

**D9 — `402` on `POST /v2/payment-methods/{paymentMethodId}/set-default`.** `402` is declared on twelve operations, eleven of which move money. Setting a default payment method does not. Either an error in the declaration or a behaviour that needs documenting.

**Sandbox test cards.** `5555555555554444` is rejected at BIN lookup with `DeclineBinNotFound`, though `payment-config` advertises MasterCard under `availableCardTypes`. `4111111111111111` works. Unclear whether the sandbox BIN table is incomplete or that is simply the wrong test card; a documented list of sandbox test cards would resolve it.
