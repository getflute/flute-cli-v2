# Phase 2 — One group per pull request

> Part of the [flute-cli-v2 implementation plan](00-overview.md). Read [`00-overview.md`](00-overview.md) first — its global constraints and testing contract apply to every task here.

> **This phase is deliberately thin.** It is rewritten as a compact per-group checklist after the slice review gate (Task 14), once the harness has proven what detail is actually needed. Do not treat the current prose as complete.

# Phase 2 — One group per pull request

Every task below follows the same nine-point definition of done from The Testing Contract. Dependency order is fixed: `payment-methods` before the rest of `transactions`, because saved-instrument charges depend on it.

### Task 15: Complete `customers`

**Files:** Modify `src/groups/customers.rs`, `src/api/endpoints/customers.rs`, `tests/cmd_customers.rs`

| Command | Method | Path |
|---|---|---|
| `list` | GET | `/v2/customers` |
| `update` | PATCH | `/v2/customers/{customerId}` |
| `delete` | DELETE | `/v2/customers/{customerId}` |

- [ ] **Step 1: Write the failing tests**

```rust
// in tests/cmd_customers.rs
#[tokio::test]
async fn list_sends_page_index_and_page_size_as_query_params() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET")).and(path("/v2/customers"))
        .and(wiremock::matchers::query_param("pageIndex", "2"))
        .and(wiremock::matchers::query_param("pageSize", "50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [], "pageInfo": {"pageIndex": 2, "pageSize": 50,
                "totalItems": 0, "totalPages": 0, "hasMore": false}})))
        .mount(&server).await;

    support::bin(&server)
        .args(["customers", "list", "--page-index", "2", "--page-size", "50"])
        .assert().success();
}

/// Absent flags mean the server's own defaults govern.
#[tokio::test]
async fn list_omits_pagination_params_when_flags_absent() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET")).and(path("/v2/customers"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [], "pageInfo": {"hasMore": false}})))
        .mount(&server).await;

    support::bin(&server).args(["customers", "list"]).assert().success();
    let req = &server.received_requests().await.unwrap()
        .iter().find(|r| r.url.path() == "/v2/customers").unwrap().url.clone();
    assert!(req.query().is_none_or(|q| !q.contains("pageSize")));
}

/// The API bounds pageSize to 1..=100; spend no round trip proving it.
#[tokio::test]
async fn page_size_over_one_hundred_exits_3() {
    let server = support::mock_with_token().await;
    support::bin(&server).args(["customers", "list", "--page-size", "101"])
        .assert().code(3);
}

/// --all exhausts the collection by following hasMore.
#[tokio::test]
async fn all_follows_has_more_until_exhausted() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET")).and(path("/v2/customers"))
        .and(wiremock::matchers::query_param("pageIndex", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [{"customerId": "c1"}],
            "pageInfo": {"pageIndex": 0, "hasMore": true}})))
        .mount(&server).await;
    Mock::given(method("GET")).and(path("/v2/customers"))
        .and(wiremock::matchers::query_param("pageIndex", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [{"customerId": "c2"}],
            "pageInfo": {"pageIndex": 1, "hasMore": false}})))
        .mount(&server).await;

    let out = support::bin(&server).args(["--output", "json", "customers", "list", "--all"])
        .assert().success().get_output().stdout.clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["data"].as_array().unwrap().len(), 2);
}

/// No pageInfo field is required, so an absent hasMore terminates the walk.
#[tokio::test]
async fn all_stops_when_has_more_is_absent() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET")).and(path("/v2/customers"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [{"customerId": "c1"}], "pageInfo": {"pageIndex": 0}})))
        .mount(&server).await;

    support::bin(&server).args(["customers", "list", "--all"]).assert().success();
    assert_eq!(server.received_requests().await.unwrap()
        .iter().filter(|r| r.url.path() == "/v2/customers").count(), 1);
}

/// Re-deleting an already-deleted customer is success.
#[tokio::test]
async fn delete_on_missing_customer_exits_zero() {
    let server = support::mock_with_token().await;
    Mock::given(method("DELETE")).and(path("/v2/customers/gone"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server).await;

    support::bin(&server).args(["customers", "delete", "gone", "--yes"]).assert().success();
}

/// The confirmation gate is client-side: refusing must cost no request.
#[tokio::test]
async fn delete_without_yes_exits_3_and_sends_nothing() {
    let server = support::mock_with_token().await;
    support::bin(&server).args(["customers", "delete", "cus_1"]).assert().code(3);
    assert!(server.received_requests().await.unwrap()
        .iter().all(|r| r.url.path() == "/oauth2/token"));
}
```

