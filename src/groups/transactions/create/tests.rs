use super::*;
use crate::cli::common::{AccountHolderType, AccountType, PricingType};

/// A card charge that passes every rule. Each test mutates one field, so
/// the assertion names the single reason the request is invalid.
fn valid_args() -> CreateTransactionArgs {
    CreateTransactionArgs {
        instrument: InstrumentArgs {
            card: Some("4111111111111111".into()),
            cvv: Some("123".into()),
            exp: Some("12/2032".into()),
            ..Default::default()
        },
        payment_processor_id: "pp-1".into(),
        amount: "10.50".parse().unwrap(),
        customer_id: Some("cus_1".into()),
        capture_method: CaptureMethod::Auto,
        ..Default::default()
    }
}

#[test]
fn rejects_non_positive_base_amount() {
    let mut a = valid_args();
    a.amount = Decimal::ZERO;
    assert!(
        validate_create_transaction(&a)
            .unwrap_err()
            .to_string()
            .contains("greater than zero")
    );
}

#[test]
fn rejects_missing_payment_processor_id_and_names_the_way_to_find_it() {
    let mut a = valid_args();
    a.payment_processor_id = String::new();
    let msg = validate_create_transaction(&a).unwrap_err().to_string();
    assert!(msg.contains("settings payment-config"), "{msg}");
}

#[test]
fn rejects_a_missing_instrument() {
    let mut a = valid_args();
    a.instrument.card = None;
    a.instrument.cvv = None;
    a.instrument.exp = None;
    assert!(
        validate_create_transaction(&a)
            .unwrap_err()
            .to_string()
            .contains("exactly one")
    );
}

/// A partial card is a different error from no card: naming it saves a
/// round trip and a confusing 400.
#[test]
fn rejects_a_partial_card() {
    for missing in 0..3 {
        let mut a = valid_args();
        match missing {
            0 => a.instrument.card = None,
            1 => a.instrument.cvv = None,
            _ => a.instrument.exp = None,
        }
        let msg = validate_create_transaction(&a).unwrap_err().to_string();
        assert!(msg.contains("together"), "{msg}");
    }
}

#[test]
fn builds_card_body_with_undocumented_capture_method_and_customer_id() {
    let body = build_create_transaction_body(&valid_args()).unwrap();
    assert_eq!(
        body["transactionDetails"]["cardData"]["captureMethod"],
        "Auto"
    );
    assert_eq!(body["customerId"], "cus_1");
    assert_eq!(
        body["transactionDetails"]["cardData"]["paymentMethodDetails"]["expirationMonth"],
        12
    );
    assert_eq!(
        body["transactionDetails"]["cardData"]["paymentMethodDetails"]["expirationYear"],
        2032
    );
}

/// `customerId` is accepted, not required, so the absent case is asserted
/// too — asserting only the present one would leave it looking mandatory.
#[test]
fn omits_customer_id_when_the_flag_is_absent() {
    let mut a = valid_args();
    a.customer_id = None;
    let body = build_create_transaction_body(&a).unwrap();
    assert!(body.get("customerId").is_none());
}

/// The wire value is capitalised, and manual capture is the only way to
/// create an authorization rather than a charge.
#[test]
fn manual_capture_sends_the_capitalised_wire_value() {
    let mut a = valid_args();
    a.capture_method = CaptureMethod::Manual;
    let body = build_create_transaction_body(&a).unwrap();
    assert_eq!(
        body["transactionDetails"]["cardData"]["captureMethod"],
        "Manual"
    );
    assert_eq!(serde_json::json!(CaptureMethod::Auto), "Auto");
    assert_eq!(serde_json::json!(CaptureMethod::Manual), "Manual");
}

/// Amounts must survive as exact decimals, never as floats.
#[test]
fn base_amount_serializes_exactly() {
    let mut a = valid_args();
    a.amount = "1234567.89".parse().unwrap();
    let body = build_create_transaction_body(&a).unwrap();
    assert_eq!(
        serde_json::to_string(&body["baseAmount"]).unwrap(),
        "1234567.89"
    );

    let mut a = valid_args();
    a.amount = "10.50".parse().unwrap();
    let body = build_create_transaction_body(&a).unwrap();
    assert_eq!(serde_json::to_string(&body["baseAmount"]).unwrap(), "10.50");
}

