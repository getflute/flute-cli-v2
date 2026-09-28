//! `transactions create`: choosing the instrument, validating the flags, and
//! building the body.
//!
//! The riskiest path in the CLI: a real card number shape, exact decimals, and
//! two request fields the published schema does not document.

use super::*;
use crate::cli::address::BillingArgs;
use crate::cli::common::{CaptureMethod, parse_exp};
use crate::cli::money::{parse_amount, parse_rate, to_amount_number};
use anyhow::Result;
use rust_decimal::Decimal;
use serde_json::{Map, Value};

/// The `requesterIpAddress` an ACH instrument sends when `--requester-ip` is
/// not given. It is applied here rather than as a clap default so a card can
/// tell an explicit `--requester-ip` from none.
const DEFAULT_REQUESTER_IP: &str = "127.0.0.1";

/// Which of the four instrument shapes the flags describe.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Chosen {
    NewCard,
    SavedCard,
    NewAch,
    SavedAch,
}

impl Chosen {
    pub(super) fn is_ach(self) -> bool {
        matches!(self, Self::NewAch | Self::SavedAch)
    }
}

/// Decide which instrument shape was asked for, refusing anything but one.
///
/// `paymentMethodId` and `paymentMethodDetails` are mutually exclusive on
/// both `cardData` and `achData`, so two instruments is as wrong as none.
pub fn chosen_instrument(args: &InstrumentArgs) -> Result<Chosen> {
    let filled = |v: &Option<String>| v.as_ref().is_some_and(|s| !s.is_empty());
    let card_parts = [&args.card, &args.cvv, &args.exp]
        .iter()
        .filter(|f| filled(f))
        .count();
    let ach_parts = usize::from(filled(&args.ach_account_number))
        + usize::from(filled(&args.ach_routing_number))
        + usize::from(args.ach_account_type.is_some())
        + usize::from(args.ach_account_holder_type.is_some());
    let saved = filled(&args.payment_method_id);

    let mut chosen = Vec::new();
    if card_parts > 0 {
        if card_parts != 3 {
            anyhow::bail!("--card, --cvv and --exp must be supplied together");
        }
        chosen.push(Chosen::NewCard);
    }
    if ach_parts > 0 {
        if ach_parts != 4 {
            anyhow::bail!(
                "--ach-account-number, --ach-routing-number, --ach-account-type and \
                 --ach-account-holder-type must be supplied together"
            );
        }
        chosen.push(Chosen::NewAch);
    }
    if saved {
        match args.instrument {
            Some(Instrument::Card) => chosen.push(Chosen::SavedCard),
            Some(Instrument::Ach) => chosen.push(Chosen::SavedAch),
            // A stored id says nothing about its own type, and cardData and
            // achData are different objects.
            None => anyhow::bail!(
                "--payment-method-id needs --instrument card or --instrument ach: a \
                 stored instrument id does not say which it is"
            ),
        }
    }
    if let [only] = chosen[..] {
        refuse_fields_the_instrument_drops(args, only)?;
    }
    match chosen.len() {
        1 => Ok(chosen[0]),
        0 => anyhow::bail!(
            "exactly one payment instrument is required: pass --card with --cvv and \
             --exp, the --ach-* flags, or --payment-method-id with --instrument"
        ),
        _ => anyhow::bail!(
            "exactly one payment instrument is allowed; {} were supplied",
            chosen.len()
        ),
    }
}

/// Refuse a flag the chosen instrument has no field for, which would
/// otherwise be dropped from the request: `secCode`, `requesterIpAddress` and
/// `isSameDayProcessing` live on `achData`, and `taxId` on a new account's
/// details.
fn refuse_fields_the_instrument_drops(args: &InstrumentArgs, chosen: Chosen) -> Result<()> {
    let mut dropped = Vec::new();
    if !chosen.is_ach() {
        if args.sec_code.is_some() {
            dropped.push("--sec-code");
        }
        if args.requester_ip_address.is_some() {
            dropped.push("--requester-ip");
        }
        if args.is_same_day_processing {
            dropped.push("--same-day");
        }
    }
    if chosen != Chosen::NewAch && args.ach_tax_id.is_some() {
        dropped.push("--ach-tax-id");
    }
    if dropped.is_empty() {
        return Ok(());
    }
    let only = if chosen.is_ach() {
        "a new bank account (the --ach-* account flags)"
    } else {
        "an ACH instrument"
    };
    anyhow::bail!("{} applies to {only} only", dropped.join(", "))
}