- [ ] **Step 2: Run and watch them fail**

Run: `cargo test --test cmd_customers`
Expected: FAIL — subcommands not implemented.

- [ ] **Step 3: Implement `list`, `update`, `delete`, plus shared pagination args in `src/cli/common.rs`**

`common.rs` holds `PaginationArgs { page_index: Option<u32>, page_size: Option<u32>, all: bool }` and the `--all` walk. Every group's `list` reuses it — this is the one place the walk is written.

- [ ] **Step 4: Run and watch them pass**

Run: `cargo test --test cmd_customers`
Expected: PASS.

- [ ] **Step 5: Apply the nine-point definition of done, then commit**

```bash
git add -A
git commit -m "ARISE-4928 feat(customers): add list, update, delete"
```

---

### Task 16: `payment-methods` (7 operations)

**Files:** Create `src/api/endpoints/payment_methods.rs`, `src/groups/payment_methods.rs`, `tests/cmd_payment_methods.rs`

| Command | Method | Path |
|---|---|---|
| `list` | GET | `/v2/payment-methods` |
| `get` | GET | `/v2/payment-methods/{paymentMethodId}` |
| `add-card` | POST | `/v2/payment-methods/cards` |
| `add-ach` | POST | `/v2/payment-methods/ach` |
| `update` | PATCH | `/v2/payment-methods/{paymentMethodId}` |
| `delete` | DELETE | `/v2/payment-methods/{paymentMethodId}` |
| `set-default` | POST | `/v2/payment-methods/{paymentMethodId}/set-default` |

- [ ] **Step 1: Write the failing command→HTTP test**

```rust
// tests/cmd_payment_methods.rs
mod support;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn add_card_posts_to_the_cards_subpath() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/payment-methods/cards"))
        .and(body_json(serde_json::json!({
            "customerId": "cus_1",
            "cardNumber": "4111111111111111",
            "securityCode": "123",
            "expirationMonth": 12,
            "expirationYear": 2032
        })))
        .respond_with(ResponseTemplate::new(200)
            .set_body_json(serde_json::json!({"paymentMethodId": "pm_1"})))
        .mount(&server).await;

    support::bin(&server).args([
        "--output", "quiet", "payment-methods", "add-card",
        "--customer-id", "cus_1", "--card", "4111111111111111",
        "--cvv", "123", "--exp", "12/2032"])
        .assert().success().stdout(predicates::str::contains("pm_1"));
}

/// customerId is a *required query parameter*, and the success carries no
/// body at all — so this exercises the bodyless path as well as the query.
#[tokio::test]
async fn set_default_sends_the_customer_id_query_and_tolerates_no_body() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST"))
        .and(path("/v2/payment-methods/pm_1/set-default"))
        .and(wiremock::matchers::query_param("customerId", "cus_1"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server).await;

    support::bin(&server)
        .args(["payment-methods", "set-default", "pm_1", "--customer-id", "cus_1"])
        .assert().success();
}

#[tokio::test]
async fn set_default_without_customer_id_exits_3() {
    let server = support::mock_with_token().await;
    support::bin(&server).args(["payment-methods", "set-default", "pm_1"])
        .assert().code(3);
}

#[tokio::test]
async fn delete_on_missing_method_exits_zero() {
    let server = support::mock_with_token().await;
    Mock::given(method("DELETE")).and(path("/v2/payment-methods/gone"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server).await;
    support::bin(&server).args(["payment-methods", "delete", "gone", "--yes"]).assert().success();
}

#[tokio::test]
async fn delete_without_yes_exits_3_and_sends_nothing() {
    let server = support::mock_with_token().await;
    support::bin(&server).args(["payment-methods", "delete", "pm_1"]).assert().code(3);
    assert!(server.received_requests().await.unwrap()
        .iter().all(|r| r.url.path() == "/oauth2/token"));
}
```

- [ ] **Step 2: Run, fail, implement, pass**

Run: `cargo test --test cmd_payment_methods` — FAIL, then implement all seven, then PASS.

- [ ] **Step 3: Add builder unit tests for `add-card` and `add-ach`**

Cover every flag that reaches the wire and omission when absent, mirroring Task 10's shape.

- [ ] **Step 4: Register both payment-method request shapes in conformance**

`CreateCardPaymentMethodRequestDto` (requires `cardNumber`, `expirationMonth`, `expirationYear`) **and** `CreateAchPaymentMethodRequestDto` (requires `routingNumber`, `accountNumber`, `accountHolderType`, `accountType`). Registering only the card shape leaves the ACH body unchecked, which is where the unfamiliar required fields are.

