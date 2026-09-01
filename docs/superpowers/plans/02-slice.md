# Phase 1 — The vertical slice

> Part of the [flute-cli-v2 implementation plan](00-overview.md). Read [`00-overview.md`](00-overview.md) first — its global constraints and testing contract apply to every task here.

# Phase 1 — The vertical slice

`ping`, `customers create/get`, and `transactions` card create/get, end to end. Deliberately risk-loaded: OAuth against a real token flow, money precision, PCI redaction of a real-looking PAN, error parsing, all three output modes, and the two request fields the specification omits. Roughly 600 lines. **Do not start Phase 2 until this is reviewed.**

### Task 9: `ping`

The smallest possible proof that auth, transport, and output work together.

**Files:**
- Create: `src/api/endpoints/ping.rs`, `src/groups/ping.rs`, `tests/cmd_ping.rs`

**Interfaces:**
- Consumes: `Ctx`, `Envelope`, `ApiClient::request`.
- Produces: `groups::ping::dispatch(&Ctx) -> anyhow::Result<()>`.

`GET /v2/ping` returns 200 with **no response body**, so the renderer reports reachability and the correlation id, not a payload.

- [ ] **Step 1: Write the failing test**

```rust
// tests/cmd_ping.rs
mod support;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn ping_sends_bearer_to_v2_ping_and_reports_reachable() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path("/v2/ping"))
        .and(header("authorization", "Bearer tok-xyz"))
        .respond_with(ResponseTemplate::new(200)
            .insert_header("x-correlation-id", "corr-ping"))
        .mount(&server).await;

    let out = support::bin(&server).args(["--output", "json", "ping"])
        .assert().success().get_output().stdout.clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["object"], "ping");
    assert_eq!(v["meta"]["correlation_id"], "corr-ping");
    assert_eq!(v["meta"]["environment"], "sandbox");
}
```

- [ ] **Step 2: Run and watch it fail**

Run: `cargo test --test cmd_ping`
Expected: FAIL — unrecognised subcommand `ping`.

- [ ] **Step 3: Implement the endpoint and dispatch**

```rust
// src/api/endpoints/ping.rs
impl crate::api::ApiClient {
    /// Returns the correlation id; `GET /v2/ping` answers 200 with no body.
    pub async fn ping(&self) -> Result<Option<String>, crate::api::ApiError> {
        let resp = self.request(reqwest::Method::GET, "/v2/ping", &[], None).await?;
        // 200 with no body is this endpoint's documented success.
        Ok(resp.correlation_id)
    }
}
```

- [ ] **Step 4: Run and watch it pass**

Run: `cargo test --test cmd_ping`
Expected: PASS.

- [ ] **Step 5: Add the conformance registration**

