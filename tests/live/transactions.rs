//! Live scenarios for the rest of `transactions`. **Committed.**

use crate::*;

/// Charging an instrument that was stored earlier — the route v1 reached with
/// `--payment-method-id` on four commands.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_saved_card_sale() {
    let stored = json(&[
        "payment-methods",
        "add-card",
        "--card",
        "4111111111111111",
        "--cvv",
        "123",
        "--exp",
        "12/2033",
    ]);
    let payment_method_id = stored["data"]["paymentMethodId"]
        .as_str()
        .unwrap()
        .to_string();

    let v = json(&[
        "transactions",
        "create",
        "--payment-processor-id",
        &card_processor_id(),
        "--amount",
        &unique_amount(),
        "--instrument",
        "card",
        "--payment-method-id",
        &payment_method_id,
        "--reference-id",
        &unique_reference_id(),
    ]);
    assert!(v["data"]["transactionId"].is_string(), "{v}");

    live_bin()
        .args(["payment-methods", "delete", &payment_method_id, "--yes"])
        .assert()
        .success();
}

/// **First direction.** A *new* ACH account requires `billingAddress`,
/// `contactInfo.mobilePhoneNumber` and a name pair or company name — a
/// conditional rule OpenAPI's `required` array cannot express, so it lives
/// only in field descriptions.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_new_ach_debit_with_the_conditional_requirements() {
    let v = json(&[
        "transactions",
        "create",
        "--payment-processor-id",
        &ach_processor_id(),
        "--amount",
        &unique_amount(),
        "--ach-account-number",
        "123456789",
        "--ach-routing-number",
        "021000021",
        "--ach-account-type",
        "checking",
        "--ach-account-holder-type",
        "personal",
        "--sec-code",
        "web",
        "--requester-ip",
        "203.0.113.10",
        "--contact-email",
        "ada@example.com",
        "--billing-line1",
        "1 Main St",
        "--billing-city",
        "Austin",
        "--billing-state",
        "TX",
        "--billing-postal-code",
        "78701",
        "--billing-country",
        "US",
        "--contact-first-name",
        "Ada",
        "--contact-last-name",
        "Lovelace",
        "--contact-phone",
        "+14155552309",
        "--reference-id",
        &unique_reference_id(),
    ]);
    assert!(v["data"]["transactionId"].is_string(), "{v}");
}

/// **The block has to exist somewhere; the linkage decides where.**
///
/// This scenario asserted that a saved ACH instrument needs no billing
/// address. Two live failures corrected it in turn. An unattached method
/// answered `Billing address is required for orphan payment method
/// transactions.` — so being *saved* was never the exemption. Attaching it to
/// a bare customer then answered `Customer's billing address is required.;
/// Customer's mobile phone is required.`
///
/// So the rule is not "orphan or not" either: an ACH debit needs a billing
/// address and a phone number, and they come **from the request** for an
/// unattached method or **from the customer record** for an attached one.
/// This establishes the second half; the new-ACH scenario next door
/// establishes the first. Neither alone shows the requirement moves rather
/// than disappears.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_saved_ach_debit_for_a_customer_needs_no_billing_address() {
    // The customer carries what the request would otherwise have to: an ACH
    // debit against an attached method reads the address and phone from here.
    let customer = json(&[
        "customers",
        "create",
        "--first-name",
        "Ada",
        "--last-name",
        "Lovelace",
        "--email",
        "ada@example.com",
        "--mobile",
        "+14155552309",
        "--billing-line1",
        "123 Test St",
        "--billing-city",
        "Austin",
        "--billing-state",
        "TX",
        "--billing-postal-code",
        "10001",
        "--billing-country",
        "US",
    ]);
    let customer_id = customer["data"]["customerId"]
        .as_str()
        .expect("create returned no customerId")
        .to_string();

    let stored = json(&[
        "payment-methods",
        "add-ach",
        "--account",
        "123456789",
        "--routing",
        "021000021",
        "--account-type",
        "checking",
        "--account-holder-type",
        "personal",
        // The whole point: attached, so not an orphan.
        "--customer-id",
        &customer_id,
    ]);
    let payment_method_id = stored["data"]["paymentMethodId"]
        .as_str()
        .expect("add-ach returned no paymentMethodId")
        .to_string();

    let v = json(&[
        "transactions",
        "create",
        "--payment-processor-id",
        &ach_processor_id(),
        "--amount",
        &unique_amount(),
        "--instrument",
        "ach",
        "--payment-method-id",
        &payment_method_id,
        "--customer-id",
        &customer_id,
        "--sec-code",
        "ppd",
        "--requester-ip",
        "203.0.113.10",
        "--reference-id",
        &unique_reference_id(),
    ]);
    assert!(
        v["data"]["transactionId"].is_string(),
        "an ACH debit against a fully populated customer still demanded the \
         block on the request, so it does not read it from the customer: {v}"
    );

    live_bin()
        .args(["payment-methods", "delete", &payment_method_id, "--yes"])
        .assert()
        .success();
    live_bin()
        .args(["customers", "delete", &customer_id, "--yes"])
        .assert()
        .success();
}

