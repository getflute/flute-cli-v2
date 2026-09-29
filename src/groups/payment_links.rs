//! `payment-links`: create, get, list, update, delete, share.
//!
//! Two things here are unlike the rest of the API. `delete` and `share` answer
//! **204** where every other write answers 200, so there is no body to parse
//! on either. And `update` is documented as an RFC 7396 merge patch, where an
//! explicit `null` clears a clearable field — a capability an omitted key
//! cannot express, and the reason an empty value is spelt the way it is.

use crate::Ctx;
use crate::api::ApiPath;
use crate::cli::common::{self, PaginationArgs};
use crate::cli::money::{PatchNumber, parse_amount, parse_amount_patch, to_amount_number};
use crate::cli::render::{self, Cell, Column, Resource};
use anyhow::Result;
use reqwest::Method;
use rust_decimal::Decimal;
use serde_json::{Map, Value};

/// `MultiUse` or `SingleUse`, capitalised on the wire.
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum, serde::Serialize)]
pub enum LinkType {
    MultiUse,
    SingleUse,
}

/// The four declared payment link statuses.
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum, serde::Serialize)]
pub enum PaymentLinkStatus {
    Active,
    Completed,
    Expired,
    Inactive,
}

/// `SharePaymentLinkRequestDto.shareBy` declares exactly these two.
///
/// Deliberately **not** `groups::transactions::ShareBy`, which carries `Sms`
/// alone because that endpoint accepts no other channel. Sharing that enum
/// would drop a channel this operation accepts.
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum, serde::Serialize)]
pub enum ShareChannel {
    Email,
    Sms,
}

/// A field `--clear` can null out, and the wire key it nulls.
///
/// `currencyCode` and `name` are absent on purpose: the schema says both
/// cannot be cleared, so a null would be a round trip spent on a rejection.
/// They are named in the refusal instead.
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Clearable {
    Amount,
    CustomerId,
    ReferenceId,
    Description,
    ExpiresOn,
}

impl Clearable {
    fn wire(self) -> &'static str {
        match self {
            Self::Amount => "baseAmount",
            Self::CustomerId => "customerId",
            Self::ReferenceId => "referenceId",
            Self::Description => "description",
            Self::ExpiresOn => "expiresOn",
        }
    }
}

/// Parse an expiry as the API takes one: a UTC date-time in ISO 8601 with a
/// `Z` suffix, e.g. `2026-09-15T00:00:00Z`, with optional fractional seconds.
///
/// The API answers anything else with a 400: a word or a bare date with a
/// .NET conversion error that names no flag, and a date-time without the `Z`
/// or with an offset as not a UTC date-time. Whether it is in the future is
/// left to the server, whose clock decides it. An empty value passes through,
/// because on `update` it is the removal.
pub(crate) fn parse_utc_expiry(raw: &str) -> Result<String> {
    if raw.is_empty() || is_utc_date_time(raw) {
        return Ok(raw.to_string());
    }
    anyhow::bail!("expected a UTC date-time in ISO 8601 ending in Z, e.g. 2026-09-15T00:00:00Z")
}

fn is_utc_date_time(raw: &str) -> bool {
    let Some(rest) = raw.strip_suffix('Z') else {
        return false;
    };
    let (main, fraction) = rest.split_once('.').unwrap_or((rest, "0"));
    const SHAPE: &[u8] = b"dddd-dd-ddTdd:dd:dd";
    let shaped = main.len() == SHAPE.len()
        && main.bytes().zip(SHAPE).all(|(c, &want)| match want {
            b'd' => c.is_ascii_digit(),
            _ => c == want,
        });
    if !shaped || fraction.is_empty() || !fraction.bytes().all(|c| c.is_ascii_digit()) {
        return false;
    }
    // Every slice is ASCII digits, checked above.
    let field = |r: std::ops::Range<usize>| main[r].parse::<u32>().unwrap_or(u32::MAX);
    let (year, month, day) = (field(0..4), field(5..7), field(8..10));
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&day) && field(11..13) < 24 && field(14..16) < 60 && field(17..19) < 60
}