```rust
// in tests/cmd_ping.rs — the mock is driven by the contract fixture, so the
// bytes asserted here and the bytes validated by conformance are one value.
#[tokio::test]
async fn ping_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-get-ping", "default").await;
    support::bin(&server).args(["ping"]).assert().success();
    support::assert_exchange_observed(&server, &ex).await;
}
```

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "ARISE-4928 feat(ping): add ping"
```

---

### Task 10: `customers create` and `customers get`

**Files:**
- Create: `src/api/endpoints/customers.rs`, `src/groups/customers.rs`, `src/cli/address.rs`, `tests/cmd_customers.rs`

**Interfaces:**
- Produces: `build_create_customer_body(&CreateCustomerArgs) -> anyhow::Result<Value>`, `groups::customers::dispatch(&Ctx, CustomersCommand)`.

`CreateCustomerRequestDto` requires `firstName` and `lastName`. Optional: `companyName`, `email`, `mobilePhoneNumber`, `hasSmsConsent`, `shouldUseBillingAsShippingAddress`, `billingAddress`, `shippingAddress`.

- [ ] **Step 1: Write the failing builder tests**

```rust
// in src/groups/customers.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_minimal_body_with_only_required_fields() {
        let args = CreateCustomerArgs {
            first_name: "Ada".into(), last_name: "Lovelace".into(),
            email: None, company_name: None, mobile_phone_number: None,
            billing: Default::default(),
        };
        let body = build_create_customer_body(&args).unwrap();
        assert_eq!(body, serde_json::json!({
            "firstName": "Ada", "lastName": "Lovelace"
        }));
    }

    /// An absent flag must not become a null; the API distinguishes them.
    #[test]
    fn omits_optional_fields_entirely_when_absent() {
        let args = CreateCustomerArgs {
            first_name: "Ada".into(), last_name: "Lovelace".into(),
            email: Some("ada@example.com".into()),
            company_name: None, mobile_phone_number: None,
            billing: Default::default(),
        };
        let body = build_create_customer_body(&args).unwrap();
        assert!(body.get("companyName").is_none());
        assert_eq!(body["email"], "ada@example.com");
    }

    #[test]
    fn nests_billing_address_under_billing_address_key() {
        let args = CreateCustomerArgs {
            first_name: "Ada".into(), last_name: "Lovelace".into(),
            email: None, company_name: None, mobile_phone_number: None,
            billing: BillingArgs {
                line1: Some("1 Main St".into()), city: Some("Austin".into()),
                state: Some("TX".into()), postal_code: Some("78701".into()),
                country: Some("US".into()), line2: None },
        };
        let body = build_create_customer_body(&args).unwrap();
        assert_eq!(body["billingAddress"]["city"], "Austin");
        assert!(body["billingAddress"].get("line2").is_none());
    }
}
```

- [ ] **Step 2: Run and watch them fail**

Run: `cargo test customers`
Expected: FAIL — `build_create_customer_body` not found.

- [ ] **Step 3: Write the command→HTTP test**

```rust
// tests/cmd_customers.rs
mod support;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn create_posts_exact_body_to_v2_customers() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/customers"))
        .and(body_json(serde_json::json!({
            "firstName": "Ada", "lastName": "Lovelace", "email": "ada@example.com"
        })))
        .respond_with(ResponseTemplate::new(200)
            .set_body_json(serde_json::json!({"customerId": "cus_1"})))
        .mount(&server).await;

    support::bin(&server)
        .args(["--output", "quiet", "customers", "create",
               "--first-name", "Ada", "--last-name", "Lovelace",
               "--email", "ada@example.com"])
        .assert().success().stdout(predicates::str::contains("cus_1"));
}

#[tokio::test]
async fn get_renders_json_envelope_with_object_customer() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path("/v2/customers/cus_1"))
        .respond_with(ResponseTemplate::new(200)
            .set_body_json(serde_json::json!({"customerId": "cus_1", "firstName": "Ada"})))
        .mount(&server).await;

    let out = support::bin(&server)
        .args(["--output", "json", "customers", "get", "cus_1"])
        .assert().success().get_output().stdout.clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["object"], "customer");
    assert_eq!(v["data"]["customerId"], "cus_1");
}

/// A 404 on a read is not found; the idempotent-success rule covers deletes only.
#[tokio::test]
async fn get_on_missing_customer_exits_4() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET"))
        .and(path("/v2/customers/nope"))
        .respond_with(ResponseTemplate::new(404)
            .set_body_json(serde_json::json!({"Title":"Not found","CorrelationId":"c-9"})))
        .mount(&server).await;

    support::bin(&server).args(["customers", "get", "nope"]).assert().code(4);
}
```

- [ ] **Step 4: Implement the builder, endpoints, and renderers**

Add `create` and `get` only. `list`, `update`, and `delete` arrive in Task 15.

- [ ] **Step 5: Run and watch them pass**

Run: `cargo test customers`
Expected: PASS, 6 tests.

- [ ] **Step 6: Register conformance**

```rust
Add a `Contract` row for `flute-v2-post-customers` with a `minimal` variant
whose fixture is the body below; `every_variant_conforms_to_its_own_operation`
then validates the whole exchange with no per-group conformance test to write.
The command test consumes the same fixture:

