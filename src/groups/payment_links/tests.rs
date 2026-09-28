use super::*;

/// The minimal create the **API** accepts, which is not the minimal one
/// the schema declares: `currencyCode` is required in practice.
fn card_only() -> CreatePaymentLinkArgs {
    CreatePaymentLinkArgs {
        card_enabled: true,
        currency_code: Some("USD".into()),
        ..Default::default()
    }
}

/// `paymentMethods` is the only field the schema requires, and a method
/// named with no configuration under it is the documented way to offer it
/// with defaults. The currency is there because the **API** requires it,
/// which is the difference this body records.
#[test]
fn a_minimal_create_is_one_payment_method_and_a_currency() {
    let body = build_create_payment_link_body(&card_only()).unwrap();
    assert_eq!(
        body,
        serde_json::json!({
            "paymentMethods": {"card": {"enabled": true}},
            "currencyCode": "USD"})
    );
}

/// An empty processor id is a value that went missing, such as an unset
/// shell variable, and sending the method without it would route payments
/// through the account's default processor instead of the one named.
#[test]
fn an_empty_processor_id_is_refused_on_create() {
    for (flag, args) in [
        (
            "--card-processor-id",
            CreatePaymentLinkArgs {
                card_processor_id: Some(String::new()),
                ..card_only()
            },
        ),
        (
            "--ach-processor-id",
            CreatePaymentLinkArgs {
                ach_processor_id: Some(String::new()),
                ..card_only()
            },
        ),
    ] {
        let err = build_create_payment_link_body(&args)
            .unwrap_err()
            .to_string();
        assert!(err.contains(flag), "{flag}: {err}");
    }
}

#[test]
fn every_create_flag_reaches_the_body_under_its_wire_name() {
    let args = CreatePaymentLinkArgs {
        card_enabled: true,
        card_processor_id: Some("pp-card".into()),
        ach_enabled: true,
        ach_processor_id: Some("pp-ach".into()),
        base_amount: Some("25.00".parse().unwrap()),
        currency_code: Some("USD".into()),
        link_type: Some(LinkType::MultiUse),
        customer_id: Some("cus-1".into()),
        reference_id: Some("ORDER-1042".into()),
        name: Some("Spring campaign".into()),
        description: Some("notes".into()),
        expires_on: Some("2026-09-15T00:00:00.000Z".into()),
    };
    let body = build_create_payment_link_body(&args).unwrap();
    assert_eq!(body["paymentMethods"]["card"]["processorId"], "pp-card");
    assert_eq!(body["paymentMethods"]["ach"]["processorId"], "pp-ach");
    assert_eq!(body["baseAmount"].to_string(), "25.00");
    assert_eq!(body["currencyCode"], "USD");
    assert_eq!(body["linkType"], "MultiUse");
    assert_eq!(body["customerId"], "cus-1");
    assert_eq!(body["referenceId"], "ORDER-1042");
    assert_eq!(body["name"], "Spring campaign");
    assert_eq!(body["description"], "notes");
    assert_eq!(body["expiresOn"], "2026-09-15T00:00:00.000Z");
}

/// A link with no amount is a flexible-amount link, so the key is absent
/// rather than zero or null.
#[test]
fn a_create_without_an_amount_omits_the_key() {
    let body = build_create_payment_link_body(&card_only()).unwrap();
    assert!(body.get("baseAmount").is_none(), "{body}");
}

/// **`currencyCode` is required in practice**, though the schema names
/// only `paymentMethods`. Unconditionally so: a create with an amount and
/// no currency is refused exactly as one with neither is.
#[test]
fn a_create_without_a_currency_code_is_refused() {
    let without = CreatePaymentLinkArgs {
        currency_code: None,
        ..card_only()
    };
    let err = build_create_payment_link_body(&without)
        .unwrap_err()
        .to_string();
    assert!(err.contains("--currency-code"), "{err}");

    // Unconditional: an amount does not make it optional either.
    let with_amount = CreatePaymentLinkArgs {
        base_amount: Some("25.00".parse().unwrap()),
        ..without
    };
    assert!(build_create_payment_link_body(&with_amount).is_err());

    assert!(build_create_payment_link_body(&card_only()).is_ok());
}

#[test]
fn a_create_with_no_payment_method_is_refused() {
    let err = build_create_payment_link_body(&CreatePaymentLinkArgs::default())
        .unwrap_err()
        .to_string();
    assert!(err.contains("--card-enabled"), "{err}");
}

