# API v2 — open defects

Divergences between the published v2 API reference and observed behaviour, still to be raised.

Two kinds, because they are fixed in different places:

- **Documentation defects** — the API behaves sensibly, the reference is wrong or silent about it.
- **Behaviour defects** — the reference describes the intended contract, the implementation diverges from it.

Every confirmed entry is verified against the sandbox API, not by reading the specification.

## Verifying

```sh
export API=https://sandbox.api.flute.com
export SPEC=https://developer.flute.com/_bundle/api-reference/@v2/index.json
export TOKEN=$(flute auth token)
```

`$SPEC` is the OpenAPI bundle the documentation site renders from.

Pipe responses through `jq -S` so keys sort and two payloads compare line by line. Never project fields blindly — `jq '{transactionId, status}'` renders an error envelope as a wall of nulls. Prefer `jq -S 'if has("transactionId") then {transactionId, transactionStatus} else . end'`.

## Documentation defects

### D7 — `servers` block points at the wrong host for the token endpoint

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

Documented as a `GET /v2/transactions` query parameter; not bound. A malformed value passes, where a malformed value on a genuinely bound parameter is rejected.

```sh
curl -s -o /dev/null -w "merchantId=notauuid -> %{http_code}\n" \
  "$API/v2/transactions?pageSize=1&merchantId=notauuid" -H "Authorization: Bearer $TOKEN"   # 200, ignored
curl -s -o /dev/null -w "customerId=notauuid -> %{http_code}\n" \
  "$API/v2/transactions?pageSize=1&customerId=notauuid" -H "Authorization: Bearer $TOKEN"   # 400, bound
```

A documented filter that accepts anything and changes nothing fails silently, which is worse than an absent one.

### D13 — `CardDataDto.paymentMethodId` description is ACH text

The card schema's `paymentMethodId` description is character-for-character identical to the ACH one and describes the wrong payment method: "a previously saved **ACH account** to charge", and "use `paymentMethodId` instead of **`accountNumber`**". For a card it should reference a saved card and `cardNumber`.

```sh
curl -s $SPEC | jq -r '.components.schemas.CardDataDto.properties.paymentMethodId.description'
curl -s $SPEC | jq -r '.components.schemas.AchDataDto.properties.paymentMethodId.description'
```

### D14 — `cardData` and `achData` carry no descriptions

`transactionDetails.cardData` and `transactionDetails.achData` are mutually exclusive — exactly one must be supplied — and neither property has a description, so the rule is never stated where a reader would look. It can only be inferred from the request examples or from the nested `paymentMethodId` text.

```sh
curl -s $SPEC | jq -S '.components.schemas.TransactionDetailsDto.properties
  | {cardData:(.cardData.description // "NONE"), achData:(.achData.description // "NONE")}'
```

Related, as an improvement rather than a defect: the four payload shapes are modelled as one flat object with no `oneOf` or `discriminator`, so none of the exactly-one-of rules are machine-readable and schema validation accepts payloads the API rejects.

## Behaviour defects

### D10 — Nothing returns `201`

No endpoint declares `201`; fifty-nine declare `200` and two declare `204`. The API design conventions specify `201 Created` for `POST /customers`, `/transactions`, and `/payment-methods`. Either the conventions document or the implementation is wrong; the reference accurately reflects the implementation.

```sh
curl -s $SPEC | jq -r '[.paths[][] | select(type=="object") | .responses // {} | keys[]] | group_by(.) | map({(.[0]): length}) | add'
```

## Open, not yet confirmed

**D9 — `402` on `POST /v2/payment-methods/{paymentMethodId}/set-default`.** `402` is declared on twelve operations, eleven of which move money. Setting a default payment method does not. Either an error in the declaration or a behaviour that needs documenting.

```sh
curl -s $SPEC | jq -r '.paths | to_entries[] | .key as $p | .value | to_entries[]
  | select(.value.responses."402") | "\(.key|ascii_upcase) \($p)"'
```

**Sandbox test cards.** `5555555555554444` is rejected at BIN lookup with `DeclineBinNotFound`, though `payment-config` advertises MasterCard under `availableCardTypes`. `4111111111111111` works. Unclear whether the sandbox BIN table is incomplete or that is the wrong test card; a documented list of sandbox test cards would resolve it.