```rust
// in tests/cmd_customers.rs
#[tokio::test]
async fn create_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(&server, "flute-v2-post-customers", "minimal").await;
    support::bin(&server).args([
        "customers", "create", "--first-name", "Ada", "--last-name", "Lovelace",
        "--email", "ada@example.com"]).assert().success();
    support::assert_exchange_observed(&server, &ex).await;
}
```

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "ARISE-4928 feat(customers): add create and get"
```

---

### Task 11: `transactions create` (new card, both capture methods) and `transactions get`

**Scope, stated because the previous draft contradicted itself.** This task owns exactly **two** of the five instrument variants: new card with automatic capture, and new card with manual capture. Saved card, new ACH, and saved ACH — with their validation rules, contract variants, surface rows, and parity rows — belong to Task 17, after `payment-methods` exists in Task 16 to create the instruments they reference.

The slice is meant to prove the architecture on the riskiest *thin* path, not to finish the group. Two variants already exercise money precision, PCI redaction of a real PAN, the `captureMethod` divergence, the single-object response, and all three output modes; adding ACH would add breadth without adding a new kind of risk, and it would need a saved payment method that does not exist yet.

Register both variants in the contract matrix now. Task 17 adds the remaining three.

The highest-risk task in the plan. It sends a real card number shape, exact decimals, and two fields the published schema does not document.

**Files:**
- Create: `src/api/endpoints/transactions.rs`, `src/groups/transactions.rs`, `tests/cmd_transactions.rs`

**Interfaces:**
- Produces: `build_create_transaction_body(&CreateTransactionArgs) -> anyhow::Result<Value>`, `validate_create_transaction(&CreateTransactionArgs) -> anyhow::Result<()>`.

Wire shape, from `CreateTransactionRequestDto` (required: `paymentProcessorId`, `baseAmount`, `transactionDetails`):

```json
{ "paymentProcessorId": "pp-1",
  "baseAmount": 10.50,
  "transactionDetails": {
    "cardData": {
      "captureMethod": "Auto",
      "paymentMethodDetails": {
        "cardNumber": "4111111111111111",
        "securityCode": "123",
        "expirationMonth": 12,
        "expirationYear": 2032 } } } }
```

**`captureMethod` is case-sensitive on the wire: `Auto` or `Manual`.** The `CaptureMethod` enum declares exactly those two values with `Auto` as the default. `--capture-method` accepts lowercase on the command line, as clap value-enums conventionally do, and maps to the capitalised wire value. Sending `"auto"` is not a near-miss the server forgives.

Two fields are **absent from the published schema** but real (D4, D5), and they are not the same kind of thing:

- `captureMethod` on `CardDataDto` — send it. It is the only way to create an authorization rather than an immediate charge, so omitting it removes a capability.
- `customerId` on the request — send it **only when `--customer-id` is given**. The observation records that it is *accepted* and links the transaction to a customer; nothing observed says it is required. Treating it as mandatory would force a customer record onto every charge.

Both are skipped in the conformance registration citing their defect IDs, and sent anyway: this CLI builds against the running implementation.

**Four instrument shapes, exactly one of which must be supplied.** The saved routes are not extras — v1 accepts `--payment-method-id` on four transaction commands, so omitting them regresses it.


| Shape      | Flags                                                     | Wire                            |
| ---------- | --------------------------------------------------------- | ------------------------------- |
| New card   | `--card`, `--cvv`, `--exp`                                | `cardData.paymentMethodDetails` |
| Saved card | `--payment-method-id`, `--instrument card`                | `cardData.paymentMethodId`      |
| New ACH    | `--ach-account-number`, `--ach-routing-number`, `--ach-*` | `achData.paymentMethodDetails`  |
| Saved ACH  | `--payment-method-id`, `--instrument ach`                 | `achData.paymentMethodId`       |


ACH carries requirements card does not. `AchDataDto` requires **`requesterIpAddress`** and **`secCode`** by schema, and a *new* ACH account additionally requires `billingAddress` and `contactInfo.mobilePhoneNumber` plus either a name pair or `companyName` (D2, a conditional rule OpenAPI cannot express). The saved-ACH route requires none of the latter. `--requester-ip` and `--sec-code` are required whenever ACH is selected.

- [ ] **Step 1: Write the failing validation tests**

```rust
// in src/groups/transactions.rs
#[cfg(test)]
mod tests {
    use super::*;

    /// A card charge that passes all four rules. Each test mutates one field
    /// so the assertion names the single reason the request is invalid.
    fn valid_args() -> CreateTransactionArgs {
        CreateTransactionArgs {
            payment_processor_id: "pp-1".into(),
            amount: "10.50".parse().unwrap(),
            customer_id: Some("cus_1".into()),
            capture_method: CaptureMethod::Auto,
            card: Some("4111111111111111".into()),
            cvv: Some("123".into()),
            exp: Some("12/2032".into()),
            ach_account_number: None,
            ach_routing_number: None,
            ach_account_type: None,
            reference_id: None,
            currency_code: None,
            billing: Default::default(),
        }
    }

