//! `payment-methods`: list, get, add-card, add-ach, update, delete, set-default.
//!
//! This is a top-level group because a payment method is a first-class
//! resource: it has its own list endpoint, its own filters, and it can exist
//! with no customer at all.

use crate::Ctx;
use crate::api::ApiPath;
use crate::cli::common::{self, AccountHolderType, AccountType, PaginationArgs, parse_exp};
use crate::cli::output::OutputFormat;
use crate::cli::render::{self, Cell, Column, Resource};
use anyhow::Result;
use reqwest::Method;
use serde_json::{Map, Value};

/// The one field `--clear` can null out: the label, `paymentName` on the wire.
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Clearable {
    Name,
}

impl Clearable {
    /// The value as `--clear` spells it on the command line.
    fn flag(self) -> String {
        clap::ValueEnum::to_possible_value(&self)
            .map(|v| v.get_name().to_string())
            .unwrap_or_default()
    }
}

/// One variant per command, and the arg-bearing ones are large.
///
/// `large_enum_variant` is allowed rather than fixed: boxing a variant breaks
/// `#[derive(Subcommand)]`, which needs the args struct inline, and the enum
/// is constructed exactly once per process.
#[allow(clippy::large_enum_variant)]
#[derive(clap::Subcommand, Debug)]
pub enum PaymentMethodsCommand {
    /// List vaulted payment methods (GET /v2/payment-methods).
    List(ListPaymentMethodsArgs),
    /// Fetch a single payment method by ID
    /// (GET /v2/payment-methods/{paymentMethodId}).
    Get {
        /// Payment method UUID to retrieve (positional).
        payment_method_id: String,
    },
    /// Vault a card (POST /v2/payment-methods/cards).
    AddCard(AddCardArgs),
    /// Vault an ACH account (POST /v2/payment-methods/ach).
    AddAch(AddAchArgs),
    /// Rename a vaulted payment method
    /// (PATCH /v2/payment-methods/{paymentMethodId}).
    Update(UpdatePaymentMethodArgs),
    /// Delete a payment method from the vault
    /// (DELETE /v2/payment-methods/{paymentMethodId}).
    ///
    /// Requires `--yes` to prevent accidental deletions.
    Delete {
        /// Payment method UUID to delete (positional).
        payment_method_id: String,
        /// Confirm the deletion (required).
        #[arg(long)]
        yes: bool,
    },
    /// Make a payment method the customer's default
    /// (POST /v2/payment-methods/{paymentMethodId}/set-default).
    SetDefault {
        /// Payment method UUID (positional).
        payment_method_id: String,
        /// Customer UUID whose default this becomes (required).
        #[arg(
            long = "customer-id",
            id = "set_default_customer_id",
            value_name = "CUSTOMER_ID"
        )]
        customer_id: String,
    },
}

#[derive(clap::Args, Debug, Default)]
pub struct ListPaymentMethodsArgs {
    #[command(flatten)]
    pub pagination: PaginationArgs,
    /// Sort results by this field name.
    #[arg(long, id = "pm_sort_by", value_name = "SORT_BY")]
    pub sort_by: Option<String>,
    /// Sort descending.
    #[arg(long, id = "pm_desc")]
    pub desc: bool,
    /// Match payment methods whose label (the `--name` they were given)
    /// contains this text, case-sensitively.
    #[arg(long)]
    pub search: Option<String>,
    /// Filter by customer UUID.
    #[arg(long, id = "pm_list_customer_id", value_name = "CUSTOMER_ID")]
    pub customer_id: Option<String>,
    /// Filter results created from this date-time inclusive (ISO 8601).
    #[arg(long, id = "pm_created_from", value_name = "CREATED_FROM")]
    pub created_from: Option<String>,
    /// Filter results created up to this date-time inclusive (ISO 8601).
    #[arg(long, id = "pm_created_to", value_name = "CREATED_TO")]
    pub created_to: Option<String>,
}

