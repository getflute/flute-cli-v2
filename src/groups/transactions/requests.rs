//! Query and body builders for every transaction command except `create`.

use super::*;
use crate::cli::common::parse_exp;
use crate::cli::money::{refuse_pair, to_amount_number};
use anyhow::Result;
use rust_decimal::Decimal;
use serde_json::{Map, Value};

/// The `GET /v2/transactions` query, omitting every absent flag.
pub fn build_list_transactions_query(
    args: &ListTransactionsArgs,
) -> Result<Vec<(&'static str, String)>> {
    args.pagination.validate()?;
    let mut query = args.pagination.query();
    query.extend(common::sort_order(args.asc, args.desc));
    common::push_str(&mut query, "sortBy", &args.sort_by);
    common::push_str(&mut query, "fromDate", &args.from_date);
    common::push_str(&mut query, "toDate", &args.to_date);
    if let Some(source) = args.source_type {
        query.push(("sourceType", common::wire(source)));
    }
    common::push_id(&mut query, "--source-id", "sourceId", &args.source_id)?;
    common::push_id(&mut query, "--batch-id", "batchId", &args.batch_id)?;
    if let Some(status) = args.transaction_status {
        query.push(("transactionStatus", common::wire(status)));
    }
    common::push_str(&mut query, "paymentMethodType", &args.payment_method_type);
    common::push_id(&mut query, "--customer-id", "customerId", &args.customer_id)?;
    common::push_id(&mut query, "--merchant-id", "merchantId", &args.merchant_id)?;
    // Amounts reach the query as their exact digits, never through an f64.
    for (key, value) in [
        ("minAmount", args.min_amount),
        ("maxAmount", args.max_amount),
    ] {
        if let Some(v) = value {
            query.push((key, v.to_string()));
        }
    }
    common::push_id(
        &mut query,
        "--reference-id",
        "referenceId",
        &args.reference_id,
    )?;
    Ok(query)
}

/// `CaptureRequestDto` declares exactly one property, `captureAmount`, with
/// `additionalProperties: false`; the operation's own
/// request example sends `{"amount": 50}`. Both cannot be right, and because
/// an unknown field is rejected rather than ignored, the wrong choice cannot
/// capture at all.
///
/// The schema is normative and an example is not, so this builds
/// `captureAmount` — and the runtime agrees:
/// `live_partial_capture_field_name` captures part of an authorization with
/// it and reads the reduced amount back, so the example is the defect.
pub fn build_capture_body(amount: Option<Decimal>) -> Result<Option<Value>> {
    single_amount_body("captureAmount", amount)
}

/// A full reversal sends no body, as a full capture does.
pub fn build_reversal_body(amount: Option<Decimal>) -> Result<Option<Value>> {
    single_amount_body("reversalAmount", amount)
}

/// Refuse a partial reversal the API would carry out in full.
///
/// The API voids a card transaction whenever a void is still possible, which
/// is until it settles, and reverses an ACH transaction without reading
/// `reversalAmount` at all. Either way the whole amount moves. Only a settled
/// card transaction is refunded for the amount asked, so that is the one
/// state let through; a transaction whose state cannot be read is refused
/// rather than assumed settled.
pub fn refuse_a_partial_reversal_the_api_ignores(transaction: &Value) -> Result<()> {
    let field = |key: &str| transaction.get(key).and_then(Value::as_str);
    let id = field("transactionId").unwrap_or("this transaction");
    match (field("paymentMethodType"), field("transactionStatus")) {
        (Some("Card"), Some("Settled" | "Refunded")) => Ok(()),
        (Some("ACH"), _) => anyhow::bail!(
            "--amount would be ignored: the API reverses an ACH transaction in full. \
             Omit --amount to reverse all of {id}."
        ),
        (Some("Card"), Some(status)) => anyhow::bail!(
            "--amount would be ignored: {id} is {status} and not yet settled, so the API \
             would void the whole amount. Omit --amount to void it, or refund part of it \
             once it has settled."
        ),
        _ => anyhow::bail!(
            "--amount was not sent: the payment method and status of {id} could not be \
             read, so whether the API would honour a partial reversal is unknown"
        ),
    }
}

/// The body for a capture or a reversal, whose single property is optional.
///
/// **No amount is an empty object, not an absent body.** Both operations
/// declare a request schema, and the API answers
/// `400: A non-empty request body is required` to a bodyless POST on one that
/// does — so a full capture sends `{}` and lets the server infer the whole
/// amount. A truly bodyless POST stays right only where the spec declares no
/// request body at all, which is `ach-hold`, `ach-release`, `set-default` and
/// the two cancels.
fn single_amount_body(key: &str, amount: Option<Decimal>) -> Result<Option<Value>> {
    let mut map = Map::new();
    if let Some(v) = amount {
        // Zero moves nothing, and the whole amount is said by omitting the
        // flag rather than by naming nothing.
        if v <= Decimal::ZERO {
            anyhow::bail!("--amount must be greater than zero");
        }
        map.insert(key.to_string(), to_amount_number(v)?);
    }
    Ok(Some(Value::Object(map)))
}