/// Level 2 and level 3 data, plus the extra amounts. v1 could reach five of
/// these fields; v2 declares seventeen, and nothing but a live call says the
/// API accepts them together.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_card_sale_with_level_three_data() {
    let v = json(&[
        "transactions",
        "create",
        "--payment-processor-id",
        &card_processor_id(),
        "--amount",
        &unique_amount(),
        "--card",
        "4111111111111111",
        "--cvv",
        "123",
        "--exp",
        "12/2032",
        // The address the sandbox's AVS fixture approves; without it the
        // charge is declined and the assertion below means nothing.
        "--billing-line1",
        "123 Test St",
        "--billing-postal-code",
        "10001",
        "--billing-country",
        "US",
        "--l2-tax-rate",
        "8.25",
        // No hyphen: the API accepts only letters, numbers and spaces here,
        // and the schema's own example carries one. `--l3-po` next door has
        // no such restriction, which is why one has a hyphen and one does not.
        "--l3-invoice",
        "INV1",
        "--l3-po",
        "PO-1",
        "--l3-shipping",
        "4.99",
        "--l3-product",
        "productName=Widget,productCode=W-1,unitPrice=9.99,quantity=2",
        "--pricing-type",
        "card",
        "--customer-initiated",
        "--reference-id",
        &unique_reference_id(),
    ]);
    assert!(v["data"]["transactionId"].is_string(), "{v}");
}

// ── lifecycle, reads and utilities ───────────────────────────────────────────

#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_transaction_list_first_page() {
    let v = json(&["transactions", "list"]);
    assert_eq!(v["object"], "transaction_list");
    assert!(v["data"].is_array(), "{v}");
    assert!(v["meta"]["page_info"].is_object(), "{v}");
}

/// `sortOrder` declares a default of `"asc"` and no enum, so `"desc"` is an
/// assumption about its other value. This is what can check it.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_transaction_list_descending_is_accepted() {
    let v = json(&["transactions", "list", "--desc", "--page-size", "5"]);
    assert!(v["data"].is_array(), "{v}");
}

#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_transaction_list_filtered_by_status() {
    let v = json(&[
        "transactions",
        "list",
        "--status",
        "captured",
        "--page-size",
        "5",
    ]);
    for item in v["data"].as_array().unwrap() {
        assert_eq!(item["transactionStatus"], "Captured", "{item}");
    }
}