    /// v1 sent requests with no payment instrument at all and the API
    /// answered 500. Four rules are checked before a round trip is spent.
    #[test]
    fn rejects_non_positive_base_amount() {
        let mut a = valid_args();
        a.amount = rust_decimal::Decimal::ZERO;
        assert!(validate_create_transaction(&a).unwrap_err().to_string().contains("greater than zero"));
    }

    #[test]
    fn rejects_missing_payment_processor_id_and_names_the_way_to_find_it() {
        let mut a = valid_args();
        a.payment_processor_id = String::new();
        let msg = validate_create_transaction(&a).unwrap_err().to_string();
        assert!(msg.contains("settings payment-config"));
    }

    #[test]
    fn rejects_both_card_and_ach() {
        let mut a = valid_args();
        a.ach_account_number = Some("000123456789".into());
        assert!(validate_create_transaction(&a).unwrap_err().to_string().contains("exactly one"));
    }

    #[test]
    fn rejects_neither_card_nor_ach() {
        let mut a = valid_args();
        a.card = None;
        assert!(validate_create_transaction(&a).unwrap_err().to_string().contains("exactly one"));
    }

    #[test]
    fn builds_card_body_with_undocumented_capture_method_and_customer_id() {
        let body = build_create_transaction_body(&valid_args()).unwrap();
        assert_eq!(body["transactionDetails"]["cardData"]["captureMethod"], "Auto");
        assert_eq!(body["customerId"], "cus_1");
        assert_eq!(body["transactionDetails"]["cardData"]["paymentMethodDetails"]["expirationMonth"], 12);
    }

    /// Amounts must survive as exact decimals, never as floats.
    #[test]
    fn base_amount_serializes_exactly() {
        let mut a = valid_args();
        a.amount = "1234567.89".parse().unwrap();
        let body = build_create_transaction_body(&a).unwrap();
        assert_eq!(serde_json::to_string(&body["baseAmount"]).unwrap(), "1234567.89");
    }
}
```

- [ ] **Step 2: Run and watch them fail**

Run: `cargo test transactions`
Expected: FAIL — items not found.

- [ ] **Step 3: Write the command→HTTP and decline tests**

```rust
// tests/cmd_transactions.rs
mod support;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn create_posts_the_documented_and_undocumented_fields() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/transactions"))
        .and(body_json(serde_json::json!({
            "paymentProcessorId": "pp-1",
            "baseAmount": 10.50,
            "transactionDetails": { "cardData": {
                "captureMethod": "Auto",
                "paymentMethodDetails": {
                    "cardNumber": "4111111111111111",
                    "securityCode": "123",
                    "expirationMonth": 12,
                    "expirationYear": 2032 } } }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "transactionId": "txn_1", "transactionStatus": "Captured",
            "processedAmount": 10.50, "currencyCode": "USD"
        })))
        .mount(&server).await;

    let out = support::bin(&server).args([
        "--output", "json", "transactions", "create",
        "--payment-processor-id", "pp-1", "--amount", "10.50",
        "--card", "4111111111111111", "--cvv", "123", "--exp", "12/2032",
        "--capture-method", "auto"])
        .assert().success().get_output().stdout.clone();

    // The response is a bare transaction object, not a page — see D11.
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["object"], "transaction");
    assert_eq!(v["data"]["transactionId"], "txn_1");
    assert!(v["meta"].get("page_info").is_none());
}

/// Transitional: the published schema says PageOfGetTransactionResponseDto.
/// If the server ever converges on it, a one-item page must still work.
/// Delete this arm and this test when D11 resolves.
#[tokio::test]
async fn create_also_accepts_a_one_item_page() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST")).and(path("/v2/transactions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [{"transactionId": "txn_1", "transactionStatus": "Captured"}],
            "pageInfo": {"pageIndex": 0, "pageSize": 1, "totalItems": 1,
                         "totalPages": 1, "hasMore": false}
        })))
        .mount(&server).await;

    let out = support::bin(&server).args([
        "--output", "json", "transactions", "create",
        "--payment-processor-id", "pp-1", "--amount", "10.50",
        "--card", "4111111111111111", "--cvv", "123", "--exp", "12/2032"])
        .assert().success().get_output().stdout.clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["data"]["transactionId"], "txn_1");
    assert!(v["meta"].get("page_info").is_none());
}

