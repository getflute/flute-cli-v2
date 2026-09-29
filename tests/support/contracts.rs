//! The contract matrix: one row per non-webhook operation, one sub-row per
//! request variant.
//!
//! Each variant carries the **exchange itself** rather than describing it, so
//! conformance validates the same value the group's command test drives its
//! mock from. A row that merely named a schema and a test proved nothing: the
//! names were never read, and the named test might exercise a different
//! operation entirely.

use super::spec::amount;
use super::spec::{Exchange, RequestFixture, ResponseFixture};
use serde_json::json;

pub enum Mapping {
    Command(&'static str),
    Internal(&'static str),
    Excluded(&'static str),
}

pub enum Live {
    Test(&'static str),
    Skip(&'static str),
}

pub struct Variant {
    pub name: &'static str,
    /// The single source of truth for this variant. Conformance validates it;
    /// the command test mounts it.
    pub exchange: fn() -> Exchange,
    pub live: Live,
}

pub struct Contract {
    /// The harness's name for the operation, used by every other table here.
    pub operation_id: &'static str,
    /// `METHOD /path/template`: the key the row is matched to its bundle
    /// operation by.
    pub route: &'static str,
    pub mapping: Mapping,
    pub variants: &'static [Variant],
}

/// Look up one variant's exchange. Command tests call this rather than
/// rebuilding the body, so a mock and its conformance check cannot disagree.
pub fn exchange(operation_id: &str, variant: &str) -> Exchange {
    let c = CONTRACTS
        .iter()
        .find(|c| c.operation_id == operation_id)
        .unwrap_or_else(|| panic!("no contract row for {operation_id}"));
    let v = c
        .variants
        .iter()
        .find(|v| v.name == variant)
        .unwrap_or_else(|| panic!("{operation_id} has no variant {variant}"));
    (v.exchange)()
}

pub fn req(method: &str, path: &str) -> RequestFixture {
    RequestFixture {
        method: method.into(),
        path: path.into(),
        path_params: vec![],
        query: vec![],
        content_type: None,
        body: None,
    }
}

pub fn ok(body: Option<serde_json::Value>) -> ResponseFixture {
    ResponseFixture { status: 200, body }
}

pub static CONTRACTS: &[Contract] = &[
    // The token endpoint is one of the bundle's operations, not an extra. It
    // needs no command: the client obtains a bearer on every authenticated
    // call, and `auth token` prints one.
    Contract {
        operation_id: "flute-v2-get-ping",
        route: "GET /v2/ping",
        mapping: Mapping::Command("ping"),
        variants: &[Variant {
            name: "default",
            exchange: || Exchange {
                request: req("GET", "/v2/ping"),
                // 200 with no body; the harness rejects an invented one.
                response: ResponseFixture {
                    status: 200,
                    body: None,
                },
            },
            live: Live::Test("live_ping_succeeds"),
        }],
    },
    Contract {
        operation_id: "flute-v2-post-customers",
        route: "POST /v2/customers",
        mapping: Mapping::Command("customers create"),
        variants: &[Variant {
            name: "minimal",
            exchange: || Exchange {
                request: RequestFixture {
                    body: Some(json!({
                        "firstName": "Ada",
                        "lastName": "Lovelace",
                        "email": "ada@example.com"})),
                    ..req("POST", "/v2/customers")
                },
                response: ok(Some(json!({
                    "customerId": "8db2ff47-b143-4adb-ab58-a11111111111"}))),
            },
            live: Live::Test("live_customer_create_get_delete"),
        }],
    },
    Contract {
        operation_id: "flute-v2-get-customers-customerId",
        route: "GET /v2/customers/{customerId}",
        mapping: Mapping::Command("customers get"),
        variants: &[Variant {
            name: "default",
            exchange: || Exchange {
                request: RequestFixture {
                    path_params: vec![(
                        "customerId".into(),
                        "8db2ff47-b143-4adb-ab58-a11111111111".into(),
                    )],
                    ..req("GET", "/v2/customers/8db2ff47-b143-4adb-ab58-a11111111111")
                },
                response: ok(Some(json!({
                    "customerId": "8db2ff47-b143-4adb-ab58-a11111111111",
                    "firstName": "Ada",
                    "lastName": "Lovelace",
                    "email": "ada@example.com"}))),
            },
            live: Live::Test("live_customer_create_get_delete"),
        }],
    },
    Contract {
        operation_id: "flute-v2-get-customers",
        route: "GET /v2/customers",
        mapping: Mapping::Command("customers list"),
        variants: &[
            Variant {
                // No query at all: the flags are omitted when absent so the
                // server's declared defaults (pageIndex 0, pageSize 20)
                // govern.
                name: "first page, server defaults",
                exchange: || Exchange {
                    request: req("GET", "/v2/customers"),
                    response: ok(Some(json!({
                        "items": [{
                            "customerId": "8db2ff47-b143-4adb-ab58-a11111111111",
                            "firstName": "Ada",
                            "lastName": "Lovelace",
                            "email": "ada@example.com",
                            "mobilePhoneNumber": "+14155552309",
                            "billingAddress": {"city": "Austin", "stateCode": "TX"},
                            "lastTransactionDate": "2026-05-29T22:04:31.771Z",
                            "paymentMethodsCount": 1}],
                        "pageInfo": {
                            "pageIndex": 0, "pageSize": 20, "totalItems": 1,
                            "totalPages": 1, "hasMore": false}}))),
                },
                live: Live::Test("live_customer_list_first_page"),
            },
            Variant {
                name: "filtered and paged",
                exchange: || Exchange {
                    request: RequestFixture {
                        query: vec![
                            ("pageIndex".into(), "1".into()),
                            ("pageSize".into(), "5".into()),
                            ("email".into(), "ada@example.com".into()),
                            ("asc".into(), "false".into()),
                            ("sortBy".into(), "lastName".into()),
                        ],
                        ..req("GET", "/v2/customers")
                    },
                    response: ok(Some(json!({
                        "items": [],
                        "pageInfo": {
                            "pageIndex": 1, "pageSize": 5, "totalItems": 1,
                            "totalPages": 1, "hasMore": false}}))),
                },
                live: Live::Test("live_customer_list_filtered_by_email"),
            },
        ],
    },
    Contract {
        operation_id: "flute-v2-patch-customers-customerId",
        route: "PATCH /v2/customers/{customerId}",
        mapping: Mapping::Command("customers update"),
        variants: &[Variant {
            name: "changed fields",
            exchange: || Exchange {
                request: RequestFixture {
                    path_params: vec![(
                        "customerId".into(),
                        "8db2ff47-b143-4adb-ab58-a11111111111".into(),
                    )],
                    body: Some(json!({
                        "companyName": "Analytical Engines",
                        "email": "ada@example.com",
                        "billingAddress": {"city": "Austin", "stateCode": "TX"}})),
                    ..req(
                        "PATCH",
                        "/v2/customers/8db2ff47-b143-4adb-ab58-a11111111111",
                    )
                },
                // 200 with no body; the harness rejects an invented one.
                response: ResponseFixture {
                    status: 200,
                    body: None,
                },
            },
            live: Live::Test("live_customer_update_then_delete"),
        }],
    },
    Contract {
        operation_id: "flute-v2-delete-customers-customerId",
        route: "DELETE /v2/customers/{customerId}",
        mapping: Mapping::Command("customers delete"),
        variants: &[Variant {
            name: "default",
            exchange: || Exchange {
                request: RequestFixture {
                    path_params: vec![(
                        "customerId".into(),
                        "8db2ff47-b143-4adb-ab58-a11111111111".into(),
                    )],
                    ..req(
                        "DELETE",
                        "/v2/customers/8db2ff47-b143-4adb-ab58-a11111111111",
                    )
                },
                response: ResponseFixture {
                    status: 200,
                    body: None,
                },
            },
            live: Live::Test("live_customer_update_then_delete"),
        }],
    },
    Contract {
        operation_id: "flute-v2-get-payment-methods",
        route: "GET /v2/payment-methods",
        mapping: Mapping::Command("payment-methods list"),
        variants: &[
            Variant {
                name: "first page, server defaults",
                exchange: || Exchange {
                    request: req("GET", "/v2/payment-methods"),
                    response: ok(Some(json!({
                        "items": [{
                            "paymentMethodId": "6e22f0f3-f3ed-407c-a46f-ec9cffde5d5b",
                            "customerId": "588f57a5-fe6a-4844-851e-e98914e81980",
                            "name": "My Visa Card",
                            "type": "Card",
                            "isDefault": true,
                            "createdOn": "2026-01-15T10:30:00.000Z",
                            "card": {
                                "cardMask": "************1111",
                                "expirationMonth": 12,
                                "expirationYear": 2033,
                                "cardTokenType": "Local"}}],
                        "pageInfo": {
                            "pageIndex": 0, "pageSize": 20, "totalItems": 1,
                            "totalPages": 1, "hasMore": false}}))),
                },
                live: Live::Test("live_payment_method_list_for_a_customer"),
            },
            Variant {
                name: "filtered by customer",
                exchange: || Exchange {
                    request: RequestFixture {
                        query: vec![
                            ("search".into(), "visa".into()),
                            (
                                "customerId".into(),
                                "588f57a5-fe6a-4844-851e-e98914e81980".into(),
                            ),
                        ],
                        ..req("GET", "/v2/payment-methods")
                    },
                    response: ok(Some(json!({
                        "items": [],
                        "pageInfo": {"hasMore": false}}))),
                },
                live: Live::Test("live_payment_method_list_for_a_customer"),
            },
        ],
    },
    Contract {
        operation_id: "flute-v2-get-payment-methods-paymentMethodId",
        route: "GET /v2/payment-methods/{paymentMethodId}",
        mapping: Mapping::Command("payment-methods get"),
        // Two requests, not one shape with two responses: a card and an ACH
        // account are different resources at different ids, and reading only
        // one leaves the other's response fields unexercised.
        variants: &[
            Variant {
                name: "card",
                exchange: || Exchange {
                    request: RequestFixture {
                        path_params: vec![(
                            "paymentMethodId".into(),
                            "6e22f0f3-f3ed-407c-a46f-ec9cffde5d5b".into(),
                        )],
                        ..req(
                            "GET",
                            "/v2/payment-methods/6e22f0f3-f3ed-407c-a46f-ec9cffde5d5b",
                        )
                    },
                    response: ok(Some(json!({
                        "paymentMethodId": "6e22f0f3-f3ed-407c-a46f-ec9cffde5d5b",
                        "customerId": "588f57a5-fe6a-4844-851e-e98914e81980",
                        "name": "My Visa Card",
                        "type": "Card",
                        "isDefault": true,
                        "createdOn": "2026-01-15T10:30:00.000Z",
                        "card": {
                            "cardMask": "************1111",
                            "expirationMonth": 12,
                            "expirationYear": 2033,
                            "cardTokenType": "Local"}}))),
                },
                live: Live::Test("live_payment_method_add_card_get_delete"),
            },
            Variant {
                name: "ach",
                exchange: || Exchange {
                    request: RequestFixture {
                        path_params: vec![(
                            "paymentMethodId".into(),
                            "6bbfbe3e-04dd-41cd-82bf-1466e0159007".into(),
                        )],
                        ..req(
                            "GET",
                            "/v2/payment-methods/6bbfbe3e-04dd-41cd-82bf-1466e0159007",
                        )
                    },
                    // `type` is "ACH" here, not the "ElectronicCheck" the
                    // operation's own ACH example uses: the declared enum is
                    // exactly Card, ACH and Cash, so the example contradicts
                    // its own schema. `live_payment_method_type_matches_the_
                    // declared_enum` is what can settle which is wrong.
                    response: ok(Some(json!({
                        "paymentMethodId": "6bbfbe3e-04dd-41cd-82bf-1466e0159007",
                        "customerId": null,
                        "name": "Acme Bus Savings",
                        "type": "ACH",
                        "isDefault": false,
                        "createdOn": "2026-02-01T09:00:00.000Z",
                        "card": null,
                        "ach": {
                            "accountNumber": "****6789",
                            "routingNumber": "021000021",
                            "accountType": "Checking",
                            "accountHolderType": "Business",
                            "taxId": "***-**-6789",
                            "companyName": "Acme Corp Inc"}}))),
                },
                live: Live::Test("live_payment_method_type_matches_the_declared_enum"),
            },
        ],
    },
    Contract {
        operation_id: "flute-v2-post-payment-methods-cards",
        route: "POST /v2/payment-methods/cards",
        mapping: Mapping::Command("payment-methods add-card"),
        variants: &[
            Variant {
                name: "required only",
                exchange: || Exchange {
                    request: RequestFixture {
                        body: Some(json!({
                            "cardNumber": "4111111111111111",
                            "expirationMonth": 12,
                            "expirationYear": 2033})),
                        ..req("POST", "/v2/payment-methods/cards")
                    },
                    response: ok(Some(
                        json!({"paymentMethodId": "6e22f0f3-f3ed-407c-a46f-ec9cffde5d5b"}),
                    )),
                },
                live: Live::Test("live_payment_method_add_card_get_delete"),
            },
            Variant {
                name: "linked to a customer and named",
                exchange: || Exchange {
                    request: RequestFixture {
                        body: Some(json!({
                            "cardNumber": "4111111111111111",
                            "securityCode": "123",
                            "expirationMonth": 12,
                            "expirationYear": 2033,
                            "customerId": "588f57a5-fe6a-4844-851e-e98914e81980",
                            "paymentName": "My Visa Card"})),
                        ..req("POST", "/v2/payment-methods/cards")
                    },
                    response: ok(Some(
                        json!({"paymentMethodId": "6e22f0f3-f3ed-407c-a46f-ec9cffde5d5b"}),
                    )),
                },
                live: Live::Test("live_payment_method_list_for_a_customer"),
            },
        ],
    },
    Contract {
        operation_id: "flute-v2-post-payment-methods-ach",
        route: "POST /v2/payment-methods/ach",
        mapping: Mapping::Command("payment-methods add-ach"),
        variants: &[
            Variant {
                // The four the schema requires, and nothing else — the ACH
                // shape is where the unfamiliar required fields are.
                name: "required only",
                exchange: || Exchange {
                    request: RequestFixture {
                        body: Some(json!({
                            "routingNumber": "021000021",
                            "accountNumber": "123456789",
                            "accountHolderType": "Personal",
                            "accountType": "Checking"})),
                        ..req("POST", "/v2/payment-methods/ach")
                    },
                    response: ok(Some(
                        json!({"paymentMethodId": "6bbfbe3e-04dd-41cd-82bf-1466e0159007"}),
                    )),
                },
                live: Live::Test("live_payment_method_type_matches_the_declared_enum"),
            },
            Variant {
                name: "business account, fully specified",
                exchange: || Exchange {
                    request: RequestFixture {
                        body: Some(json!({
                            "routingNumber": "021000021",
                            "accountNumber": "123456789",
                            "accountHolderType": "Business",
                            "accountType": "Savings",
                            "taxId": "123456789",
                            "customerId": "588f57a5-fe6a-4844-851e-e98914e81980",
                            "name": "Business Savings",
                            "companyName": "Analytical Engines"})),
                        ..req("POST", "/v2/payment-methods/ach")
                    },
                    response: ok(Some(
                        json!({"paymentMethodId": "6bbfbe3e-04dd-41cd-82bf-1466e0159007"}),
                    )),
                },
                live: Live::Test("live_payment_method_add_ach_business"),
            },
        ],
    },
    Contract {
        operation_id: "flute-v2-patch-payment-methods-paymentMethodId",
        route: "PATCH /v2/payment-methods/{paymentMethodId}",
        mapping: Mapping::Command("payment-methods update"),
        variants: &[Variant {
            name: "rename",
            exchange: || Exchange {
                request: RequestFixture {
                    path_params: vec![(
                        "paymentMethodId".into(),
                        "6e22f0f3-f3ed-407c-a46f-ec9cffde5d5b".into(),
                    )],
                    body: Some(json!({"paymentName": "After"})),
                    ..req(
                        "PATCH",
                        "/v2/payment-methods/6e22f0f3-f3ed-407c-a46f-ec9cffde5d5b",
                    )
                },
                response: ResponseFixture {
                    status: 200,
                    body: None,
                },
            },
            live: Live::Test("live_payment_method_update_name"),
        }],
    },
    Contract {
        operation_id: "flute-v2-delete-payment-methods-paymentMethodId",
        route: "DELETE /v2/payment-methods/{paymentMethodId}",
        mapping: Mapping::Command("payment-methods delete"),
        variants: &[Variant {
            name: "default",
            exchange: || Exchange {
                request: RequestFixture {
                    path_params: vec![(
                        "paymentMethodId".into(),
                        "6e22f0f3-f3ed-407c-a46f-ec9cffde5d5b".into(),
                    )],
                    ..req(
                        "DELETE",
                        "/v2/payment-methods/6e22f0f3-f3ed-407c-a46f-ec9cffde5d5b",
                    )
                },
                response: ResponseFixture {
                    status: 200,
                    body: None,
                },
            },
            live: Live::Test("live_payment_method_delete_twice_is_still_success"),
        }],
    },
    Contract {
        operation_id: "flute-v2-post-payment-methods-paymentMethodId-set-default",
        route: "POST /v2/payment-methods/{paymentMethodId}/set-default",
        mapping: Mapping::Command("payment-methods set-default"),
        variants: &[Variant {
            // `customerId` is a **required query parameter**, and the success
            // carries no body. Both are easy to miss and both fail
            // conformance if missed.
            name: "default",
            exchange: || Exchange {
                request: RequestFixture {
                    path_params: vec![(
                        "paymentMethodId".into(),
                        "6e22f0f3-f3ed-407c-a46f-ec9cffde5d5b".into(),
                    )],
                    query: vec![(
                        "customerId".into(),
                        "588f57a5-fe6a-4844-851e-e98914e81980".into(),
                    )],
                    ..req(
                        "POST",
                        "/v2/payment-methods/6e22f0f3-f3ed-407c-a46f-ec9cffde5d5b/set-default",
                    )
                },
                response: ResponseFixture {
                    status: 200,
                    body: None,
                },
            },
            live: Live::Test("live_payment_method_set_default"),
        }],
    },
    Contract {
        operation_id: "flute-v2-post-transactions",
        route: "POST /v2/transactions",
        mapping: Mapping::Command("transactions create"),
        variants: &[
            Variant {
                name: "new card, automatic capture",
                exchange: || Exchange {
                    request: RequestFixture {
                        body: Some(json!({
                            "paymentProcessorId": "8db2ff47-b143-4adb-ab58-a11111111111",
                            // Never a float literal: json!(10.50) goes through
                            // f64 and serialises as 10.5, which is not what
                            // the CLI sends.
                            "baseAmount": amount("10.50"),
                            "transactionDetails": {"cardData": {
                                "captureMethod": "Auto",
                                "paymentMethodDetails": {
                                    "cardNumber": "4111111111111111",
                                    "securityCode": "123",
                                    "expirationMonth": 12,
                                    "expirationYear": 2032}}}
                        })),
                        ..req("POST", "/v2/transactions")
                    },
                    response: ok(Some(json!({
                        "transactionId": "90d084d6-55b8-4fb8-b658-861534d07f9a",
                        "transactionStatus": "Captured",
                        "processedAmount": amount("10.50"),
                        "currencyCode": "USD"}))),
                },
                live: Live::Test("live_card_sale_auto_capture"),
            },
            Variant {
                name: "new card, manual capture",
                exchange: || Exchange {
                    request: RequestFixture {
                        body: Some(json!({
                            "paymentProcessorId": "8db2ff47-b143-4adb-ab58-a11111111111",
                            "baseAmount": amount("10.50"),
                            "transactionDetails": {"cardData": {
                                "captureMethod": "Manual",
                                "paymentMethodDetails": {
                                    "cardNumber": "4111111111111111",
                                    "securityCode": "123",
                                    "expirationMonth": 12,
                                    "expirationYear": 2032}}}
                        })),
                        ..req("POST", "/v2/transactions")
                    },
                    response: ok(Some(json!({
                        "transactionId": "90d084d6-55b8-4fb8-b658-861534d07f9a",
                        "transactionStatus": "Authorized",
                        "processedAmount": amount("10.50"),
                        "currencyCode": "USD"}))),
                },
                live: Live::Test("live_card_auth_manual_capture"),
            },
            Variant {
                name: "saved card",
                exchange: || Exchange {
                    request: RequestFixture {
                        body: Some(json!({
                            "paymentProcessorId": "8db2ff47-b143-4adb-ab58-a11111111111",
                            "baseAmount": amount("10.50"),
                            "transactionDetails": {"cardData": {
                                "captureMethod": "Auto",
                                "paymentMethodId":
                                    "6e22f0f3-f3ed-407c-a46f-ec9cffde5d5b"}}
                        })),
                        ..req("POST", "/v2/transactions")
                    },
                    response: ok(Some(json!({
                        "transactionId": "90d084d6-55b8-4fb8-b658-861534d07f9a",
                        "transactionStatus": "Captured",
                        "processedAmount": amount("10.50"),
                        "currencyCode": "USD"}))),
                },
                live: Live::Test("live_saved_card_sale"),
            },
            Variant {
                // `secCode` and `requesterIpAddress` are required by schema
                // whenever ACH is selected. `billingAddress` and
                // `contactInfo` are required only for a *new* account, which
                // no `required` array can express.
                name: "new ACH",
                exchange: || Exchange {
                    request: RequestFixture {
                        body: Some(json!({
                            "paymentProcessorId": "8db2ff47-b143-4adb-ab58-a11111111111",
                            "baseAmount": amount("10.50"),
                            "billingAddress": {
                                "addressLine1": "1 Main St",
                                "city": "Austin",
                                "stateCode": "TX",
                                "postalCode": "78701",
                                "countryCode": "US"},
                            // The email is required for a new ACH payment —
                            // the API says so and the schema does not:
                            // `ContactInfo.Email is required for new ACH
                            // payments.`
                            "contactInfo": {
                                "firstName": "Ada",
                                "lastName": "Lovelace",
                                "mobilePhoneNumber": "+14155552309",
                                "email": "ada@example.com"},
                            "transactionDetails": {"achData": {
                                "secCode": "Web",
                                "requesterIpAddress": "203.0.113.10",
                                "paymentMethodDetails": {
                                    "accountNumber": "123456789",
                                    "routingNumber": "021000021",
                                    "accountType": "Checking",
                                    "accountHolderType": "Personal"}}}
                        })),
                        ..req("POST", "/v2/transactions")
                    },
                    response: ok(Some(json!({
                        "transactionId": "90d084d6-55b8-4fb8-b658-861534d07f9a",
                        "transactionStatus": "Pending",
                        "processedAmount": amount("10.50"),
                        "currencyCode": "USD"}))),
                },
                live: Live::Test("live_new_ach_debit_with_the_conditional_requirements"),
            },
            Variant {
                // The other direction of the conditional rule: no billing
                // address, no contact info, and the same two ACH-wide
                // required fields.
                name: "saved ACH",
                exchange: || Exchange {
                    request: RequestFixture {
                        body: Some(json!({
                            "paymentProcessorId": "8db2ff47-b143-4adb-ab58-a11111111111",
                            "baseAmount": amount("10.50"),
                            "transactionDetails": {"achData": {
                                "secCode": "PPD",
                                "requesterIpAddress": "203.0.113.10",
                                "paymentMethodId":
                                    "6bbfbe3e-04dd-41cd-82bf-1466e0159007"}}
                        })),
                        ..req("POST", "/v2/transactions")
                    },
                    response: ok(Some(json!({
                        "transactionId": "90d084d6-55b8-4fb8-b658-861534d07f9a",
                        "transactionStatus": "Pending",
                        "processedAmount": amount("10.50"),
                        "currencyCode": "USD"}))),
                },
                live: Live::Test("live_saved_ach_debit_for_a_customer_needs_no_billing_address"),
            },
            Variant {
                // Everything a level-3 card sale can carry, in one exchange:
                // v2 declares seventeen of these fields.
                name: "card with level three data",
                exchange: || Exchange {
                    request: RequestFixture {
                        body: Some(json!({
                            "paymentProcessorId": "8db2ff47-b143-4adb-ab58-a11111111111",
                            "baseAmount": amount("10.50"),
                            "isCustomerInitiatedTransaction": true,
                            "pricingType": "Card",
                            "extraAmounts": {
                                "tipAmount": amount("1.50"),
                                "discountAmount": amount("0.50"),
                                "surchargeRate": amount("0.0300")},
                            "contactInfo": {
                                "firstName": "Ada",
                                "lastName": "Lovelace",
                                "companyName": "Analytical Engines",
                                "email": "ada@example.com",
                                "mobilePhoneNumber": "+14155552309",
                                "hasSmsConsent": true},
                            "transactionEnhancedData": {
                                "salesTaxRate": amount("8.25"),
                                // No hyphen: the API rejects an invoice
                                // number containing one, and the schema's own
                                // example, `INV-001234`, would be rejected
                                // too. `purchaseOrder` has no such rule.
                                "invoiceNumber": "INV1",
                                "purchaseOrder": "PO-1",
                                "shippingCharges": amount("4.99"),
                                "products": [{
                                    "productName": "Widget",
                                    "productDescription": "A widget",
                                    "productCode": "W-1",
                                    "unitPrice": amount("9.99"),
                                    "measurementUnit": "EA",
                                    "quantity": amount("2"),
                                    "taxAmount": amount("0.82"),
                                    "discountRate": amount("1.50")}]},
                            "transactionDetails": {"cardData": {
                                "captureMethod": "Auto",
                                "paymentMethodDetails": {
                                    "cardNumber": "4111111111111111",
                                    "securityCode": "123",
                                    "expirationMonth": 12,
                                    "expirationYear": 2032}}}
                        })),
                        ..req("POST", "/v2/transactions")
                    },
                    response: ok(Some(json!({
                        "transactionId": "90d084d6-55b8-4fb8-b658-861534d07f9a",
                        "transactionStatus": "Captured",
                        "processedAmount": amount("10.50"),
                        "currencyCode": "USD"}))),
                },
                live: Live::Test("live_card_sale_with_level_three_data"),
            },
        ],
    },
    Contract {
        operation_id: "flute-v2-get-transactions-transactionId",
        route: "GET /v2/transactions/{transactionId}",
        mapping: Mapping::Command("transactions get"),
        variants: &[Variant {
            name: "default",
            exchange: || Exchange {
                request: RequestFixture {
                    path_params: vec![(
                        "transactionId".into(),
                        "90d084d6-55b8-4fb8-b658-861534d07f9a".into(),
                    )],
                    ..req(
                        "GET",
                        "/v2/transactions/90d084d6-55b8-4fb8-b658-861534d07f9a",
                    )
                },
                response: ok(Some(json!({
                    "transactionId": "90d084d6-55b8-4fb8-b658-861534d07f9a",
                    "transactionStatus": "Captured",
                    "processedAmount": amount("10.50"),
                    "currencyCode": "USD"}))),
            },
            live: Live::Test("live_card_sale_auto_capture"),
        }],
    },
    Contract {
        operation_id: "flute-v2-get-transactions",
        route: "GET /v2/transactions",
        mapping: Mapping::Command("transactions list"),
        variants: &[
            Variant {
                name: "first page, server defaults",
                exchange: || Exchange {
                    request: req("GET", "/v2/transactions"),
                    response: ok(Some(json!({
                        "items": [{
                            "transactionId": "90d084d6-55b8-4fb8-b658-861534d07f9a",
                            "transactionStatus": "Captured",
                            "transactionType": "Sale",
                            "transactionDateTime": "2026-05-29T22:04:31.771Z",
                            "processedAmount": amount("10.50"),
                            "currencyCode": "USD",
                            "customerId": "588f57a5-fe6a-4844-851e-e98914e81980"}],
                        "pageInfo": {
                            "pageIndex": 0, "pageSize": 20, "totalItems": 1,
                            "totalPages": 1, "hasMore": false}}))),
                },
                live: Live::Test("live_transaction_list_first_page"),
            },
            Variant {
                // Every declared filter in one exchange: sixteen parameters
                // is the largest query surface in the API.
                name: "every filter",
                exchange: || Exchange {
                    request: RequestFixture {
                        query: vec![
                            ("pageIndex".into(), "1".into()),
                            ("pageSize".into(), "5".into()),
                            ("sortBy".into(), "transactionDateTime".into()),
                            ("sortOrder".into(), "desc".into()),
                            ("fromDate".into(), "2026-01-01T00:00:00Z".into()),
                            ("toDate".into(), "2026-12-31T23:59:59Z".into()),
                            ("sourceType".into(), "ApiKey".into()),
                            (
                                "sourceId".into(),
                                "588f57a5-fe6a-4844-851e-e98914e81980".into(),
                            ),
                            ("batchId".into(), "batch-1".into()),
                            ("transactionStatus".into(), "Captured".into()),
                            ("paymentMethodType".into(), "Card".into()),
                            (
                                "customerId".into(),
                                "588f57a5-fe6a-4844-851e-e98914e81980".into(),
                            ),
                            (
                                "merchantId".into(),
                                "8db2ff47-b143-4adb-ab58-a11111111111".into(),
                            ),
                            ("minAmount".into(), "1.00".into()),
                            ("maxAmount".into(), "999.99".into()),
                            ("referenceId".into(), "ref-1".into()),
                        ],
                        ..req("GET", "/v2/transactions")
                    },
                    response: ok(Some(json!({
                        "items": [],
                        "pageInfo": {"hasMore": false}}))),
                },
                live: Live::Test("live_transaction_list_filtered_by_status"),
            },
        ],
    },
    Contract {
        operation_id: "flute-v2-post-transactions-transactionId-capture",
        route: "POST /v2/transactions/{transactionId}/capture",
        mapping: Mapping::Command("transactions capture"),
        variants: &[
            Variant {
                // No amount is an **empty object**, not an absent body. The
                // API answers `400: A non-empty request body is required` to a
                // bodyless POST on an operation that declares a request
                // schema — observed on `reversal`, and this operation
                // declares the same shape. A truly bodyless POST is only
                // right where the spec declares no request body at all.
                name: "full",
                exchange: || Exchange {
                    request: RequestFixture {
                        path_params: vec![(
                            "transactionId".into(),
                            "90d084d6-55b8-4fb8-b658-861534d07f9a".into(),
                        )],
                        body: Some(json!({})),
                        ..req(
                            "POST",
                            "/v2/transactions/90d084d6-55b8-4fb8-b658-861534d07f9a/capture",
                        )
                    },
                    response: ok(Some(json!({
                        "transactionId": "90d084d6-55b8-4fb8-b658-861534d07f9a",
                        "transactionStatus": "Captured",
                        "processedAmount": amount("10.50"),
                        "currencyCode": "USD"}))),
                },
                live: Live::Test("live_full_capture_sends_no_body"),
            },
            Variant {
                // The schema declares `captureAmount` with
                // `additionalProperties: false`; the operation's own request
                // example sends `{"amount": 50}`. The schema is normative and
                // an example is not, so this is what the CLI sends.
                // `live_partial_capture_field_name` settles it against the
                // running API.
                name: "partial",
                exchange: || Exchange {
                    request: RequestFixture {
                        path_params: vec![(
                            "transactionId".into(),
                            "90d084d6-55b8-4fb8-b658-861534d07f9a".into(),
                        )],
                        body: Some(json!({"captureAmount": amount("5.00")})),
                        ..req(
                            "POST",
                            "/v2/transactions/90d084d6-55b8-4fb8-b658-861534d07f9a/capture",
                        )
                    },
                    response: ok(Some(json!({
                        "transactionId": "90d084d6-55b8-4fb8-b658-861534d07f9a",
                        "transactionStatus": "Captured",
                        "processedAmount": amount("10.50"),
                        "currencyCode": "USD"}))),
                },
                live: Live::Test("live_partial_capture_field_name"),
            },
        ],
    },
    Contract {
        operation_id: "flute-v2-post-transactions-transactionId-reversal",
        route: "POST /v2/transactions/{transactionId}/reversal",
        mapping: Mapping::Command("transactions reversal"),
        // One endpoint detects the settled state server-side, so a void and a
        // refund are these two variants.
        variants: &[
            Variant {
                // An empty object, for the reason spelled out on `capture`:
                // this is the operation that produced the 400.
                name: "full",
                exchange: || Exchange {
                    request: RequestFixture {
                        path_params: vec![(
                            "transactionId".into(),
                            "90d084d6-55b8-4fb8-b658-861534d07f9a".into(),
                        )],
                        body: Some(json!({})),
                        ..req(
                            "POST",
                            "/v2/transactions/90d084d6-55b8-4fb8-b658-861534d07f9a/reversal",
                        )
                    },
                    response: ok(Some(json!({
                        "transactionId": "90d084d6-55b8-4fb8-b658-861534d07f9a",
                        "transactionStatus": "Voided",
                        "processedAmount": amount("10.50"),
                        "currencyCode": "USD"}))),
                },
                live: Live::Test("live_reversal_of_a_sale"),
            },
            Variant {
                name: "partial",
                exchange: || Exchange {
                    request: RequestFixture {
                        path_params: vec![(
                            "transactionId".into(),
                            "90d084d6-55b8-4fb8-b658-861534d07f9a".into(),
                        )],
                        body: Some(json!({"reversalAmount": amount("5.00")})),
                        ..req(
                            "POST",
                            "/v2/transactions/90d084d6-55b8-4fb8-b658-861534d07f9a/reversal",
                        )
                    },
                    response: ok(Some(json!({
                        "transactionId": "90d084d6-55b8-4fb8-b658-861534d07f9a",
                        "transactionStatus": "Captured",
                        "processedAmount": amount("10.50"),
                        "currencyCode": "USD"}))),
                },
                live: Live::Test("live_reversal_of_a_sale"),
            },
        ],
    },
    Contract {
        operation_id: "flute-v2-post-transactions-transactionId-tip-adjustment",
        route: "POST /v2/transactions/{transactionId}/tip-adjustment",
        mapping: Mapping::Command("transactions tip-adjust"),
        variants: &[
            Variant {
                name: "amount",
                exchange: || Exchange {
                    request: RequestFixture {
                        path_params: vec![(
                            "transactionId".into(),
                            "90d084d6-55b8-4fb8-b658-861534d07f9a".into(),
                        )],
                        body: Some(json!({"tipAmount": amount("1.00")})),
                        ..req(
                            "POST",
                            "/v2/transactions/90d084d6-55b8-4fb8-b658-861534d07f9a/tip-adjustment",
                        )
                    },
                    response: ok(Some(json!({
                        "transactionId": "90d084d6-55b8-4fb8-b658-861534d07f9a",
                        "transactionStatus": "Captured",
                        "processedAmount": amount("10.50"),
                        "currencyCode": "USD"}))),
                },
                live: Live::Test("live_tip_adjustment_needs_terminal"),
            },
            Variant {
                name: "rate",
                exchange: || Exchange {
                    request: RequestFixture {
                        path_params: vec![(
                            "transactionId".into(),
                            "90d084d6-55b8-4fb8-b658-861534d07f9a".into(),
                        )],
                        body: Some(json!({"tipRate": amount("0.1500")})),
                        ..req(
                            "POST",
                            "/v2/transactions/90d084d6-55b8-4fb8-b658-861534d07f9a/tip-adjustment",
                        )
                    },
                    response: ok(Some(json!({
                        "transactionId": "90d084d6-55b8-4fb8-b658-861534d07f9a",
                        "transactionStatus": "Captured",
                        "processedAmount": amount("10.50"),
                        "currencyCode": "USD"}))),
                },
                live: Live::Test("live_tip_adjustment_needs_terminal"),
            },
        ],
    },
    Contract {
        operation_id: "flute-v2-post-transactions-transactionId-ach-hold",
        route: "POST /v2/transactions/{transactionId}/ach-hold",
        mapping: Mapping::Command("transactions ach-hold"),
        variants: &[Variant {
            // The operation declares no request body at all.
            name: "default",
            exchange: || Exchange {
                request: RequestFixture {
                    path_params: vec![(
                        "transactionId".into(),
                        "90d084d6-55b8-4fb8-b658-861534d07f9a".into(),
                    )],
                    ..req(
                        "POST",
                        "/v2/transactions/90d084d6-55b8-4fb8-b658-861534d07f9a/ach-hold",
                    )
                },
                // The addressed transaction, in the read's shape, like every
                // other single-transaction write.
                response: ok(Some(json!({
                        "transactionId": "90d084d6-55b8-4fb8-b658-861534d07f9a",
                        "transactionStatus": "Held",
                        "transactionType": "Sale",
                        "paymentMethodType": "ACH",
                        "processedAmount": amount("10.50"),
                        "currencyCode": "USD"}))),
            },
            live: Live::Skip(
                "the action answers with the processor's reference in place of \
                 the merchant's, so a live scenario cannot confirm which \
                 transaction it acted on",
            ),
        }],
    },
    Contract {
        operation_id: "flute-v2-post-transactions-transactionId-ach-release",
        route: "POST /v2/transactions/{transactionId}/ach-release",
        mapping: Mapping::Command("transactions ach-release"),
        variants: &[Variant {
            name: "default",
            exchange: || Exchange {
                request: RequestFixture {
                    path_params: vec![(
                        "transactionId".into(),
                        "90d084d6-55b8-4fb8-b658-861534d07f9a".into(),
                    )],
                    ..req(
                        "POST",
                        "/v2/transactions/90d084d6-55b8-4fb8-b658-861534d07f9a/ach-release",
                    )
                },
                response: ok(Some(json!({
                        "transactionId": "90d084d6-55b8-4fb8-b658-861534d07f9a",
                        "transactionStatus": "Pending",
                        "transactionType": "Sale",
                        "paymentMethodType": "ACH",
                        "processedAmount": amount("10.50"),
                        "currencyCode": "USD"}))),
            },
            live: Live::Skip(
                "the action answers with the processor's reference in place of \
                 the merchant's, so a live scenario cannot confirm which \
                 transaction it acted on",
            ),
        }],
    },
    Contract {
        operation_id: "flute-v2-post-transactions-transactionId-share-receipt",
        route: "POST /v2/transactions/{transactionId}/share-receipt",
        mapping: Mapping::Command("transactions share-receipt"),
        variants: &[Variant {
            name: "sms",
            exchange: || Exchange {
                request: RequestFixture {
                    path_params: vec![(
                        "transactionId".into(),
                        "90d084d6-55b8-4fb8-b658-861534d07f9a".into(),
                    )],
                    body: Some(json!({
                        "shareBy": "Sms",
                        "recipient": "+14155552309",
                        "hasCustomerConsent": true})),
                    ..req(
                        "POST",
                        "/v2/transactions/90d084d6-55b8-4fb8-b658-861534d07f9a/share-receipt",
                    )
                },
                // 200 with no body; the harness rejects an invented one.
                response: ResponseFixture {
                    status: 200,
                    body: None,
                },
            },
            live: Live::Test("live_share_receipt_attended"),
        }],
    },
    Contract {
        operation_id: "flute-v2-post-transactions-calculate-amount",
        route: "POST /v2/transactions/calculate-amount",
        mapping: Mapping::Command("transactions calculate-amount"),
        variants: &[Variant {
            name: "default",
            exchange: || Exchange {
                request: RequestFixture {
                    body: Some(json!({
                        "baseAmount": amount("100.00"),
                        "currencyCode": "USD",
                        "pricingType": "Card",
                        "tipAmount": amount("15.00"),
                        "discountAmount": amount("5.00"),
                        "surchargeRate": amount("0.0300")})),
                    ..req("POST", "/v2/transactions/calculate-amount")
                },
                response: ok(Some(json!({
                    "currencyCode": "USD",
                    "zeroCostProcessingOption": "Surcharge",
                    "pricingType": "Card",
                    "creditCard": {
                        "baseAmount": amount("100.00"),
                        "surchargeAmount": amount("3.50"),
                        "tipAmount": amount("15.00"),
                        "totalAmount": amount("118.50")}}))),
            },
            live: Live::Test("live_calculate_amount"),
        }],
    },
    Contract {
        operation_id: "flute-v2-post-transactions-credit",
        route: "POST /v2/transactions/credit",
        mapping: Mapping::Command("transactions credit"),
        // `referenceId` is **required** here and optional on `create`, and
        // `creditDetails.cardData` declares no `captureMethod` — a credit is
        // not an authorization.
        variants: &[
            Variant {
                name: "new ACH",
                exchange: || Exchange {
                    request: RequestFixture {
                        body: Some(json!({
                            "paymentProcessorId": "8db2ff47-b143-4adb-ab58-a11111111111",
                            "baseAmount": amount("10.50"),
                            "referenceId": "credit-1",
                            // The conditional ACH rule reaches this endpoint
                            // too: `BillingAddress is required for new ACH
                            // credits.; ContactInfo is required for new ACH
                            // credits.`
                            "billingAddress": {
                                "addressLine1": "1 Main St",
                                "city": "Austin",
                                "stateCode": "TX",
                                "postalCode": "78701",
                                "countryCode": "US"},
                            "contactInfo": {
                                "firstName": "Ada",
                                "lastName": "Lovelace",
                                "mobilePhoneNumber": "+14155552309",
                                "email": "ada@example.com"},
                            "creditDetails": {"achData": {
                                "secCode": "PPD",
                                "requesterIpAddress": "203.0.113.10",
                                "paymentMethodDetails": {
                                    "accountNumber": "123456789",
                                    "routingNumber": "021000021",
                                    "accountType": "Checking",
                                    "accountHolderType": "Personal"}}}
                        })),
                        ..req("POST", "/v2/transactions/credit")
                    },
                    response: ok(Some(json!({
                        "transactionId": "90d084d6-55b8-4fb8-b658-861534d07f9a",
                        "transactionStatus": "Pending",
                        "processedAmount": amount("10.50"),
                        "currencyCode": "USD"}))),
                },
                live: Live::Test("live_ach_credit"),
            },
            Variant {
                // A card credit, beside the ACH one; v1 ships only `ach credit`.
                name: "new card",
                exchange: || Exchange {
                    request: RequestFixture {
                        body: Some(json!({
                            "paymentProcessorId": "8db2ff47-b143-4adb-ab58-a11111111111",
                            "baseAmount": amount("10.50"),
                            "referenceId": "credit-2",
                            "currencyCode": "USD",
                            "customerId": "588f57a5-fe6a-4844-851e-e98914e81980",
                            "billingAddress": {"city": "Austin", "stateCode": "TX"},
                            "contactInfo": {"email": "ada@example.com"},
                            "creditDetails": {"cardData": {
                                "paymentMethodDetails": {
                                    "cardNumber": "4111111111111111",
                                    "securityCode": "123",
                                    "expirationMonth": 12,
                                    "expirationYear": 2032}}}
                        })),
                        ..req("POST", "/v2/transactions/credit")
                    },
                    response: ok(Some(json!({
                        "transactionId": "90d084d6-55b8-4fb8-b658-861534d07f9a",
                        "transactionStatus": "Pending",
                        "processedAmount": amount("10.50"),
                        "currencyCode": "USD"}))),
                },
                live: Live::Test("live_card_credit"),
            },
        ],
    },
    Contract {
        operation_id: "flute-v2-post-pos-transactions",
        route: "POST /v2/pos/transactions",
        mapping: Mapping::Command("pos create"),
        variants: &[
            Variant {
                // `waitForAcceptanceByTerminal` is present even here, because
                // `false` is what "do not hold the response" has to say, and
                // `captureMethod` because it defaults — only the other four
                // keys are the required set.
                name: "required only",
                exchange: || Exchange {
                    request: RequestFixture {
                        body: Some(json!({
                            "terminalId": "b1d4c0a7-3e18-4a6f-8c25-9f7e6d5b4a31",
                            "posDeviceId": "POS-DEVICE-001",
                            "baseAmount": amount("42.75"),
                            "currencyCode": "USD",
                            "captureMethod": "Auto",
                            "waitForAcceptanceByTerminal": false})),
                        ..req("POST", "/v2/pos/transactions")
                    },
                    response: ok(Some(json!({
                        "posTransactionId": "7c9a3d1e-5b42-4f80-9a1c-6d3e2f8b4a05",
                        "posTransactionStatus": "InProgress",
                        "terminalId": "b1d4c0a7-3e18-4a6f-8c25-9f7e6d5b4a31",
                        "posDeviceId": "POS-DEVICE-001",
                        "baseAmount": amount("42.75"),
                        "currencyCode": "USD"}))),
                },
                live: Live::Test("live_pos_create_without_a_reference_id_needs_terminal"),
            },
            Variant {
                name: "fully specified",
                exchange: || Exchange {
                    request: RequestFixture {
                        body: Some(json!({
                            "terminalId": "b1d4c0a7-3e18-4a6f-8c25-9f7e6d5b4a31",
                            "posDeviceId": "POS-DEVICE-001",
                            "baseAmount": amount("42.75"),
                            "currencyCode": "USD",
                            "waitForAcceptanceByTerminal": false,
                            "captureMethod": "Manual",
                            "initiationChannel": "Deeplink",
                            "readingMethod": "KeyedEntry",
                            "pricingType": "Cash",
                            "paymentProcessorId": "8db2ff47-b143-4adb-ab58-a11111111111",
                            "customerId": "588f57a5-fe6a-4844-851e-e98914e81980",
                            "referenceId": "REF-POS-1",
                            "requestPaymentMethodStorageConsent": true,
                            "extraAmounts": {
                                "tipAmount": amount("5.00")}})),
                        ..req("POST", "/v2/pos/transactions")
                    },
                    response: ok(Some(json!({
                        "posTransactionId": "7c9a3d1e-5b42-4f80-9a1c-6d3e2f8b4a05",
                        "posTransactionStatus": "InProgress",
                        "captureMethod": "Manual",
                        "terminalId": "b1d4c0a7-3e18-4a6f-8c25-9f7e6d5b4a31",
                        "posDeviceId": "POS-DEVICE-001",
                        "referenceId": "REF-POS-1",
                        "baseAmount": amount("42.75"),
                        "currencyCode": "USD",
                        "extraAmounts": {
                            "tipAmount": amount("5.00")}}))),
                },
                live: Live::Test("live_pos_create_get_cancel_needs_terminal"),
            },
            Variant {
                // The create half of `--wait`. The get half is a query
                // parameter on a different operation, and the poll that joins
                // them is a sequence rather than an exchange.
                name: "waiting for terminal acceptance",
                exchange: || Exchange {
                    request: RequestFixture {
                        body: Some(json!({
                            "terminalId": "b1d4c0a7-3e18-4a6f-8c25-9f7e6d5b4a31",
                            "posDeviceId": "POS-DEVICE-001",
                            "baseAmount": amount("42.75"),
                            "currencyCode": "USD",
                            "captureMethod": "Auto",
                            "waitForAcceptanceByTerminal": true})),
                        ..req("POST", "/v2/pos/transactions")
                    },
                    response: ok(Some(json!({
                        "posTransactionId": "7c9a3d1e-5b42-4f80-9a1c-6d3e2f8b4a05",
                        "posTransactionStatus": "InProgress",
                        "terminalId": "b1d4c0a7-3e18-4a6f-8c25-9f7e6d5b4a31",
                        "baseAmount": amount("42.75"),
                        "currencyCode": "USD"}))),
                },
                live: Live::Test("live_pos_create_with_wait_ends_on_a_terminal_status_attended"),
            },
        ],
    },
    Contract {
        operation_id: "flute-v2-get-pos-transactions-posTransactionId",
        route: "GET /v2/pos/transactions/{posTransactionId}",
        mapping: Mapping::Command("pos get"),
        variants: &[
            Variant {
                name: "default",
                exchange: || Exchange {
                    request: RequestFixture {
                        path_params: vec![(
                            "posTransactionId".into(),
                            "7c9a3d1e-5b42-4f80-9a1c-6d3e2f8b4a05".into(),
                        )],
                        ..req(
                            "GET",
                            "/v2/pos/transactions/7c9a3d1e-5b42-4f80-9a1c-6d3e2f8b4a05",
                        )
                    },
                    // `posTransactionStatus`, not the `status` the
                    // operation's own example sends: the schema declares the
                    // former under `additionalProperties: false`, and a
                    // schema is normative where an example is not.
                    response: ok(Some(json!({
                        "posTransactionId": "7c9a3d1e-5b42-4f80-9a1c-6d3e2f8b4a05",
                        "posTransactionStatus": "Completed",
                        "terminalId": "b1d4c0a7-3e18-4a6f-8c25-9f7e6d5b4a31",
                        "posDeviceId": "POS-DEVICE-001",
                        "transactionId": "90d084d6-55b8-4fb8-b658-861534d07f9a",
                        "processedAmount": amount("47.75"),
                        "currencyCode": "USD",
                        "createdOn": "2026-06-03T21:59:31.891Z"}))),
                },
                live: Live::Test("live_pos_get_uses_the_declared_status_key_needs_terminal"),
            },
            Variant {
                // The get half of `--wait`: each poll blocks server-side
                // rather than spinning.
                name: "long polling",
                exchange: || Exchange {
                    request: RequestFixture {
                        path_params: vec![(
                            "posTransactionId".into(),
                            "7c9a3d1e-5b42-4f80-9a1c-6d3e2f8b4a05".into(),
                        )],
                        query: vec![("waitForTransactionProcessing".into(), "true".into())],
                        ..req(
                            "GET",
                            "/v2/pos/transactions/7c9a3d1e-5b42-4f80-9a1c-6d3e2f8b4a05",
                        )
                    },
                    response: ok(Some(json!({
                        "posTransactionId": "7c9a3d1e-5b42-4f80-9a1c-6d3e2f8b4a05",
                        "posTransactionStatus": "Completed",
                        "terminalId": "b1d4c0a7-3e18-4a6f-8c25-9f7e6d5b4a31",
                        "processedAmount": amount("47.75"),
                        "currencyCode": "USD"}))),
                },
                live: Live::Test("live_pos_create_with_wait_ends_on_a_terminal_status_attended"),
            },
        ],
    },
    Contract {
        operation_id: "flute-v2-get-pos-transactions",
        route: "GET /v2/pos/transactions",
        mapping: Mapping::Command("pos list"),
        variants: &[
            Variant {
                name: "first page, server defaults",
                exchange: || Exchange {
                    request: req("GET", "/v2/pos/transactions"),
                    response: ok(Some(json!({
                        "items": [{
                            "posTransactionId": "7c9a3d1e-5b42-4f80-9a1c-6d3e2f8b4a05",
                            "posTransactionStatus": "Completed",
                            "terminalId": "b1d4c0a7-3e18-4a6f-8c25-9f7e6d5b4a31",
                            "transactionId": "90d084d6-55b8-4fb8-b658-861534d07f9a",
                            "processedAmount": amount("47.75"),
                            "currencyCode": "USD",
                            "createdOn": "2026-06-03T21:54:31.884Z"}],
                        "pageInfo": {
                            "pageIndex": 0, "pageSize": 20, "totalItems": 1,
                            "totalPages": 1, "hasMore": false}}))),
                },
                live: Live::Test("live_pos_list_first_page"),
            },
            Variant {
                name: "every filter",
                exchange: || Exchange {
                    request: RequestFixture {
                        query: vec![
                            ("pageIndex".into(), "1".into()),
                            ("pageSize".into(), "5".into()),
                            ("sortBy".into(), "createdOn".into()),
                            ("sortOrder".into(), "desc".into()),
                            (
                                "terminalId".into(),
                                "b1d4c0a7-3e18-4a6f-8c25-9f7e6d5b4a31".into(),
                            ),
                            ("fromDate".into(), "2026-01-01T00:00:00Z".into()),
                            ("toDate".into(), "2026-12-31T23:59:59Z".into()),
                            ("posTransactionStatus".into(), "Completed".into()),
                        ],
                        ..req("GET", "/v2/pos/transactions")
                    },
                    response: ok(Some(json!({
                        "items": [],
                        "pageInfo": {"hasMore": false}}))),
                },
                live: Live::Test("live_pos_list_filtered_by_status"),
            },
        ],
    },
    Contract {
        operation_id: "flute-v2-post-pos-transactions-posTransactionId-cancel",
        route: "POST /v2/pos/transactions/{posTransactionId}/cancel",
        mapping: Mapping::Command("pos cancel"),
        variants: &[Variant {
            // No request body, and the response is the create schema — whose
            // own example answers with `status: TransactionProcessing`, a key
            // it does not declare carrying a value outside the declared enum.
            // The fixture follows the schema.
            name: "default",
            exchange: || Exchange {
                request: RequestFixture {
                    path_params: vec![(
                        "posTransactionId".into(),
                        "7c9a3d1e-5b42-4f80-9a1c-6d3e2f8b4a05".into(),
                    )],
                    ..req(
                        "POST",
                        "/v2/pos/transactions/7c9a3d1e-5b42-4f80-9a1c-6d3e2f8b4a05/cancel",
                    )
                },
                response: ok(Some(json!({
                    "posTransactionId": "7c9a3d1e-5b42-4f80-9a1c-6d3e2f8b4a05",
                    "posTransactionStatus": "Cancelled"}))),
            },
            live: Live::Test(
                "live_pos_cancel_twice_reports_the_second_as_a_state_error_needs_terminal",
            ),
        }],
    },
    Contract {
        operation_id: "flute-v2-post-pos-transactions-posTransactionId-print-receipt",
        route: "POST /v2/pos/transactions/{posTransactionId}/print-receipt",
        mapping: Mapping::Command("pos print-receipt"),
        variants: &[Variant {
            name: "default",
            exchange: || Exchange {
                request: RequestFixture {
                    path_params: vec![(
                        "posTransactionId".into(),
                        "7c9a3d1e-5b42-4f80-9a1c-6d3e2f8b4a05".into(),
                    )],
                    body: Some(json!({
                        "terminalId": "b1d4c0a7-3e18-4a6f-8c25-9f7e6d5b4a31"})),
                    ..req(
                        "POST",
                        "/v2/pos/transactions/7c9a3d1e-5b42-4f80-9a1c-6d3e2f8b4a05/print-receipt",
                    )
                },
                response: ResponseFixture {
                    status: 200,
                    body: None,
                },
            },
            live: Live::Test("live_pos_print_receipt_attended"),
        }],
    },
    Contract {
        operation_id: "flute-v2-get-terminals",
        route: "GET /v2/terminals",
        mapping: Mapping::Command("terminals list"),
        variants: &[
            Variant {
                name: "first page, server defaults",
                exchange: || Exchange {
                    request: req("GET", "/v2/terminals"),
                    // `terminalStatus` is `Active` here and `Ready` in the
                    // operation's own example: the response enum declares
                    // `Active`, `Busy`, `Offline`, and a schema is normative
                    // where an example is not.
                    response: ok(Some(json!({
                        "items": [{
                            "terminalId": "b1d4c0a7-3e18-4a6f-8c25-9f7e6d5b4a31",
                            "merchantId": "8db2ff47-b143-4adb-ab58-a11111111111",
                            "merchantCompanyName": "Analytical Engines",
                            "serialNumber": "SN100001",
                            "terminalManufacturer": "Sunmi",
                            "terminalModel": "SunmiP2",
                            "terminalMode": "SemiIntegrated",
                            "terminalStatus": "Active",
                            "connectionStatus": "Online",
                            "lastSeenOn": "2026-08-11T18:44:45.638Z"}],
                        "pageInfo": {
                            "pageIndex": 0, "pageSize": 20, "totalItems": 1,
                            "totalPages": 1, "hasMore": false}}))),
                },
                live: Live::Test("live_terminal_list_first_page"),
            },
            Variant {
                // All nine declared parameters at once. `terminalStatus`
                // takes `Ready` here, because the *query* parameter declares
                // `Ready`, `Busy`, `Offline` — a different enum from the
                // response field of the same name.
                name: "every filter",
                exchange: || Exchange {
                    request: RequestFixture {
                        query: vec![
                            ("pageIndex".into(), "1".into()),
                            ("pageSize".into(), "5".into()),
                            ("sortBy".into(), "terminalManufacturer".into()),
                            ("sortOrder".into(), "desc".into()),
                            ("terminalStatus".into(), "Ready".into()),
                            ("terminalMode".into(), "SemiIntegrated".into()),
                            ("connectionStatus".into(), "Online".into()),
                            ("serialNumber".into(), "SN100001".into()),
                            ("search".into(), "front counter".into()),
                        ],
                        ..req("GET", "/v2/terminals")
                    },
                    response: ok(Some(json!({
                        "items": [],
                        "pageInfo": {"hasMore": false}}))),
                },
                live: Live::Test("live_terminal_list_filtered_by_the_declared_query_status"),
            },
        ],
    },
    Contract {
        operation_id: "flute-v2-get-terminals-terminalId-status",
        route: "GET /v2/terminals/{terminalId}/status",
        mapping: Mapping::Command("terminals status"),
        variants: &[Variant {
            name: "default",
            exchange: || Exchange {
                request: RequestFixture {
                    path_params: vec![(
                        "terminalId".into(),
                        "b1d4c0a7-3e18-4a6f-8c25-9f7e6d5b4a31".into(),
                    )],
                    ..req(
                        "GET",
                        "/v2/terminals/b1d4c0a7-3e18-4a6f-8c25-9f7e6d5b4a31/status",
                    )
                },
                response: ok(Some(json!({
                    "terminalId": "b1d4c0a7-3e18-4a6f-8c25-9f7e6d5b4a31",
                    "merchantId": "8db2ff47-b143-4adb-ab58-a11111111111",
                    "terminalStatus": "Active",
                    "lastSeenOn": "2026-08-11T18:44:45.645Z",
                    "debitPinKey": "Injected",
                    "terminalAppVersion": "1.0.0",
                    "connectionStatus": "Online",
                    "connectionType": "WiFi",
                    "wifiConnectionStrength": 85,
                    "mobileConnectionStrength": 0,
                    "batteryLevel": 90,
                    "printerStatus": "Normal"}))),
            },
            live: Live::Test("live_terminal_status_reports_a_declared_status_needs_terminal"),
        }],
    },
    Contract {
        operation_id: "flute-v2-get-settlements-batches",
        route: "GET /v2/settlements/batches",
        // Two commands reach this one operation: `list` and the `get` that
        // has no endpoint of its own. `list` is named here because it is the
        // command carrying the flags the surface matrix asks `--help` about.
        mapping: Mapping::Command("settlements list"),
        variants: &[
            Variant {
                name: "first page, server defaults",
                exchange: || Exchange {
                    request: req("GET", "/v2/settlements/batches"),
                    response: ok(Some(json!({
                        "items": [{
                            "batchId": "21c75430-a316-456f-9126-365760dca33a",
                            "paymentProcessorId": "8db2ff47-b143-4adb-ab58-a11111111111",
                            "paymentProcessorName": "TSYS",
                            "externalBatchId": "BATCH-001",
                            "createdOn": "2026-03-23T12:00:00Z",
                            "transactionCount": 15,
                            "totalNetAmount": amount("1250.00"),
                            "totalSalesAmount": amount("1300.00"),
                            "totalRefundsAmount": amount("50.00"),
                            "batchStatus": "Settled"}],
                        "pageInfo": {
                            "pageIndex": 0, "pageSize": 20, "totalItems": 1,
                            "totalPages": 1, "hasMore": false}}))),
                },
                live: Live::Test("live_settlement_list_first_page"),
            },
            Variant {
                // All nine declared parameters, including the API's only two
                // array-typed ones. An array parameter is repeated pairs on
                // the wire, so `batchIds` appears twice.
                name: "every filter",
                exchange: || Exchange {
                    request: RequestFixture {
                        query: vec![
                            ("pageIndex".into(), "1".into()),
                            ("pageSize".into(), "5".into()),
                            ("sortBy".into(), "createdOn".into()),
                            ("sortOrder".into(), "asc".into()),
                            ("fromDate".into(), "2026-01-01T00:00:00Z".into()),
                            ("toDate".into(), "2026-12-31T23:59:59Z".into()),
                            (
                                "batchIds".into(),
                                "21c75430-a316-456f-9126-365760dca33a".into(),
                            ),
                            (
                                "batchIds".into(),
                                "0f19d58c-3d4e-4f5a-6b7c-8d9e0f1a2b58".into(),
                            ),
                            (
                                "paymentProcessorIds".into(),
                                "8db2ff47-b143-4adb-ab58-a11111111111".into(),
                            ),
                            ("batchStatus".into(), "Open".into()),
                        ],
                        ..req("GET", "/v2/settlements/batches")
                    },
                    response: ok(Some(json!({
                        "items": [],
                        "pageInfo": {"hasMore": false}}))),
                },
                live: Live::Test("live_settlement_list_filtered_by_repeated_array_parameters"),
            },
            Variant {
                // What `settlements get` sends: the documented filter,
                // applied server-side. A batch beyond the first page would be
                // reported missing by a client-side scan of page zero.
                name: "one batch by id",
                exchange: || Exchange {
                    request: RequestFixture {
                        query: vec![(
                            "batchIds".into(),
                            "21c75430-a316-456f-9126-365760dca33a".into(),
                        )],
                        ..req("GET", "/v2/settlements/batches")
                    },
                    response: ok(Some(json!({
                        "items": [{
                            "batchId": "21c75430-a316-456f-9126-365760dca33a",
                            "paymentProcessorId": "8db2ff47-b143-4adb-ab58-a11111111111",
                            "paymentProcessorName": "TSYS",
                            "externalBatchId": "BATCH-001",
                            "createdOn": "2026-03-23T12:00:00Z",
                            "transactionCount": 15,
                            "totalNetAmount": amount("1250.00"),
                            "totalSalesAmount": amount("1300.00"),
                            "totalRefundsAmount": amount("50.00"),
                            "batchStatus": "Settled"}],
                        "pageInfo": {
                            "pageIndex": 0, "pageSize": 20, "totalItems": 1,
                            "totalPages": 1, "hasMore": false}}))),
                },
                live: Live::Test("live_settlement_get_reads_one_batch_by_id"),
            },
        ],
    },
    Contract {
        operation_id: "flute-v2-post-settlements-batches-close",
        route: "POST /v2/settlements/batches/close",
        mapping: Mapping::Command("settlements close"),
        variants: &[Variant {
            // One required field, and a response that carries the resulting
            // batch status and nothing else — no batch id.
            name: "default",
            exchange: || Exchange {
                request: RequestFixture {
                    body: Some(json!({
                        "paymentProcessorId": "8db2ff47-b143-4adb-ab58-a11111111111"})),
                    ..req("POST", "/v2/settlements/batches/close")
                },
                response: ok(Some(json!({"batchStatus": "PendingSettlement"}))),
            },
            live: Live::Test("live_settlement_close_the_open_batch_irreversible"),
        }],
    },
    Contract {
        operation_id: "flute-v2-get-settings-payment-config",
        route: "GET /v2/settings/payment-config",
        mapping: Mapping::Command("settings payment-config"),
        variants: &[Variant {
            name: "default",
            exchange: || Exchange {
                request: req("GET", "/v2/settings/payment-config"),
                response: ok(Some(json!({
                    "zeroCostProcessingOption": "None",
                    "defaultTipsOptions": [amount("10"), amount("15"), amount("20")],
                    "defaultSurchargeRate": amount("1.5"),
                    "availableCurrencies": ["USD"],
                    "availableCardTypes": ["Visa", "MasterCard"],
                    "availableTransactionTypes": ["Sale", "Refund"],
                    "isTipsEnabled": true,
                    "availablePaymentProcessors": [{
                        "paymentProcessorId": "8db2ff47-b143-4adb-ab58-a11111111111",
                        "processorName": "TSYS",
                        "isDefault": true,
                        "type": "Tsys",
                        "settlementBatchTimeSlots": [{
                            "hours": 2, "minutes": 10,
                            "timezoneName": "America/New_York"}]}],
                    "addressVerificationServiceOptions": {
                        "isEnabled": true, "profile": "Strict"},
                    "isCustomerCardSavingByTerminalEnabled": true,
                    "companyName": "Analytical Engines",
                    "mccCode": "5411",
                    "currency": "USD",
                    "maxTransactionAmount": amount("10000.00")}))),
            },
            live: Live::Test("live_settings_payment_config_lists_the_processor_ids"),
        }],
    },
    Contract {
        operation_id: "flute-v2-get-settings-contact-information",
        route: "GET /v2/settings/contact-information",
        mapping: Mapping::Command("settings contact-info"),
        variants: &[Variant {
            name: "default",
            exchange: || Exchange {
                request: req("GET", "/v2/settings/contact-information"),
                response: ok(Some(json!({
                    "contactInfos": [{
                        "contactInfoId": "8d638f27-1a16-4f1b-92fe-0ea16d4a6461",
                        "addressName": "Main Office",
                        "email": "contact@example.com",
                        "mobilePhoneNumber": "+14155552309",
                        "isMainAddress": true,
                        "isDefaultAddress": true,
                        "addressLine1": "1 Main St",
                        "city": "Austin",
                        "stateCode": "TX",
                        "postalCode": "78701",
                        "countryCode": "US"}]}))),
            },
            live: Live::Test("live_settings_contact_info"),
        }],
    },
    Contract {
        operation_id: "flute-v2-get-settings-transaction-autofill",
        route: "GET /v2/settings/transaction-autofill",
        mapping: Mapping::Command("settings autofill"),
        variants: &[Variant {
            name: "default",
            exchange: || Exchange {
                request: req("GET", "/v2/settings/transaction-autofill"),
                response: ok(Some(json!({
                    "level2Settings": {"taxRate": amount("8.5")},
                    "level3Settings": {
                        "product": {
                            "productName": "Office Supplies",
                            "code": "OFF001",
                            "unitPrice": amount("25.00"),
                            "measurementUnit": "pcs",
                            "quantity": amount("10"),
                            "discountPercentage": amount("5"),
                            "description": "Standard office supplies"},
                        "shippingChargeRate": amount("5"),
                        "dutyChargeRate": amount("2.5")}}))),
            },
            live: Live::Test("live_settings_autofill_read"),
        }],
    },
    Contract {
        operation_id: "flute-v2-patch-settings-transaction-autofill",
        route: "PATCH /v2/settings/transaction-autofill",
        mapping: Mapping::Command("settings update-autofill"),
        variants: &[
            Variant {
                // One field: the schema declares nothing required, so a PATCH
                // of a single rate is the smallest legal body — and the only
                // shape that can show the rest are optional.
                name: "one rate",
                exchange: || Exchange {
                    request: RequestFixture {
                        body: Some(json!({"level2Settings": {"taxRate": amount("7.25")}})),
                        ..req("PATCH", "/v2/settings/transaction-autofill")
                    },
                    response: ResponseFixture {
                        status: 200,
                        body: None,
                    },
                },
                live: Live::Test("live_settings_update_autofill_round_trip"),
            },
            Variant {
                name: "every field",
                exchange: || Exchange {
                    request: RequestFixture {
                        body: Some(json!({
                            "level2Settings": {"taxRate": amount("8.5")},
                            "level3Settings": {
                                "shippingChargeRate": amount("5.0"),
                                "dutyChargeRate": amount("2.5"),
                                "product": {
                                    "productName": "Office Supplies",
                                    "code": "OFF001",
                                    "measurementUnit": "pcs",
                                    "unitPrice": amount("25.00"),
                                    "quantity": amount("10.0"),
                                    "discountPercentage": amount("5.0")}}})),
                        ..req("PATCH", "/v2/settings/transaction-autofill")
                    },
                    response: ResponseFixture {
                        status: 200,
                        body: None,
                    },
                },
                live: Live::Test("live_settings_update_autofill_round_trip"),
            },
        ],
    },
    Contract {
        operation_id: "flute-v2-post-payment-links",
        route: "POST /v2/payment-links",
        mapping: Mapping::Command("payment-links create"),
        variants: &[
            Variant {
                // `paymentMethods` is the only field the *schema* requires,
                // and naming a method with no configuration under it is how
                // it documents offering that method with defaults. The
                // currency is here because the **API** requires it too,
                // unconditionally, and answers
                // `currencyCode must be a valid ISO 4217 currency code` for a
                // key that was never sent. So this is the smallest request
                // that works rather than the smallest the schema allows.
                name: "required only",
                exchange: || Exchange {
                    request: RequestFixture {
                        body: Some(json!({
                            "paymentMethods": {"card": {"enabled": true}},
                            "currencyCode": "USD"})),
                        ..req("POST", "/v2/payment-links")
                    },
                    response: ok(Some(json!({
                        "paymentLinkId": "3f8b2c1d-6e40-4a92-b7c5-1d2e3f4a5b60",
                        "linkType": "SingleUse",
                        "paymentMethods": {"card": {"enabled": true}},
                        "paymentLinkStatus": "Active",
                        "shortUrl": "https://pay.example.com/l/abc123",
                        "paymentCount": 0,
                        "totalCollectedAmount": amount("0"),
                        "createdOn": "2026-08-12T09:15:22.100Z",
                        "modifiedOn": "2026-08-12T09:15:22.100Z"}))),
                },
                live: Live::Test("live_payment_link_without_an_amount_is_flexible"),
            },
            Variant {
                name: "fully specified",
                exchange: || Exchange {
                    request: RequestFixture {
                        body: Some(json!({
                            "paymentMethods": {
                                "card": {
                                    "enabled": true,
                                    "processorId": "8db2ff47-b143-4adb-ab58-a11111111111"},
                                "ach": {
                                    "enabled": true,
                                    "processorId": "6bbfbe3e-04dd-41cd-82bf-1466e0159007"}},
                            "baseAmount": amount("25.00"),
                            "currencyCode": "USD",
                            "linkType": "MultiUse",
                            "customerId": "588f57a5-fe6a-4844-851e-e98914e81980",
                            "referenceId": "ORDER-1042",
                            "name": "Spring campaign",
                            "description": "Shared with the campaign team",
                            "expiresOn": "2026-09-15T00:00:00.000Z"})),
                        ..req("POST", "/v2/payment-links")
                    },
                    response: ok(Some(json!({
                        "paymentLinkId": "3f8b2c1d-6e40-4a92-b7c5-1d2e3f4a5b60",
                        "linkType": "MultiUse",
                        "paymentMethods": {
                            "card": {"enabled": true},
                            "ach": {"enabled": true}},
                        "baseAmount": amount("25.00"),
                        "currencyCode": "USD",
                        "paymentLinkStatus": "Active",
                        "name": "Spring campaign",
                        "description": "Shared with the campaign team",
                        "shortUrl": "https://pay.example.com/l/abc123",
                        "customerId": "588f57a5-fe6a-4844-851e-e98914e81980",
                        "referenceId": "ORDER-1042",
                        "expiresOn": "2026-09-15T00:00:00.000Z",
                        "paymentCount": 0,
                        "totalCollectedAmount": amount("0"),
                        "createdOn": "2026-08-12T09:15:22.100Z",
                        "modifiedOn": "2026-08-12T09:15:22.100Z"}))),
                },
                live: Live::Test("live_payment_link_create_get_update_delete"),
            },
        ],
    },
    Contract {
        operation_id: "flute-v2-get-payment-links-paymentLinkId",
        route: "GET /v2/payment-links/{paymentLinkId}",
        mapping: Mapping::Command("payment-links get"),
        variants: &[Variant {
            name: "default",
            exchange: || Exchange {
                request: RequestFixture {
                    path_params: vec![(
                        "paymentLinkId".into(),
                        "3f8b2c1d-6e40-4a92-b7c5-1d2e3f4a5b60".into(),
                    )],
                    ..req(
                        "GET",
                        "/v2/payment-links/3f8b2c1d-6e40-4a92-b7c5-1d2e3f4a5b60",
                    )
                },
                response: ok(Some(json!({
                    "paymentLinkId": "3f8b2c1d-6e40-4a92-b7c5-1d2e3f4a5b60",
                    "linkType": "SingleUse",
                    "paymentMethods": {"card": {"enabled": true}},
                    "baseAmount": amount("25.00"),
                    "currencyCode": "USD",
                    "paymentLinkStatus": "Active",
                    "name": "Spring campaign",
                    "shortUrl": "https://pay.example.com/l/abc123",
                    "customerFirstName": "Ada",
                    "customerLastName": "Lovelace",
                    "paymentCount": 3,
                    "totalCollectedAmount": amount("75.00"),
                    "createdOn": "2026-08-12T09:15:22.100Z",
                    "lastPaymentOn": "2026-08-14T11:02:00.000Z",
                    "modifiedOn": "2026-08-14T11:02:00.000Z"}))),
            },
            live: Live::Test("live_payment_link_create_get_update_delete"),
        }],
    },
    Contract {
        operation_id: "flute-v2-get-payment-links",
        route: "GET /v2/payment-links",
        mapping: Mapping::Command("payment-links list"),
        variants: &[
            Variant {
                name: "first page, server defaults",
                exchange: || Exchange {
                    request: req("GET", "/v2/payment-links"),
                    response: ok(Some(json!({
                        "items": [{
                            "paymentLinkId": "3f8b2c1d-6e40-4a92-b7c5-1d2e3f4a5b60",
                            "linkType": "SingleUse",
                            "paymentMethods": {"card": {"enabled": true}},
                            "baseAmount": amount("25.00"),
                            "currencyCode": "USD",
                            "paymentLinkStatus": "Active",
                            "name": "Spring campaign",
                            "shortUrl": "https://pay.example.com/l/abc123",
                            "paymentCount": 0,
                            "totalCollectedAmount": amount("0"),
                            "createdOn": "2026-08-12T09:15:22.100Z",
                            "modifiedOn": "2026-08-12T09:15:22.100Z"}],
                        "pageInfo": {
                            "pageIndex": 0, "pageSize": 20, "totalItems": 1,
                            "totalPages": 1, "hasMore": false}}))),
                },
                live: Live::Test("live_payment_link_list_first_page"),
            },
            Variant {
                name: "every filter",
                exchange: || Exchange {
                    request: RequestFixture {
                        query: vec![
                            ("pageIndex".into(), "1".into()),
                            ("pageSize".into(), "5".into()),
                            ("sortOrder".into(), "desc".into()),
                            ("sortBy".into(), "createdOn".into()),
                            ("search".into(), "spring".into()),
                            ("linkType".into(), "SingleUse".into()),
                            ("paymentLinkStatus".into(), "Active".into()),
                        ],
                        ..req("GET", "/v2/payment-links")
                    },
                    response: ok(Some(json!({
                        "items": [],
                        "pageInfo": {"hasMore": false}}))),
                },
                live: Live::Test("live_payment_link_list_filtered"),
            },
        ],
    },
    Contract {
        operation_id: "flute-v2-patch-payment-links-paymentLinkId",
        route: "PATCH /v2/payment-links/{paymentLinkId}",
        mapping: Mapping::Command("payment-links update"),
        variants: &[
            Variant {
                name: "rename",
                exchange: || Exchange {
                    request: RequestFixture {
                        path_params: vec![(
                            "paymentLinkId".into(),
                            "3f8b2c1d-6e40-4a92-b7c5-1d2e3f4a5b60".into(),
                        )],
                        body: Some(json!({"name": "Spring campaign (extended)"})),
                        ..req(
                            "PATCH",
                            "/v2/payment-links/3f8b2c1d-6e40-4a92-b7c5-1d2e3f4a5b60",
                        )
                    },
                    response: ok(Some(json!({
                        "paymentLinkId": "3f8b2c1d-6e40-4a92-b7c5-1d2e3f4a5b60",
                        "linkType": "SingleUse",
                        "paymentMethods": {"card": {"enabled": true}},
                        "paymentLinkStatus": "Active",
                        "name": "Spring campaign (extended)",
                        "paymentCount": 0,
                        "totalCollectedAmount": amount("0"),
                        "createdOn": "2026-08-12T09:15:22.100Z",
                        "modifiedOn": "2026-08-14T11:02:00.000Z"}))),
                },
                live: Live::Test("live_payment_link_create_get_update_delete"),
            },
            Variant {
                // The merge-patch half: an explicit null clears a clearable
                // field, which an omitted key cannot express.
                name: "clearing a field",
                exchange: || Exchange {
                    request: RequestFixture {
                        path_params: vec![(
                            "paymentLinkId".into(),
                            "3f8b2c1d-6e40-4a92-b7c5-1d2e3f4a5b60".into(),
                        )],
                        body: Some(json!({"description": null})),
                        ..req(
                            "PATCH",
                            "/v2/payment-links/3f8b2c1d-6e40-4a92-b7c5-1d2e3f4a5b60",
                        )
                    },
                    response: ok(Some(json!({
                        "paymentLinkId": "3f8b2c1d-6e40-4a92-b7c5-1d2e3f4a5b60",
                        "linkType": "SingleUse",
                        "paymentMethods": {"card": {"enabled": true}},
                        "paymentLinkStatus": "Active",
                        "description": null,
                        "paymentCount": 0,
                        "totalCollectedAmount": amount("0"),
                        "createdOn": "2026-08-12T09:15:22.100Z",
                        "modifiedOn": "2026-08-14T11:02:00.000Z"}))),
                },
                live: Live::Test("live_payment_link_update_can_clear_a_field"),
            },
        ],
    },
    Contract {
        operation_id: "flute-v2-delete-payment-links-paymentLinkId",
        route: "DELETE /v2/payment-links/{paymentLinkId}",
        mapping: Mapping::Command("payment-links delete"),
        variants: &[Variant {
            // **204**, not the 200 every other delete in the API answers with.
            name: "default",
            exchange: || Exchange {
                request: RequestFixture {
                    path_params: vec![(
                        "paymentLinkId".into(),
                        "3f8b2c1d-6e40-4a92-b7c5-1d2e3f4a5b60".into(),
                    )],
                    ..req(
                        "DELETE",
                        "/v2/payment-links/3f8b2c1d-6e40-4a92-b7c5-1d2e3f4a5b60",
                    )
                },
                response: ResponseFixture {
                    status: 204,
                    body: None,
                },
            },
            live: Live::Test("live_payment_link_delete_answers_no_content"),
        }],
    },
    Contract {
        operation_id: "flute-v2-post-payment-links-paymentLinkId-share",
        route: "POST /v2/payment-links/{paymentLinkId}/share",
        mapping: Mapping::Command("payment-links share"),
        variants: &[Variant {
            // Also **204**. `shareBy` here declares a clean two-value enum,
            // unlike `SendReceiptRequestDto.shareBy` on the transaction
            // receipt, which declares an E.164 pattern its own example fails.
            name: "by sms",
            exchange: || Exchange {
                request: RequestFixture {
                    path_params: vec![(
                        "paymentLinkId".into(),
                        "3f8b2c1d-6e40-4a92-b7c5-1d2e3f4a5b60".into(),
                    )],
                    body: Some(json!({
                        "shareBy": "Sms",
                        "recipient": "+14155552309",
                        "hasCustomerConsent": true})),
                    ..req(
                        "POST",
                        "/v2/payment-links/3f8b2c1d-6e40-4a92-b7c5-1d2e3f4a5b60/share",
                    )
                },
                response: ResponseFixture {
                    status: 204,
                    body: None,
                },
            },
            live: Live::Test("live_payment_link_share_attended"),
        }],
    },
    Contract {
        operation_id: "flute-v2-post-payment-sessions",
        route: "POST /v2/payment-sessions",
        mapping: Mapping::Command("payment-sessions create"),
        variants: &[
            Variant {
                // `CreatePaymentSessionRequestDto` declares **nothing**
                // required, and an absent `amount` is documented as a
                // flexible-amount session set at checkout — so an empty body
                // is the smallest legal request this endpoint takes.
                name: "flexible amount",
                exchange: || Exchange {
                    request: RequestFixture {
                        body: Some(json!({})),
                        ..req("POST", "/v2/payment-sessions")
                    },
                    response: ok(Some(json!({
                        "id": "9c1f7a3e-2b58-4d06-8e93-4a5b6c7d8e90",
                        "paymentMethods": {"card": {"enabled": true}}}))),
                },
                live: Live::Test("live_payment_session_with_no_amount_is_flexible"),
            },
            Variant {
                // A vault-only session, where the documented rule is that the
                // amount must be **exactly zero** rather than absent.
                name: "vault only",
                exchange: || Exchange {
                    request: RequestFixture {
                        body: Some(json!({
                            "mode": "SaveMethod",
                            "amount": amount("0"),
                            "customerHandling": "CreateCustomer"})),
                        ..req("POST", "/v2/payment-sessions")
                    },
                    response: ok(Some(json!({
                        "id": "9c1f7a3e-2b58-4d06-8e93-4a5b6c7d8e90",
                        "paymentMethods": {"card": {"enabled": true}}}))),
                },
                live: Live::Test("live_payment_session_vault_only_sends_a_zero_amount"),
            },
            Variant {
                name: "fully specified",
                exchange: || Exchange {
                    request: RequestFixture {
                        body: Some(json!({
                            "mode": "PaymentAndSave",
                            // Not the schema's own example of 64.99: `amount`
                            // declares `multipleOf: 0.01`, which a float
                            // validator applies unevenly, and 64.99 is one of
                            // the values it rejects. The CLI sends what it is
                            // given either way; the fixture picks an amount
                            // whose conformance says something about the CLI.
                            "amount": amount("25.00"),
                            "tipAmount": amount("14.50"),
                            "customerId": "588f57a5-fe6a-4844-851e-e98914e81980",
                            "customerHandling": "TokenOnly",
                            "referenceId": "ORDER-10001",
                            "returnUrl": "https://example.com/done",
                            "skipAddressVerification": true,
                            "pageName": "Analytical Engines checkout",
                            "paymentNotes": "Repair deposit",
                            "afterCompletionMessage": "Thank you.",
                            "expiresAt": "2027-02-19T20:24:52.934Z",
                            "metadata": {"orderId": "9921"},
                            "paymentMethods": {
                                "card": {
                                    "enabled": true,
                                    "processorId": "8db2ff47-b143-4adb-ab58-a11111111111"},
                                "ach": {
                                    "enabled": true,
                                    "processorId": "6bbfbe3e-04dd-41cd-82bf-1466e0159007"}}})),
                        ..req("POST", "/v2/payment-sessions")
                    },
                    response: ok(Some(json!({
                        "id": "9c1f7a3e-2b58-4d06-8e93-4a5b6c7d8e90",
                        "paymentMethods": {
                            "card": {"enabled": true},
                            "ach": {"enabled": true}}}))),
                },
                live: Live::Test("live_payment_session_metadata_round_trip"),
            },
        ],
    },
    Contract {
        operation_id: "flute-v2-get-payment-sessions-paymentSessionId",
        route: "GET /v2/payment-sessions/{paymentSessionId}",
        mapping: Mapping::Command("payment-sessions get"),
        variants: &[Variant {
            // `GetPaymentSessionResponseDto` declares no identifier of any
            // kind, so a read cannot be correlated back to the session that
            // was read. The fixture carries what the schema declares.
            name: "default",
            exchange: || Exchange {
                request: RequestFixture {
                    path_params: vec![(
                        "paymentSessionId".into(),
                        "9c1f7a3e-2b58-4d06-8e93-4a5b6c7d8e90".into(),
                    )],
                    ..req(
                        "GET",
                        "/v2/payment-sessions/9c1f7a3e-2b58-4d06-8e93-4a5b6c7d8e90",
                    )
                },
                response: ok(Some(json!({
                    "status": "Created",
                    "mode": "Payment",
                    "skipAddressVerification": false,
                    "customerId": "588f57a5-fe6a-4844-851e-e98914e81980",
                    "referenceId": "ORDER-10001",
                    "tipAmount": amount("14.50"),
                    "returnUrl": "https://example.com/done",
                    "pageName": "Analytical Engines checkout",
                    "paymentNotes": "Repair deposit",
                    "afterCompletionMessage": "Thank you.",
                    "expiresAt": "2027-02-19T20:24:52.934Z",
                    "surchargeAmount": amount("1.95"),
                    "metadata": {"orderId": "9921"},
                    "paymentMethods": {"card": {"enabled": true}}}))),
            },
            live: Live::Test("live_payment_session_get_carries_no_identifier"),
        }],
    },
    Contract {
        operation_id: "flute-v2-post-payment-sessions-paymentSessionId-cancel",
        route: "POST /v2/payment-sessions/{paymentSessionId}/cancel",
        mapping: Mapping::Command("payment-sessions cancel"),
        variants: &[Variant {
            // No request body, 200 with no response body, and the one
            // destructive verb in the API that is neither a delete nor a
            // revoke.
            name: "default",
            exchange: || Exchange {
                request: RequestFixture {
                    path_params: vec![(
                        "paymentSessionId".into(),
                        "9c1f7a3e-2b58-4d06-8e93-4a5b6c7d8e90".into(),
                    )],
                    ..req(
                        "POST",
                        "/v2/payment-sessions/9c1f7a3e-2b58-4d06-8e93-4a5b6c7d8e90/cancel",
                    )
                },
                response: ResponseFixture {
                    status: 200,
                    body: None,
                },
            },
            live: Live::Test(
                "live_payment_session_cancel_twice_reports_the_second_as_a_state_error",
            ),
        }],
    },
    Contract {
        operation_id: "flute-v2-post-api-keys",
        route: "POST /v2/api-keys",
        mapping: Mapping::Command("api-keys create"),
        variants: &[Variant {
            // Both fields are required, so there is one request shape. The
            // response carries the client secret, which the schema documents
            // as returned here and nowhere else.
            name: "default",
            exchange: || Exchange {
                request: RequestFixture {
                    body: Some(json!({
                        "merchantId": "8db2ff47-b143-4adb-ab58-a11111111111",
                        "apiKeyName": "Production API Key"})),
                    ..req("POST", "/v2/api-keys")
                },
                response: ok(Some(json!({
                    "clientId": "aa98aa89-07ae-4a23-b441-48725f0386e6",
                    "clientSecret": "1f80d6e1-9901-48b6-a862-6cfad1612e99"}))),
            },
            live: Live::Test("live_api_key_create_list_revoke_needs_partner"),
        }],
    },
    Contract {
        operation_id: "flute-v2-get-api-keys",
        route: "GET /v2/api-keys",
        mapping: Mapping::Command("api-keys list"),
        variants: &[
            Variant {
                // **Not paginated.** `GetApiKeysResponseDto` declares one
                // property, `apiKeys`, and no `pageInfo` — so there is no
                // page query to send and none to report.
                name: "every key",
                exchange: || Exchange {
                    request: req("GET", "/v2/api-keys"),
                    response: ok(Some(json!({
                        "apiKeys": [{
                            "clientId": "aa98aa89-07ae-4a23-b441-48725f0386e6",
                            "merchantId": "8db2ff47-b143-4adb-ab58-a11111111111",
                            "apiKeyName": "Production API Key"}]}))),
                },
                live: Live::Test("live_api_key_list_without_a_merchant_filter_needs_partner"),
            },
            Variant {
                name: "for one merchant",
                exchange: || Exchange {
                    request: RequestFixture {
                        query: vec![(
                            "merchantId".into(),
                            "8db2ff47-b143-4adb-ab58-a11111111111".into(),
                        )],
                        ..req("GET", "/v2/api-keys")
                    },
                    response: ok(Some(json!({
                        "apiKeys": [{
                            "clientId": "aa98aa89-07ae-4a23-b441-48725f0386e6",
                            "merchantId": "8db2ff47-b143-4adb-ab58-a11111111111",
                            "apiKeyName": "Production API Key"}]}))),
                },
                live: Live::Test("live_api_key_create_list_revoke_needs_partner"),
            },
        ],
    },
    Contract {
        operation_id: "flute-v2-delete-api-keys-clientId",
        route: "DELETE /v2/api-keys/{clientId}",
        mapping: Mapping::Command("api-keys revoke"),
        variants: &[Variant {
            name: "default",
            exchange: || Exchange {
                request: RequestFixture {
                    path_params: vec![(
                        "clientId".into(),
                        "aa98aa89-07ae-4a23-b441-48725f0386e6".into(),
                    )],
                    ..req(
                        "DELETE",
                        "/v2/api-keys/aa98aa89-07ae-4a23-b441-48725f0386e6",
                    )
                },
                response: ResponseFixture {
                    status: 200,
                    body: None,
                },
            },
            live: Live::Test("live_api_key_revoke_twice_is_still_success_needs_partner"),
        }],
    },
    Contract {
        operation_id: "get-oauth-token",
        route: "POST /oauth2/token",
        mapping: Mapping::Internal("obtained by the client on every authenticated request"),
        // No command reaches it, and it is still an exchange the CLI makes on
        // every authenticated call — so it carries a fixture like the rest,
        // and conformance validates the form body against the bundle.
        variants: &[Variant {
            name: "client credentials",
            exchange: || Exchange {
                request: RequestFixture {
                    content_type: Some("application/x-www-form-urlencoded".into()),
                    body: Some(json!({
                        "grant_type": "client_credentials",
                        "client_id": "test-id",
                        "client_secret": "test-secret",
                        "scope": "offline_access"})),
                    ..req("POST", "/oauth2/token")
                },
                response: ok(Some(json!({
                    "access_token": "tok-xyz",
                    "expires_in": 3600,
                    "token_type": "Bearer"}))),
            },
            live: Live::Skip(
                "every live scenario opens with this exchange, so a malformed \
                 one fails the whole live pass rather than one scenario",
            ),
        }],
    },
];