- [ ] **Step 5: Apply the nine-point checklist, commit**

```bash
git commit -am "ARISE-4928 feat(payment-methods): add all seven operations"
```

---

### Task 17: Remaining `transactions` operations (9)

**Files:** Modify `src/groups/transactions.rs`, `src/api/endpoints/transactions.rs`, `tests/cmd_transactions.rs`

| Command | Method | Path |
|---|---|---|
| `list` | GET | `/v2/transactions` |
| `capture` | POST | `/v2/transactions/{transactionId}/capture` |
| `reversal` | POST | `/v2/transactions/{transactionId}/reversal` |
| `credit` | POST | `/v2/transactions/credit` |
| `tip-adjust` | POST | `/v2/transactions/{transactionId}/tip-adjustment` |
| `ach-hold` | POST | `/v2/transactions/{transactionId}/ach-hold` |
| `ach-release` | POST | `/v2/transactions/{transactionId}/ach-release` |
| `calculate-amount` | POST | `/v2/transactions/calculate-amount` |
| `share-receipt` | POST | `/v2/transactions/{transactionId}/share-receipt` |
| `inspect` | — | client-side detailed view |

`reversal` replaces v1's `void` and `refund`: one endpoint, with payment method and settled state auto-detected server-side. ACH create folds into `transactions create --ach-*`.

**Capture is a documented inconsistency — resolve it before writing the builder.** `CaptureRequestDto` declares one property, `captureAmount`, with `additionalProperties: false`; the capture operation's own request example says `{"amount": 50}`. The two cannot both be right, and `additionalProperties: false` means the wrong choice is rejected rather than ignored. **Step 0 of this task is a live sandbox capture** against a real authorization, trying `captureAmount` first. Record the result in `NOTES.local.md`, file the inconsistency in `docs/doc-defects.md`, and only then write the test. The plan assumes `captureAmount` because the schema is normative and the example is not, but that assumption is exactly what the live call exists to check.

- [ ] **Step 1: Write the failing tests**

```rust
// in tests/cmd_transactions.rs
#[tokio::test]
async fn reversal_posts_to_the_transaction_subpath_with_no_verb_flag() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST")).and(path("/v2/transactions/txn_1/reversal"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "transactionId": "txn_1", "transactionStatus": "Reversed"})))
        .mount(&server).await;

    support::bin(&server).args(["transactions", "reversal", "txn_1"]).assert().success();
}

/// CaptureRequestDto's only property is `captureAmount`, and the schema sets
/// additionalProperties:false — so sending `amount` is rejected, not ignored.
/// The operation's own example still says {"amount": 50}; the schema wins.
#[tokio::test]
async fn partial_capture_sends_capture_amount_as_an_exact_decimal() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST")).and(path("/v2/transactions/txn_1/capture"))
        .and(body_json(serde_json::json!({"captureAmount": 5.25})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "transactionId": "txn_1", "transactionStatus": "Captured"})))
        .mount(&server).await;

    support::bin(&server).args(["transactions", "capture", "txn_1", "--amount", "5.25"])
        .assert().success();
}

/// A full capture sends an empty body rather than the original amount.
#[tokio::test]
async fn full_capture_omits_the_capture_amount_field() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST")).and(path("/v2/transactions/txn_1/capture"))
        .and(body_json(serde_json::json!({})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "transactionId": "txn_1", "transactionStatus": "Captured"})))
        .mount(&server).await;

    support::bin(&server).args(["transactions", "capture", "txn_1"]).assert().success();
}

/// ACH folds into create; exactly one of card or ACH still holds.
#[tokio::test]
async fn ach_create_nests_under_ach_data() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST")).and(path("/v2/transactions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "transactionId": "txn_3", "transactionStatus": "Pending"})))
        .mount(&server).await;

    // requesterIpAddress and secCode are required by AchDataDto, and a new
    // ACH account also needs billing and contact information (D2).
    support::bin(&server).args([
        "transactions", "create", "--payment-processor-id", "pp-1",
        "--amount", "20.00", "--ach-account-number", "000123456789",
        "--ach-routing-number", "111000025", "--ach-account-type", "checking",
        "--requester-ip", "203.0.113.10", "--sec-code", "WEB",
        "--billing-line1", "1 Main St", "--billing-city", "Austin",
        "--billing-state", "TX", "--billing-postal-code", "78701",
        "--billing-country", "US", "--contact-first-name", "Ada",
        "--contact-last-name", "Lovelace", "--contact-mobile", "+15125550123"])
        .assert().success();

    let reqs = server.received_requests().await.unwrap();
    let txn = reqs.iter().find(|r| r.url.path() == "/v2/transactions").unwrap();
    let body: serde_json::Value = serde_json::from_slice(&txn.body).unwrap();
    assert!(body["transactionDetails"]["achData"].is_object());
    assert!(body["transactionDetails"].get("cardData").is_none());
}
```