/// Under the transitional page arm, any count but one is a contract break
/// rather than an invitation to pick the first element.
#[tokio::test]
async fn create_returning_two_items_is_a_decode_error() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST")).and(path("/v2/transactions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [{"transactionId": "a"}, {"transactionId": "b"}]
        })))
        .mount(&server).await;

    support::bin(&server).args([
        "transactions", "create", "--payment-processor-id", "pp-1",
        "--amount", "1.00", "--card", "4111111111111111",
        "--cvv", "123", "--exp", "12/2032"])
        .assert().code(1);
}

/// A decline is HTTP 200 with a declined status. The caller reads
/// transactionStatus; the exit code stays 0.
#[tokio::test]
async fn declined_transaction_exits_zero() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST")).and(path("/v2/transactions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "transactionId": "txn_2", "transactionStatus": "Declined",
            "responseDetails": {"reason": "DeclineBinNotFound"}
        })))
        .mount(&server).await;

    let out = support::bin(&server).args([
        "--output", "json", "transactions", "create",
        "--payment-processor-id", "pp-1", "--amount", "1.00",
        "--card", "4111111111111111", "--cvv", "123", "--exp", "12/2032"])
        .assert().success().get_output().stdout.clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["data"]["transactionStatus"], "Declined");
}

/// Client-side validation spends no round trip.
#[tokio::test]
async fn missing_instrument_exits_3_without_calling_the_api() {
    let server = support::mock_with_token().await;
    support::bin(&server).args([
        "transactions", "create", "--payment-processor-id", "pp-1", "--amount", "1.00"])
        .assert().code(3);
    assert!(server.received_requests().await.unwrap()
        .iter().all(|r| r.url.path() == "/oauth2/token"));
}
```

- [ ] **Step 3b: Write the omission and negative tests**

A positive test cannot establish that a field is *optional*. Every conditional field needs a test proving the request succeeds without it — this is precisely how the false "`customerId` is required" claim survived review, and it is the cheapest layer in the plan.

```rust
/// customerId is accepted, not required (D5). Asserting only the present case
/// is what let the plan claim it was mandatory.
#[tokio::test]
async fn create_succeeds_without_customer_id_and_omits_the_field() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST")).and(path("/v2/transactions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "transactionId": "txn_1", "transactionStatus": "Captured"})))
        .mount(&server).await;

    support::bin(&server).args([
        "transactions", "create", "--payment-processor-id", "pp-1",
        "--amount", "1.00", "--card", "4111111111111111",
        "--cvv", "123", "--exp", "12/2032"])
        .assert().success();

    let reqs = server.received_requests().await.unwrap();
    let txn = reqs.iter().find(|r| r.url.path() == "/v2/transactions").unwrap();
    let body: serde_json::Value = serde_json::from_slice(&txn.body).unwrap();
    assert!(body.get("customerId").is_none(), "absent flag must not become a field");
}

// The saved-card, new-ACH and saved-ACH tests below belong to Task 17, not
// to the slice. They are written here so the shapes are settled while the
// evidence is fresh; move them with the variants when Task 17 lands.

/// A saved card carries no raw card fields at all.
#[tokio::test]
async fn saved_card_sends_payment_method_id_and_no_card_details() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST")).and(path("/v2/transactions"))
        .and(body_json(serde_json::json!({
            "paymentProcessorId": "pp-1", "baseAmount": 1.00,
            "transactionDetails": {"cardData": {"paymentMethodId": "pm_1"}}})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "transactionId": "txn_1", "transactionStatus": "Captured"})))
        .mount(&server).await;

    support::bin(&server).args([
        "transactions", "create", "--payment-processor-id", "pp-1",
        "--amount", "1.00", "--instrument", "card", "--payment-method-id", "pm_1"])
        .assert().success();
}