#[allow(clippy::large_enum_variant)]
#[derive(clap::Subcommand, Debug)]
pub enum PaymentLinksCommand {
    /// Create a new payment link (POST /v2/payment-links).
    Create(CreatePaymentLinkArgs),
    /// Fetch a single payment link by ID
    /// (GET /v2/payment-links/{paymentLinkId}).
    Get {
        /// Payment link UUID to retrieve (positional).
        payment_link_id: String,
    },
    /// List payment links (GET /v2/payment-links).
    List(ListPaymentLinksArgs),
    /// Update a payment link (PATCH /v2/payment-links/{paymentLinkId}).
    ///
    /// Omitted flags retain their existing server values; an empty value
    /// removes one outright.
    Update(UpdatePaymentLinkArgs),
    /// Delete a payment link (DELETE /v2/payment-links/{paymentLinkId}).
    ///
    /// Requires `--yes` to prevent accidental deletions.
    Delete {
        /// Payment link UUID to delete (positional).
        payment_link_id: String,
        /// Confirm the deletion (required).
        #[arg(long)]
        yes: bool,
    },
    /// Send a payment link to a customer by email or text message
    /// (POST /v2/payment-links/{paymentLinkId}/share).
    Share(SharePaymentLinkArgs),
}

#[derive(clap::Args, Debug, Default)]
pub struct CreatePaymentLinkArgs {
    /// Accept card payments. At least one payment method is required.
    #[arg(long = "card-enabled")]
    pub card_enabled: bool,
    /// Charge card payments through this processor.
    #[arg(long = "card-processor-id")]
    pub card_processor_id: Option<String>,
    /// Accept ACH payments.
    #[arg(long = "ach-enabled")]
    pub ach_enabled: bool,
    /// Charge ACH payments through this processor.
    #[arg(long = "ach-processor-id")]
    pub ach_processor_id: Option<String>,
    /// Amount to collect. Plain decimal, e.g. `100.00`. Omit for a link the
    /// payer fills in.
    #[arg(long = "amount", value_parser = parse_amount, value_name = "AMOUNT", allow_negative_numbers = true)]
    pub base_amount: Option<Decimal>,
    // Optional to clap rather than `required`, so that the missing-method
    // refusal is reported first and this one can name the flag itself.
    /// Currency code, e.g. `USD` (required).
    #[arg(long)]
    pub currency_code: Option<String>,
    /// Link type: `single-use` (default) or `multi-use`.
    #[arg(long, value_enum)]
    pub link_type: Option<LinkType>,
    /// Customer UUID to issue the link for.
    #[arg(long)]
    pub customer_id: Option<String>,
    /// Merchant-assigned reference ID.
    #[arg(long)]
    pub reference_id: Option<String>,
    /// Merchant-facing label for the link.
    #[arg(long)]
    pub name: Option<String>,
    /// Merchant-internal notes. Never shown to customers.
    #[arg(long)]
    pub description: Option<String>,
    /// UTC expiry (ISO 8601, ending in `Z`), e.g. `2026-09-15T00:00:00Z`.
    /// Omit for a link that never expires.
    #[arg(long, value_parser = parse_utc_expiry)]
    pub expires_on: Option<String>,
}