/// A processor id implies the method: naming where to charge a card and
/// then not accepting cards is a contradiction nobody means.
#[test]
fn a_processor_id_alone_enables_its_method() {
    let args = CreatePaymentLinkArgs {
        ach_processor_id: Some("pp-ach".into()),
        card_enabled: false,
        currency_code: Some("USD".into()),
        ..Default::default()
    };
    let body = build_create_payment_link_body(&args).unwrap();
    assert_eq!(body["paymentMethods"]["ach"]["enabled"], true);
    assert!(body["paymentMethods"].get("card").is_none(), "{body}");
}

/// Zero is not an amount; a flexible link omits the key instead.
#[test]
fn a_zero_amount_is_refused_with_the_flexible_alternative_named() {
    let args = CreatePaymentLinkArgs {
        base_amount: Some(Decimal::ZERO),
        ..card_only()
    };
    let err = build_create_payment_link_body(&args)
        .unwrap_err()
        .to_string();
    assert!(err.contains("Omit it"), "{err}");
}

fn update_args() -> UpdatePaymentLinkArgs {
    UpdatePaymentLinkArgs {
        payment_link_id: "link-1".into(),
        ..Default::default()
    }
}

/// On an update the flexible alternative is a clear, which the refusal names.
#[test]
fn an_update_refuses_a_zero_amount_with_the_clear_named() {
    let args = UpdatePaymentLinkArgs {
        base_amount: Some(PatchNumber::Set(Decimal::ZERO)),
        ..update_args()
    };
    let err = build_update_payment_link_body(&args)
        .unwrap_err()
        .to_string();
    assert!(err.contains("greater than zero"), "{err}");
    assert!(err.contains("--clear amount"), "{err}");
}

#[test]
fn every_update_flag_reaches_the_body_under_its_wire_name() {
    let args = UpdatePaymentLinkArgs {
        card_enabled: Some(true),
        card_processor_id: Some("pp-card".into()),
        ach_enabled: Some(false),
        base_amount: Some(PatchNumber::Set("30.00".parse().unwrap())),
        currency_code: Some("USD".into()),
        link_type: Some(LinkType::SingleUse),
        payment_link_status: Some(PaymentLinkStatus::Inactive),
        customer_id: Some("cus-1".into()),
        reference_id: Some("ORDER-2".into()),
        name: Some("After".into()),
        description: Some("notes".into()),
        expires_on: Some("2026-10-15T00:00:00.000Z".into()),
        ..update_args()
    };
    let body = build_update_payment_link_body(&args).unwrap();
    assert_eq!(body["paymentMethods"]["card"]["enabled"], true);
    assert_eq!(body["paymentMethods"]["card"]["processorId"], "pp-card");
    assert_eq!(body["paymentMethods"]["ach"]["enabled"], false);
    assert_eq!(body["baseAmount"].to_string(), "30.00");
    assert_eq!(body["currencyCode"], "USD");
    assert_eq!(body["linkType"], "SingleUse");
    assert_eq!(body["paymentLinkStatus"], "Inactive");
    assert_eq!(body["name"], "After");
    assert_eq!(body["expiresOn"], "2026-10-15T00:00:00.000Z");
}

/// The asymmetry with `create` is the point: a PATCH has to be able to
/// turn a payment method off, and a bare switch cannot express `false`.
#[test]
fn an_update_can_disable_a_payment_method() {
    let args = UpdatePaymentLinkArgs {
        ach_enabled: Some(false),
        ..update_args()
    };
    let body = build_update_payment_link_body(&args).unwrap();
    assert_eq!(
        body,
        serde_json::json!({"paymentMethods": {"ach": {"enabled": false}}})
    );
}

#[test]
fn an_update_with_no_fields_is_refused() {
    let err = build_update_payment_link_body(&update_args())
        .unwrap_err()
        .to_string();
    assert!(err.contains("nothing to update"), "{err}");
}

/// The merge-patch capability: an explicit null, which an absent key
/// cannot express.
#[test]
fn an_empty_value_sends_an_explicit_null_under_the_wire_name() {
    let args = UpdatePaymentLinkArgs {
        description: Some(String::new()),
        expires_on: Some(String::new()),
        ..update_args()
    };
    let body = build_update_payment_link_body(&args).unwrap();
    assert_eq!(body["description"], Value::Null);
    assert_eq!(body["expiresOn"], Value::Null);
    assert_eq!(body.as_object().unwrap().len(), 2);
}