/// The amount-or-rate exclusions and the declared minimums shared by
/// `transactions create` and `transactions calculate-amount`. A non-zero
/// value cannot go to both halves of a pair.
pub(super) fn validate_extra_amounts(
    tip_amount: Option<Decimal>,
    tip_rate: Option<Decimal>,
    discount_amount: Option<Decimal>,
    discount_rate: Option<Decimal>,
) -> Result<()> {
    let non_zero = |v: Option<Decimal>| v.is_some_and(|d| !d.is_zero());
    for (amount, rate, both) in [
        (
            "--tip-amount",
            "--tip-rate",
            non_zero(tip_amount) && non_zero(tip_rate),
        ),
        (
            "--discount-amount",
            "--discount-rate",
            non_zero(discount_amount) && non_zero(discount_rate),
        ),
    ] {
        if both {
            anyhow::bail!("pass {amount} or {rate}, not both: they set the same value");
        }
    }
    let min = Decimal::new(1, 2);
    for (flag, value) in [
        ("--tip-amount", tip_amount),
        ("--discount-amount", discount_amount),
    ] {
        if let Some(v) = value {
            if v < min {
                anyhow::bail!("{flag} must be at least 0.01 (got {v})");
            }
        }
    }
    Ok(())
}

/// Mirror the server's documented rules before spending a round trip.
///
/// A request with no payment instrument is refused here: the API answers one
/// with a 500.
pub fn validate_create_transaction(args: &CreateTransactionArgs) -> Result<Chosen> {
    if args.amount <= Decimal::ZERO {
        anyhow::bail!("--amount must be greater than zero");
    }
    if args.payment_processor_id.trim().is_empty() {
        anyhow::bail!(
            "--payment-processor-id is required. List the processors configured \
             for this account with `flute2 settings payment-config`."
        );
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
            "payment",
        )?;
        // `AchDataDto` has no `captureMethod`, so an ACH charge cannot be an
        // authorization and would run as an immediate debit.
        if args.capture_method == CaptureMethod::Manual {
            anyhow::bail!(
                "--capture-method manual applies to a card only: an ACH transaction \
                 cannot be authorized for later capture"
            );
        }
    }
    validate_extra_amounts(
        args.tip_amount,
        args.tip_rate,
        args.discount_amount,
        args.discount_rate,
    )?;
    validate_declared_bounds(args)?;
    Ok(chosen)
}

/// Refuse an ACH charge or credit that lacks any requirement, naming all of
/// them in one error.
///
/// `secCode` and `requesterIpAddress` are required by schema on `AchDataDto`,
/// so both ACH routes need them. A *new* ACH account additionally requires
/// `billingAddress`, `contactInfo.mobilePhoneNumber`, `contactInfo.email`,
/// and either a name pair or a company name. That rule is conditional — it
/// does not apply to a card, or to ACH against a saved `paymentMethodId` —
/// which OpenAPI's `required` array cannot express, so it appears nowhere in
/// the schema. Posting without them returns 400 with
/// `"ContactInfo is required for new ACH payments."`.
pub(super) fn refuse_missing_ach_requirements(
    chosen: Chosen,
    instrument: &InstrumentArgs,
    billing: &BillingArgs,
    contact: &ContactArgs,
    noun: &str,
) -> Result<()> {
    let filled = |v: &Option<String>| v.as_ref().is_some_and(|s| !s.is_empty());
    let mut missing = Vec::new();
    if instrument.sec_code.is_none() {
        missing.push("--sec-code");
    }
    if instrument.requester_ip_address.as_deref() == Some("") {
        missing.push("--requester-ip");
    }
    if chosen == Chosen::NewAch {
        if billing.to_address().is_none() {
            missing.push("a billing address (--billing-*)");
        }
        if !filled(&contact.mobile_phone_number) {
            missing.push("--contact-phone");
        }
        if !filled(&contact.email) {
            missing.push("--contact-email");
        }
        let has_person = filled(&contact.first_name) && filled(&contact.last_name);
        if !has_person && !filled(&contact.company_name) {
            missing.push(
                "a name (--contact-first-name with --contact-last-name, or --contact-company)",
            );
        }
    }
    if missing.is_empty() {
        return Ok(());
    }
    anyhow::bail!("an ACH {noun} also needs: {}", missing.join(", "))
}