#[derive(clap::Args, Debug, Default)]
pub struct AddCardArgs {
    /// Card PAN (primary account number), e.g. `4111111111111111` (required).
    #[arg(long, id = "pm_card", value_name = "CARD")]
    pub card: String,
    /// Card expiry in MM/YY or MM/YYYY format, e.g. `12/26` (required).
    #[arg(long, id = "pm_exp", value_name = "EXP")]
    pub exp: String,
    /// Card CVV/security code.
    #[arg(long, id = "pm_cvv", value_name = "CVV")]
    pub cvv: Option<String>,
    /// Customer UUID to vault the card against.
    #[arg(long, id = "pm_card_customer_id", value_name = "CUSTOMER_ID")]
    pub customer_id: Option<String>,
    /// Friendly label for this payment method.
    #[arg(long = "name", id = "pm_card_name", value_name = "PAYMENT_NAME")]
    pub payment_name: Option<String>,
}

#[derive(clap::Args, Debug)]
pub struct AddAchArgs {
    /// Bank account number (required).
    #[arg(long = "account", id = "pm_account", value_name = "ACCOUNT_NUMBER")]
    pub account_number: String,
    /// ABA routing number (required).
    #[arg(long = "routing", id = "pm_routing", value_name = "ROUTING_NUMBER")]
    pub routing_number: String,
    /// Account type: `checking` (default) or `savings`.
    #[arg(long, value_enum, id = "pm_account_type", value_name = "ACCOUNT_TYPE",
          default_value_t = AccountType::Checking)]
    pub account_type: AccountType,
    /// Account holder type: `business` or `personal` (required).
    #[arg(
        long,
        value_enum,
        id = "pm_account_holder_type",
        value_name = "ACCOUNT_HOLDER_TYPE"
    )]
    pub account_holder_type: AccountHolderType,
    /// Tax ID (optional).
    #[arg(long, id = "pm_tax_id", value_name = "TAX_ID")]
    pub tax_id: Option<String>,
    /// Customer UUID to vault the account against.
    #[arg(long, id = "pm_ach_customer_id", value_name = "CUSTOMER_ID")]
    pub customer_id: Option<String>,
    /// Friendly label for this payment method.
    #[arg(long = "name", id = "pm_ach_name", value_name = "NAME")]
    pub name: Option<String>,
    /// Company name, for a business account. With `--customer-id` the
    /// customer's company name is used instead and this value is not stored,
    /// so a business account needs a company name on the customer.
    #[arg(long, id = "pm_company_name", value_name = "COMPANY_NAME")]
    pub company_name: Option<String>,
}

#[derive(clap::Args, Debug, Default)]
pub struct UpdatePaymentMethodArgs {
    /// Payment method UUID to update (positional).
    pub payment_method_id: String,
    /// New friendly label. The only field this endpoint accepts.
    #[arg(long = "name", id = "pm_update_name", value_name = "PAYMENT_NAME")]
    pub payment_name: Option<String>,
    /// Clear the label back to nothing.
    ///
    /// An empty value does the same: `--name ""` and `--clear name` are one
    /// request.
    #[arg(long = "clear", value_enum, value_name = "FIELD")]
    pub clear: Vec<Clearable>,
}

/// The `GET /v2/payment-methods` query, omitting every absent flag.
pub fn build_list_payment_methods_query(
    args: &ListPaymentMethodsArgs,
) -> Result<Vec<(&'static str, String)>> {
    args.pagination.validate()?;
    let mut query = args.pagination.query();
    if args.desc {
        query.push(("asc", "false".into()));
    }
    common::push_str(&mut query, "sortBy", &args.sort_by);
    common::push_str(&mut query, "search", &args.search);
    common::push_id(&mut query, "--customer-id", "customerId", &args.customer_id)?;
    common::push_str(&mut query, "createdFrom", &args.created_from);
    common::push_str(&mut query, "createdTo", &args.created_to);
    Ok(query)
}

