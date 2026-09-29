use super::*;
use crate::cli::address::BillingArgs;
use crate::cli::common::{AccountHolderType, AccountType, PaginationArgs, PricingType};

#[test]
fn an_unfiltered_transaction_list_sends_nothing() {
    assert!(
        build_list_transactions_query(&ListTransactionsArgs::default())
            .unwrap()
            .is_empty()
    );
}

/// Sixteen parameters, the largest query surface in the API.
#[test]
fn every_transaction_filter_reaches_the_query_under_its_wire_name() {
    let args = ListTransactionsArgs {
        pagination: PaginationArgs {
            page_index: Some(1),
            page_size: Some(5),
            all: false,
        },
        sort_by: Some("transactionDateTime".into()),
        asc: false,
        desc: true,
        from_date: Some("2026-01-01T00:00:00Z".into()),
        to_date: Some("2026-12-31T23:59:59Z".into()),
        source_type: Some(SourceType::ApiKey),
        source_id: Some("src-1".into()),
        batch_id: Some("batch-1".into()),
        transaction_status: Some(TransactionStatus::Captured),
        payment_method_type: Some("Card".into()),
        customer_id: Some("cus_1".into()),
        merchant_id: Some("mer_1".into()),
        min_amount: Some("1.00".parse().unwrap()),
        max_amount: Some("999.99".parse().unwrap()),
        reference_id: Some("ref-1".into()),
    };
    let query = build_list_transactions_query(&args).unwrap();
    let names: Vec<&str> = query.iter().map(|(k, _)| *k).collect();
    assert_eq!(
        names,
        [
            "pageIndex",
            "pageSize",
            "sortOrder",
            "sortBy",
            "fromDate",
            "toDate",
            "sourceType",
            "sourceId",
            "batchId",
            "transactionStatus",
            "paymentMethodType",
            "customerId",
            "merchantId",
            "minAmount",
            "maxAmount",
            "referenceId"
        ]
    );
    // Amounts reach the query as their exact digits, never through f64.
    assert_eq!(
        query.iter().find(|(k, _)| *k == "minAmount").unwrap().1,
        "1.00"
    );
    assert_eq!(
        query.iter().find(|(k, _)| *k == "sortOrder").unwrap().1,
        "desc"
    );
}

/// The enums are capitalised on the wire, and eighteen statuses is enough
/// for a lowercase near-miss to be plausible.
#[test]
fn the_list_enums_keep_their_declared_casing() {
    assert_eq!(serde_json::json!(SourceType::WebComponent), "WebComponent");
    assert_eq!(serde_json::json!(SourceType::TapToPay), "TapToPay");
    assert_eq!(
        serde_json::json!(TransactionStatus::HeldByProcessor),
        "HeldByProcessor"
    );
    assert_eq!(
        serde_json::json!(TransactionStatus::InProgress),
        "InProgress"
    );
    assert_eq!(serde_json::json!(ShareBy::Sms), "Sms");
}

/// The schema declares `captureAmount`; the operation's own request
/// example sends `amount`. The schema is normative, so this is what is
/// built, and `live_partial_capture_field_name` settles it.
#[test]
fn a_partial_capture_sends_the_schemas_field_name() {
    let body = build_capture_body(Some("5.00".parse().unwrap()))
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::to_string(&body["captureAmount"]).unwrap(),
        "5.00"
    );
    assert!(body.get("amount").is_none());
}

/// A full capture or reversal sends an **empty object**, not an absent
/// body.
///
/// Both operations declare a request schema whose every property is
/// optional, and the API answers `400: A non-empty request body is
/// required` to a bodyless POST on such an operation. `{}` is how "no
/// fields" is said; an absent body is how "this operation takes none" is
/// said, and these two take one.
#[test]
fn a_full_capture_or_reversal_sends_an_empty_object() {
    for body in [
        build_capture_body(None).unwrap(),
        build_reversal_body(None).unwrap(),
    ] {
        let body = body.expect("a declared request schema needs a body");
        assert_eq!(body, serde_json::json!({}));
    }
}

/// And the amount, when there is one, is the only key.
#[test]
fn a_partial_capture_sends_only_the_capture_amount() {
    let body = build_capture_body(Some("5.00".parse().unwrap()))
        .unwrap()
        .unwrap();
    assert_eq!(body.as_object().unwrap().len(), 1);
    assert_eq!(body["captureAmount"].to_string(), "5.00");
}