- [ ] **Step 2: Run, fail, implement all nine plus `inspect`, pass**

Run: `cargo test --test cmd_transactions`

- [ ] **Step 3: Apply the nine-point checklist, commit**

```bash
git commit -am "ARISE-4928 feat(transactions): add remaining nine operations"
```

---

### Task 18: `pos` (5 operations)

**Files:** Create `src/api/endpoints/pos.rs`, `src/groups/pos.rs`, `tests/cmd_pos.rs`

| Command | Method | Path |
|---|---|---|
| `create` (`--wait`) | POST | `/v2/pos/transactions` |
| `get` | GET | `/v2/pos/transactions/{posTransactionId}` |
| `list` | GET | `/v2/pos/transactions` |
| `cancel` | POST | `/v2/pos/transactions/{posTransactionId}/cancel` — requires `--yes` |
| `print-receipt` | POST | `/v2/pos/transactions/{posTransactionId}/print-receipt` |

`--wait` sets `waitForAcceptanceByTerminal` on the **create** request and then polls `GET /v2/pos/transactions/{id}`. It is not a flag on `get`. `--wait-timeout` defaults to 120 seconds, as in v1.

`CreatePosTransactionRequestDto` requires four fields — `terminalId`, `posDeviceId`, `baseAmount`, `currencyCode` — so `--terminal-id`, `--pos-device-id`, `--amount`, and `--currency-code` are all `required = true`. This differs from v1, which additionally forced `--reference-id` because the v1 API rejected creates without it; v2 does not declare it required, so it stays optional here and any rejection becomes a filed defect rather than a silently reinstated flag.

- [ ] **Step 1: Write the failing tests**

```rust
// tests/cmd_pos.rs
mod support;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn wait_sets_the_long_polling_field_on_create() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST")).and(path("/v2/pos/transactions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "posTransactionId": "pos_1", "posTransactionStatus": "Completed"})))
        .mount(&server).await;

    support::bin(&server).args([
        "pos", "create", "--terminal-id", "t1", "--pos-device-id", "d1",
        "--currency-code", "USD", "--amount", "5.00", "--wait"])
        .assert().success();

    let reqs = server.received_requests().await.unwrap();
    let create = reqs.iter().find(|r| r.url.path() == "/v2/pos/transactions").unwrap();
    let body: serde_json::Value = serde_json::from_slice(&create.body).unwrap();
    assert_eq!(body["waitForAcceptanceByTerminal"], true);
}

#[tokio::test]
async fn without_wait_the_field_is_false() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST")).and(path("/v2/pos/transactions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "posTransactionId": "pos_1", "posTransactionStatus": "InProgress"})))
        .mount(&server).await;

    support::bin(&server).args(["pos", "create", "--terminal-id", "t1", "--pos-device-id", "d1",
               "--currency-code", "USD", "--amount", "5.00"])
        .assert().success();

    let reqs = server.received_requests().await.unwrap();
    let create = reqs.iter().find(|r| r.url.path() == "/v2/pos/transactions").unwrap();
    let body: serde_json::Value = serde_json::from_slice(&create.body).unwrap();
    assert_eq!(body["waitForAcceptanceByTerminal"], false);
}

/// The API forbids long polling on a Deeplink channel; catch it locally.
#[tokio::test]
async fn wait_with_deeplink_channel_exits_3_without_calling_the_api() {
    let server = support::mock_with_token().await;
    support::bin(&server).args([
        "pos", "create", "--terminal-id", "t1", "--pos-device-id", "d1",
        "--currency-code", "USD", "--amount", "5.00",
        "--initiation-channel", "deeplink", "--wait"])
        .assert().code(3);
    assert!(server.received_requests().await.unwrap()
        .iter().all(|r| r.url.path() == "/oauth2/token"));
}

/// On timeout: last-known envelope to stdout, warning to stderr, exit 1.
#[tokio::test]
async fn wait_timeout_prints_last_known_state_and_exits_1() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST")).and(path("/v2/pos/transactions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "posTransactionId": "pos_1", "posTransactionStatus": "InProgress"})))
        .mount(&server).await;
    Mock::given(method("GET")).and(path("/v2/pos/transactions/pos_1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "posTransactionId": "pos_1", "posTransactionStatus": "InProgress"})))
        .mount(&server).await;

    support::bin(&server).args([
        "--output", "json", "pos", "create", "--terminal-id", "t1",
        "--pos-device-id", "d1", "--currency-code", "USD",
        "--amount", "5.00", "--wait", "--wait-timeout", "1"])
        .assert().code(1)
        .stdout(predicates::str::contains("pos_1"))
        .stderr(predicates::str::contains("wait-timeout"));
}
```