/// Build the `CreateCardPaymentMethodRequestDto` body.
///
/// The label is `paymentName` here and plain `name` on the ACH route. One
/// `--name` flag covers both, so an inconsistency in the API does not become
/// an inconsistency in the CLI.
pub fn build_add_card_body(args: &AddCardArgs) -> Result<Value> {
    if args.card.trim().is_empty() {
        anyhow::bail!("--card is required");
    }
    let (month, year) = parse_exp(&args.exp)?;
    let mut body = Map::new();
    body.insert("cardNumber".into(), Value::String(args.card.clone()));
    body.insert("expirationMonth".into(), Value::from(month));
    body.insert("expirationYear".into(), Value::from(year));
    common::put_str(&mut body, "securityCode", &args.cvv);
    common::put_str(&mut body, "customerId", &args.customer_id);
    common::put_str(&mut body, "paymentName", &args.payment_name);
    Ok(Value::Object(body))
}

/// Build the `CreateAchPaymentMethodRequestDto` body.
///
/// Four fields are required by schema, and the two enums are capitalised on
/// the wire.
pub fn build_add_ach_body(args: &AddAchArgs) -> Result<Value> {
    for (flag, value) in [
        ("--account", &args.account_number),
        ("--routing", &args.routing_number),
    ] {
        if value.trim().is_empty() {
            anyhow::bail!("{flag} is required");
        }
    }
    let mut body = Map::new();
    body.insert(
        "routingNumber".into(),
        Value::String(args.routing_number.clone()),
    );
    body.insert(
        "accountNumber".into(),
        Value::String(args.account_number.clone()),
    );
    body.insert(
        "accountHolderType".into(),
        serde_json::json!(args.account_holder_type),
    );
    body.insert("accountType".into(), serde_json::json!(args.account_type));
    common::put_str(&mut body, "taxId", &args.tax_id);
    common::put_str(&mut body, "customerId", &args.customer_id);
    common::put_str(&mut body, "name", &args.name);
    common::put_str(&mut body, "companyName", &args.company_name);
    Ok(Value::Object(body))
}

#[cfg(test)]
mod clearing_tests {
    use super::*;

    /// The label is the only field this endpoint takes, and it is nullable —
    /// an empty value removes it rather than renaming the method to nothing.
    #[test]
    fn an_empty_name_clears_the_label() {
        let args = UpdatePaymentMethodArgs {
            payment_method_id: "pm_1".into(),
            payment_name: Some(String::new()),
            clear: vec![],
        };
        let body = build_update_payment_method_body(&args).unwrap();
        assert_eq!(body["paymentName"], Value::Null);
    }

    /// The refusal names the field as `--clear` spells it, not `paymentName`.
    #[test]
    fn a_contradicting_clear_is_named_as_the_caller_spelled_it() {
        let args = UpdatePaymentMethodArgs {
            payment_method_id: "pm_1".into(),
            payment_name: Some("x".into()),
            clear: vec![Clearable::Name],
        };
        let err = build_update_payment_method_body(&args)
            .unwrap_err()
            .to_string();
        assert!(err.starts_with("--clear name "), "{err}");
    }
}

/// Build the `UpdatePaymentMethodRequestDto` body — one property, so an
/// absent `--name` leaves nothing to send.
pub fn build_update_payment_method_body(args: &UpdatePaymentMethodArgs) -> Result<Value> {
    let name = match (common::patch_string(&args.payment_name), args.clear.first()) {
        (Some(_), Some(field)) => {
            anyhow::bail!(
                "--clear {} contradicts the value given for it; pass one or the other",
                field.flag()
            )
        }
        (Some(v), None) => v,
        (None, Some(_)) => Value::Null,
        (None, None) => anyhow::bail!("nothing to update: pass --name"),
    };
    Ok(Value::Object(Map::from_iter([(
        "paymentName".to_string(),
        name,
    )])))
}