/// A zero is not a partial amount: capturing or reversing nothing is a
/// request the API cannot act on, and the empty body is how "the whole of
/// it" is said.
#[test]
fn a_zero_capture_reversal_or_tip_amount_is_refused() {
    for body in [
        build_capture_body(Some(Decimal::ZERO)),
        build_reversal_body(Some(Decimal::ZERO)),
    ] {
        let err = body.unwrap_err().to_string();
        assert!(err.contains("--amount must be greater than zero"), "{err}");
    }
    let err = build_tip_adjustment_body(Some(Decimal::ZERO), None)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("--tip-amount must be greater than zero"),
        "{err}"
    );
}

#[test]
fn a_partial_reversal_sends_the_reversal_amount() {
    let body = build_reversal_body(Some("5.00".parse().unwrap()))
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::to_string(&body["reversalAmount"]).unwrap(),
        "5.00"
    );
}

/// An amount and a rate say different things about the same tip, and
/// neither leaves nothing to adjust.
#[test]
fn a_tip_adjustment_takes_exactly_one_of_amount_and_rate() {
    let by_amount = build_tip_adjustment_body(Some("1.00".parse().unwrap()), None).unwrap();
    assert_eq!(
        serde_json::to_string(&by_amount["tipAmount"]).unwrap(),
        "1.00"
    );
    assert!(by_amount.get("tipRate").is_none());

    let by_rate = build_tip_adjustment_body(None, Some("0.1500".parse().unwrap())).unwrap();
    assert_eq!(
        serde_json::to_string(&by_rate["tipRate"]).unwrap(),
        "0.1500"
    );
    assert!(by_rate.get("tipAmount").is_none());

    assert!(
        build_tip_adjustment_body(Some(Decimal::ONE), Some(Decimal::ONE))
            .unwrap_err()
            .to_string()
            .contains("not both")
    );
    assert!(build_tip_adjustment_body(None, None).is_err());
}

/// The API checks presence, so a zero half beside the other is still both.
#[test]
fn a_tip_adjustment_with_a_zero_half_is_refused_as_a_pair() {
    for (amount, rate) in [
        (Some(Decimal::ONE), Some(Decimal::ZERO)),
        (Some(Decimal::ZERO), Some(Decimal::ONE)),
    ] {
        let err = build_tip_adjustment_body(amount, rate)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("--tip-amount or --tip-rate, not both"),
            "{err}"
        );
    }
}

/// All three fields are required by schema, so all three are always sent
/// — `hasCustomerConsent` false included, because omitting it would fail
/// the required check rather than mean "no consent".
#[test]
fn a_share_receipt_body_always_carries_all_three_required_fields() {
    let args = ShareReceiptArgs {
        transaction_id: "txn_1".into(),
        share_by: ShareBy::Sms,
        recipient: "+14155552309".into(),
        has_customer_consent: false,
    };
    let body = build_share_receipt_body(&args).unwrap();
    assert_eq!(body["shareBy"], "Sms");
    assert_eq!(body["recipient"], "+14155552309");
    assert_eq!(body["hasCustomerConsent"], false);
    assert_eq!(body.as_object().unwrap().len(), 3);
}

#[test]
fn a_share_receipt_without_a_recipient_is_refused() {
    let args = ShareReceiptArgs {
        transaction_id: "txn_1".into(),
        share_by: ShareBy::Sms,
        recipient: String::new(),
        has_customer_consent: true,
    };
    assert!(build_share_receipt_body(&args).is_err());
}

#[test]
fn calculate_amount_sends_the_amount_and_omits_every_absent_option() {
    let args = CalculateAmountArgs {
        base_amount: "100.00".parse().unwrap(),
        ..Default::default()
    };
    let body = build_calculate_amount_body(&args).unwrap();
    assert_eq!(
        serde_json::to_string(&body["baseAmount"]).unwrap(),
        "100.00"
    );
    assert_eq!(body.as_object().unwrap().len(), 1);
}