**`--wait` drives two distinct API controls, not one.** v2 exposes `waitForAcceptanceByTerminal` on the create *body* and `waitForTransactionProcessing` on the get *query*; they are different mechanisms and the plan must set both. Create-time acceptance holds the create response until the terminal accepts or declines; get-time processing holds each poll open until the state changes. With only the first, the CLI busy-polls for the result; with only the second, create returns before the terminal has acknowledged anything.

```rust
#[tokio::test]
async fn wait_uses_the_long_poll_query_on_each_get() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST")).and(path("/v2/pos/transactions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "posTransactionId": "pos_1", "posTransactionStatus": "InProgress"})))
        .mount(&server).await;
    Mock::given(method("GET")).and(path("/v2/pos/transactions/pos_1"))
        .and(wiremock::matchers::query_param("waitForTransactionProcessing", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "posTransactionId": "pos_1", "posTransactionStatus": "Completed"})))
        .mount(&server).await;

    support::bin(&server).args([
        "pos", "create", "--terminal-id", "t1", "--pos-device-id", "d1",
        "--currency-code", "USD", "--amount", "5.00", "--wait"])
        .assert().success();
}
```

Without `--wait`, the create body carries `false` and the get sends no long-poll query.

**The poll terminates on status, not on a boolean.** v2's create and get responses are the same schema, both carrying `posTransactionId` and `posTransactionStatus`, whose enum is exactly `InProgress`, `Completed`, `Cancelled`, `Failed`. The loop continues only while the status is `InProgress` and stops on the other three. v1's `isCompleted` boolean, its `id`/`posTransactionId` split between get and create shapes, and its `"Pending"` status value **do not exist in v2** — v1's `pos_id()` helper, which reconciled the two id fields, has nothing left to reconcile and is not transplanted. An unrecognised status value is a decode error rather than a reason to keep polling forever.

Add a unit test for the terminating predicate covering all four values plus an unknown one, so the loop's exit condition is provable without waiting on a timer.

`pos cancel` is destructive and takes `--yes`, with the same pair of tests every gated command carries:

```rust
#[tokio::test]
async fn cancel_with_yes_posts_to_the_cancel_subpath() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST")).and(path("/v2/pos/transactions/pos_1/cancel"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "posTransactionId": "pos_1", "posTransactionStatus": "Cancelled"})))
        .mount(&server).await;

    support::bin(&server).args(["pos", "cancel", "pos_1", "--yes"]).assert().success();
}

#[tokio::test]
async fn cancel_without_yes_exits_3_and_sends_nothing() {
    let server = support::mock_with_token().await;
    support::bin(&server).args(["pos", "cancel", "pos_1"]).assert().code(3);
    assert!(server.received_requests().await.unwrap()
        .iter().all(|r| r.url.path() == "/oauth2/token"));
}
```

Ctrl-C (exit 130, stdout empty in every mode) is not reachable from `assert_cmd`; cover it in `tests/live/` and state the gap in the PR.

- [ ] **Step 2: Run, fail, implement, pass**

Run: `cargo test --test cmd_pos`

- [ ] **Step 3: Apply the nine-point checklist, commit**

```bash
git commit -am "ARISE-4928 feat(pos): add all five operations"
```

---

### Task 19: `terminals` (2 operations)

**Files:** Create `src/api/endpoints/terminals.rs`, `src/groups/terminals.rs`, `tests/cmd_terminals.rs`

| Command | Method | Path |
|---|---|---|
| `list` | GET | `/v2/terminals` |
| `status` | GET | `/v2/terminals/{terminalId}/status` |

- [ ] **Step 1: Write the failing test**

```rust
// tests/cmd_terminals.rs
mod support;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn status_hits_the_status_subpath() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server, "flute-v2-get-terminals-terminalId-status", "default").await;

    let out = support::bin(&server).args(["--output", "json", "terminals", "status", "t1"])
        .assert().success().get_output().stdout.clone();
    support::assert_exchange_observed(&server, &ex).await;
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    // Four distinct status-ish fields, each with its own enum. `status` is
    // not one of them, and none of them takes the value "Online" except
    // connectionStatus.
    assert_eq!(v["data"]["terminalStatus"], "Active");
    assert_eq!(v["data"]["connectionStatus"], "Online");
}
```

