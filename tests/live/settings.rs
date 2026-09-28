//! Live scenarios for `settings`. **Committed.**
//!
//! Three reads and one write. The write changes account configuration rather
//! than moving money, so it reads the current value first and puts it back.

use crate::*;

/// **The scenario that makes an error message elsewhere true.**
///
/// Two commands tell a caller missing `--payment-processor-id` to run
/// `settings payment-config`. That advice is only useful if the processor ids
/// are actually in the answer.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_settings_payment_config_lists_the_processor_ids() {
    let config = json(&["settings", "payment-config"]);
    assert_eq!(config["object"], "payment_config");
    let processors = config["data"]["availablePaymentProcessors"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        !processors.is_empty(),
        "payment-config returned no availablePaymentProcessors, so the advice \
         two other commands give about this one is wrong: {}",
        config["data"]
    );
    for p in &processors {
        assert!(
            p["paymentProcessorId"].is_string(),
            "a processor entry with no paymentProcessorId: {p}"
        );
    }
}

#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_settings_contact_info() {
    let info = json(&["settings", "contact-info"]);
    assert_eq!(info["object"], "contact_info");
    assert!(info["data"]["contactInfos"].is_array(), "{info}");
}

#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_settings_autofill_read() {
    let autofill = json(&["settings", "autofill"]);
    assert_eq!(autofill["object"], "transaction_autofill");
}

/// A write that answers 200 with no body, so the only way to see it took is
/// to read it back. The previous rate is restored on the way out — this
/// changes account configuration, not a transaction.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_settings_update_autofill_round_trip() {
    let before = json(&["settings", "autofill"]);
    let previous = before["data"]["level2Settings"]["taxRate"].clone();

    json(&["settings", "update-autofill", "--l2-tax-rate", "7.25"]);
    let after = json(&["settings", "autofill"]);
    assert_eq!(
        after["data"]["level2Settings"]["taxRate"].to_string(),
        "7.25",
        "the rate did not come back as it was sent, so the PATCH is not reaching \
         level2Settings.taxRate"
    );

    // Put it back. `to_string` on the number keeps its exact digits, and the
    // check is presence rather than value — reading a rate through `f64` to
    // decide whether it exists would be the one thing this repository does
    // not do with money.
    if !previous.is_null() {
        json(&[
            "settings",
            "update-autofill",
            "--l2-tax-rate",
            &previous.to_string(),
        ]);
    }
}

/// The declared bound is 0 to 22, which is not the 0.01 to 100 that the same
/// idea carries on `transactions create`. A refusal here spends no round trip.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_settings_update_autofill_rejects_a_rate_above_the_declared_maximum() {
    live_bin()
        .args(["settings", "update-autofill", "--l2-tax-rate", "23"])
        .assert()
        .code(3);
}
