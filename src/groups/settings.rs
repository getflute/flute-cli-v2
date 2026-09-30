//! `settings`: payment-config, contact-info, autofill, update-autofill.
//!
//! Three reads and one write, none of them paginated and none taking a query
//! parameter — every settings resource is a singleton at a fixed path.
//!
//! `payment-config` is the command two other errors send people to. A caller
//! who omitted `--payment-processor-id` on a charge or a batch close is told
//! to run it, so its table puts the processor ids above the account trivia.

use crate::Ctx;
use crate::cli::common;
use crate::cli::money::{
    self, PatchNumber, parse_amount_patch, parse_rate_patch, to_amount_number,
};
use crate::cli::output::OutputFormat;
use crate::cli::render::{self, Cell, Column, Resource};
use anyhow::Result;
use reqwest::Method;
use rust_decimal::Decimal;
use serde_json::{Map, Value};

/// The declared bound on `level2Settings.taxRate`.
///
/// Not the bound the same idea carries on `transactions create`, where
/// `salesTaxRate` runs 0.01 to 100. Both are taken from their own schema
/// rather than harmonised, because the API is what rejects the value.
const TAX_RATE: std::ops::RangeInclusive<i64> = 0..=22;

/// A default `--clear` can null out, and where the wire key sits.
///
/// `taxRate` is absent on purpose: it maps to a non-nullable column, so a null
/// is answered `cannot be cleared` and the round trip is spent on a refusal.
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Clearable {
    L3ShippingRate,
    L3DutyRate,
    ProductName,
    ProductCode,
    ProductUnit,
    ProductUnitPrice,
    ProductQuantity,
    ProductDiscount,
}

impl Clearable {
    fn wire(self) -> &'static str {
        match self {
            Self::L3ShippingRate => "shippingChargeRate",
            Self::L3DutyRate => "dutyChargeRate",
            Self::ProductName => "productName",
            Self::ProductCode => "code",
            Self::ProductUnit => "measurementUnit",
            Self::ProductUnitPrice => "unitPrice",
            Self::ProductQuantity => "quantity",
            Self::ProductDiscount => "discountPercentage",
        }
    }

    /// The product's defaults nest one level below the level-3 group.
    fn in_product(self) -> bool {
        !matches!(self, Self::L3ShippingRate | Self::L3DutyRate)
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(clap::Subcommand, Debug)]
pub enum SettingsCommand {
    /// Show the account's payment configuration
    /// (GET /v2/settings/payment-config).
    ///
    /// Processors, card types, tips and limits configured for the account.
    PaymentConfig,
    /// Show the account's contact information
    /// (GET /v2/settings/contact-information).
    ContactInfo,
    /// Show the level 2 and level 3 autofill values
    /// (GET /v2/settings/transaction-autofill).
    Autofill,
    /// Update the level 2 and level 3 autofill values
    /// (PATCH /v2/settings/transaction-autofill).
    UpdateAutofill(UpdateAutofillArgs),
}

#[derive(clap::Args, Debug, Default)]
pub struct UpdateAutofillArgs {
    /// Default level-2 sales tax rate as a percentage, 0 to 22.
    #[arg(long = "l2-tax-rate", value_parser = parse_rate_patch, value_name = "TAX_RATE", allow_negative_numbers = true)]
    pub tax_rate: Option<PatchNumber>,
    /// Default level-3 shipping charge rate, as a percentage.
    #[arg(long = "l3-shipping-rate", value_parser = parse_rate_patch, value_name = "SHIPPING_CHARGE_RATE", allow_negative_numbers = true)]
    pub shipping_charge_rate: Option<PatchNumber>,
    /// Default level-3 duty charge rate, as a percentage.
    #[arg(long = "l3-duty-rate", value_parser = parse_rate_patch, value_name = "DUTY_CHARGE_RATE", allow_negative_numbers = true)]
    pub duty_charge_rate: Option<PatchNumber>,
    /// Default product name.
    #[arg(long = "product-name")]
    pub product_name: Option<String>,
    /// Default product code.
    #[arg(long = "product-code")]
    pub code: Option<String>,
    /// Default unit of measurement, e.g. `EA`.
    #[arg(long = "product-unit")]
    pub measurement_unit: Option<String>,
    /// Default unit price.
    #[arg(long = "product-unit-price", value_parser = parse_amount_patch, value_name = "UNIT_PRICE", allow_negative_numbers = true)]
    pub unit_price: Option<PatchNumber>,
    /// Default quantity.
    #[arg(long = "product-quantity", value_parser = parse_rate_patch, value_name = "QUANTITY", allow_negative_numbers = true)]
    pub quantity: Option<PatchNumber>,
    /// Default product discount, as a percentage.
    #[arg(long = "product-discount", value_parser = parse_rate_patch, value_name = "DISCOUNT_PERCENTAGE", allow_negative_numbers = true)]
    pub discount_percentage: Option<PatchNumber>,
    /// Clear a stored default back to nothing. Repeat for several defaults.
    ///
    /// An empty value does the same: `--product-code ""` and
    /// `--clear product-code` are one request. `l2-tax-rate` cannot be cleared
    /// and is not offered.
    #[arg(long = "clear", value_enum, value_name = "FIELD")]
    pub clear: Vec<Clearable>,
}