- [ ] **Step 2: Run, fail, implement both, pass; apply the checklist, commit**

```bash
git commit -am "ARISE-4928 feat(terminals): add list and status"
```

---

### Task 20: `settlements` (2 endpoints + 1 client-side)

**Files:** Create `src/api/endpoints/settlements.rs`, `src/groups/settlements.rs`, `tests/cmd_settlements.rs`

| Command | Method | Path |
|---|---|---|
| `list` | GET | `/v2/settlements/batches` |
| `close` | POST | `/v2/settlements/batches/close` |
| `get` | GET | `/v2/settlements/batches?batchIds={id}` |

`close` replaces v1's `transactions settle`, and requires `--payment-processor-id` because `SettleTransactionsRequestDto` requires `paymentProcessorId`.

`get` has no dedicated endpoint. It is the list endpoint with the documented `batchIds` filter applied server-side, and **says so in its help text**. It does not fetch page 0 and filter locally: a batch beyond the first page would report not-found, which is a wrong answer rather than a slow one.

- [ ] **Step 1: Write the failing tests**

```rust
// tests/cmd_settlements.rs
mod support;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

/// There is no single-batch endpoint, but the list endpoint documents a
/// batchIds filter. Pushing the filter to the server is what keeps a batch
/// on page 7 from reporting not-found.
#[tokio::test]
async fn get_sends_the_batch_id_as_a_server_side_filter() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET")).and(path("/v2/settlements/batches"))
        .and(wiremock::matchers::query_param("batchIds", "b2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [{"batchId": "b2"}], "pageInfo": {"hasMore": false}})))
        .mount(&server).await;

    let out = support::bin(&server).args(["--output", "json", "settlements", "get", "b2"])
        .assert().success().get_output().stdout.clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["object"], "settlement_batch");
    assert_eq!(v["data"]["batchId"], "b2");
}

#[tokio::test]
async fn get_on_an_unknown_batch_exits_4() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET")).and(path("/v2/settlements/batches"))
        .and(wiremock::matchers::query_param("batchIds", "nope"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [], "pageInfo": {"hasMore": false}})))
        .mount(&server).await;

    support::bin(&server).args(["settlements", "get", "nope"]).assert().code(4);
}

/// close carries a body: SettleTransactionsRequestDto requires
/// paymentProcessorId, so --payment-processor-id is required here too.
#[tokio::test]
async fn close_posts_the_processor_id_to_the_close_subpath() {
    let server = support::mock_with_token().await;
    // The fixture carries {"paymentProcessorId": "pp-1"}; conformance validates
    // it and the mock matches on it, from the one value.
    let ex = support::mount(
        &server, "flute-v2-post-settlements-batches-close", "default").await;

    // SettleTransactionsResponseDto declares one property, batchStatus.
    // Neither batchId nor status exists on it.
    support::bin(&server)
        .args(["settlements", "close", "--payment-processor-id", "pp-1"])
        .assert().success();
    support::assert_exchange_observed(&server, &ex).await;
}

#[tokio::test]
async fn close_without_a_processor_id_exits_3() {
    let server = support::mock_with_token().await;
    support::bin(&server).args(["settlements", "close"]).assert().code(3);
}
```

- [ ] **Step 2: Run, fail, implement, pass; apply the checklist, commit**

```bash
git commit -am "ARISE-4928 feat(settlements): add list, close, and client-side get"
```

---

### Task 21: `settings` (4 operations)

**Files:** Create `src/api/endpoints/settings.rs`, `src/groups/settings.rs`, `tests/cmd_settings.rs`

| Command | Method | Path |
|---|---|---|
| `payment-config` | GET | `/v2/settings/payment-config` |
| `contact-info` | GET | `/v2/settings/contact-information` |
| `autofill` | GET | `/v2/settings/transaction-autofill` |
| `update-autofill` | PATCH | `/v2/settings/transaction-autofill` |

`payment-config` is the command the missing-`--payment-processor-id` error names, so its table output must show processor ids prominently.

- [ ] **Step 1: Write the failing test**