#[derive(clap::Args, Debug, Default)]
pub struct UpdatePaymentLinkArgs {
    /// Payment link UUID to update (positional).
    pub payment_link_id: String,
    // Valued rather than a bare switch: a PATCH has to be able to turn a
    // method off, and a switch cannot express `false`.
    /// Accept card payments: `--card-enabled` or `--card-enabled false`.
    #[arg(long = "card-enabled", id = "update_card_enabled", value_name = "CARD_ENABLED",
          num_args = 0..=1, default_missing_value = "true")]
    pub card_enabled: Option<bool>,
    /// Charge card payments through this processor.
    #[arg(
        long = "card-processor-id",
        id = "update_card_processor_id",
        value_name = "CARD_PROCESSOR_ID"
    )]
    pub card_processor_id: Option<String>,
    /// Accept ACH payments: `--ach-enabled` or `--ach-enabled false`.
    #[arg(long = "ach-enabled", id = "update_ach_enabled", value_name = "ACH_ENABLED",
          num_args = 0..=1, default_missing_value = "true")]
    pub ach_enabled: Option<bool>,
    /// Charge ACH payments through this processor.
    #[arg(
        long = "ach-processor-id",
        id = "update_ach_processor_id",
        value_name = "ACH_PROCESSOR_ID"
    )]
    pub ach_processor_id: Option<String>,
    /// New amount to collect. Plain decimal, e.g. `100.00`.
    #[arg(long = "amount", value_parser = parse_amount_patch,
          id = "update_link_amount", value_name = "AMOUNT",
          allow_negative_numbers = true)]
    pub base_amount: Option<PatchNumber>,
    /// New currency code. Frozen once the link has taken a payment.
    #[arg(long, id = "update_link_currency_code", value_name = "CURRENCY_CODE")]
    pub currency_code: Option<String>,
    /// New link type: `single-use` or `multi-use`.
    #[arg(long, value_enum, id = "update_link_type", value_name = "LINK_TYPE")]
    pub link_type: Option<LinkType>,
    /// Set the link's status, e.g. `inactive` to stop accepting payments.
    #[arg(
        long = "status",
        value_enum,
        id = "update_link_status",
        value_name = "PAYMENT_LINK_STATUS"
    )]
    pub payment_link_status: Option<PaymentLinkStatus>,
    /// Customer UUID to attach the link to.
    #[arg(long, id = "update_link_customer_id", value_name = "CUSTOMER_ID")]
    pub customer_id: Option<String>,
    /// New merchant-assigned reference ID.
    #[arg(long, id = "update_link_reference_id", value_name = "REFERENCE_ID")]
    pub reference_id: Option<String>,
    /// New merchant-facing label.
    #[arg(long, id = "update_link_name", value_name = "NAME")]
    pub name: Option<String>,
    /// New merchant-internal notes.
    #[arg(long, id = "update_link_description", value_name = "DESCRIPTION")]
    pub description: Option<String>,
    /// New UTC expiry (ISO 8601, ending in `Z`), e.g.
    /// `2026-09-15T00:00:00Z`. Must be in the future.
    #[arg(long, id = "update_link_expires_on", value_name = "EXPIRES_ON",
          value_parser = parse_utc_expiry)]
    pub expires_on: Option<String>,
    /// Clear a field back to nothing. Repeat the flag for several fields.
    ///
    /// An empty value does the same: `--description ""` and
    /// `--clear description` are one request. `name`, `currency-code`,
    /// `card-processor-id` and `ach-processor-id` cannot be cleared and are
    /// not offered.
    #[arg(long = "clear", value_enum, value_name = "FIELD")]
    pub clear: Vec<Clearable>,
}

#[derive(clap::Args, Debug, Default)]
pub struct ListPaymentLinksArgs {
    #[command(flatten)]
    pub pagination: PaginationArgs,
    /// Server-side text search.
    #[arg(long, id = "link_search", value_name = "SEARCH")]
    pub search: Option<String>,
    /// Filter by link type.
    #[arg(long, value_enum, id = "list_link_type", value_name = "LINK_TYPE")]
    pub link_type: Option<LinkType>,
    /// Filter results by status.
    #[arg(
        long = "status",
        value_enum,
        id = "list_link_status",
        value_name = "PAYMENT_LINK_STATUS"
    )]
    pub payment_link_status: Option<PaymentLinkStatus>,
    /// Sort results by this field name.
    #[arg(long, id = "link_sort_by", value_name = "SORT_BY")]
    pub sort_by: Option<String>,
    /// Sort ascending. With neither `--asc` nor `--desc`, results come back
    /// newest first.
    #[arg(long, id = "link_asc", conflicts_with = "link_desc")]
    pub asc: bool,
    /// Sort descending.
    #[arg(long, id = "link_desc")]
    pub desc: bool,
}