#[test]
fn calculate_amount_carries_every_option_under_its_wire_name() {
    let amounts = CalculateAmountArgs {
        base_amount: "100.00".parse().unwrap(),
        currency_code: Some("USD".into()),
        pricing_type: Some(PricingType::Card),
        tip_amount: Some("15.00".parse().unwrap()),
        discount_amount: Some("5.00".parse().unwrap()),
        surcharge_rate: Some("0.0300".parse().unwrap()),
        ..Default::default()
    };
    let body = build_calculate_amount_body(&amounts).unwrap();
    assert_eq!(body["currencyCode"], "USD");
    assert_eq!(body["pricingType"], "Card");
    for key in ["tipAmount", "discountAmount", "surchargeRate"] {
        assert!(body.get(key).is_some(), "{key} missing");
    }

    let rates = CalculateAmountArgs {
        base_amount: "100.00".parse().unwrap(),
        tip_rate: Some("0.1500".parse().unwrap()),
        discount_rate: Some("0.0500".parse().unwrap()),
        ..Default::default()
    };
    let body = build_calculate_amount_body(&rates).unwrap();
    for key in ["tipRate", "discountRate"] {
        assert!(body.get(key).is_some(), "{key} missing");
    }
}

/// The amount-or-rate exclusions and the declared minimums are `create`'s.
#[test]
fn calculate_amount_refuses_an_amount_and_rate_pair() {
    let base = || CalculateAmountArgs {
        base_amount: "100.00".parse().unwrap(),
        ..Default::default()
    };
    let mut tip = base();
    tip.tip_amount = Some("5.00".parse().unwrap());
    tip.tip_rate = Some("10".parse().unwrap());
    let err = build_calculate_amount_body(&tip).unwrap_err().to_string();
    assert!(err.contains("--tip-amount or --tip-rate"), "{err}");

    let mut discount = base();
    discount.discount_amount = Some("5.00".parse().unwrap());
    discount.discount_rate = Some("10".parse().unwrap());
    let err = build_calculate_amount_body(&discount)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("--discount-amount or --discount-rate"),
        "{err}"
    );

    // A zero rate beside an amount is still both.
    let mut zero_rate = base();
    zero_rate.tip_amount = Some("5.00".parse().unwrap());
    zero_rate.tip_rate = Some(Decimal::ZERO);
    let err = build_calculate_amount_body(&zero_rate)
        .unwrap_err()
        .to_string();
    assert!(err.contains("--tip-amount or --tip-rate"), "{err}");
}

#[test]
fn calculate_amount_refuses_an_amount_below_the_declared_minimum() {
    for (tip, discount, flag) in [
        (Some(Decimal::ZERO), None, "--tip-amount"),
        (None, Some(Decimal::ZERO), "--discount-amount"),
    ] {
        let args = CalculateAmountArgs {
            base_amount: "100.00".parse().unwrap(),
            tip_amount: tip,
            discount_amount: discount,
            ..Default::default()
        };
        let err = build_calculate_amount_body(&args).unwrap_err().to_string();
        assert!(
            err.contains(&format!("{flag} must be at least 0.01")),
            "{err}"
        );
    }
}

#[test]
fn calculate_amount_refuses_a_non_positive_amount() {
    let args = CalculateAmountArgs {
        base_amount: Decimal::ZERO,
        ..Default::default()
    };
    assert!(build_calculate_amount_body(&args).is_err());
}

// ── credit ─────────────────────────────────────────────────────────