```rust
// tests/cmd_settings.rs
mod support;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

/// This is where a user finds the id that every transaction requires.
#[tokio::test]
async fn payment_config_table_shows_processor_ids() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET")).and(path("/v2/settings/payment-config"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "availablePaymentProcessors": [
                {"paymentProcessorId": "pp-1", "processorName": "Primary", "isDefault": true}]})))
        .mount(&server).await;

    support::bin(&server).args(["settings", "payment-config"])
        .assert().success().stdout(predicates::str::contains("pp-1"));
}

#[tokio::test]
async fn contact_info_uses_the_contact_information_path() {
    let server = support::mock_with_token().await;
    let ex = support::mount(
        &server, "flute-v2-get-settings-contact-information", "default").await;

    // ContactInfoResponseDto wraps everything in `contactInfos`; there is no
    // top-level email to render.
    support::bin(&server).args(["settings", "contact-info"]).assert().success();
    support::assert_exchange_observed(&server, &ex).await;
}
```

- [ ] **Step 2: Run, fail, implement all four, pass; apply the checklist, commit**

```bash
git commit -am "ARISE-4928 feat(settings): add payment-config, contact-info, autofill"
```

---

### Task 22: `payment-links` (6 operations)

**Files:** Create `src/api/endpoints/payment_links.rs`, `src/groups/payment_links.rs`, `tests/cmd_payment_links.rs`

| Command | Method | Path |
|---|---|---|
| `create` | POST | `/v2/payment-links` |
| `list` | GET | `/v2/payment-links` |
| `get` | GET | `/v2/payment-links/{paymentLinkId}` |
| `update` | PATCH | `/v2/payment-links/{paymentLinkId}` |
| `delete` | DELETE | `/v2/payment-links/{paymentLinkId}` |
| `share` | POST | `/v2/payment-links/{paymentLinkId}/share` |

`DELETE /v2/payment-links/{id}` returns **204**, unlike the other deletes which return 200. The renderer must not try to parse a body.

- [ ] **Step 1: Write the failing tests**

```rust
// tests/cmd_payment_links.rs
mod support;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

/// This delete answers 204 with no body; parsing one would be a decode error.
#[tokio::test]
async fn delete_handles_a_204_with_no_body() {
    let server = support::mock_with_token().await;
    Mock::given(method("DELETE")).and(path("/v2/payment-links/pl_1"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server).await;

    support::bin(&server).args(["payment-links", "delete", "pl_1", "--yes"])
        .assert().success();
}

#[tokio::test]
async fn delete_without_yes_exits_3_and_sends_nothing() {
    let server = support::mock_with_token().await;
    support::bin(&server).args(["payment-links", "delete", "pl_1"]).assert().code(3);
    assert!(server.received_requests().await.unwrap()
        .iter().all(|r| r.url.path() == "/oauth2/token"));
}

/// share answers 204 with no body; there is no resource to render.
#[tokio::test]
async fn share_posts_to_the_share_subpath_and_handles_204() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST")).and(path("/v2/payment-links/pl_1/share"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server).await;

    support::bin(&server).args(["payment-links", "share", "pl_1",
        "--email", "buyer@example.com"]).assert().success();
}
```

- [ ] **Step 2: Run, fail, implement all six, pass; apply the checklist, commit**

```bash
git commit -am "ARISE-4928 feat(payment-links): add all six operations"
```

---

### Task 23: `payment-sessions` (3 operations)

**Files:** Create `src/api/endpoints/payment_sessions.rs`, `src/groups/payment_sessions.rs`, `tests/cmd_payment_sessions.rs`

| Command | Method | Path |
|---|---|---|
| `create` | POST | `/v2/payment-sessions` |
| `get` | GET | `/v2/payment-sessions/{paymentSessionId}` |
| `cancel` | POST | `/v2/payment-sessions/{paymentSessionId}/cancel` |

- [ ] **Step 1: Write the failing test**

```rust
// tests/cmd_payment_sessions.rs
mod support;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

/// The cancel success is documented with no content; there is no session
/// object to render, so the renderer confirms rather than prints a resource.
#[tokio::test]
async fn cancel_posts_to_the_cancel_subpath_and_expects_no_body() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST")).and(path("/v2/payment-sessions/ps_1/cancel"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server).await;

    support::bin(&server).args(["payment-sessions", "cancel", "ps_1", "--yes"])
        .assert().success();
}

#[tokio::test]
async fn cancel_without_yes_exits_3_and_sends_nothing() {
    let server = support::mock_with_token().await;
    support::bin(&server).args(["payment-sessions", "cancel", "ps_1"]).assert().code(3);
    assert!(server.received_requests().await.unwrap()
        .iter().all(|r| r.url.path() == "/oauth2/token"));
}

/// Cancel is one of the idempotent verbs.
#[tokio::test]
async fn cancel_on_missing_session_exits_zero() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST")).and(path("/v2/payment-sessions/gone/cancel"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server).await;

    support::bin(&server).args(["payment-sessions", "cancel", "gone", "--yes"]).assert().success();
}
```