/// `CaptureRequestDto` declares exactly one property, `captureAmount`, with
/// `additionalProperties: false`; the operation's own
/// request example sends `{"amount": 50}`. Both cannot be right, and because
/// unknown fields are rejected rather than ignored, the wrong choice cannot
/// capture at all.
///
/// The CLI builds the schema's `captureAmount`, because a schema is normative
/// and an example is not. If this fails with a 400 naming `captureAmount`,
/// the example is right, the fixture changes, and a narrow executable
/// divergence is added.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_partial_capture_field_name() {
    let authorized = json(&[
        "transactions",
        "create",
        "--payment-processor-id",
        &card_processor_id(),
        "--amount",
        "20.00",
        "--card",
        "4111111111111111",
        "--cvv",
        "123",
        "--exp",
        "12/2032",
        // The address the sandbox's AVS fixture approves; without it the
        // charge is declined and the assertion below means nothing.
        "--billing-line1",
        "123 Test St",
        "--billing-postal-code",
        "10001",
        "--billing-country",
        "US",
        "--capture-method",
        "manual",
        "--reference-id",
        &unique_reference_id(),
    ]);
    let id = authorized["data"]["transactionId"].as_str().unwrap();

    let v = json(&[
        "transactions",
        "capture",
        "--transaction-id",
        id,
        "--amount",
        "5.00",
    ]);
    assert!(
        v["data"]["transactionId"].is_string(),
        "partial capture failed. If the error names captureAmount, the \
         operation's example is right after all and the fixture must send \
         `amount`: {v}"
    );
}

/// A capture with no amount sends **no body at all**, which is the
/// bodyless-POST `Content-Length: 0` path.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_full_capture_sends_no_body() {
    let authorized = json(&[
        "transactions",
        "create",
        "--payment-processor-id",
        &card_processor_id(),
        "--amount",
        &unique_amount(),
        "--card",
        "4111111111111111",
        "--cvv",
        "123",
        "--exp",
        "12/2032",
        // The address the sandbox's AVS fixture approves; without it the
        // charge is declined and the assertion below means nothing.
        "--billing-line1",
        "123 Test St",
        "--billing-postal-code",
        "10001",
        "--billing-country",
        "US",
        "--capture-method",
        "manual",
        "--reference-id",
        &unique_reference_id(),
    ]);
    let id = authorized["data"]["transactionId"].as_str().unwrap();
    let v = json(&["transactions", "capture", "--transaction-id", id]);
    assert!(v["data"]["transactionId"].is_string(), "{v}");
}

/// `reversal` replaces v1's `void` **and** `refund`: one endpoint, with the
/// settled state detected server-side.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_reversal_of_a_sale() {
    let sale = json(&[
        "transactions",
        "create",
        "--payment-processor-id",
        &card_processor_id(),
        "--amount",
        &unique_amount(),
        "--card",
        "4111111111111111",
        "--cvv",
        "123",
        "--exp",
        "12/2032",
        // The address the sandbox's AVS fixture approves; without it the
        // charge is declined and the assertion below means nothing.
        "--billing-line1",
        "123 Test St",
        "--billing-postal-code",
        "10001",
        "--billing-country",
        "US",
        "--reference-id",
        &unique_reference_id(),
    ]);
    let id = sale["data"]["transactionId"].as_str().unwrap();
    let v = json(&["transactions", "reversal", "--transaction-id", id]);
    assert!(v["data"]["transactionId"].is_string(), "{v}");
}

/// **A partial reversal of an unsettled sale is refused**, because the API
/// would void the whole amount. The sale is then voided in full, so nothing
/// is left behind.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_partial_reversal_of_an_unsettled_sale_is_refused() {
    let sale = json(&[
        "transactions",
        "create",
        "--payment-processor-id",
        &card_processor_id(),
        "--amount",
        &unique_amount(),
        "--card",
        "4111111111111111",
        "--cvv",
        "123",
        "--exp",
        "12/2032",
        // The address the sandbox's AVS fixture approves.
        "--billing-line1",
        "123 Test St",
        "--billing-postal-code",
        "10001",
        "--billing-country",
        "US",
        "--reference-id",
        &unique_reference_id(),
    ]);
    let id = sale["data"]["transactionId"].as_str().unwrap();
    live_bin()
        .args([
            "--output",
            "json",
            "transactions",
            "reversal",
            "--transaction-id",
            id,
            "--amount",
            "1.00",
        ])
        .assert()
        .code(3);
    let v = json(&["transactions", "reversal", "--transaction-id", id]);
    assert_eq!(v["data"]["transactionStatus"], "Voided", "{v}");
}