/// The envelope names are `payment_method` for one and `payment_methods` for
/// a collection. The plural is inconsistent with `customer_list`, and
/// regularising it would change the output contract — an agent branching on
/// `object` would see it.
pub static PAYMENT_METHOD: Resource = Resource {
    object: "payment_method",
    object_list: "payment_methods",
    id: "/paymentMethodId",
    detail: &[
        "/paymentMethodId",
        "/type",
        "/name",
        "/isDefault",
        "/customerId",
        "/createdOn",
        "/card/cardMask",
        "/card/expirationMonth",
        "/card/expirationYear",
        "/card/cardTokenType",
        "/ach/accountNumber",
        "/ach/routingNumber",
        "/ach/accountType",
        "/ach/accountHolderType",
        "/ach/companyName",
        "/ach/taxId",
    ],
    columns: &[
        Column {
            header: "ID",
            width: 36,
            cell: Cell::Path("/paymentMethodId"),
        },
        Column {
            header: "TYPE",
            width: 12,
            cell: Cell::Path("/type"),
        },
        Column {
            header: "PAN/ACCT",
            width: 20,
            cell: Cell::Derived(masked_number),
        },
        Column {
            header: "EXP",
            width: 7,
            cell: Cell::Derived(expiry),
        },
        Column {
            header: "DEFAULT",
            width: 7,
            cell: Cell::Path("/isDefault"),
        },
    ],
    amounts: &[],
    yes_no: &["/isDefault"],
};

/// A payment method as `add-card` and `add-ach` answer for it: the
/// identifier and nothing else.
///
/// Both create responses carry `paymentMethodId` alone, so the read's rows
/// would all be dashes; `payment-methods get` reads the record back. The same
/// envelope name and identifier as [`PAYMENT_METHOD`].
pub static PAYMENT_METHOD_CREATED: Resource = Resource {
    object: "payment_method",
    object_list: "payment_methods",
    id: "/paymentMethodId",
    detail: &["/paymentMethodId"],
    columns: &[],
    amounts: &[],
    yes_no: &[],
};

/// `get`'s table for a card: the rows [`PAYMENT_METHOD`] declares, less the
/// bank account's, with the expiry on one row. `/card/expiry` is not a field
/// the API sends: [`detail_view`] puts it in the table's copy of the response.
pub static CARD_VIEW: Resource = Resource {
    object: "payment_method",
    object_list: "payment_methods",
    id: "/paymentMethodId",
    detail: &[
        "/paymentMethodId",
        "/type",
        "/name",
        "/isDefault",
        "/customerId",
        "/createdOn",
        "/card/cardMask",
        "/card/expiry",
        "/card/cardTokenType",
    ],
    columns: &[],
    amounts: &[],
    yes_no: &["/isDefault"],
};

/// `get`'s table for a bank account: the rows [`PAYMENT_METHOD`] declares,
/// less the card's.
pub static ACH_VIEW: Resource = Resource {
    object: "payment_method",
    object_list: "payment_methods",
    id: "/paymentMethodId",
    detail: &[
        "/paymentMethodId",
        "/type",
        "/name",
        "/isDefault",
        "/customerId",
        "/createdOn",
        "/ach/accountNumber",
        "/ach/routingNumber",
        "/ach/accountType",
        "/ach/accountHolderType",
        "/ach/companyName",
        "/ach/taxId",
    ],
    columns: &[],
    amounts: &[],
    yes_no: &["/isDefault"],
};

/// `get`'s table: the rows of the instrument the method is, and none of the
/// other's. The response carries both containers and nulls the one that does
/// not apply, so a card would otherwise report six bank-account fields as
/// missing. Any other `type` keeps both. A card's month and year become one
/// `card.expiry` row, spelt as `list`'s EXP column spells it.
fn detail_view(data: &Value) -> String {
    let (view, other) = match data.get("type").and_then(Value::as_str) {
        Some("Card") => (&CARD_VIEW, "ach"),
        Some("ACH") => (&ACH_VIEW, "card"),
        _ => return render::detail_table(&PAYMENT_METHOD, data),
    };
    let mut shown = data.clone();
    // A populated other container is data the view does not name, and still
    // prints after the declared rows.
    if let Some(map) = shown.as_object_mut() {
        if map.get(other).is_some_and(Value::is_null) {
            map.remove(other);
        }
    }
    if let (Some(exp), Some(card)) = (
        expiry(data),
        shown.pointer_mut("/card").and_then(Value::as_object_mut),
    ) {
        card.remove("expirationMonth");
        card.remove("expirationYear");
        card.insert("expiry".into(), Value::String(exp));
    }
    render::detail_table(view, &shown)
}