/// Build the `UpdateTransactionAutofillRequestDto` body.
///
/// The two containers appear only when something inside them does, and a
/// PATCH with nothing in it is refused: the schema declares no required
/// fields, so the API would accept the round trip and change nothing.
///
/// **The product's wire names are not the transaction product's.** A
/// `TransactionProductIsvDto` spells them `productCode` and `discountRate`;
/// the autofill product spells the same two `code` and `discountPercentage`.
/// Each is taken from its own schema. The autofill product carries no
/// description at all: the API has no member for one and rejects a body
/// naming it, so there is no flag to send it with.
pub fn build_update_autofill_body(args: &UpdateAutofillArgs) -> Result<Value> {
    validate_update_autofill(args)?;

    let mut body = Map::new();
    if let Some(rate) = common::patch_number(args.tax_rate, to_amount_number)? {
        body.insert(
            "level2Settings".into(),
            Value::Object(Map::from_iter([("taxRate".to_string(), rate)])),
        );
    }

    let mut product = Map::new();
    common::put_patch(&mut product, "productName", &args.product_name);
    common::put_patch(&mut product, "code", &args.code);
    common::put_patch(&mut product, "measurementUnit", &args.measurement_unit);
    for (key, value) in [
        ("unitPrice", args.unit_price),
        ("quantity", args.quantity),
        ("discountPercentage", args.discount_percentage),
    ] {
        if let Some(v) = common::patch_number(value, to_amount_number)? {
            product.insert(key.to_string(), v);
        }
    }

    let mut level3 = Map::new();
    for (key, value) in [
        ("shippingChargeRate", args.shipping_charge_rate),
        ("dutyChargeRate", args.duty_charge_rate),
    ] {
        if let Some(v) = common::patch_number(value, to_amount_number)? {
            level3.insert(key.to_string(), v);
        }
    }

    for field in &args.clear {
        let (key, target) = (
            field.wire(),
            if field.in_product() {
                &mut product
            } else {
                &mut level3
            },
        );
        if target.contains_key(key) {
            anyhow::bail!(
                "--clear {} contradicts the value given for it; pass one or \
                 the other",
                clap::ValueEnum::to_possible_value(field)
                    .expect("every variant is a possible value")
                    .get_name()
            );
        }
        target.insert(key.to_string(), Value::Null);
    }
    if !product.is_empty() {
        level3.insert("product".into(), Value::Object(product));
    }
    if !level3.is_empty() {
        body.insert("level3Settings".into(), Value::Object(level3));
    }

    if body.is_empty() {
        anyhow::bail!(
            "nothing to update: pass at least one field, e.g. --l2-tax-rate or \
             --product-name"
        );
    }
    Ok(Value::Object(body))
}

fn validate_update_autofill(args: &UpdateAutofillArgs) -> Result<()> {
    if args.tax_rate == Some(PatchNumber::Clear) {
        anyhow::bail!("--l2-tax-rate cannot be cleared; pass a value.");
    }
    if let Some(PatchNumber::Set(rate)) = args.tax_rate {
        let (lo, hi) = (
            Decimal::from(*TAX_RATE.start()),
            Decimal::from(*TAX_RATE.end()),
        );
        if rate < lo || rate > hi {
            anyhow::bail!(
                "--l2-tax-rate must be between {} and {} (got {rate})",
                TAX_RATE.start(),
                TAX_RATE.end()
            );
        }
    }
    Ok(())
}