- [ ] **Step 2: Run, fail, implement all three, pass; apply the checklist, commit**

```bash
git commit -am "ARISE-4928 feat(payment-sessions): add create, get, cancel"
```

---

### Task 24: `api-keys` (3 operations)

**Files:** Create `src/api/endpoints/api_keys.rs`, `src/groups/api_keys.rs`, `tests/cmd_api_keys.rs`

| Command | Method | Path |
|---|---|---|
| `create` | POST | `/v2/api-keys` |
| `list` | GET | `/v2/api-keys` — **not paginated** |
| `revoke` | DELETE | `/v2/api-keys/{clientId}` |

`GetApiKeysResponseDto` has one property, `apiKeys`, and no `pageInfo`. So `api-keys list` takes neither `--page-index`/`--page-size` nor `--all`, and its envelope carries no `page_info`. Add a test asserting the pagination flags are rejected here, so the shared `PaginationArgs` is not wired in by reflex.

This group sits behind a server feature flag. With the flag disabled the endpoint answers 404, which reads as a wrong URL rather than a disabled feature — so a 404 **from this group** carries a message saying so. That collides with the idempotent-delete rule: `revoke` still exits 0 on 404, but `create` and `list` explain the feature flag.

- [ ] **Step 1: Write the failing tests**

```rust
// tests/cmd_api_keys.rs
mod support;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

/// A 404 here almost always means the feature flag is off, not a bad URL.
#[tokio::test]
async fn list_404_explains_the_feature_flag() {
    let server = support::mock_with_token().await;
    Mock::given(method("GET")).and(path("/v2/api-keys"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server).await;

    support::bin(&server).args(["api-keys", "list"])
        .assert().code(4)
        .stderr(predicates::str::contains("feature"));
}

/// Revoke stays idempotent even though its 404 is ambiguous.
#[tokio::test]
async fn revoke_on_missing_key_exits_zero() {
    let server = support::mock_with_token().await;
    Mock::given(method("DELETE")).and(path("/v2/api-keys/gone"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server).await;

    support::bin(&server).args(["api-keys", "revoke", "gone", "--yes"]).assert().success();
}

#[tokio::test]
async fn revoke_without_yes_exits_3_and_sends_nothing() {
    let server = support::mock_with_token().await;
    support::bin(&server).args(["api-keys", "revoke", "ck_1"]).assert().code(3);
    assert!(server.received_requests().await.unwrap()
        .iter().all(|r| r.url.path() == "/oauth2/token"));
}

/// CreateApiKeyRequestDto requires apiKeyName and merchantId, and declares no
/// other properties. --name maps to apiKeyName; --merchant-id is required.
#[tokio::test]
async fn create_posts_both_required_fields() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST")).and(path("/v2/api-keys"))
        .and(wiremock::matchers::body_json(
            serde_json::json!({"apiKeyName": "ci", "merchantId": "m-1"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "clientId": "ck_1", "clientSecret": "sec_live_abc123"})))
        .mount(&server).await;

    support::bin(&server)
        .args(["api-keys", "create", "--name", "ci", "--merchant-id", "m-1"])
        .assert().success();
}

#[tokio::test]
async fn create_without_merchant_id_exits_3() {
    let server = support::mock_with_token().await;
    support::bin(&server).args(["api-keys", "create", "--name", "ci"])
        .assert().code(3);
}

/// A freshly created secret is shown once; it must reach stdout intact and
/// must never reach the debug log.
#[tokio::test]
async fn create_prints_the_secret_but_never_logs_it() {
    let server = support::mock_with_token().await;
    Mock::given(method("POST")).and(path("/v2/api-keys"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "clientId": "ck_1", "clientSecret": "sec_live_abc123"})))
        .mount(&server).await;

    let out = support::bin(&server)
        .args(["--debug", "--output", "json", "api-keys", "create",
               "--name", "ci", "--merchant-id", "m-1"])
        .assert().success().get_output().clone();

    assert!(String::from_utf8_lossy(&out.stdout).contains("sec_live_abc123"));
    assert!(!String::from_utf8_lossy(&out.stderr).contains("sec_live_abc123"));
}
```

- [ ] **Step 2: Run, fail, implement all three, pass; apply the checklist, commit**

```bash
git commit -am "ARISE-4928 feat(api-keys): add create, list, revoke"
```

---