/// Each clearable flag nulls the key the schema declares for it.
#[test]
fn an_empty_value_maps_each_flag_to_the_key_the_schema_declares() {
    let args = UpdatePaymentLinkArgs {
        customer_id: Some(String::new()),
        reference_id: Some(String::new()),
        description: Some(String::new()),
        expires_on: Some(String::new()),
        ..update_args()
    };
    let body = build_update_payment_link_body(&args).unwrap();
    for key in ["customerId", "referenceId", "description", "expiresOn"] {
        assert_eq!(body[key], Value::Null, "{key}");
    }
}

/// A processor id cannot be cleared, and an empty one dropped from the
/// patch would leave the method on whatever processor it had while the
/// caller believes it changed.
#[test]
fn an_empty_processor_id_is_refused_on_update() {
    for (flag, args) in [
        (
            "--card-processor-id",
            UpdatePaymentLinkArgs {
                card_enabled: Some(true),
                card_processor_id: Some(String::new()),
                ..update_args()
            },
        ),
        (
            "--ach-processor-id",
            UpdatePaymentLinkArgs {
                ach_processor_id: Some(String::new()),
                ..update_args()
            },
        ),
    ] {
        let err = build_update_payment_link_body(&args)
            .unwrap_err()
            .to_string();
        assert!(err.contains(flag), "{err}");
        assert!(err.contains("settings payment-config"), "{err}");
    }
}

fn share_args(consent: bool) -> SharePaymentLinkArgs {
    SharePaymentLinkArgs {
        payment_link_id: "link-1".into(),
        share_by: ShareChannel::Sms,
        recipient: "+14155552309".into(),
        has_customer_consent: consent,
    }
}

/// All three fields are required, so the consent flag is always sent —
/// including when it is false, which is what lets the API refuse.
#[test]
fn a_share_always_sends_all_three_required_fields() {
    let body = build_share_payment_link_body(&share_args(true)).unwrap();
    assert_eq!(
        body,
        serde_json::json!({
            "shareBy": "Sms",
            "recipient": "+14155552309",
            "hasCustomerConsent": true})
    );
    let body = build_share_payment_link_body(&share_args(false)).unwrap();
    assert_eq!(body["hasCustomerConsent"], false);
}

#[test]
fn a_share_with_no_recipient_is_refused() {
    let mut args = share_args(true);
    args.recipient = "  ".into();
    assert!(build_share_payment_link_body(&args).is_err());
}

/// This operation's `shareBy` declares two values. The transaction
/// receipt's declares three, and sharing one enum would offer a value
/// this schema rejects.
#[test]
fn the_share_channel_offers_only_this_schemas_two_values() {
    assert_eq!(serde_json::json!(ShareChannel::Email), "Email");
    assert_eq!(serde_json::json!(ShareChannel::Sms), "Sms");
    for channel in [ShareChannel::Email, ShareChannel::Sms] {
        assert_ne!(serde_json::json!(channel), "None");
    }
}

fn list_args() -> ListPaymentLinksArgs {
    ListPaymentLinksArgs::default()
}

#[test]
fn an_unfiltered_link_list_sends_nothing() {
    assert!(
        build_list_payment_links_query(&list_args())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn every_link_filter_reaches_the_query_under_its_wire_name() {
    let args = ListPaymentLinksArgs {
        pagination: PaginationArgs {
            page_index: Some(1),
            page_size: Some(5),
            all: false,
        },
        search: Some("spring".into()),
        link_type: Some(LinkType::SingleUse),
        payment_link_status: Some(PaymentLinkStatus::Active),
        sort_by: Some("createdOn".into()),
        asc: false,
        desc: true,
    };
    let q = build_list_payment_links_query(&args).unwrap();
    let names: Vec<&str> = q.iter().map(|(k, _)| *k).collect();
    assert_eq!(
        names,
        [
            "pageIndex",
            "pageSize",
            "sortOrder",
            "linkType",
            "paymentLinkStatus",
            "sortBy",
            "search"
        ]
    );
}

/// Omitting `sortOrder` answers newest first whatever the parameter's
/// declared default says, so each direction is sent explicitly, and neither
/// flag sends nothing.
#[test]
fn link_list_sends_the_sort_order_it_is_asked_for() {
    let order = |asc, desc| {
        let mut args = list_args();
        args.asc = asc;
        args.desc = desc;
        build_list_payment_links_query(&args)
            .unwrap()
            .into_iter()
            .find(|(k, _)| *k == "sortOrder")
            .map(|(_, v)| v)
    };
    assert_eq!(order(true, false).as_deref(), Some("asc"));
    assert_eq!(order(false, true).as_deref(), Some("desc"));
    assert_eq!(order(false, false), None);
}