/// The processors come first: two other commands' errors name this one as the
/// way to find a `--payment-processor-id`, and an answer that buries them
/// under the account's company name does not make that advice true.
pub static PAYMENT_CONFIG: Resource = Resource {
    object: "payment_config",
    object_list: "payment_configs",
    // A singleton at a fixed path, so there is no id, and no one pointer can
    // name what `quiet` prints: the processors a caller came here for are a
    // list, and the list is printed a line at a time.
    id: "",
    detail: &[
        "/availablePaymentProcessors/[]/paymentProcessorId",
        "/availablePaymentProcessors/[]/processorName",
        "/availablePaymentProcessors/[]/isDefault",
        "/availablePaymentProcessors/[]/type",
        "/availablePaymentProcessors/[]/settlementBatchTimeSlots/[]/hours",
        "/availablePaymentProcessors/[]/settlementBatchTimeSlots/[]/minutes",
        "/availablePaymentProcessors/[]/settlementBatchTimeSlots/[]/timezoneName",
        "/currency",
        "/availableCurrencies",
        "/availableCardTypes",
        "/availableTransactionTypes",
        "/maxTransactionAmount",
        "/isTipsEnabled",
        "/defaultTipsOptions",
        "/zeroCostProcessingOption",
        "/defaultSurchargeRate",
        "/defaultCashDiscountRate",
        "/defaultDualPricingRate",
        "/addressVerificationServiceOptions/isEnabled",
        "/addressVerificationServiceOptions/profile",
        "/isCustomerCardSavingByTerminalEnabled",
        "/companyName",
        "/mccCode",
        "/mccCodeDescription",
        "/ttpIosTerminalProfileId",
    ],
    columns: &[Column {
        header: "PROCESSOR",
        width: 36,
        cell: Cell::Path("/availablePaymentProcessors/0/paymentProcessorId"),
    }],
    amounts: &["/maxTransactionAmount"],
    yes_no: &["/availablePaymentProcessors/[]/isDefault"],
};

pub static CONTACT_INFO: Resource = Resource {
    object: "contact_info",
    object_list: "contact_infos",
    id: "/contactInfos/0/contactInfoId",
    detail: &[
        "/contactInfos/[]/contactInfoId",
        "/contactInfos/[]/addressName",
        "/contactInfos/[]/isMainAddress",
        "/contactInfos/[]/isDefaultAddress",
        "/contactInfos/[]/email",
        "/contactInfos/[]/mobilePhoneNumber",
        "/contactInfos/[]/addressLine1",
        "/contactInfos/[]/addressLine2",
        "/contactInfos/[]/city",
        "/contactInfos/[]/stateCode",
        "/contactInfos/[]/postalCode",
        "/contactInfos/[]/countryCode",
    ],
    columns: &[Column {
        header: "CONTACT",
        width: 36,
        cell: Cell::Path("/contactInfos/0/contactInfoId"),
    }],
    amounts: &[],
    yes_no: &[],
};

/// A settings document with no identifier of any kind, as `autofill` reads it
/// and `update-autofill` answers with it. The product template carries no
/// description.
pub static TRANSACTION_AUTOFILL: Resource = Resource {
    object: "transaction_autofill",
    object_list: "transaction_autofills",
    id: "/level2Settings/taxRate",
    detail: &[
        "/level2Settings/taxRate",
        "/level3Settings/shippingChargeRate",
        "/level3Settings/dutyChargeRate",
        "/level3Settings/product/productName",
        "/level3Settings/product/code",
        "/level3Settings/product/measurementUnit",
        "/level3Settings/product/unitPrice",
        "/level3Settings/product/quantity",
        "/level3Settings/product/discountPercentage",
    ],
    columns: &[Column {
        header: "TAX RATE",
        width: 10,
        cell: Cell::Path("/level2Settings/taxRate"),
    }],
    amounts: &["/level3Settings/product/unitPrice"],
    yes_no: &[],
};