/// Bounds the schemas declare, refused here rather than spent on a 400.
fn validate_declared_bounds(args: &CreateTransactionArgs) -> Result<()> {
    let min = Decimal::new(1, 2);
    if let Some(rate) = args.sales_tax_rate {
        if rate < min || rate > Decimal::from(100) {
            anyhow::bail!("--l2-tax-rate must be between 0.01 and 100 (got {rate})");
        }
    }
    Ok(())
}

/// Build the `CreateTransactionRequestDto` body.
///
/// Two fields here are **absent from the published schema** and real, and they
/// are not the same kind of thing. `captureMethod` is sent on every card
/// charge: it is the only way to create an authorization rather than an
/// immediate charge, so omitting it removes a capability. `achData` declares
/// no such field, so an ACH charge carries none. `customerId` is sent only when the flag
/// is given — it is documented nowhere and observed to be *accepted*, and
/// nothing observed says it is required, so treating it as mandatory would
/// force a customer record onto every charge.
pub fn build_create_transaction_body(args: &CreateTransactionArgs) -> Result<Value> {
    let chosen = validate_create_transaction(args)?;

    let mut body = Map::new();
    body.insert(
        "paymentProcessorId".into(),
        Value::String(args.payment_processor_id.clone()),
    );
    // Never through f64: the amount must survive as exact decimals.
    body.insert("baseAmount".into(), to_amount_number(args.amount)?);
    body.insert(
        "transactionDetails".into(),
        instrument_envelope(&args.instrument, chosen, Some(args.capture_method))?,
    );
    common::put_str(&mut body, "customerId", &args.customer_id);
    common::put_str(&mut body, "referenceId", &args.reference_id);
    common::put_str(&mut body, "currencyCode", &args.currency_code);

    // A bare switch cannot distinguish "not passed" from "passed false", so
    // only the true case is sent and the server's default governs otherwise.
    if args.is_customer_initiated_transaction {
        body.insert("isCustomerInitiatedTransaction".into(), Value::Bool(true));
    }
    if let Some(pricing) = args.pricing_type {
        body.insert("pricingType".into(), serde_json::json!(pricing));
    }
    if let Some(extra) = extra_amounts(args)? {
        body.insert("extraAmounts".into(), extra);
    }
    if let Some(contact) = contact_info(&args.contact) {
        body.insert("contactInfo".into(), contact);
    }
    if let Some(enhanced) = enhanced_data(args)? {
        body.insert("transactionEnhancedData".into(), enhanced);
    }
    if let Some(a) = args.billing.to_address() {
        body.insert("billingAddress".into(), a);
    }
    if let Some(a) = args.shipping.to_address() {
        body.insert("shippingAddress".into(), a);
    }
    Ok(Value::Object(body))
}