#[derive(clap::Args, Debug)]
pub struct SharePaymentLinkArgs {
    /// Payment link UUID to share (positional).
    pub payment_link_id: String,
    /// How to send the link: `email` or `sms` (required).
    #[arg(long, value_enum, id = "link_share_by", value_name = "SHARE_BY")]
    pub share_by: ShareChannel,
    /// Email address or mobile number in E.164 form to send it to (required).
    #[arg(long, id = "link_recipient", value_name = "RECIPIENT")]
    pub recipient: String,
    /// The customer has consented to receive it (required).
    #[arg(long = "consent", id = "link_consent")]
    pub has_customer_consent: bool,
}

/// The `paymentMethods` object, which is the one required field.
///
/// A method named with no configuration under it is how the schema documents
/// offering that method with the account's defaults, so `enabled` is what a
/// bare switch sets and a processor id is an extra rather than a
/// precondition.
fn payment_methods_for_create(args: &CreatePaymentLinkArgs) -> Result<Value> {
    let methods = common::payment_methods(
        (
            common::offered(args.card_enabled, &args.card_processor_id),
            &args.card_processor_id,
        ),
        (
            common::offered(args.ach_enabled, &args.ach_processor_id),
            &args.ach_processor_id,
        ),
    )?;
    if methods.is_empty() {
        anyhow::bail!(
            "a payment link has to accept something: pass --card-enabled, \
             --ach-enabled, or a processor id for either"
        );
    }
    Ok(Value::Object(methods))
}

/// Build the `CreatePaymentLinkRequestDto` body.
///
/// **`currencyCode` is required, though the schema names only
/// `paymentMethods`.** A create without it is answered
/// `currencyCode: currencyCode must be a valid ISO 4217 currency code` — for a
/// field that was never sent — and unconditionally, whether or not an amount
/// is present. So it is required here, where the message can name the flag,
/// rather than paid for in a round trip that reports a validation failure on
/// an absent key.
pub fn build_create_payment_link_body(args: &CreatePaymentLinkArgs) -> Result<Value> {
    let mut body = Map::new();
    // The schema-required field is reported first, so a caller who supplied
    // neither hears about the documented one before the undocumented one.
    body.insert("paymentMethods".into(), payment_methods_for_create(args)?);
    if args
        .currency_code
        .as_ref()
        .is_none_or(|s| s.trim().is_empty())
    {
        anyhow::bail!(
            "--currency-code is required, e.g. `--currency-code USD`. The schema \
             marks it optional and the API does not."
        );
    }

    if let Some(v) = args.base_amount {
        if v <= Decimal::ZERO {
            anyhow::bail!(
                "--amount must be greater than zero. Omit it entirely for a link \
                 the payer fills in."
            );
        }
        body.insert("baseAmount".into(), to_amount_number(v)?);
    }
    if let Some(v) = args.link_type {
        body.insert("linkType".into(), serde_json::json!(v));
    }
    common::put_str(&mut body, "currencyCode", &args.currency_code);
    common::put_str(&mut body, "customerId", &args.customer_id);
    common::put_str(&mut body, "referenceId", &args.reference_id);
    common::put_str(&mut body, "name", &args.name);
    common::put_str(&mut body, "description", &args.description);
    common::put_str(&mut body, "expiresOn", &args.expires_on);
    Ok(Value::Object(body))
}

#[cfg(test)]
mod clearing_tests;

