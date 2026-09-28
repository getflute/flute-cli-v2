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