/// `cardData` or `achData`, and within each the saved id or the raw details —
/// which the schema declares mutually exclusive.
///
/// `capture_method` is `Some` only on `transactions create`:
/// `CreditRequestDto` has none, because a credit is not an authorization.
fn instrument_details(
    args: &InstrumentArgs,
    chosen: Chosen,
    capture_method: Option<CaptureMethod>,
) -> Result<(&'static str, Value)> {
    let mut inner = Map::new();
    let key = match chosen {
        Chosen::NewCard | Chosen::SavedCard => {
            if let Some(method) = capture_method {
                inner.insert("captureMethod".into(), serde_json::json!(method));
            }
            if chosen == Chosen::SavedCard {
                inner.insert(
                    "paymentMethodId".into(),
                    Value::String(args.payment_method_id.clone().unwrap_or_default()),
                );
            } else {
                let (month, year) = parse_exp(args.exp.as_deref().unwrap_or_default())?;
                let mut details = Map::new();
                details.insert(
                    "cardNumber".into(),
                    Value::String(args.card.clone().unwrap_or_default()),
                );
                details.insert(
                    "securityCode".into(),
                    Value::String(args.cvv.clone().unwrap_or_default()),
                );
                details.insert("expirationMonth".into(), Value::from(month));
                details.insert("expirationYear".into(), Value::from(year));
                inner.insert("paymentMethodDetails".into(), Value::Object(details));
            }
            "cardData"
        }
        Chosen::NewAch | Chosen::SavedAch => {
            // Required by schema on `AchDataDto`, so both routes carry them.
            if let Some(sec) = args.sec_code {
                inner.insert("secCode".into(), serde_json::json!(sec));
            }
            let ip = args
                .requester_ip_address
                .as_deref()
                .unwrap_or(DEFAULT_REQUESTER_IP);
            if !ip.is_empty() {
                inner.insert("requesterIpAddress".into(), Value::String(ip.into()));
            }
            if args.is_same_day_processing {
                inner.insert("isSameDayProcessing".into(), Value::Bool(true));
            }
            if chosen == Chosen::SavedAch {
                inner.insert(
                    "paymentMethodId".into(),
                    Value::String(args.payment_method_id.clone().unwrap_or_default()),
                );
            } else {
                let mut details = Map::new();
                details.insert(
                    "accountNumber".into(),
                    Value::String(args.ach_account_number.clone().unwrap_or_default()),
                );
                details.insert(
                    "routingNumber".into(),
                    Value::String(args.ach_routing_number.clone().unwrap_or_default()),
                );
                if let Some(ty) = args.ach_account_type {
                    details.insert("accountType".into(), serde_json::json!(ty));
                }
                if let Some(ty) = args.ach_account_holder_type {
                    details.insert("accountHolderType".into(), serde_json::json!(ty));
                }
                if let Some(tax) = args.ach_tax_id.as_ref().filter(|s| !s.is_empty()) {
                    details.insert("taxId".into(), Value::String(tax.clone()));
                }
                inner.insert("paymentMethodDetails".into(), Value::Object(details));
            }
            "achData"
        }
    };
    Ok((key, Value::Object(inner)))
}

/// Wrap the instrument in the container the operation declares:
/// `transactionDetails` on create, `creditDetails` on credit.
pub(super) fn instrument_envelope(
    args: &InstrumentArgs,
    chosen: Chosen,
    capture_method: Option<CaptureMethod>,
) -> Result<Value> {
    let (key, value) = instrument_details(args, chosen, capture_method)?;
    Ok(Value::Object(Map::from_iter([(key.to_string(), value)])))
}

/// `None` when no component was supplied, so an absent object is an absent
/// key rather than an empty one the API would have to interpret.
fn extra_amounts(args: &CreateTransactionArgs) -> Result<Option<Value>> {
    let mut map = Map::new();
    for (key, value) in [
        ("tipAmount", args.tip_amount),
        ("tipRate", args.tip_rate),
        ("discountAmount", args.discount_amount),
        ("discountRate", args.discount_rate),
        ("surchargeRate", args.surcharge_rate),
    ] {
        if let Some(v) = value {
            map.insert(key.to_string(), to_amount_number(v)?);
        }
    }
    Ok((!map.is_empty()).then_some(Value::Object(map)))
}

pub(super) fn contact_info(args: &ContactArgs) -> Option<Value> {
    let mut map = Map::new();
    common::put_str(&mut map, "firstName", &args.first_name);
    common::put_str(&mut map, "lastName", &args.last_name);
    common::put_str(&mut map, "companyName", &args.company_name);
    common::put_str(&mut map, "email", &args.email);
    common::put_str(&mut map, "mobilePhoneNumber", &args.mobile_phone_number);
    if args.has_sms_consent {
        map.insert("hasSmsConsent".into(), Value::Bool(true));
    }
    (!map.is_empty()).then_some(Value::Object(map))
}