/// Build the `UpdatePaymentLinkRequestDto` merge-patch body.
///
/// A PATCH with nothing in it is refused: the schema declares no required
/// fields, so the API would accept the round trip and change nothing.
pub fn build_update_payment_link_body(args: &UpdatePaymentLinkArgs) -> Result<Value> {
    for (flag, value) in [("name", &args.name), ("currency-code", &args.currency_code)] {
        common::reject_unclearable(flag, value.as_deref())?;
    }
    let mut body = Map::new();

    let methods = common::payment_methods(
        (args.card_enabled, &args.card_processor_id),
        (args.ach_enabled, &args.ach_processor_id),
    )?;
    if !methods.is_empty() {
        body.insert("paymentMethods".into(), Value::Object(methods));
    }

    if args
        .base_amount
        .and_then(PatchNumber::set)
        .is_some_and(|v| v <= Decimal::ZERO)
    {
        anyhow::bail!(
            "--amount must be greater than zero. Clear it with `--clear amount` \
             for a link the payer fills in."
        );
    }
    if let Some(v) = common::patch_number(args.base_amount, to_amount_number)? {
        body.insert("baseAmount".into(), v);
    }
    if let Some(v) = args.link_type {
        body.insert("linkType".into(), serde_json::json!(v));
    }
    if let Some(v) = args.payment_link_status {
        body.insert("paymentLinkStatus".into(), serde_json::json!(v));
    }
    common::put_patch(&mut body, "currencyCode", &args.currency_code);
    common::put_patch(&mut body, "customerId", &args.customer_id);
    common::put_patch(&mut body, "referenceId", &args.reference_id);
    common::put_patch(&mut body, "name", &args.name);
    common::put_patch(&mut body, "description", &args.description);
    common::put_patch(&mut body, "expiresOn", &args.expires_on);

    for field in &args.clear {
        let key = field.wire();
        if body.contains_key(key) {
            anyhow::bail!(
                "--clear {} contradicts the value given for it; pass one or \
                 the other",
                clap::ValueEnum::to_possible_value(field)
                    .expect("every variant is a possible value")
                    .get_name()
            );
        }
        body.insert(key.to_string(), Value::Null);
    }

    if body.is_empty() {
        anyhow::bail!("nothing to update: pass at least one field, e.g. --name or --status");
    }
    Ok(Value::Object(body))
}