/// **Attended: the endpoint needs a card-present transaction and a merchant
/// with tips switched on, and this sandbox account has neither.**
///
/// The operation documents three preconditions. Transaction state is not the
/// blocker — an `Authorized` charge, which it names as valid, answers 409 just
/// as a captured one does. The other two are out of a test's reach: a
/// card-present transaction needs a physical terminal, and
/// `settings payment-config` reports `isTipsEnabled: false` for this account,
/// which the operation requires alongside `IsTipAdjustmentEnabled`.
///
/// The 409 names none of them — it is the generic `ConflictException`, so the
/// unmet precondition has to be read off the account rather than the response.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_tip_adjustment_needs_terminal() {
    let sale = json(&[
        "transactions",
        "create",
        "--payment-processor-id",
        &card_processor_id(),
        "--amount",
        &unique_amount(),
        "--card",
        "4111111111111111",
        "--cvv",
        "123",
        "--exp",
        "12/2032",
        // The address the sandbox's AVS fixture approves; without it the
        // charge is declined and the assertion below means nothing.
        "--billing-line1",
        "123 Test St",
        "--billing-postal-code",
        "10001",
        "--billing-country",
        "US",
        "--reference-id",
        &unique_reference_id(),
    ]);
    let id = sale["data"]["transactionId"].as_str().unwrap();
    let v = json(&[
        "transactions",
        "tip-adjust",
        "--transaction-id",
        id,
        "--tip-amount",
        "1.00",
    ]);
    assert!(v["data"]["transactionId"].is_string(), "{v}");
}

/// A pure calculation: no transaction is created, so this is the one write
/// path in the group that is safe to run repeatedly.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_calculate_amount() {
    let v = json(&[
        "transactions",
        "calculate-amount",
        "--amount",
        "100.00",
        "--tip-amount",
        "15.00",
        "--pricing-type",
        "card",
    ]);
    assert!(
        v["data"]["creditCard"].is_object() || v["data"]["cash"].is_object(),
        "{v}"
    );
}

/// 200 with no body, so the confirmation comes from the request.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_share_receipt_attended() {
    let sale = json(&[
        "transactions",
        "create",
        "--payment-processor-id",
        &card_processor_id(),
        "--amount",
        &unique_amount(),
        "--card",
        "4111111111111111",
        "--cvv",
        "123",
        "--exp",
        "12/2032",
        // The address the sandbox's AVS fixture approves; without it the
        // charge is declined and the assertion below means nothing.
        "--billing-line1",
        "123 Test St",
        "--billing-postal-code",
        "10001",
        "--billing-country",
        "US",
        "--reference-id",
        &unique_reference_id(),
    ]);
    let id = sale["data"]["transactionId"].as_str().unwrap();
    let v = json(&[
        "transactions",
        "share-receipt",
        id,
        "--share-by",
        "sms",
        "--recipient",
        &share_recipient(),
        "--consent",
    ]);
    assert_eq!(v["data"]["shared"], true, "{v}");
}

/// `inspect` has no endpoint of its own: it reads the same transaction and
/// prints the curated view.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_transaction_inspect_reads_the_same_transaction() {
    let sale = json(&[
        "transactions",
        "create",
        "--payment-processor-id",
        &card_processor_id(),
        "--amount",
        &unique_amount(),
        "--card",
        "4111111111111111",
        "--cvv",
        "123",
        "--exp",
        "12/2032",
        // The address the sandbox's AVS fixture approves; without it the
        // charge is declined and the assertion below means nothing.
        "--billing-line1",
        "123 Test St",
        "--billing-postal-code",
        "10001",
        "--billing-country",
        "US",
        "--reference-id",
        &unique_reference_id(),
    ]);
    let id = sale["data"]["transactionId"].as_str().unwrap();
    let v = json(&["transactions", "inspect", id]);
    assert_eq!(v["data"]["transactionId"], id);
}

// ── credit ───────────────────────────────────────────────────────────────────