fn enhanced_data(args: &CreateTransactionArgs) -> Result<Option<Value>> {
    let mut map = Map::new();
    for (key, value) in [
        ("salesTaxRate", args.sales_tax_rate),
        ("shippingCharges", args.shipping_charges),
    ] {
        if let Some(v) = value {
            map.insert(key.to_string(), to_amount_number(v)?);
        }
    }
    for (key, value) in [
        ("invoiceNumber", &args.invoice_number),
        ("purchaseOrder", &args.purchase_order),
    ] {
        if let Some(v) = value.as_ref().filter(|s| !s.is_empty()) {
            map.insert(key.to_string(), Value::String(v.clone()));
        }
    }
    if !args.products.is_empty() {
        let products: Result<Vec<Value>> = args.products.iter().map(|s| parse_product(s)).collect();
        map.insert("products".into(), Value::Array(products?));
    }
    Ok((!map.is_empty()).then_some(Value::Object(map)))
}

/// The wire keys `TransactionProductIsvDto` declares, and which parser each
/// one's value goes through.
///
/// `unitPrice` and `taxAmount` are money and take two decimal places;
/// `quantity` and `discountRate` are not, and take four — a quantity of 2.5
/// and a rate of 1.0500 are both legitimate.
const PRODUCT_FIELDS: &[(&str, Numeric)] = &[
    ("productName", Numeric::No),
    ("productDescription", Numeric::No),
    ("productCode", Numeric::No),
    ("measurementUnit", Numeric::No),
    ("unitPrice", Numeric::Money),
    ("taxAmount", Numeric::Money),
    ("quantity", Numeric::Rate),
    ("discountRate", Numeric::Rate),
];

#[derive(Copy, Clone, PartialEq, Eq)]
enum Numeric {
    No,
    Money,
    Rate,
}

/// Parse one `--l3-product` value: comma-separated `key=value` pairs whose
/// keys are the wire names.
///
/// The product declares **eight** fields, and a positional list of eight is
/// neither readable nor extendable. An unknown key is an error
/// rather than a silent drop: the schema sets `additionalProperties: false`,
/// so a typo would be rejected by the API anyway — with a worse message.
pub fn parse_product(raw: &str) -> Result<Value> {
    let mut map = Map::new();
    for pair in raw.split(',') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        let Some((key, value)) = pair.split_once('=') else {
            anyhow::bail!(
                "--l3-product takes comma-separated key=value pairs; '{pair}' has no '='"
            );
        };
        let (key, value) = (key.trim(), value.trim());
        let Some((_, numeric)) = PRODUCT_FIELDS.iter().find(|(k, _)| *k == key) else {
            anyhow::bail!(
                "--l3-product: unknown key '{key}'. Known keys: {}",
                PRODUCT_FIELDS
                    .iter()
                    .map(|(k, _)| *k)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        };
        let parsed = match numeric {
            Numeric::No => Value::String(value.to_string()),
            Numeric::Money => to_amount_number(parse_amount(value)?)?,
            Numeric::Rate => to_amount_number(parse_rate(value)?)?,
        };
        map.insert(key.to_string(), parsed);
    }
    if map.is_empty() {
        anyhow::bail!("--l3-product needs at least one key=value pair");
    }
    Ok(Value::Object(map))
}

/// Pull the transaction out of whatever the API sent.
///
/// The seven single-transaction writes are *documented* as returning a paged
/// collection and return a bare object. The page arm is explicitly
/// **transitional**: it exists so that the server converging on its own
/// published schema is not an outage, and it is deleted when that happens.
/// A page with any count but one is a contract break rather than an invitation
/// to pick the first element.
pub fn unwrap_single_transaction(body: Value) -> Result<Value> {
    let Some(items) = body.get("items").and_then(Value::as_array) else {
        return Ok(body);
    };
    match items.len() {
        1 => Ok(items[0].clone()),
        // `Decode`, not a bare error: the API sent something the CLI cannot
        // read, which the contract classifies as exit 1 and `kind: "decode"`.
        // A bare anyhow error would land in the client arm and exit 3, which
        // blames the caller for the server's response.
        n => Err(crate::api::ApiError::Decode(format!(
            "expected a single transaction, the API returned a page of {n}. \
             This endpoint is documented as paged and observed to return one \
             object; neither shape allows {n}."
        ))
        .into()),
    }
}

#[cfg(test)]
mod tests;