/// Build the `TipAdjustmentRequestDto` body.
///
/// An amount or a rate, not both, as on `transactions create`. The one given
/// must be greater than zero: zero moves no tip.
pub fn build_tip_adjustment_body(
    tip_amount: Option<Decimal>,
    tip_rate: Option<Decimal>,
) -> Result<Value> {
    refuse_pair(("--tip-amount", tip_amount), ("--tip-rate", tip_rate))?;
    let mut map = Map::new();
    for (flag, key, value) in [
        ("--tip-amount", "tipAmount", tip_amount),
        ("--tip-rate", "tipRate", tip_rate),
    ] {
        if let Some(v) = value {
            if v <= Decimal::ZERO {
                anyhow::bail!("{flag} must be greater than zero");
            }
            map.insert(key.to_string(), to_amount_number(v)?);
        }
    }
    if map.is_empty() {
        anyhow::bail!("pass --tip-amount or --tip-rate");
    }
    Ok(Value::Object(map))
}

/// Build the `SendReceiptRequestDto` body. All three fields are required.
pub fn build_share_receipt_body(args: &ShareReceiptArgs) -> Result<Value> {
    if args.recipient.trim().is_empty() {
        anyhow::bail!("--recipient is required");
    }
    Ok(Value::Object(Map::from_iter([
        ("shareBy".to_string(), serde_json::json!(args.share_by)),
        (
            "recipient".to_string(),
            Value::String(args.recipient.clone()),
        ),
        (
            "hasCustomerConsent".to_string(),
            Value::Bool(args.has_customer_consent),
        ),
    ])))
}

/// Build the calculate-amount body. No field is required by schema, but an
/// amount to calculate on is the point of the call. The tip and discount
/// rules are `create`'s.
pub fn build_calculate_amount_body(args: &CalculateAmountArgs) -> Result<Value> {
    if args.base_amount <= Decimal::ZERO {
        anyhow::bail!("--amount must be greater than zero");
    }
    validate_extra_amounts(
        args.tip_amount,
        args.tip_rate,
        args.discount_amount,
        args.discount_rate,
    )?;
    let mut body = Map::new();
    body.insert("baseAmount".into(), to_amount_number(args.base_amount)?);
    if let Some(code) = args.currency_code.as_ref().filter(|s| !s.is_empty()) {
        body.insert("currencyCode".into(), Value::String(code.clone()));
    }
    if let Some(pricing) = args.pricing_type {
        body.insert("pricingType".into(), serde_json::json!(pricing));
    }
    for (key, value) in [
        ("tipAmount", args.tip_amount),
        ("tipRate", args.tip_rate),
        ("discountAmount", args.discount_amount),
        ("discountRate", args.discount_rate),
        ("surchargeRate", args.surcharge_rate),
    ] {
        if let Some(v) = value {
            body.insert(key.to_string(), to_amount_number(v)?);
        }
    }
    Ok(Value::Object(body))
}

/// Build the `CreditRequestDto` body.
///
/// The instrument rules are `create`'s, because they are one declaration.
/// Two things differ: `referenceId` is **required**, and
/// `creditDetails.cardData` declares no `captureMethod` — a credit is not an
/// authorization, so none is sent.
///
/// **The conditional ACH rule reaches this endpoint too**, so it is enforced
/// here as well: `POST /v2/transactions/credit` answers `BillingAddress is
/// required for new ACH credits.; ContactInfo is required for new ACH
/// credits.`
pub fn build_credit_body(args: &CreditArgs) -> Result<Value> {
    if args.base_amount <= Decimal::ZERO {
        anyhow::bail!("--amount must be greater than zero");
    }
    if args.payment_processor_id.trim().is_empty() {
        anyhow::bail!(
            "--payment-processor-id is required. List the processors configured \
             for this account with `flute2 settings payment-config`."
        );
    }
    if args.reference_id.trim().is_empty() {
        anyhow::bail!("--reference-id is required on a credit");
    }

    let chosen = chosen_instrument(&args.instrument)?;
    if chosen == Chosen::NewCard {
        parse_exp(args.instrument.exp.as_deref().unwrap_or_default())?;
    }
    if chosen.is_ach() {
        refuse_missing_ach_requirements(
            chosen,
            &args.instrument,
            &args.billing,
            &args.contact,
            "credit",
        )?;
    }

    let mut body = Map::new();
    body.insert(
        "paymentProcessorId".into(),
        Value::String(args.payment_processor_id.clone()),
    );
    body.insert("baseAmount".into(), to_amount_number(args.base_amount)?);
    body.insert(
        "referenceId".into(),
        Value::String(args.reference_id.clone()),
    );
    body.insert(
        "creditDetails".into(),
        instrument_envelope(&args.instrument, chosen, None)?,
    );
    for (key, value) in [
        ("currencyCode", &args.currency_code),
        ("customerId", &args.customer_id),
    ] {
        if let Some(v) = value.as_ref().filter(|s| !s.is_empty()) {
            body.insert(key.to_string(), Value::String(v.clone()));
        }
    }
    if let Some(contact) = contact_info(&args.contact) {
        body.insert("contactInfo".into(), contact);
    }
    if let Some(a) = args.billing.to_address() {
        body.insert("billingAddress".into(), a);
    }
    if let Some(a) = args.shipping.to_address() {
        body.insert("shippingAddress".into(), a);
    }
    Ok(Value::Object(body))
}

#[cfg(test)]
mod tests;