/// paymentMethodId and paymentMethodDetails are mutually exclusive.
#[tokio::test]
async fn saved_card_with_raw_card_fields_exits_3() {
    let server = support::mock_with_token().await;
    support::bin(&server).args([
        "transactions", "create", "--payment-processor-id", "pp-1",
        "--amount", "1.00", "--instrument", "card",
        "--payment-method-id", "pm_1", "--card", "4111111111111111"])
        .assert().code(3);
    assert!(server.received_requests().await.unwrap()
        .iter().all(|r| r.url.path() == "/oauth2/token"));
}

/// A new ACH account requires billing and contact information (D2); a saved
/// one does not. Both directions are asserted, because only the pair proves
/// the rule is conditional rather than universal.
#[tokio::test]
async fn new_ach_without_billing_or_contact_exits_3() {
    let server = support::mock_with_token().await;
    support::bin(&server).args([
        "transactions", "create", "--payment-processor-id", "pp-1",
        "--amount", "1.00", "--ach-account-number", "000123456789",
        "--ach-routing-number", "111000025", "--ach-account-type", "checking",
        "--requester-ip", "203.0.113.10", "--sec-code", "WEB"])
        .assert().code(3);
    assert!(server.received_requests().await.unwrap()
        .iter().all(|r| r.url.path() == "/oauth2/token"));
}

#[tokio::test]
async fn saved_ach_needs_no_billing_contact_or_account_details() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST")).and(path("/v2/transactions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "transactionId": "txn_1", "transactionStatus": "Pending"})))
        .mount(&server).await;

    support::bin(&server).args([
        "transactions", "create", "--payment-processor-id", "pp-1",
        "--amount", "1.00", "--instrument", "ach", "--payment-method-id", "pm_2",
        "--requester-ip", "203.0.113.10", "--sec-code", "WEB"])
        .assert().success();
}

/// ACH requires these two by schema regardless of new-versus-saved.
#[tokio::test]
async fn ach_without_requester_ip_or_sec_code_exits_3() {
    let server = support::mock_with_token().await;
    for extra in [vec!["--sec-code", "WEB"], vec!["--requester-ip", "203.0.113.10"]] {
        let mut args = vec!["transactions", "create", "--payment-processor-id", "pp-1",
            "--amount", "1.00", "--instrument", "ach", "--payment-method-id", "pm_2"];
        args.extend(extra);
        support::bin(&server).args(&args).assert().code(3);
    }
}
```

- [ ] **Step 4: Implement validation, builder, unwrapping, and renderers**

The envelope unwraps `items[0]` for the seven single-transaction writes and carries no `page_info` for them. `transactions get` returns the full schema (five extra fields); one renderer prints whichever fields are present.

- [ ] **Step 5: Run and watch them pass**

Run: `cargo test transactions`
Expected: PASS, 10 tests.

- [ ] **Step 6: Register conformance with the documented skip**

```rust
The `flute-v2-post-transactions` row already carries this variant's fixture,
including `captureMethod: "Auto"`. Conformance strips only the `D4` and `D5`
pointers before validating, so the undocumented fields are exempt by name and
everything else — including any *new* undocumented field — still fails.

There is no separate conformance test to write for this group. The command
test consumes the same fixture:

```rust
// in tests/cmd_transactions.rs
#[tokio::test]
async fn create_exchange_matches_the_contract() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server, "flute-v2-post-transactions", "new card, automatic capture").await;
    support::bin(&server).args([
        "--output", "json", "transactions", "create",
        "--payment-processor-id", "pp-1", "--amount", "10.50",
        "--card", "4111111111111111", "--cvv", "123", "--exp", "12/2032",
        "--capture-method", "auto"]).assert().success();
    support::assert_exchange_observed(&server, &ex).await;
}
```

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "ARISE-4928 feat(transactions): add card create and get"
```

---

### Task 12: The redaction guard, end to end

Layer 6. This is the test that must never be weakened.

**Files:**
- Create: `tests/redaction.rs`
- Source: v1 `tests/redaction_e2e.rs`, ported near-verbatim and extended

- [ ] **Step 1: Write the failing test**

```rust
// tests/redaction.rs
mod support;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