/// A bare object passes through; a one-item page is unwrapped; any other
/// count is a contract break.
#[test]
fn unwraps_a_one_item_page_and_refuses_any_other_count() {
    let bare = serde_json::json!({"transactionId": "txn_1"});
    assert_eq!(unwrap_single_transaction(bare.clone()).unwrap(), bare);

    let page = serde_json::json!({"items": [{"transactionId": "txn_1"}]});
    assert_eq!(
        unwrap_single_transaction(page).unwrap(),
        serde_json::json!({"transactionId": "txn_1"})
    );

    let two = serde_json::json!({"items": [{"transactionId": "a"}, {"transactionId": "b"}]});
    assert!(unwrap_single_transaction(two).is_err());
    let empty = serde_json::json!({"items": []});
    assert!(unwrap_single_transaction(empty).is_err());
}

// ── the other three instrument shapes ──────────────────────────────

fn saved_card_args() -> CreateTransactionArgs {
    CreateTransactionArgs {
        instrument: InstrumentArgs {
            payment_method_id: Some("pm_1".into()),
            instrument: Some(Instrument::Card),
            ..Default::default()
        },
        payment_processor_id: "pp-1".into(),
        amount: "10.50".parse().unwrap(),
        ..Default::default()
    }
}

fn new_ach_args() -> CreateTransactionArgs {
    CreateTransactionArgs {
        instrument: InstrumentArgs {
            ach_account_number: Some("123456789".into()),
            ach_routing_number: Some("021000021".into()),
            ach_account_type: Some(AccountType::Checking),
            ach_account_holder_type: Some(AccountHolderType::Personal),
            sec_code: Some(SecCode::Web),
            requester_ip_address: Some("203.0.113.10".into()),
            ..Default::default()
        },
        contact: ContactArgs {
            first_name: Some("Ada".into()),
            last_name: Some("Lovelace".into()),
            mobile_phone_number: Some("+14155552309".into()),
            email: Some("ada@example.com".into()),
            ..Default::default()
        },
        payment_processor_id: "pp-1".into(),
        amount: "10.50".parse().unwrap(),
        billing: BillingArgs {
            line1: Some("1 Main St".into()),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn saved_ach_args() -> CreateTransactionArgs {
    CreateTransactionArgs {
        instrument: InstrumentArgs {
            payment_method_id: Some("pm_2".into()),
            instrument: Some(Instrument::Ach),
            sec_code: Some(SecCode::Ppd),
            requester_ip_address: Some("203.0.113.10".into()),
            ..Default::default()
        },
        payment_processor_id: "pp-1".into(),
        amount: "10.50".parse().unwrap(),
        ..Default::default()
    }
}

/// The saved routes go in the same container as their new counterparts,
/// and the two keys are mutually exclusive there.
#[test]
fn a_saved_card_sends_a_payment_method_id_and_no_details() {
    let body = build_create_transaction_body(&saved_card_args()).unwrap();
    let card = &body["transactionDetails"]["cardData"];
    assert_eq!(card["paymentMethodId"], "pm_1");
    assert!(card.get("paymentMethodDetails").is_none());
    // captureMethod still governs sale versus authorization.
    assert_eq!(card["captureMethod"], "Auto");
    assert!(body["transactionDetails"].get("achData").is_none());
}

#[test]
fn a_saved_ach_sends_a_payment_method_id_and_no_details() {
    let body = build_create_transaction_body(&saved_ach_args()).unwrap();
    let ach = &body["transactionDetails"]["achData"];
    assert_eq!(ach["paymentMethodId"], "pm_2");
    assert!(ach.get("paymentMethodDetails").is_none());
    assert_eq!(ach["secCode"], "PPD");
    assert_eq!(ach["requesterIpAddress"], "203.0.113.10");
    assert!(body["transactionDetails"].get("cardData").is_none());
}

#[test]
fn a_new_ach_sends_the_account_details_under_their_wire_names() {
    let mut args = new_ach_args();
    args.instrument.ach_tax_id = Some("123456789".into());
    args.instrument.is_same_day_processing = true;
    let body = build_create_transaction_body(&args).unwrap();
    let details = &body["transactionDetails"]["achData"]["paymentMethodDetails"];
    assert_eq!(details["accountNumber"], "123456789");
    assert_eq!(details["routingNumber"], "021000021");
    assert_eq!(details["accountType"], "Checking");
    assert_eq!(details["accountHolderType"], "Personal");
    assert_eq!(details["taxId"], "123456789");
    assert_eq!(
        body["transactionDetails"]["achData"]["isSameDayProcessing"],
        true
    );
}

/// Absent is absent: an omitted tax id must not become a null.
#[test]
fn a_new_ach_omits_an_absent_tax_id_and_same_day_flag() {
    let body = build_create_transaction_body(&new_ach_args()).unwrap();
    let ach = &body["transactionDetails"]["achData"];
    assert!(ach["paymentMethodDetails"].get("taxId").is_none());
    assert!(ach.get("isSameDayProcessing").is_none());
}

/// Case-sensitive on the wire, and neither all-caps nor all-title-case.
#[test]
fn the_sec_code_enum_keeps_its_declared_casing() {
    assert_eq!(serde_json::json!(SecCode::Web), "Web");
    assert_eq!(serde_json::json!(SecCode::Ppd), "PPD");
    assert_eq!(serde_json::json!(SecCode::Ccd), "CCD");
    assert_eq!(serde_json::json!(PricingType::Card), "Card");
    assert_eq!(serde_json::json!(PricingType::Cash), "Cash");
}

/// Exactly one instrument, and each of the four is recognised.
#[test]
fn each_of_the_four_shapes_is_chosen_and_two_are_refused() {
    assert_eq!(
        chosen_instrument(&valid_args().instrument).unwrap(),
        Chosen::NewCard
    );
    assert_eq!(
        chosen_instrument(&saved_card_args().instrument).unwrap(),
        Chosen::SavedCard
    );
    assert_eq!(
        chosen_instrument(&new_ach_args().instrument).unwrap(),
        Chosen::NewAch
    );
    assert_eq!(
        chosen_instrument(&saved_ach_args().instrument).unwrap(),
        Chosen::SavedAch
    );

    let mut both = valid_args();
    both.instrument.payment_method_id = Some("pm_1".into());
    both.instrument.instrument = Some(Instrument::Card);
    assert!(
        chosen_instrument(&both.instrument)
            .unwrap_err()
            .to_string()
            .contains("exactly one")
    );
}

/// A stored id says nothing about its own type, and `cardData` and
/// `achData` are different objects.
#[test]
fn a_saved_id_without_an_instrument_is_refused() {
    let mut args = saved_card_args();
    args.instrument.instrument = None;
    assert!(
        chosen_instrument(&args.instrument)
            .unwrap_err()
            .to_string()
            .contains("--instrument")
    );
}

/// Partial ACH details name the missing half rather than reporting no
/// instrument at all.
#[test]
fn partial_ach_details_are_refused_as_a_group() {
    let mut args = new_ach_args();
    args.instrument.ach_routing_number = None;
    assert!(
        chosen_instrument(&args.instrument)
            .unwrap_err()
            .to_string()
            .contains("together")
    );
}

/// Both directions. Only the pair proves the rule is conditional rather
/// than universal.
#[test]
fn the_conditional_ach_requirements_apply_to_a_new_account_only() {
    // A new account without any of them is refused three ways.
    let mut no_address = new_ach_args();
    no_address.billing = BillingArgs::default();
    assert!(
        validate_create_transaction(&no_address)
            .unwrap_err()
            .to_string()
            .contains("billing address")
    );

    let mut no_phone = new_ach_args();
    no_phone.contact.mobile_phone_number = None;
    assert!(
        validate_create_transaction(&no_phone)
            .unwrap_err()
            .to_string()
            .contains("--contact-phone")
    );

    let mut no_name = new_ach_args();
    no_name.contact.first_name = None;
    no_name.contact.last_name = None;
    assert!(validate_create_transaction(&no_name).is_err());

    // A company name satisfies the name half on its own.
    let mut company = no_name;
    company.contact.company_name = Some("Analytical".into());
    assert!(validate_create_transaction(&company).is_ok());

    // And the saved route requires none of them.
    assert!(validate_create_transaction(&saved_ach_args()).is_ok());
}

/// Required by schema on `AchDataDto` itself, so both routes need them.
#[test]
fn both_ach_routes_require_a_sec_code_and_a_requester_ip() {
    for base in [new_ach_args(), saved_ach_args()] {
        let mut no_sec = clone_args(&base);
        no_sec.instrument.sec_code = None;
        assert!(validate_create_transaction(&no_sec).is_err());

        let mut no_ip = clone_args(&base);
        no_ip.instrument.requester_ip_address = Some(String::new());
        assert!(validate_create_transaction(&no_ip).is_err());
    }
}

/// A card sends none of the ACH-only fields, so naming one is refused rather
/// than dropped.
#[test]
fn a_card_refuses_the_ach_only_flags() {
    for (flag, set) in [
        (
            "--sec-code",
            (|a: &mut InstrumentArgs| a.sec_code = Some(SecCode::Web)) as fn(&mut InstrumentArgs),
        ),
        ("--requester-ip", |a| {
            a.requester_ip_address = Some("203.0.113.10".into())
        }),
        ("--same-day", |a| a.is_same_day_processing = true),
        ("--ach-tax-id", |a| a.ach_tax_id = Some("123456789".into())),
    ] {
        for mut args in [valid_args(), saved_card_args()] {
            set(&mut args.instrument);
            let err = chosen_instrument(&args.instrument).unwrap_err().to_string();
            assert!(err.contains(flag), "{flag}: {err}");
        }
    }
}

/// A tax id belongs to new account details, which a saved ACH id has none of.
#[test]
fn a_saved_ach_refuses_a_tax_id() {
    let mut args = saved_ach_args();
    args.instrument.ach_tax_id = Some("123456789".into());
    let err = chosen_instrument(&args.instrument).unwrap_err().to_string();
    assert!(err.contains("--ach-tax-id"), "{err}");
}

/// A card charge needs neither, so the ACH rules are genuinely scoped.
#[test]
fn a_card_charge_needs_no_sec_code_or_requester_ip() {
    let args = valid_args();
    assert!(args.instrument.sec_code.is_none());
    assert!(validate_create_transaction(&args).is_ok());
}

#[test]
fn extra_amounts_reach_the_wire_and_are_omitted_when_absent() {
    assert!(
        build_create_transaction_body(&valid_args())
            .unwrap()
            .get("extraAmounts")
            .is_none()
    );

    let mut args = valid_args();
    args.tip_amount = Some("1.50".parse().unwrap());
    args.discount_amount = Some("0.50".parse().unwrap());
    args.surcharge_rate = Some("0.0300".parse().unwrap());
    let extra = &build_create_transaction_body(&args).unwrap()["extraAmounts"];
    assert_eq!(serde_json::to_string(&extra["tipAmount"]).unwrap(), "1.50");
    assert_eq!(
        serde_json::to_string(&extra["discountAmount"]).unwrap(),
        "0.50"
    );

    let mut args = valid_args();
    args.tip_rate = Some("0.1500".parse().unwrap());
    args.discount_rate = Some("0.0500".parse().unwrap());
    args.surcharge_rate = Some("0.0300".parse().unwrap());
    let extra = &build_create_transaction_body(&args).unwrap()["extraAmounts"];
    assert_eq!(serde_json::to_string(&extra["tipRate"]).unwrap(), "0.1500");
    assert_eq!(
        serde_json::to_string(&extra["discountRate"]).unwrap(),
        "0.0500"
    );
    assert_eq!(
        serde_json::to_string(&extra["surchargeRate"]).unwrap(),
        "0.0300"
    );
}

/// An amount and a rate set the same tip, or the same discount, and the
/// API refuses a charge that carries both.
#[test]
fn a_tip_or_discount_given_as_both_an_amount_and_a_rate_is_refused() {
    let mut args = valid_args();
    args.tip_amount = Some("1.50".parse().unwrap());
    args.tip_rate = Some("15".parse().unwrap());
    let err = build_create_transaction_body(&args)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("--tip-amount or --tip-rate, not both"),
        "{err}"
    );

    let mut args = valid_args();
    args.discount_amount = Some("0.50".parse().unwrap());
    args.discount_rate = Some("5".parse().unwrap());
    let err = build_create_transaction_body(&args)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("--discount-amount or --discount-rate, not both"),
        "{err}"
    );
}

#[test]
fn contact_info_reaches_the_wire_and_is_omitted_when_absent() {
    assert!(
        build_create_transaction_body(&valid_args())
            .unwrap()
            .get("contactInfo")
            .is_none()
    );

    let mut args = valid_args();
    args.contact.first_name = Some("Ada".into());
    args.contact.last_name = Some("Lovelace".into());
    args.contact.company_name = Some("Analytical".into());
    args.contact.email = Some("ada@example.com".into());
    args.contact.mobile_phone_number = Some("+14155552309".into());
    args.contact.has_sms_consent = true;
    let contact = &build_create_transaction_body(&args).unwrap()["contactInfo"];
    assert_eq!(contact["firstName"], "Ada");
    assert_eq!(contact["lastName"], "Lovelace");
    assert_eq!(contact["companyName"], "Analytical");
    assert_eq!(contact["email"], "ada@example.com");
    assert_eq!(contact["mobilePhoneNumber"], "+14155552309");
    assert_eq!(contact["hasSmsConsent"], true);
}

#[test]
fn enhanced_data_reaches_the_wire_and_is_omitted_when_absent() {
    assert!(
        build_create_transaction_body(&valid_args())
            .unwrap()
            .get("transactionEnhancedData")
            .is_none()
    );

    let mut args = valid_args();
    args.sales_tax_rate = Some("8.25".parse().unwrap());
    args.invoice_number = Some("INV-1".into());
    args.purchase_order = Some("PO-1".into());
    args.shipping_charges = Some("4.99".parse().unwrap());
    args.products = vec!["productName=Widget,unitPrice=9.99,quantity=2".into()];
    let data = &build_create_transaction_body(&args).unwrap()["transactionEnhancedData"];
    assert_eq!(data["invoiceNumber"], "INV-1");
    assert_eq!(data["purchaseOrder"], "PO-1");
    assert_eq!(
        serde_json::to_string(&data["salesTaxRate"]).unwrap(),
        "8.25"
    );
    assert_eq!(
        serde_json::to_string(&data["shippingCharges"]).unwrap(),
        "4.99"
    );
    // No `dutyCharges`: the server's `TransactionEnhancedDataDto` has no
    // member for it, so the flag is gone and the key must not appear.
    assert!(data.get("dutyCharges").is_none(), "{data}");
    assert_eq!(data["products"][0]["productName"], "Widget");
}

/// Every declared key, and an unknown one is an error rather than a
/// silent drop: `additionalProperties: false` would reject it anyway,
/// with a worse message.
#[test]
fn a_product_takes_key_value_pairs_and_rejects_an_unknown_key() {
    let p = parse_product(
        "productName=Widget,productDescription=A widget,productCode=W-1,\
         unitPrice=9.99,measurementUnit=EA,quantity=2,taxAmount=0.82,\
         discountRate=1.50",
    )
    .unwrap();
    assert_eq!(p["productName"], "Widget");
    assert_eq!(p["productDescription"], "A widget");
    assert_eq!(p["productCode"], "W-1");
    assert_eq!(p["measurementUnit"], "EA");
    assert_eq!(serde_json::to_string(&p["unitPrice"]).unwrap(), "9.99");
    assert_eq!(serde_json::to_string(&p["taxAmount"]).unwrap(), "0.82");
    assert_eq!(serde_json::to_string(&p["quantity"]).unwrap(), "2");
    assert_eq!(serde_json::to_string(&p["discountRate"]).unwrap(), "1.50");

    assert!(parse_product("sku=W-1").is_err());
    assert!(parse_product("productName").is_err());
    assert!(parse_product("").is_err());
}

/// Repeatable, because a level 3 sale has more than one line.
#[test]
fn several_product_flags_become_an_array() {
    let mut args = valid_args();
    args.products = vec![
        "productName=One,unitPrice=1.00".into(),
        "productName=Two,unitPrice=2.00".into(),
    ];
    let data = &build_create_transaction_body(&args).unwrap()["transactionEnhancedData"];
    assert_eq!(data["products"].as_array().unwrap().len(), 2);
    assert_eq!(data["products"][1]["productName"], "Two");
}

/// Bounds the schemas declare, refused here rather than spent on a 400.
#[test]
fn declared_bounds_are_refused_before_the_wire() {
    let mut args = valid_args();
    args.tip_amount = Some(Decimal::ZERO);
    assert!(validate_create_transaction(&args).is_err());

    let mut args = valid_args();
    args.discount_amount = Some(Decimal::ZERO);
    assert!(validate_create_transaction(&args).is_err());

    let mut args = valid_args();
    args.sales_tax_rate = Some("100.01".parse().unwrap());
    assert!(validate_create_transaction(&args).is_err());
    args.sales_tax_rate = Some("100".parse().unwrap());
    assert!(validate_create_transaction(&args).is_ok());
}

/// The args struct is not `Clone` — it is a clap target, constructed
/// once — so tests that need a second copy rebuild the fields they care
/// about.
fn clone_args(a: &CreateTransactionArgs) -> CreateTransactionArgs {
    CreateTransactionArgs {
        instrument: InstrumentArgs {
            payment_method_id: a.instrument.payment_method_id.clone(),
            instrument: a.instrument.instrument,
            ach_account_number: a.instrument.ach_account_number.clone(),
            ach_routing_number: a.instrument.ach_routing_number.clone(),
            ach_account_type: a.instrument.ach_account_type,
            ach_account_holder_type: a.instrument.ach_account_holder_type,
            sec_code: a.instrument.sec_code,
            requester_ip_address: a.instrument.requester_ip_address.clone(),
            ..Default::default()
        },
        contact: ContactArgs {
            first_name: a.contact.first_name.clone(),
            last_name: a.contact.last_name.clone(),
            mobile_phone_number: a.contact.mobile_phone_number.clone(),
            ..Default::default()
        },
        payment_processor_id: a.payment_processor_id.clone(),
        amount: a.amount,
        billing: a.billing.clone(),
        ..Default::default()
    }
}

/// **The contact block includes an email**:
/// `ContactInfo.Email is required for new ACH payments.`
#[test]
fn a_new_ach_transaction_requires_a_contact_email() {
    let mut args = new_ach_args();
    args.contact.email = None;
    let err = build_create_transaction_body(&args)
        .unwrap_err()
        .to_string();
    assert!(err.contains("--contact-email"), "{err}");

    args.contact.email = Some("ada@example.com".into());
    assert!(build_create_transaction_body(&args).is_ok());
}

/// Every missing ACH requirement is named in one error, so a caller fixes the
/// charge in one pass rather than one flag per attempt.
#[test]
fn a_new_ach_charge_names_every_missing_requirement_at_once() {
    let mut args = new_ach_args();
    args.instrument.sec_code = None;
    args.instrument.requester_ip_address = Some(String::new());
    args.billing = BillingArgs::default();
    args.contact = ContactArgs::default();
    let err = validate_create_transaction(&args).unwrap_err().to_string();
    for flag in [
        "--sec-code",
        "--requester-ip",
        "--billing-*",
        "--contact-phone",
        "--contact-email",
        "--contact-first-name",
        "--contact-company",
    ] {
        assert!(err.contains(flag), "{flag} is not named in: {err}");
    }
}