/// The credit endpoint. `referenceId` is **required** here where it is
/// optional on `create`.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_ach_credit() {
    let v = json(&[
        "transactions",
        "credit",
        "--payment-processor-id",
        &ach_processor_id(),
        "--amount",
        &unique_amount(),
        "--reference-id",
        &unique_reference_id(),
        "--ach-account-number",
        "123456789",
        "--ach-routing-number",
        "021000021",
        "--ach-account-type",
        "checking",
        "--ach-account-holder-type",
        "personal",
        "--sec-code",
        "ppd",
        "--requester-ip",
        "203.0.113.10",
        "--billing-line1",
        "1 Main St",
        "--billing-city",
        "Austin",
        "--billing-state",
        "TX",
        "--billing-postal-code",
        "78701",
        "--billing-country",
        "US",
        "--contact-first-name",
        "Ada",
        "--contact-last-name",
        "Lovelace",
        "--contact-phone",
        "+14155552309",
        "--contact-email",
        "ada@example.com",
    ]);
    assert!(v["data"]["transactionId"].is_string(), "{v}");
    // Again: a single object, not the declared page.
    assert!(v["data"].get("items").is_none(), "{v}");
}

/// A credit to a card.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_card_credit() {
    let v = json(&[
        "transactions",
        "credit",
        "--payment-processor-id",
        &card_processor_id(),
        "--amount",
        &unique_amount(),
        "--reference-id",
        &unique_reference_id(),
        "--card",
        "4111111111111111",
        "--cvv",
        "123",
        "--exp",
        "12/2032",
        // The address the sandbox's AVS fixture approves; without it the
        // charge is declined and the assertion below means nothing.
        "--billing-line1",
        "123 Test St",
        "--billing-postal-code",
        "10001",
        "--billing-country",
        "US",
    ]);
    assert!(v["data"]["transactionId"].is_string(), "{v}");
}

/// **The exit-code contract's biggest assumption, settled.**
///
/// §5 rests on a declined charge being HTTP 200 with a declined status rather
/// than a 402: a decline exits **0** and the caller reads
/// `transactionStatus`. If declines came back 402 the same decline would exit
/// 1 through one path and 0 through the other, and the spec says a dedicated
/// decline code would become worth adding.
///
/// Nothing could check it until AVS gave us a decline on demand. An address
/// the fixture does not recognise answers `N` and the charge is refused — so
/// this asserts the whole contract: exit 0, a `Declined` status, and a decline
/// reason the caller can act on.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_a_declined_card_exits_zero_and_reports_why() {
    let (processor, amount, reference) =
        (card_processor_id(), unique_amount(), unique_reference_id());
    let mut args = vec![
        "transactions",
        "create",
        "--payment-processor-id",
        &processor,
        "--amount",
        &amount,
        "--card",
        "4111111111111111",
        "--cvv",
        "123",
        "--exp",
        "12/2032",
        "--reference-id",
        &reference,
    ];
    args.extend_from_slice(&AVS_DOES_NOT_MATCH);

    // Exit 0 is the contract: `json` asserts success, so reaching the parse at
    // all is half the assertion.
    let v = json(&args);
    assert_eq!(
        v["data"]["transactionStatus"], "Declined",
        "the charge was not declined, so this scenario is no longer testing \
         the decline path: {}",
        v["data"]
    );
    assert!(
        v["data"]["declineDetails"]["message"].is_string(),
        "a decline exits 0, so the reason is the only thing the caller has: {}",
        v["data"]
    );
}

/// **The conditional ACH rule reaches credit**, which is why the refusal is
/// client-side. The endpoint answers:
///
/// > BillingAddress is required for new ACH credits.; ContactInfo is required
/// > for new ACH credits.
///
/// This asserts the client-side refusal — no request, exit 3. If a later API
/// release drops the requirement, this is the scenario that says so, and the
/// client-side rule comes out with it.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_ach_credit_without_a_billing_address() {
    live_bin()
        .args([
            "transactions",
            "credit",
            "--payment-processor-id",
            &ach_processor_id(),
            "--amount",
            &unique_amount(),
            "--reference-id",
            &unique_reference_id(),
            "--ach-account-number",
            "123456789",
            "--ach-routing-number",
            "021000021",
            "--ach-account-type",
            "checking",
            "--ach-account-holder-type",
            "personal",
            "--sec-code",
            "ppd",
            "--requester-ip",
            "203.0.113.10",
        ])
        .assert()
        .code(3);
}