/// Under --debug the CLI traces the request. Nothing that would be a
/// compliance incident may appear in that trace.
#[tokio::test]
async fn debug_traces_never_leak_pan_cvv_token_or_secret() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST")).and(path("/v2/transactions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "transactionId": "txn_1", "transactionStatus": "Captured"})))
        .mount(&server).await;

    let out = support::bin(&server).args([
        "--debug", "transactions", "create",
        "--payment-processor-id", "pp-1", "--amount", "1.00",
        "--card", "4111111111111111", "--cvv", "8371", "--exp", "12/2032"])
        .assert().get_output().clone();

    let combined = format!("{}{}",
        String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));

    assert!(!combined.contains("4111111111111111"), "PAN leaked");
    assert!(!combined.contains("8371"), "CVV leaked");
    assert!(!combined.contains("securityCode"), "CVV field survived redaction");
    assert!(!combined.contains("tok-xyz"), "bearer token leaked");
    assert!(!combined.contains("test-secret"), "client secret leaked");
    assert!(combined.contains("1111"), "masking should keep the last four");
}
```

- [ ] **Step 2: Run and watch it fail**

Run: `cargo test --test redaction`
Expected: FAIL if any value reaches the sink. Fix `redact.rs` until it passes — never the assertion.

- [ ] **Step 3: Run and watch it pass**

Run: `cargo test --test redaction`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add tests/redaction.rs
git commit -m "ARISE-4928 test(api): end-to-end redaction guard"
```

---

### Task 13: Live sandbox verification of the slice

Layer 5. **The test logic is committed**; only the account values are local.

**Files:**
- Create: `tests/live.rs`, `tests/live/mod.rs`, `tests/live/slice.rs`, `.flute2-live.env.example`
- Modify: `.gitignore` (add `.flute2-live.env` only)

Uncommitted tests cannot be a development guarantee: nothing stops them rotting, and the executable record of what the API accepts would live on one machine. Credentials already solve this problem with environment variables — if that is good enough for a client secret, it is good enough for a processor id.

- [ ] **Step 1: Write the parameter loader**

```rust
// tests/live/mod.rs — COMMITTED
pub mod slice;

/// Sandbox identifiers come from the environment. Nothing account-specific is
/// ever written into this repository, which is public.
pub fn required(var: &str) -> String {
    std::env::var(var).unwrap_or_else(|_| panic!(
        "{var} is not set. Copy .flute2-live.env.example to \
         .flute2-live.env, fill in sandbox values, and source it."))
}

pub fn processor_id() -> String { required("FLUTE2_LIVE_PROCESSOR_ID") }
pub fn merchant_id() -> String  { required("FLUTE2_LIVE_MERCHANT_ID") }
pub fn terminal_id() -> String  { required("FLUTE2_LIVE_TERMINAL_ID") }
pub fn pos_device_id() -> String { required("FLUTE2_LIVE_POS_DEVICE_ID") }

/// D12: `referenceId` is part of the duplicate-check key, and the spec states
/// that differing reference ids let the same card and amount be charged again.
/// So the dependable way to avoid a false duplicate is a fresh reference id,
/// not a fresh amount — amounts are a workaround for endpoints that take no
/// reference id.
pub fn unique_reference_id() -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static SEQ: AtomicU32 = AtomicU32::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    format!("flute2-live-{nanos}-{}", SEQ.fetch_add(1, Ordering::Relaxed))
}

/// Amounts still need to differ where no reference id is accepted.
pub fn unique_amount() -> rust_decimal::Decimal {
    use std::sync::atomic::{AtomicU32, Ordering};
    // 10.00 .. 999.99 — above trivial floors, below any sandbox ceiling.
    const LO: u32 = 1_000;
    const HI: u32 = 99_999;
    static NEXT: std::sync::OnceLock<AtomicU32> = std::sync::OnceLock::new();

    let counter = NEXT.get_or_init(|| {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() as u32;
        AtomicU32::new(LO + seed % (HI - LO))
    });
    let cents = LO + (counter.fetch_add(1, Ordering::Relaxed) - LO) % (HI - LO);
    format!("{}.{:02}", cents / 100, cents % 100).parse().unwrap()
}
```

```sh
# .flute2-live.env.example — COMMITTED, contains no real values
# Copy to .flute2-live.env, fill in sandbox values, then: source .flute2-live.env
export FLUTE2_LIVE_PROCESSOR_ID=00000000-0000-0000-0000-000000000000
export FLUTE2_LIVE_MERCHANT_ID=00000000-0000-0000-0000-000000000000
export FLUTE2_LIVE_TERMINAL_ID=00000000-0000-0000-0000-000000000000
export FLUTE2_LIVE_POS_DEVICE_ID=00000000-0000-0000-0000-000000000000
```