/// A card and a bank account keep their masks in different containers, so one
/// column has to reach both.
fn masked_number(v: &Value) -> Option<String> {
    v.pointer("/card/cardMask")
        .or_else(|| v.pointer("/ach/accountNumber"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Zero-padded `MM/YYYY`. Cards only.
fn expiry(v: &Value) -> Option<String> {
    let month = v.pointer("/card/expirationMonth")?.as_u64()?;
    let year = v.pointer("/card/expirationYear")?.as_u64()?;
    Some(format!("{month:02}/{year}"))
}

pub async fn dispatch(ctx: &Ctx, command: PaymentMethodsCommand) -> Result<()> {
    match command {
        PaymentMethodsCommand::List(args) => {
            let query = build_list_payment_methods_query(&args)?;
            common::list(
                ctx,
                &PAYMENT_METHOD,
                "/v2/payment-methods",
                &query,
                &args.pagination,
            )
            .await
        }
        PaymentMethodsCommand::Get { payment_method_id } => {
            let resp = ctx
                .api
                .request(
                    Method::GET,
                    ApiPath::from("/v2/payment-methods").id(&payment_method_id)?,
                    &[],
                    None,
                )
                .await?;
            let data = common::body_of(resp.body)?;
            match ctx.output {
                OutputFormat::Table => {
                    println!("{}", detail_view(&data));
                    Ok(())
                }
                _ => render::one(ctx, &PAYMENT_METHOD, &data, resp.correlation_id),
            }
        }
        PaymentMethodsCommand::AddCard(args) => {
            let body = build_add_card_body(&args)?;
            let resp = ctx
                .api
                .request(Method::POST, "/v2/payment-methods/cards", &[], Some(body))
                .await?;
            render::one(
                ctx,
                &PAYMENT_METHOD_CREATED,
                &common::body_of(resp.body)?,
                resp.correlation_id,
            )
        }
        PaymentMethodsCommand::AddAch(args) => {
            let body = build_add_ach_body(&args)?;
            let resp = ctx
                .api
                .request(Method::POST, "/v2/payment-methods/ach", &[], Some(body))
                .await?;
            render::one(
                ctx,
                &PAYMENT_METHOD_CREATED,
                &common::body_of(resp.body)?,
                resp.correlation_id,
            )
        }
        PaymentMethodsCommand::Update(args) => {
            let body = build_update_payment_method_body(&args)?;
            let resp = ctx
                .api
                .request(
                    Method::PATCH,
                    ApiPath::from("/v2/payment-methods").id(&args.payment_method_id)?,
                    &[],
                    Some(body),
                )
                .await?;
            render::confirmed(
                ctx,
                &PAYMENT_METHOD,
                &args.payment_method_id,
                "updated",
                &format!("Updated payment method {}.", args.payment_method_id),
                resp.correlation_id,
            )
        }
        PaymentMethodsCommand::Delete {
            payment_method_id, ..
        } => {
            // A 404 here is exit 0 but not a deletion: the server answers the
            // same for "already deleted" and "never existed".
            common::delete(
                ctx,
                &PAYMENT_METHOD,
                ApiPath::from("/v2/payment-methods").id(&payment_method_id)?,
                &payment_method_id,
                "deleted",
                &format!("Deleted payment method {payment_method_id}."),
                &format!("No payment method {payment_method_id} was found; nothing was deleted."),
            )
            .await
        }
        PaymentMethodsCommand::SetDefault {
            payment_method_id,
            customer_id,
        } => {
            common::reject_empty_id("--customer-id", &customer_id)?;
            let resp = ctx
                .api
                .request(
                    Method::POST,
                    ApiPath::from("/v2/payment-methods")
                        .id(&payment_method_id)?
                        .seg("set-default"),
                    &[("customerId", customer_id.clone())],
                    None,
                )
                .await?;
            render::confirmed(
                ctx,
                &PAYMENT_METHOD,
                &payment_method_id,
                "default",
                &format!("Set payment method {payment_method_id} as default."),
                resp.correlation_id,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card() -> AddCardArgs {
        AddCardArgs {
            card: "4111111111111111".into(),
            exp: "12/2033".into(),
            ..Default::default()
        }
    }

    /// The three the schema requires, and nothing else.
    #[test]
    fn a_minimal_card_body_carries_only_the_required_fields() {
        assert_eq!(
            build_add_card_body(&card()).unwrap(),
            serde_json::json!({
                "cardNumber": "4111111111111111",
                "expirationMonth": 12,
                "expirationYear": 2033})
        );
    }

    /// Absent is absent, never null: the schema sets
    /// `additionalProperties: false` and the API distinguishes the two.
    #[test]
    fn absent_card_options_are_omitted_rather_than_nulled() {
        let mut args = card();
        args.cvv = Some("123".into());
        let body = build_add_card_body(&args).unwrap();
        assert_eq!(body["securityCode"], "123");
        assert!(body.get("customerId").is_none());
        assert!(body.get("paymentName").is_none());
    }

    #[test]
    fn every_card_flag_reaches_the_body_under_its_wire_name() {
        let args = AddCardArgs {
            card: "4111111111111111".into(),
            exp: "01/33".into(),
            cvv: Some("123".into()),
            customer_id: Some("cus_1".into()),
            payment_name: Some("My Visa".into()),
        };
        let body = build_add_card_body(&args).unwrap();
        assert_eq!(body["securityCode"], "123");
        assert_eq!(body["customerId"], "cus_1");
        assert_eq!(body["paymentName"], "My Visa");
        // A two-digit year is expanded, as on `transactions create`.
        assert_eq!(body["expirationMonth"], 1);
        assert_eq!(body["expirationYear"], 2033);
    }

    #[test]
    fn a_card_with_an_unparseable_expiry_is_refused() {
        let mut args = card();
        args.exp = "13/2033".into();
        assert!(build_add_card_body(&args).is_err());
    }

    fn ach() -> AddAchArgs {
        AddAchArgs {
            account_number: "123456789".into(),
            routing_number: "021000021".into(),
            account_type: AccountType::Checking,
            account_holder_type: AccountHolderType::Personal,
            tax_id: None,
            customer_id: None,
            name: None,
            company_name: None,
        }
    }

    /// The four the schema requires — this shape is where the unfamiliar
    /// required fields are.
    #[test]
    fn a_minimal_ach_body_carries_exactly_the_four_required_fields() {
        assert_eq!(
            build_add_ach_body(&ach()).unwrap(),
            serde_json::json!({
                "routingNumber": "021000021",
                "accountNumber": "123456789",
                "accountHolderType": "Personal",
                "accountType": "Checking"})
        );
    }

    /// The refusal names the one flag that is empty.
    #[test]
    fn an_empty_account_or_routing_number_is_named() {
        let mut args = ach();
        args.routing_number = " ".into();
        let err = build_add_ach_body(&args).unwrap_err().to_string();
        assert_eq!(err, "--routing is required");
        let mut args = ach();
        args.account_number = String::new();
        let err = build_add_ach_body(&args).unwrap_err().to_string();
        assert_eq!(err, "--account is required");
    }

    /// Capitalised on the wire; a lowercase near-miss is rejected rather than
    /// forgiven by an `additionalProperties: false` schema.
    #[test]
    fn ach_enums_serialise_with_their_declared_casing() {
        assert_eq!(serde_json::json!(AccountType::Checking), "Checking");
        assert_eq!(serde_json::json!(AccountType::Savings), "Savings");
        assert_eq!(serde_json::json!(AccountHolderType::Business), "Business");
        assert_eq!(serde_json::json!(AccountHolderType::Personal), "Personal");
    }

    #[test]
    fn absent_ach_options_are_omitted_rather_than_nulled() {
        let body = build_add_ach_body(&ach()).unwrap();
        for key in ["taxId", "customerId", "name", "companyName"] {
            assert!(body.get(key).is_none(), "{key} should be absent");
        }
    }

    #[test]
    fn every_ach_flag_reaches_the_body_under_its_wire_name() {
        let args = AddAchArgs {
            account_type: AccountType::Savings,
            account_holder_type: AccountHolderType::Business,
            tax_id: Some("123456789".into()),
            customer_id: Some("cus_1".into()),
            name: Some("Business Savings".into()),
            company_name: Some("Analytical".into()),
            ..ach()
        };
        let body = build_add_ach_body(&args).unwrap();
        assert_eq!(body["accountType"], "Savings");
        assert_eq!(body["accountHolderType"], "Business");
        assert_eq!(body["taxId"], "123456789");
        assert_eq!(body["customerId"], "cus_1");
        assert_eq!(body["name"], "Business Savings");
        assert_eq!(body["companyName"], "Analytical");
    }

    /// One property, so an absent `--name` leaves nothing to send.
    #[test]
    fn an_update_with_no_name_is_refused() {
        let args = UpdatePaymentMethodArgs {
            payment_method_id: "pm_1".into(),
            payment_name: None,
            clear: vec![],
        };
        assert!(build_update_payment_method_body(&args).is_err());
    }

    #[test]
    fn an_unfiltered_payment_method_list_sends_nothing() {
        assert!(
            build_list_payment_methods_query(&ListPaymentMethodsArgs::default())
                .unwrap()
                .is_empty()
        );
    }

    /// An empty customer id names no customer, and dropping it would answer
    /// with every customer's payment methods.
    #[test]
    fn an_empty_customer_id_filter_is_refused() {
        let args = ListPaymentMethodsArgs {
            customer_id: Some(String::new()),
            ..Default::default()
        };
        let err = build_list_payment_methods_query(&args)
            .unwrap_err()
            .to_string();
        assert!(err.contains("--customer-id needs a value"), "{err}");
    }

    /// This endpoint declares a single `search`, unlike `customers list`'s
    /// four named filters, so `--search` maps straight onto it.
    #[test]
    fn every_payment_method_filter_reaches_the_query_under_its_wire_name() {
        let args = ListPaymentMethodsArgs {
            pagination: PaginationArgs {
                page_index: Some(1),
                page_size: Some(5),
                all: false,
            },
            sort_by: Some("createdOn".into()),
            desc: true,
            search: Some("visa".into()),
            customer_id: Some("cus_1".into()),
            created_from: Some("2026-01-01T00:00:00Z".into()),
            created_to: Some("2026-12-31T23:59:59Z".into()),
        };
        let names: Vec<&str> = build_list_payment_methods_query(&args)
            .unwrap()
            .iter()
            .map(|(k, _)| *k)
            .collect();
        assert_eq!(
            names,
            [
                "pageIndex",
                "pageSize",
                "asc",
                "sortBy",
                "search",
                "customerId",
                "createdFrom",
                "createdTo"
            ]
        );
    }

    /// A card and a bank account keep their masks in different containers.
    #[test]
    fn the_number_column_reaches_either_instrument_container() {
        assert_eq!(
            masked_number(&serde_json::json!({"card": {"cardMask": "****1111"}})).as_deref(),
            Some("****1111")
        );
        assert_eq!(
            masked_number(&serde_json::json!({"ach": {"accountNumber": "****6789"}})).as_deref(),
            Some("****6789")
        );
        assert_eq!(masked_number(&serde_json::json!({"type": "Cash"})), None);
    }

    /// Zero-padded, and absent for a non-card.
    #[test]
    fn the_expiry_column_is_zero_padded_and_card_only() {
        assert_eq!(
            expiry(&serde_json::json!({
                "card": {"expirationMonth": 1, "expirationYear": 2033}}))
            .as_deref(),
            Some("01/2033")
        );
        assert_eq!(
            expiry(&serde_json::json!({"ach": {"accountNumber": "x"}})),
            None
        );
    }
}
