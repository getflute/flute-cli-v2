use super::*;
use crate::cli::money::PatchNumber;

fn update_args() -> UpdatePaymentLinkArgs {
    UpdatePaymentLinkArgs {
        payment_link_id: "pl_1".into(),
        ..Default::default()
    }
}

/// Both spellings reach the same null, so a caller can use whichever reads
/// better at the call site.
#[test]
fn the_two_spellings_send_the_same_null() {
    let empty = UpdatePaymentLinkArgs {
        description: Some(String::new()),
        ..update_args()
    };
    let flag = UpdatePaymentLinkArgs {
        clear: vec![Clearable::Description],
        ..update_args()
    };
    assert_eq!(
        build_update_payment_link_body(&empty).unwrap(),
        build_update_payment_link_body(&flag).unwrap()
    );
}

/// A decimal has no empty spelling of its own, so the value parser gives it
/// one: both routes null `baseAmount`, which makes the link flexible.
#[test]
fn an_amount_clears_by_either_spelling() {
    for args in [
        UpdatePaymentLinkArgs {
            base_amount: Some(PatchNumber::Clear),
            ..update_args()
        },
        UpdatePaymentLinkArgs {
            clear: vec![Clearable::Amount],
            ..update_args()
        },
    ] {
        let body = build_update_payment_link_body(&args).unwrap();
        assert_eq!(body["baseAmount"], Value::Null);
    }
}

/// Setting and clearing one field in one invocation is a contradiction,
/// and preferring either silently answers a question nobody asked.
#[test]
fn setting_and_clearing_one_field_is_refused() {
    let args = UpdatePaymentLinkArgs {
        description: Some("new".into()),
        clear: vec![Clearable::Description],
        ..update_args()
    };
    let err = build_update_payment_link_body(&args)
        .unwrap_err()
        .to_string();
    assert!(err.contains("description"), "{err}");
}

/// The refusal names the flag the caller typed, not the wire key it sets.
#[test]
fn a_contradicting_clear_is_named_by_its_flag() {
    let args = UpdatePaymentLinkArgs {
        expires_on: Some("2027-01-01T00:00:00Z".into()),
        clear: vec![Clearable::ExpiresOn],
        ..update_args()
    };
    let err = build_update_payment_link_body(&args)
        .unwrap_err()
        .to_string();
    assert!(err.contains("--clear expires-on "), "{err}");
    assert!(!err.contains("expiresOn"), "{err}");
}

/// An empty value clears a nullable field, the same spelling every other
/// update uses.
#[test]
fn an_empty_value_clears_a_nullable_field() {
    let args = UpdatePaymentLinkArgs {
        payment_link_id: "pl_1".into(),
        description: Some(String::new()),
        ..Default::default()
    };
    let body = build_update_payment_link_body(&args).unwrap();
    assert_eq!(body["description"], Value::Null);
}

/// `name` and `currencyCode` are refused a null by the API, so the empty
/// spelling is refused here instead of on a round trip.
#[test]
fn the_fields_the_api_will_not_clear_are_refused_first() {
    for args in [
        UpdatePaymentLinkArgs {
            payment_link_id: "pl_1".into(),
            name: Some(String::new()),
            ..Default::default()
        },
        UpdatePaymentLinkArgs {
            payment_link_id: "pl_1".into(),
            currency_code: Some(String::new()),
            ..Default::default()
        },
    ] {
        let err = build_update_payment_link_body(&args)
            .unwrap_err()
            .to_string();
        assert!(err.contains("cannot be cleared"), "{err}");
    }
}

/// The tax fields and the metadata map clear by both spellings, and each
/// spelling is one request.
#[test]
fn tax_and_metadata_clear_by_either_spelling() {
    let by_value = UpdatePaymentLinkArgs {
        tax_amount: Some(PatchNumber::Clear),
        tax_rate: Some(PatchNumber::Clear),
        tax_mode: Some(TaxModePatch::Clear),
        metadata: vec![String::new()],
        ..update_args()
    };
    let by_flag = UpdatePaymentLinkArgs {
        clear: vec![
            Clearable::TaxAmount,
            Clearable::TaxRate,
            Clearable::TaxMode,
            Clearable::Metadata,
        ],
        ..update_args()
    };
    let body = build_update_payment_link_body(&by_value).unwrap();
    assert_eq!(body, build_update_payment_link_body(&by_flag).unwrap());
    for key in ["taxAmount", "taxRate", "taxMode", "metadata"] {
        assert_eq!(body[key], Value::Null, "{key}");
    }
}

/// A set tax amount keeps its digits, a set metadata replaces the whole
/// map, and the faster-processing flag takes a value so it can turn off.
#[test]
fn tax_metadata_and_faster_processing_set_on_update() {
    let args = UpdatePaymentLinkArgs {
        tax_amount: Some(PatchNumber::Set("0.75".parse().unwrap())),
        tax_mode: Some(TaxModePatch::Set(TaxMode::Inclusive)),
        metadata: vec!["z=1".into(), "y=a=b".into()],
        ach_enabled: Some(true),
        ach_faster_processing: Some(false),
        ..update_args()
    };
    let body = build_update_payment_link_body(&args).unwrap();
    assert_eq!(body["taxAmount"].to_string(), "0.75");
    assert_eq!(body["taxMode"], "Inclusive");
    assert_eq!(body["metadata"], serde_json::json!({"z": "1", "y": "a=b"}));
    assert_eq!(
        body["paymentMethods"]["ach"],
        serde_json::json!({"enabled": true, "fasterProcessing": false})
    );
}

#[test]
fn setting_and_clearing_a_tax_field_is_refused() {
    let args = UpdatePaymentLinkArgs {
        tax_rate: Some(PatchNumber::Set("5".parse().unwrap())),
        clear: vec![Clearable::TaxRate],
        ..update_args()
    };
    let err = build_update_payment_link_body(&args)
        .unwrap_err()
        .to_string();
    assert!(err.contains("tax-rate"), "{err}");
}

/// Faster processing cannot ride along with turning ACH off.
#[test]
fn faster_processing_with_ach_disabled_is_refused() {
    let args = UpdatePaymentLinkArgs {
        ach_enabled: Some(false),
        ach_faster_processing: Some(true),
        ..update_args()
    };
    let err = build_update_payment_link_body(&args)
        .unwrap_err()
        .to_string();
    assert!(err.contains("--ach-faster-processing"), "{err}");
}