/// The `SharePaymentLinkRequestDto` body: all three fields are required, so
/// `hasCustomerConsent` is always present and carries whatever the switch
/// said. Hiding a `false` would leave the API unable to refuse an
/// unconsented share.
pub fn build_share_payment_link_body(args: &SharePaymentLinkArgs) -> Result<Value> {
    if args.recipient.trim().is_empty() {
        anyhow::bail!("--recipient is required: a share has to go somewhere");
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

/// The `GET /v2/payment-links` query, omitting every absent flag.
pub fn build_list_payment_links_query(
    args: &ListPaymentLinksArgs,
) -> Result<Vec<(&'static str, String)>> {
    args.pagination.validate()?;
    let mut query = args.pagination.query();
    query.extend(common::sort_order(args.asc, args.desc));
    if let Some(v) = args.link_type {
        query.push(("linkType", common::wire(v)));
    }
    if let Some(v) = args.payment_link_status {
        query.push(("paymentLinkStatus", common::wire(v)));
    }
    common::push_str(&mut query, "sortBy", &args.sort_by);
    common::push_str(&mut query, "search", &args.search);
    Ok(query)
}

/// What a payment link is worth saying: what it is, what it accepts, then
/// what it has collected.
pub static PAYMENT_LINK: Resource = Resource {
    object: "payment_link",
    object_list: "payment_link_list",
    id: "/paymentLinkId",
    detail: &[
        "/paymentLinkId",
        "/shortUrl",
        "/paymentLinkStatus",
        "/linkType",
        "/name",
        "/description",
        "/baseAmount",
        "/currencyCode",
        "/paymentMethods/card/enabled",
        "/paymentMethods/card/processorId",
        "/paymentMethods/ach/enabled",
        "/paymentMethods/ach/processorId",
        "/customerId",
        "/customerFirstName",
        "/customerLastName",
        "/referenceId",
        "/paymentCount",
        "/totalCollectedAmount",
        "/lastPaymentOn",
        "/expiresOn",
        "/createdOn",
        "/modifiedOn",
    ],
    // The short URL is the thing a caller came for — a link they cannot copy
    // is not a payment link — so it earns a column of its own.
    columns: &[
        Column {
            header: "ID",
            width: 36,
            cell: Cell::Path("/paymentLinkId"),
        },
        Column {
            header: "NAME",
            width: 20,
            cell: Cell::Path("/name"),
        },
        Column {
            header: "URL",
            width: 34,
            cell: Cell::Path("/shortUrl"),
        },
        Column {
            header: "AMOUNT",
            width: 10,
            cell: Cell::Path("/baseAmount"),
        },
        Column {
            header: "STATUS",
            width: 10,
            cell: Cell::Path("/paymentLinkStatus"),
        },
        Column {
            header: "PAID",
            width: 6,
            cell: Cell::Path("/paymentCount"),
        },
        Column {
            header: "COLLECTED",
            width: 12,
            cell: Cell::Path("/totalCollectedAmount"),
        },
    ],
    amounts: &["/baseAmount", "/totalCollectedAmount"],
    yes_no: &[],
};

pub async fn dispatch(ctx: &Ctx, command: PaymentLinksCommand) -> Result<()> {
    match command {
        PaymentLinksCommand::Create(args) => {
            let body = build_create_payment_link_body(&args)?;
            let resp = ctx
                .api
                .request(Method::POST, "/v2/payment-links", &[], Some(body))
                .await?;
            render::one(
                ctx,
                &PAYMENT_LINK,
                &common::body_of(resp.body)?,
                resp.correlation_id,
            )
        }
        PaymentLinksCommand::Get { payment_link_id } => {
            let resp = ctx
                .api
                .request(
                    Method::GET,
                    ApiPath::from("/v2/payment-links").id(&payment_link_id)?,
                    &[],
                    None,
                )
                .await?;
            render::one(
                ctx,
                &PAYMENT_LINK,
                &common::body_of(resp.body)?,
                resp.correlation_id,
            )
        }
        PaymentLinksCommand::List(args) => {
            let query = build_list_payment_links_query(&args)?;
            common::list(
                ctx,
                &PAYMENT_LINK,
                "/v2/payment-links",
                &query,
                &args.pagination,
            )
            .await
        }
        PaymentLinksCommand::Update(args) => {
            let body = build_update_payment_link_body(&args)?;
            let resp = ctx
                .api
                .request(
                    Method::PATCH,
                    ApiPath::from("/v2/payment-links").id(&args.payment_link_id)?,
                    &[],
                    Some(body),
                )
                .await?;
            render::one(
                ctx,
                &PAYMENT_LINK,
                &common::body_of(resp.body)?,
                resp.correlation_id,
            )
        }
        PaymentLinksCommand::Delete {
            payment_link_id, ..
        } => {
            // 204 on success. A 404 is exit 0 but not a deletion: the server
            // answers the same for "already deleted" and "never existed".
            common::delete(
                ctx,
                &PAYMENT_LINK,
                ApiPath::from("/v2/payment-links").id(&payment_link_id)?,
                &payment_link_id,
                "deleted",
                &format!("Deleted payment link {payment_link_id}."),
                &format!("No payment link {payment_link_id} was found; nothing was deleted."),
            )
            .await
        }
        PaymentLinksCommand::Share(args) => {
            let body = build_share_payment_link_body(&args)?;
            let resp = ctx
                .api
                .request(
                    Method::POST,
                    ApiPath::from("/v2/payment-links")
                        .id(&args.payment_link_id)?
                        .seg("share"),
                    &[],
                    Some(body),
                )
                .await?;
            // 204 with no body, so the confirmation comes from the request.
            render::confirmed(
                ctx,
                &PAYMENT_LINK,
                &args.payment_link_id,
                "shared",
                &format!("Shared payment link {}.", args.payment_link_id),
                resp.correlation_id,
            )
        }
    }
}

#[cfg(test)]
mod tests;