fn credit_args() -> CreditArgs {
    CreditArgs {
        payment_processor_id: "pp-1".into(),
        base_amount: "10.50".parse().unwrap(),
        reference_id: "credit-1".into(),
        instrument: InstrumentArgs {
            card: Some("4111111111111111".into()),
            cvv: Some("123".into()),
            exp: Some("12/2032".into()),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// The container is `creditDetails`, not `transactionDetails`, and
/// `creditDetails.cardData` declares **no** `captureMethod`: a credit is
/// not an authorization.
#[test]
fn a_card_credit_uses_the_credit_container_and_sends_no_capture_method() {
    let body = build_credit_body(&credit_args()).unwrap();
    assert!(body.get("transactionDetails").is_none());
    let card = &body["creditDetails"]["cardData"];
    assert!(card.get("captureMethod").is_none(), "{card}");
    assert_eq!(
        card["paymentMethodDetails"]["cardNumber"],
        "4111111111111111"
    );
    assert_eq!(body["referenceId"], "credit-1");
}

/// `referenceId` is required here and optional on `create`.
#[test]
fn a_credit_without_a_reference_id_is_refused() {
    let mut args = credit_args();
    args.reference_id = String::new();
    assert!(
        build_credit_body(&args)
            .unwrap_err()
            .to_string()
            .contains("--reference-id")
    );
}

/// One instrument declaration means one set of rules: the ACH pair is
/// required on a credit exactly as it is on a charge.
#[test]
fn an_ach_credit_needs_a_sec_code_and_a_requester_ip() {
    let mut args = credit_args();
    args.instrument = InstrumentArgs {
        ach_account_number: Some("123456789".into()),
        ach_routing_number: Some("021000021".into()),
        ach_account_type: Some(AccountType::Checking),
        ach_account_holder_type: Some(AccountHolderType::Personal),
        ..Default::default()
    };
    assert!(build_credit_body(&args).is_err());

    args.instrument.sec_code = Some(SecCode::Ppd);
    args.instrument.requester_ip_address = Some("203.0.113.10".into());
    // A new ACH credit also needs the billing and contact block; that is
    // its own rule, tested next door, and supplied here so this test
    // fails only for the pair it is about.
    args.billing = BillingArgs {
        line1: Some("1 Main St".into()),
        ..Default::default()
    };
    args.contact = ContactArgs {
        first_name: Some("Ada".into()),
        last_name: Some("Lovelace".into()),
        mobile_phone_number: Some("+14155552309".into()),
        email: Some("ada@example.com".into()),
        ..Default::default()
    };
    let body = build_credit_body(&args).unwrap();
    let ach = &body["creditDetails"]["achData"];
    assert_eq!(ach["secCode"], "PPD");
    assert_eq!(ach["paymentMethodDetails"]["accountType"], "Checking");
}

/// The conditional ACH rule reaches credit: `POST /v2/transactions/credit`
/// answers `BillingAddress is required for new ACH credits.; ContactInfo
/// is required for new ACH credits.`
#[test]
fn a_new_ach_credit_requires_a_billing_address_and_contact_info() {
    let mut args = credit_args();
    args.instrument = InstrumentArgs {
        ach_account_number: Some("123456789".into()),
        ach_routing_number: Some("021000021".into()),
        ach_account_type: Some(AccountType::Checking),
        ach_account_holder_type: Some(AccountHolderType::Personal),
        sec_code: Some(SecCode::Ppd),
        requester_ip_address: Some("203.0.113.10".into()),
        ..Default::default()
    };
    let err = build_credit_body(&args).unwrap_err().to_string();
    assert!(err.contains("--billing-"), "{err}");

    // And with the block supplied it goes through.
    args.billing = BillingArgs {
        line1: Some("1 Main St".into()),
        ..Default::default()
    };
    args.contact = ContactArgs {
        first_name: Some("Ada".into()),
        last_name: Some("Lovelace".into()),
        mobile_phone_number: Some("+14155552309".into()),
        email: Some("ada@example.com".into()),
        ..Default::default()
    };
    let body = build_credit_body(&args).unwrap();
    assert_eq!(body["billingAddress"]["addressLine1"], "1 Main St");
    assert_eq!(body["contactInfo"]["email"], "ada@example.com");
}

/// A **card** credit is untouched by the rule, so the correction did not
/// widen into a refusal the API does not make.
#[test]
fn a_card_credit_needs_no_billing_address_or_contact_info() {
    let body = build_credit_body(&credit_args()).unwrap();
    assert!(body.get("billingAddress").is_none());
    assert!(body.get("contactInfo").is_none());
}

/// Absent optional blocks are absent keys, not empty objects.
#[test]
fn a_credit_omits_every_absent_optional() {
    let body = build_credit_body(&credit_args()).unwrap();
    for key in [
        "currencyCode",
        "customerId",
        "contactInfo",
        "billingAddress",
        "shippingAddress",
    ] {
        assert!(body.get(key).is_none(), "{key} should be absent");
    }
}

#[test]
fn a_credit_carries_every_optional_it_is_given() {
    let mut args = credit_args();
    args.currency_code = Some("USD".into());
    args.customer_id = Some("cus_1".into());
    args.contact.email = Some("ada@example.com".into());
    args.billing.city = Some("Austin".into());
    args.shipping.city = Some("Dallas".into());
    let body = build_credit_body(&args).unwrap();
    assert_eq!(body["currencyCode"], "USD");
    assert_eq!(body["customerId"], "cus_1");
    assert_eq!(body["contactInfo"]["email"], "ada@example.com");
    assert_eq!(body["billingAddress"]["city"], "Austin");
    assert_eq!(body["shippingAddress"]["city"], "Dallas");
}

#[test]
fn a_credit_refuses_a_non_positive_amount_and_a_missing_processor() {
    let mut args = credit_args();
    args.base_amount = Decimal::ZERO;
    assert!(build_credit_body(&args).is_err());

    let mut args = credit_args();
    args.payment_processor_id = String::new();
    assert!(build_credit_body(&args).is_err());
}

/// The same single error for a credit.
#[test]
fn a_new_ach_credit_names_every_missing_requirement_at_once() {
    let mut args = credit_args();
    args.instrument = InstrumentArgs {
        ach_account_number: Some("123456789".into()),
        ach_routing_number: Some("021000021".into()),
        ach_account_type: Some(AccountType::Checking),
        ach_account_holder_type: Some(AccountHolderType::Personal),
        requester_ip_address: Some(String::new()),
        ..Default::default()
    };
    let err = build_credit_body(&args).unwrap_err().to_string();
    for flag in [
        "--sec-code",
        "--requester-ip",
        "--billing-*",
        "--contact-phone",
        "--contact-email",
        "--contact-first-name",
    ] {
        assert!(err.contains(flag), "{flag} is not named in: {err}");
    }
}

/// Omitting `sortOrder` does not give ascending order: the server answers
/// newest first whatever the parameter's declared default says. So each
/// direction is sent explicitly, and neither flag sends nothing.
#[test]
fn a_transaction_list_sends_the_sort_order_it_is_asked_for() {
    let order = |asc, desc| {
        let q = build_list_transactions_query(&ListTransactionsArgs {
            asc,
            desc,
            ..Default::default()
        })
        .unwrap();
        q.iter()
            .find(|(k, _)| *k == "sortOrder")
            .map(|(_, v)| v.clone())
    };
    assert_eq!(order(true, false).as_deref(), Some("asc"));
    assert_eq!(order(false, true).as_deref(), Some("desc"));
    assert_eq!(order(false, false), None);
}

/// The API voids an unsettled card transaction in full and reverses an ACH
/// one in full whatever `reversalAmount` says, so a partial is let through
/// only for a settled card transaction.
#[test]
fn a_partial_reversal_is_let_through_only_for_a_settled_card() {
    let txn = |method: &str, status: &str| {
        serde_json::json!({
            "transactionId": "t1",
            "paymentMethodType": method,
            "transactionStatus": status,
        })
    };
    assert!(refuse_a_partial_reversal_the_api_ignores(&txn("Card", "Settled")).is_ok());
    assert!(refuse_a_partial_reversal_the_api_ignores(&txn("Card", "Refunded")).is_ok());
    for status in ["Captured", "Authorized", "Pending", "InProgress"] {
        let err = refuse_a_partial_reversal_the_api_ignores(&txn("Card", status))
            .unwrap_err()
            .to_string();
        assert!(err.contains(status) && err.contains("void"), "{err}");
    }
    let err = refuse_a_partial_reversal_the_api_ignores(&txn("ACH", "Settled"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("ACH"), "{err}");
    assert!(
        refuse_a_partial_reversal_the_api_ignores(&serde_json::json!({"transactionId": "t1"}))
            .is_err(),
        "a transaction whose state cannot be read is not assumed settled"
    );
}

/// An empty id filter names nothing, and dropping it would answer with the
/// unfiltered collection: the reconcile lookup would read as a match.
#[test]
fn an_empty_id_filter_on_the_transaction_list_is_refused() {
    type Set = fn(&mut ListTransactionsArgs, Option<String>);
    let cases: [(&str, Set); 5] = [
        ("--source-id", |a, v| a.source_id = v),
        ("--batch-id", |a, v| a.batch_id = v),
        ("--customer-id", |a, v| a.customer_id = v),
        ("--merchant-id", |a, v| a.merchant_id = v),
        ("--reference-id", |a, v| a.reference_id = v),
    ];
    for (flag, set) in cases {
        for empty in ["", "  "] {
            let mut args = ListTransactionsArgs::default();
            set(&mut args, Some(empty.into()));
            let err = build_list_transactions_query(&args)
                .unwrap_err()
                .to_string();
            assert!(err.contains(&format!("{flag} needs a value")), "{err}");
        }
    }
}