A shell file rather than TOML, because the helper reads the environment and nothing parses a config file. `KEY = "value"` is not shell assignment and `source` would not export it — the instruction and the format have to agree, or the first person to run the suite hits a panic telling them to do something that does not work.

```
# .gitignore — only the filled-in file is ignored
.flute2-live.env
```

- [ ] **Step 2: Confirm the example file leaks nothing**

Run: `git check-ignore -v .flute2-live.env && grep -c '0000-0000' .flute2-live.env.example`
Expected: the local file is ignored; the example holds only zero UUIDs. **The test sources are committed and must contain no identifier, hostname, or account value.**

- [ ] **Step 3: Write the slice's live tests**

```rust
// tests/live/slice.rs — COMMITTED
use super::*;

#[tokio::test]
#[ignore = "live sandbox; opt in with --ignored"]
async fn live_ping_succeeds() { /* GET /v2/ping, assert 200 */ }

/// Creates its own customer and deletes it, so no long-lived id is assumed.
#[tokio::test]
#[ignore = "live sandbox; opt in with --ignored"]
async fn live_customer_create_get_delete() { /* … */ }

/// Named by the contract matrix as the evidence for the new-card variant.
#[tokio::test]
#[ignore = "live sandbox; opt in with --ignored"]
async fn live_card_sale_auto_capture() {
    let _reference = unique_reference_id();   // D12: the documented dedup key
    let _amount = unique_amount();
    let _processor = processor_id();
    // POST /v2/transactions, assert 200 and a single transaction object (D11)
}

/// Evidence for D4: captureMethod is absent from the schema and accepted.
#[tokio::test]
#[ignore = "live sandbox; opt in with --ignored"]
async fn live_card_auth_manual_capture() { /* captureMethod: "Manual" -> Authorization */ }

/// Evidence for D5: customerId is absent from the schema and accepted.
#[tokio::test]
#[ignore = "live sandbox; opt in with --ignored"]
async fn live_card_sale_with_customer() { /* … */ }

/// Evidence for D15: does capture take captureAmount or amount?
#[tokio::test]
#[ignore = "live sandbox; opt in with --ignored"]
async fn live_partial_capture_field_name() { /* try captureAmount first */ }
```

Every live transaction sends a fresh `--reference-id`, which the spec names as part of the duplicate-check key. Transaction bodies are **append-only**: a reversal is a new transaction, not a deletion, so these accumulate sandbox state permanently. Everything else — customers, payment methods, payment links, api-keys — creates what it needs and deletes it in the same test.

- [ ] **Step 4: Run them by hand**

Run: `cargo test --test live -- --ignored`
Record what the API accepted in `NOTES.local.md`, and settle D15 here.

- [ ] **Step 5: Confirm a plain run stays hermetic**

Run: `cargo test`
Expected: the live tests report as ignored; no network traffic, and no `FLUTE2_LIVE_*` needed to compile or run the rest.

- [ ] **Step 6: File every divergence, then commit**

```bash
git add tests/ .gitignore .flute2-live.env.example
git commit -m "ARISE-4928 test(cli): committed live sandbox scenarios for the slice"
```

---

### Task 14: Slice review gate

- [ ] **Step 1: Run the full gate**

Run: `cargo test && cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: PASS. Record the totals.

- [ ] **Step 2: Confirm every layer touched the slice**

Layer 1 contract matrix (five operations built — ping, customers create/get, transactions create/get — across three groups; the rest pending) · layer 2 conformance (validated from those same fixtures) · layer 3 command→HTTP (ping, customers, transactions) · layer 4a surface and 4b parity (rows for the built operations) · layer 5 live (Task 13) · layer 6 invariants, redaction (Task 12) · layer 7 snapshots, deferred to Task 25 — note the deferral in the PR.

Module unit tests (money, redact, error, output, builders) are present too, but they are not one of the seven layers and do not count as coverage of an operation.

- [ ] **Step 3: Open the pull request**

Title: `ARISE-4928 feat(cli): vertical slice — ping, customers, card transactions`. Link the Jira issue at the top of the description. Paste the test command and its totals.

- [ ] **Step 4: Stop and get review before Phase 2.**

---