pub async fn dispatch(ctx: &Ctx, command: SettingsCommand) -> Result<()> {
    match command {
        SettingsCommand::PaymentConfig => {
            let resp = ctx
                .api
                .request(Method::GET, "/v2/settings/payment-config", &[], None)
                .await?;
            let data = common::body_of(resp.body)?;
            // An account can hold several processors, and `quiet` is what a
            // caller chains into `--payment-processor-id`. One of them chosen
            // by position sends a card charge to the ACH processor, so every
            // id is printed, one per line — the rule for a collection.
            if ctx.output == OutputFormat::Quiet {
                for id in data
                    .pointer("/availablePaymentProcessors")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|p| p.get("paymentProcessorId").and_then(Value::as_str))
                {
                    println!("{id}");
                }
                return Ok(());
            }
            render::one(ctx, &PAYMENT_CONFIG, &data, resp.correlation_id)
        }
        SettingsCommand::ContactInfo => {
            let resp = ctx
                .api
                .request(Method::GET, "/v2/settings/contact-information", &[], None)
                .await?;
            render::one(
                ctx,
                &CONTACT_INFO,
                &common::body_of(resp.body)?,
                resp.correlation_id,
            )
        }
        SettingsCommand::Autofill => {
            let resp = ctx
                .api
                .request(Method::GET, "/v2/settings/transaction-autofill", &[], None)
                .await?;
            render::one(
                ctx,
                &TRANSACTION_AUTOFILL,
                &common::body_of(resp.body)?,
                resp.correlation_id,
            )
        }
        SettingsCommand::UpdateAutofill(args) => {
            money::note_fractional_rates(&[
                ("--l2-tax-rate", args.tax_rate.and_then(PatchNumber::set)),
                (
                    "--l3-shipping-rate",
                    args.shipping_charge_rate.and_then(PatchNumber::set),
                ),
                (
                    "--l3-duty-rate",
                    args.duty_charge_rate.and_then(PatchNumber::set),
                ),
                (
                    "--product-discount",
                    args.discount_percentage.and_then(PatchNumber::set),
                ),
            ]);
            let body = build_update_autofill_body(&args)?;
            let resp = ctx
                .api
                .request(
                    Method::PATCH,
                    "/v2/settings/transaction-autofill",
                    &[],
                    Some(body),
                )
                .await?;
            // The API answers with the settings it stored, though the
            // published operation declares no body. A singleton has no id to
            // confirm from, so a bodyless 200 confirms with the verb alone.
            match resp.body {
                Some(stored) => {
                    render::one(ctx, &TRANSACTION_AUTOFILL, &stored, resp.correlation_id)
                }
                None => render::confirmed(
                    ctx,
                    &TRANSACTION_AUTOFILL,
                    "",
                    "updated",
                    "Updated transaction autofill settings.",
                    resp.correlation_id,
                ),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_rate_is_a_whole_body() {
        let args = UpdateAutofillArgs {
            tax_rate: Some(PatchNumber::Set("7.25".parse().unwrap())),
            ..Default::default()
        };
        let body = build_update_autofill_body(&args).unwrap();
        assert_eq!(body.as_object().unwrap().len(), 1);
        assert_eq!(body["level2Settings"]["taxRate"].to_string(), "7.25");
    }

    #[test]
    fn every_flag_reaches_the_body_under_its_wire_name() {
        let args = UpdateAutofillArgs {
            tax_rate: Some(PatchNumber::Set("8.5".parse().unwrap())),
            shipping_charge_rate: Some(PatchNumber::Set("5.0".parse().unwrap())),
            duty_charge_rate: Some(PatchNumber::Set("2.5".parse().unwrap())),
            product_name: Some("Office Supplies".into()),
            code: Some("OFF001".into()),
            measurement_unit: Some("pcs".into()),
            unit_price: Some(PatchNumber::Set("25.00".parse().unwrap())),
            quantity: Some(PatchNumber::Set("10.0".parse().unwrap())),
            discount_percentage: Some(PatchNumber::Set("5.0".parse().unwrap())),
            clear: vec![],
        };
        let body = build_update_autofill_body(&args).unwrap();
        assert_eq!(body["level2Settings"]["taxRate"].to_string(), "8.5");
        assert_eq!(
            body["level3Settings"]["shippingChargeRate"].to_string(),
            "5.0"
        );
        assert_eq!(body["level3Settings"]["dutyChargeRate"].to_string(), "2.5");
        let product = &body["level3Settings"]["product"];
        assert_eq!(product["productName"], "Office Supplies");
        assert_eq!(product["code"], "OFF001");
        assert_eq!(product["measurementUnit"], "pcs");
        assert_eq!(product["unitPrice"].to_string(), "25.00");
        assert_eq!(product["quantity"].to_string(), "10.0");
        assert_eq!(product["discountPercentage"].to_string(), "5.0");
    }

    /// The autofill product spells two of its fields differently from the
    /// transaction product. Sending the transaction's names here would be
    /// rejected under `additionalProperties: false`.
    #[test]
    fn the_product_uses_the_autofill_schemas_own_wire_names() {
        let args = UpdateAutofillArgs {
            code: Some("OFF001".into()),
            discount_percentage: Some(PatchNumber::Set("5.0".parse().unwrap())),
            ..Default::default()
        };
        let product = build_update_autofill_body(&args).unwrap()["level3Settings"]["product"]
            .as_object()
            .cloned()
            .unwrap();
        assert!(product.contains_key("code"));
        assert!(product.contains_key("discountPercentage"));
        for transaction_spelling in ["productCode", "discountRate"] {
            assert!(
                !product.contains_key(transaction_spelling),
                "{transaction_spelling} is the transaction product's name, not this one's"
            );
        }
    }

    /// A container appears only when something inside it does.
    #[test]
    fn a_container_with_nothing_in_it_is_absent() {
        let args = UpdateAutofillArgs {
            tax_rate: Some(PatchNumber::Set("8.5".parse().unwrap())),
            ..Default::default()
        };
        let body = build_update_autofill_body(&args).unwrap();
        assert!(body.get("level3Settings").is_none(), "{body}");

        let args = UpdateAutofillArgs {
            code: Some("OFF001".into()),
            ..Default::default()
        };
        let body = build_update_autofill_body(&args).unwrap();
        assert!(body.get("level2Settings").is_none(), "{body}");
        assert!(
            body["level3Settings"].get("shippingChargeRate").is_none(),
            "{body}"
        );
    }

    /// An empty PATCH is a round trip that cannot change anything, and the
    /// schema declares nothing required, so the API would take it.
    #[test]
    fn an_update_with_no_fields_is_refused() {
        let err = build_update_autofill_body(&UpdateAutofillArgs::default())
            .unwrap_err()
            .to_string();
        assert!(err.contains("nothing to update"), "{err}");
    }

    /// The declared bound, which is 0 to 22 — not `transactions create`'s
    /// 0.01 to 100 for the same idea.
    #[test]
    fn the_tax_rate_bound_is_the_one_this_schema_declares() {
        let rate = |s: &str| UpdateAutofillArgs {
            tax_rate: Some(PatchNumber::Set(s.parse().unwrap())),
            ..Default::default()
        };
        assert!(build_update_autofill_body(&rate("0")).is_ok());
        assert!(build_update_autofill_body(&rate("22")).is_ok());
        assert!(build_update_autofill_body(&rate("22.01")).is_err());
        // Legal here and refused on a transaction, whose own minimum is 0.01.
        // The two bounds are genuinely different, which is why neither is
        // written once and shared.
        assert!(build_update_autofill_body(&rate("0.005")).is_ok());
    }

    /// `taxRate` maps to a non-nullable column, so the API answers
    /// `cannot be cleared`. Refusing here spends no round trip on it, matching
    /// the `--clear` enum, which does not offer the field either.
    #[test]
    fn the_tax_rate_is_refused_an_empty_value() {
        let args = UpdateAutofillArgs {
            tax_rate: Some(PatchNumber::Clear),
            ..Default::default()
        };
        let err = build_update_autofill_body(&args).unwrap_err().to_string();
        assert!(err.contains("cannot be cleared"), "{err}");
    }

    /// The refusal names the flag the caller typed, not the wire key it sets.
    #[test]
    fn a_contradicting_clear_is_named_by_its_flag() {
        let args = UpdateAutofillArgs {
            product_name: Some("Widget".into()),
            clear: vec![Clearable::ProductName],
            ..Default::default()
        };
        let err = build_update_autofill_body(&args).unwrap_err().to_string();
        assert!(err.contains("--clear product-name "), "{err}");
        assert!(!err.contains("productName"), "{err}");
    }

    /// An empty value clears the stored default: the body carries an explicit
    /// null, which is what removes it. Writing an empty code instead would
    /// leave a blank default still applying to every later transaction.
    #[test]
    fn an_empty_product_field_clears_the_stored_default() {
        let args = UpdateAutofillArgs {
            code: Some(String::new()),
            ..Default::default()
        };
        let body = build_update_autofill_body(&args).unwrap();
        assert_eq!(body["level3Settings"]["product"]["code"], Value::Null);
    }
}
