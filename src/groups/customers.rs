//! `customers`: create, get, list, update, delete.

use crate::Ctx;
use crate::api::ApiPath;
use crate::cli::address::{BillingArgs, ShippingArgs};
use crate::cli::common::{self, PaginationArgs};
use crate::cli::render::{self, Cell, Column, Resource};
use anyhow::Result;
use reqwest::Method;
use serde_json::{Map, Value};

/// A field `--clear` can null out, and the wire key it nulls.
///
/// `firstName` and `lastName` are absent on purpose: both map to non-nullable
/// columns, so a null is answered `cannot be cleared` and the round trip is
/// spent on a refusal. The two switches take an explicit `false` instead.
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Clearable {
    Company,
    Email,
    Mobile,
}

impl Clearable {
    fn wire(self) -> &'static str {
        match self {
            Self::Company => "companyName",
            Self::Email => "email",
            Self::Mobile => "mobilePhoneNumber",
        }
    }

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
/// is constructed exactly once per process. The lint is measuring a cost that
/// is not paid here.
#[allow(clippy::large_enum_variant)]
#[derive(clap::Subcommand, Debug)]
pub enum CustomersCommand {
    /// Create a new customer (POST /v2/customers).
    Create(CreateCustomerArgs),
    /// Fetch a single customer by ID (GET /v2/customers/{customerId}).
    Get {
        /// Customer UUID to retrieve (positional).
        customer_id: String,
    },
    /// List customers (GET /v2/customers).
    List(ListCustomersArgs),
    /// Update a customer (PATCH /v2/customers/{customerId}).
    ///
    /// Omitted flags retain their existing server values.
    Update(UpdateCustomerArgs),
    /// Delete a customer (DELETE /v2/customers/{customerId}).
    ///
    /// Requires `--yes` to prevent accidental deletions.
    Delete {
        /// Customer UUID to delete (positional).
        customer_id: String,
        /// Confirm the deletion (required).
        #[arg(long)]
        yes: bool,
    },
}

#[derive(clap::Args, Debug, Default)]
pub struct ListCustomersArgs {
    #[command(flatten)]
    pub pagination: PaginationArgs,
    /// Sort results by this field name.
    #[arg(long)]
    pub sort_by: Option<String>,
    /// Sort descending. The default is ascending.
    #[arg(long)]
    pub desc: bool,
    /// Filter by full name.
    #[arg(long)]
    pub full_name: Option<String>,
    /// Filter by email address.
    #[arg(long, id = "list_email", value_name = "EMAIL")]
    pub email: Option<String>,
    /// Filter by company name.
    #[arg(long, id = "list_company_name", value_name = "COMPANY_NAME")]
    pub company_name: Option<String>,
    /// Filter by mobile number.
    #[arg(
        long = "mobile",
        id = "list_mobile",
        value_name = "MOBILE_PHONE_NUMBER"
    )]
    pub mobile_phone_number: Option<String>,
    /// Filter results created from this date-time inclusive (ISO 8601).
    #[arg(long)]
    pub created_from: Option<String>,
    /// Filter results created up to this date-time inclusive (ISO 8601).
    #[arg(long)]
    pub created_to: Option<String>,
}

#[derive(clap::Args, Debug, Default)]
pub struct UpdateCustomerArgs {
    /// Customer UUID to update (positional).
    pub customer_id: String,
    /// New first name.
    #[arg(long, id = "update_first_name", value_name = "FIRST_NAME")]
    pub first_name: Option<String>,
    /// New last name.
    #[arg(long, id = "update_last_name", value_name = "LAST_NAME")]
    pub last_name: Option<String>,
    /// New company name.
    #[arg(long = "company", id = "update_company_name", value_name = "COMPANY")]
    pub company_name: Option<String>,
    /// New email address.
    #[arg(long, id = "update_email", value_name = "EMAIL")]
    pub email: Option<String>,
    /// New mobile phone number in E.164 form, e.g. `+14155552309`.
    #[arg(
        long = "mobile",
        id = "update_mobile",
        value_name = "MOBILE_PHONE_NUMBER"
    )]
    pub mobile_phone_number: Option<String>,
    // Valued rather than a bare switch: a PATCH has to be able to clear a
    // flag, and a switch cannot express `false`.
    /// Consent to receive SMS: `--sms-consent` or `--sms-consent false`.
    #[arg(long = "sms-consent", id = "update_sms_consent", value_name = "SMS_CONSENT",
          num_args = 0..=1, default_missing_value = "true")]
    pub has_sms_consent: Option<bool>,
    /// Reuse the billing address as the shipping address:
    /// `--use-billing-as-shipping` or `--use-billing-as-shipping false`.
    #[arg(long = "use-billing-as-shipping", id = "update_use_billing_as_shipping", value_name = "USE_BILLING_AS_SHIPPING",
          num_args = 0..=1, default_missing_value = "true")]
    pub should_use_billing_as_shipping_address: Option<bool>,
    #[command(flatten)]
    pub billing: BillingArgs,
    #[command(flatten)]
    pub shipping: ShippingArgs,
    /// Clear a field back to nothing. Repeat the flag for several fields.
    ///
    /// An empty value does the same: `--email ""` and `--clear email` are one
    /// request. `first-name` and `last-name` cannot be cleared and are not
    /// offered.
    #[arg(long = "clear", value_enum, value_name = "FIELD")]
    pub clear: Vec<Clearable>,
}

/// The `GET /v2/customers` query, omitting every absent flag.
pub fn build_list_customers_query(args: &ListCustomersArgs) -> Result<Vec<(&'static str, String)>> {
    args.pagination.validate()?;
    let mut query = args.pagination.query();
    // Only the non-default direction is sent: the API's `asc` defaults to
    // true, so sending it on every call would say nothing.
    if args.desc {
        query.push(("asc", "false".into()));
    }
    common::push_str(&mut query, "sortBy", &args.sort_by);
    common::push_str(&mut query, "fullName", &args.full_name);
    common::push_str(&mut query, "email", &args.email);
    common::push_str(&mut query, "companyName", &args.company_name);
    common::push_str(&mut query, "mobilePhoneNumber", &args.mobile_phone_number);
    common::push_str(&mut query, "createdFrom", &args.created_from);
    common::push_str(&mut query, "createdTo", &args.created_to);
    Ok(query)
}

/// Build the `UpdateCustomerRequestDto` body.
///
/// A PATCH with nothing in it is refused: the schema declares no required
/// fields, so the API would accept it, spend a round trip, and change
/// nothing.
pub fn build_update_customer_body(args: &UpdateCustomerArgs) -> Result<Value> {
    for (flag, value) in [
        ("first-name", &args.first_name),
        ("last-name", &args.last_name),
    ] {
        common::reject_unclearable(flag, value.as_deref())?;
    }

    let mut body = Map::new();
    common::put_patch(&mut body, "firstName", &args.first_name);
    common::put_patch(&mut body, "lastName", &args.last_name);
    common::put_patch(&mut body, "companyName", &args.company_name);
    common::put_patch(&mut body, "email", &args.email);
    common::put_patch(&mut body, "mobilePhoneNumber", &args.mobile_phone_number);
    if let Some(v) = args.has_sms_consent {
        body.insert("hasSmsConsent".into(), Value::Bool(v));
    }
    if let Some(v) = args.should_use_billing_as_shipping_address {
        body.insert("shouldUseBillingAsShippingAddress".into(), Value::Bool(v));
    }
    if let Some(a) = args.billing.to_address() {
        body.insert("billingAddress".into(), a);
    }
    if let Some(a) = args.shipping.to_address() {
        body.insert("shippingAddress".into(), a);
    }
    for field in &args.clear {
        let key = field.wire();
        if body.contains_key(key) {
            anyhow::bail!(
                "--clear {} contradicts the value given for it; pass one or \
                 the other",
                field.flag()
            );
        }
        body.insert(key.to_string(), Value::Null);
    }

    if body.is_empty() {
        if let Some(flag) = empty_address_flag(args) {
            anyhow::bail!(
                "nothing to update: {flag} is empty, and address components cannot be \
                 cleared individually; pass a value for it"
            );
        }
        anyhow::bail!("nothing to update: pass at least one field, e.g. --email or --company");
    }
    Ok(Value::Object(body))
}

/// The first address flag given an empty value. An empty component is
/// omitted from the address it builds, so it sends nothing.
fn empty_address_flag(args: &UpdateCustomerArgs) -> Option<String> {
    let (b, s) = (&args.billing, &args.shipping);
    [
        (
            "billing",
            [
                &b.line1,
                &b.line2,
                &b.city,
                &b.state,
                &b.postal_code,
                &b.country,
            ],
        ),
        (
            "shipping",
            [
                &s.line1,
                &s.line2,
                &s.city,
                &s.state,
                &s.postal_code,
                &s.country,
            ],
        ),
    ]
    .into_iter()
    .find_map(|(prefix, values)| {
        ["line1", "line2", "city", "state", "postal-code", "country"]
            .into_iter()
            .zip(values)
            .find(|(_, v)| v.as_deref() == Some(""))
            .map(|(name, _)| format!("--{prefix}-{name}"))
    })
}

#[cfg(test)]
mod clearing_tests {
    use super::*;

    /// Both spellings reach the same null.
    #[test]
    fn the_two_spellings_send_the_same_null() {
        let empty = UpdateCustomerArgs {
            customer_id: "cus_1".into(),
            email: Some(String::new()),
            ..Default::default()
        };
        let flag = UpdateCustomerArgs {
            customer_id: "cus_1".into(),
            clear: vec![Clearable::Email],
            ..Default::default()
        };
        assert_eq!(
            build_update_customer_body(&empty).unwrap(),
            build_update_customer_body(&flag).unwrap()
        );
    }

    /// Setting and clearing one field in one invocation is a contradiction.
    #[test]
    fn setting_and_clearing_one_field_is_refused() {
        let args = UpdateCustomerArgs {
            customer_id: "cus_1".into(),
            email: Some("ada@example.com".into()),
            clear: vec![Clearable::Email],
            ..Default::default()
        };
        assert!(build_update_customer_body(&args).is_err());
    }

    /// The refusal names the field as `--clear` spells it, which is what the
    /// caller typed, rather than the wire key.
    #[test]
    fn a_contradicting_clear_is_named_as_the_caller_spelled_it() {
        for (args, flag) in [
            (
                UpdateCustomerArgs {
                    company_name: Some("X".into()),
                    clear: vec![Clearable::Company],
                    ..Default::default()
                },
                "--clear company ",
            ),
            (
                UpdateCustomerArgs {
                    mobile_phone_number: Some("+14155552309".into()),
                    clear: vec![Clearable::Mobile],
                    ..Default::default()
                },
                "--clear mobile ",
            ),
        ] {
            let err = build_update_customer_body(&args).unwrap_err().to_string();
            assert!(err.starts_with(flag), "{err}");
        }
    }

    /// The non-nullable columns are offered by neither spelling.
    #[test]
    fn the_unclearable_fields_have_no_clear_variant() {
        let offered: Vec<String> = <Clearable as clap::ValueEnum>::value_variants()
            .iter()
            .filter_map(clap::ValueEnum::to_possible_value)
            .map(|p| p.get_name().to_string())
            .collect();
        for absent in ["first-name", "last-name"] {
            assert!(!offered.contains(&absent.to_string()), "{offered:?}");
        }
    }

    /// An empty value is the clear: the body carries an explicit null, which
    /// is what removes a field under RFC 7396. Omitting the key would leave
    /// the stored value standing.
    #[test]
    fn an_empty_value_clears_the_field_with_an_explicit_null() {
        let args = UpdateCustomerArgs {
            customer_id: "cus_1".into(),
            email: Some(String::new()),
            company_name: Some(String::new()),
            ..Default::default()
        };
        let body = build_update_customer_body(&args).unwrap();
        assert_eq!(body["email"], Value::Null);
        assert_eq!(body["companyName"], Value::Null);
    }

    /// A cleared field and a populated one travel in the same body.
    #[test]
    fn a_clear_and_a_value_are_sent_together() {
        let args = UpdateCustomerArgs {
            customer_id: "cus_1".into(),
            email: Some(String::new()),
            first_name: Some("Ada".into()),
            ..Default::default()
        };
        let body = build_update_customer_body(&args).unwrap();
        assert_eq!(body["email"], Value::Null);
        assert_eq!(body["firstName"], "Ada");
    }

    /// `firstName` and `lastName` map to non-nullable columns and the API
    /// answers `cannot be cleared`. Refusing here spends no round trip on a
    /// rejection the CLI can see coming.
    #[test]
    fn a_field_the_api_will_not_clear_is_refused_before_the_request() {
        for args in [
            UpdateCustomerArgs {
                customer_id: "cus_1".into(),
                first_name: Some(String::new()),
                ..Default::default()
            },
            UpdateCustomerArgs {
                customer_id: "cus_1".into(),
                last_name: Some(String::new()),
                ..Default::default()
            },
        ] {
            let err = build_update_customer_body(&args).unwrap_err().to_string();
            assert!(err.contains("cannot be cleared"), "{err}");
        }
    }

    /// Create has nothing to clear, so an empty value there is an absent key
    /// rather than a null the API would read as a removal.
    #[test]
    fn create_omits_an_empty_value_rather_than_nulling_it() {
        let args = CreateCustomerArgs {
            first_name: "Ada".into(),
            last_name: "Lovelace".into(),
            company_name: Some(String::new()),
            ..Default::default()
        };
        let body = build_create_customer_body(&args).unwrap();
        assert!(body.get("companyName").is_none(), "{body}");
    }
}

#[derive(clap::Args, Debug, Default)]
pub struct CreateCustomerArgs {
    /// Customer first name (required).
    #[arg(long)]
    pub first_name: String,
    /// Customer last name (required).
    #[arg(long)]
    pub last_name: String,
    /// Customer company name.
    #[arg(long = "company", value_name = "COMPANY")]
    pub company_name: Option<String>,
    /// Customer email address.
    #[arg(long)]
    pub email: Option<String>,
    /// Customer mobile phone number in E.164 form, e.g. `+14155552309`.
    #[arg(long = "mobile")]
    pub mobile_phone_number: Option<String>,
    /// Record consent to receive SMS.
    #[arg(long = "sms-consent")]
    pub has_sms_consent: bool,
    /// Reuse the billing address as the shipping address.
    #[arg(long = "use-billing-as-shipping")]
    pub should_use_billing_as_shipping_address: bool,
    #[command(flatten)]
    pub billing: BillingArgs,
    #[command(flatten)]
    pub shipping: ShippingArgs,
}

/// Build the `CreateCustomerRequestDto` body.
///
/// Absent optional flags are omitted rather than nulled, because the schema
/// sets `additionalProperties: false` and the API distinguishes an absent key
/// from a null one.
pub fn build_create_customer_body(args: &CreateCustomerArgs) -> Result<Value> {
    for (flag, value) in [
        ("--first-name", &args.first_name),
        ("--last-name", &args.last_name),
    ] {
        if value.is_empty() {
            anyhow::bail!("{flag} is required");
        }
    }
    let mut body = Map::new();
    body.insert("firstName".into(), Value::String(args.first_name.clone()));
    body.insert("lastName".into(), Value::String(args.last_name.clone()));
    common::put_str(&mut body, "companyName", &args.company_name);
    common::put_str(&mut body, "email", &args.email);
    common::put_str(&mut body, "mobilePhoneNumber", &args.mobile_phone_number);

    // A `false` boolean flag is an absent flag: clap cannot distinguish
    // "not passed" from "passed false" on a bare switch, so only the true
    // case is sent and the server's default governs otherwise.
    if args.has_sms_consent {
        body.insert("hasSmsConsent".into(), Value::Bool(true));
    }
    if args.should_use_billing_as_shipping_address {
        body.insert(
            "shouldUseBillingAsShippingAddress".into(),
            Value::Bool(true),
        );
    }
    if let Some(a) = args.billing.to_address() {
        body.insert("billingAddress".into(), a);
    }
    if let Some(a) = args.shipping.to_address() {
        body.insert("shippingAddress".into(), a);
    }
    Ok(Value::Object(body))
}

/// What a customer is worth saying, in the order it is worth saying it:
/// identity, then contact, then addresses, then activity, then instruments.
///
/// The **view's fixed shape** — a pointer the response does not carry still
/// holds its row, with a dash — and anything the API sends that is not named
/// here still prints, after these.
pub static CUSTOMER: Resource = Resource {
    object: "customer",
    object_list: "customer_list",
    id: "/customerId",
    detail: &[
        "/customerId",
        "/firstName",
        "/lastName",
        "/companyName",
        "/email",
        "/mobilePhoneNumber",
        "/hasSmsConsent",
        "/billingAddress/addressLine1",
        "/billingAddress/addressLine2",
        "/billingAddress/city",
        "/billingAddress/stateCode",
        "/billingAddress/postalCode",
        "/billingAddress/countryCode",
        "/shouldUseBillingAsShippingAddress",
        "/shippingAddress/addressLine1",
        "/shippingAddress/addressLine2",
        "/shippingAddress/city",
        "/shippingAddress/stateCode",
        "/shippingAddress/postalCode",
        "/shippingAddress/countryCode",
        "/transactionsCount",
        "/transactionsVolume",
        "/lastTransactionAmount",
        "/lastTransactionDate",
        "/cards/[]/paymentMethodId",
        "/cards/[]/paymentName",
        "/cards/[]/cardMask",
        "/cards/[]/cardType",
        "/cards/[]/creditDebitType",
        "/cards/[]/expirationMonth",
        "/cards/[]/expirationYear",
        "/cards/[]/cardTokenType",
        "/cards/[]/isDefault",
        "/achAccounts/[]/paymentMethodId",
        "/achAccounts/[]/paymentName",
        "/achAccounts/[]/accountNumber",
        "/achAccounts/[]/routingNumber",
        "/achAccounts/[]/accountType",
        "/achAccounts/[]/accountHolderType",
        "/achAccounts/[]/taxId",
        "/achAccounts/[]/isDefault",
    ],
    // The list item carries no creation timestamp, so the last column
    // reports the most recent transaction instead of inventing a value.
    columns: &[
        Column {
            header: "ID",
            width: 36,
            cell: Cell::Path("/customerId"),
        },
        Column {
            header: "NAME",
            width: 28,
            cell: Cell::Derived(full_name),
        },
        Column {
            header: "EMAIL",
            width: 28,
            cell: Cell::Path("/email"),
        },
        Column {
            header: "PHONE",
            width: 14,
            cell: Cell::Path("/mobilePhoneNumber"),
        },
        Column {
            header: "LAST TXN",
            width: 10,
            cell: Cell::Derived(|v| render::date_only(v, "/lastTransactionDate")),
        },
    ],
    amounts: &["/transactionsVolume", "/lastTransactionAmount"],
    yes_no: &[
        "/hasSmsConsent",
        "/shouldUseBillingAsShippingAddress",
        "/cards/[]/isDefault",
        "/achAccounts/[]/isDefault",
    ],
};

/// A customer as `create` answers for it: the identifier and nothing else.
///
/// The create response carries `customerId` alone, so the read's rows would
/// all be dashes; `customers get` reads the record back. The same envelope
/// name and identifier as [`CUSTOMER`], so `--output json` and `quiet` cannot
/// tell them apart.
pub static CUSTOMER_CREATED: Resource = Resource {
    object: "customer",
    object_list: "customer_list",
    id: "/customerId",
    detail: &["/customerId"],
    columns: &[],
    amounts: &[],
    yes_no: &[],
};

/// A person or a company: the API supplies whichever it has, and a customer
/// with only a company name must not render as a blank row.
fn full_name(v: &Value) -> Option<String> {
    let part = |p: &str| {
        v.pointer(p)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
    };
    match (part("/firstName"), part("/lastName")) {
        (None, None) => part("/companyName").map(str::to_string),
        (first, last) => Some(
            [first, last]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" "),
        ),
    }
}

pub async fn dispatch(ctx: &Ctx, command: CustomersCommand) -> Result<()> {
    match command {
        CustomersCommand::Create(args) => {
            let body = build_create_customer_body(&args)?;
            let resp = ctx
                .api
                .request(Method::POST, "/v2/customers", &[], Some(body))
                .await?;
            render::one(
                ctx,
                &CUSTOMER_CREATED,
                &common::body_of(resp.body)?,
                resp.correlation_id,
            )
        }
        CustomersCommand::Get { customer_id } => {
            let resp = ctx
                .api
                .request(
                    Method::GET,
                    ApiPath::from("/v2/customers").id(&customer_id)?,
                    &[],
                    None,
                )
                .await?;
            render::one(
                ctx,
                &CUSTOMER,
                &common::body_of(resp.body)?,
                resp.correlation_id,
            )
        }
        CustomersCommand::List(args) => {
            let query = build_list_customers_query(&args)?;
            common::list(ctx, &CUSTOMER, "/v2/customers", &query, &args.pagination).await
        }
        CustomersCommand::Update(args) => {
            let body = build_update_customer_body(&args)?;
            let resp = ctx
                .api
                .request(
                    Method::PATCH,
                    ApiPath::from("/v2/customers").id(&args.customer_id)?,
                    &[],
                    Some(body),
                )
                .await?;
            render::confirmed(
                ctx,
                &CUSTOMER,
                &args.customer_id,
                "updated",
                &format!("Updated customer {}.", args.customer_id),
                resp.correlation_id,
            )
        }
        CustomersCommand::Delete { customer_id, .. } => {
            // A 404 here is exit 0 but not a deletion: the server answers the
            // same for "already deleted" and "never existed". A 404 from `get`
            // or `list` is still exit 4.
            common::delete(
                ctx,
                &CUSTOMER,
                ApiPath::from("/v2/customers").id(&customer_id)?,
                &customer_id,
                "deleted",
                &format!("Deleted customer {customer_id}."),
                &format!("No customer {customer_id} was found; nothing was deleted."),
            )
            .await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal() -> CreateCustomerArgs {
        CreateCustomerArgs {
            first_name: "Ada".into(),
            last_name: "Lovelace".into(),
            ..Default::default()
        }
    }

    #[test]
    fn builds_minimal_body_with_only_required_fields() {
        let body = build_create_customer_body(&minimal()).unwrap();
        assert_eq!(
            body,
            serde_json::json!({"firstName": "Ada", "lastName": "Lovelace"})
        );
    }

    /// An absent flag must not become a null; the API distinguishes them.
    #[test]
    fn omits_optional_fields_entirely_when_absent() {
        let mut args = minimal();
        args.email = Some("ada@example.com".into());
        let body = build_create_customer_body(&args).unwrap();
        assert!(body.get("companyName").is_none());
        assert!(body.get("mobilePhoneNumber").is_none());
        assert_eq!(body["email"], "ada@example.com");
    }

    #[test]
    fn nests_billing_address_under_billing_address_key() {
        let mut args = minimal();
        args.billing = BillingArgs {
            line1: Some("1 Main St".into()),
            city: Some("Austin".into()),
            state: Some("TX".into()),
            postal_code: Some("78701".into()),
            country: Some("US".into()),
            line2: None,
        };
        let body = build_create_customer_body(&args).unwrap();
        assert_eq!(body["billingAddress"]["city"], "Austin");
        assert_eq!(body["billingAddress"]["addressLine1"], "1 Main St");
        assert!(body["billingAddress"].get("addressLine2").is_none());
        assert!(body.get("shippingAddress").is_none());
    }

    /// A bare switch cannot distinguish "not passed" from "passed false", so
    /// only the true case is sent and the server's default governs otherwise.
    #[test]
    fn boolean_switches_are_sent_only_when_set() {
        let body = build_create_customer_body(&minimal()).unwrap();
        assert!(body.get("hasSmsConsent").is_none());
        assert!(body.get("shouldUseBillingAsShippingAddress").is_none());

        let mut args = minimal();
        args.has_sms_consent = true;
        args.should_use_billing_as_shipping_address = true;
        let body = build_create_customer_body(&args).unwrap();
        assert_eq!(body["hasSmsConsent"], true);
        assert_eq!(body["shouldUseBillingAsShippingAddress"], true);
    }

    /// The refusal names the one flag that is empty.
    #[test]
    fn rejects_an_empty_required_name() {
        let mut args = minimal();
        args.first_name = String::new();
        let err = build_create_customer_body(&args).unwrap_err().to_string();
        assert_eq!(err, "--first-name is required");
        let mut args = minimal();
        args.last_name = String::new();
        let err = build_create_customer_body(&args).unwrap_err().to_string();
        assert_eq!(err, "--last-name is required");
    }

    fn list_args() -> ListCustomersArgs {
        ListCustomersArgs::default()
    }

    /// No flags means no query: the server's declared defaults govern.
    #[test]
    fn an_unfiltered_list_sends_nothing() {
        assert!(build_list_customers_query(&list_args()).unwrap().is_empty());
    }

    /// Every filter, under the exact wire name the bundle declares.
    #[test]
    fn every_list_filter_reaches_the_query_under_its_wire_name() {
        let args = ListCustomersArgs {
            pagination: PaginationArgs {
                page_index: Some(1),
                page_size: Some(5),
                all: false,
            },
            sort_by: Some("lastName".into()),
            desc: true,
            full_name: Some("Ada Lovelace".into()),
            email: Some("ada@example.com".into()),
            company_name: Some("Analytical".into()),
            mobile_phone_number: Some("+14155552309".into()),
            created_from: Some("2026-01-01T00:00:00Z".into()),
            created_to: Some("2026-12-31T23:59:59Z".into()),
        };
        let q = build_list_customers_query(&args).unwrap();
        let names: Vec<&str> = q.iter().map(|(k, _)| *k).collect();
        assert_eq!(
            names,
            [
                "pageIndex",
                "pageSize",
                "asc",
                "sortBy",
                "fullName",
                "email",
                "companyName",
                "mobilePhoneNumber",
                "createdFrom",
                "createdTo"
            ]
        );
        assert_eq!(q.iter().find(|(k, _)| *k == "asc").unwrap().1, "false");
    }

    /// `asc` defaults to true server-side, so sending it on every call would
    /// say nothing. Only the reversed direction is expressed.
    #[test]
    fn ascending_is_the_default_and_sends_no_asc_parameter() {
        let mut args = list_args();
        args.desc = false;
        let q = build_list_customers_query(&args).unwrap();
        assert!(q.iter().all(|(k, _)| *k != "asc"), "{q:?}");
    }

    /// An empty filter string is not a filter.
    #[test]
    fn an_empty_list_filter_is_treated_as_absent() {
        let mut args = list_args();
        args.email = Some(String::new());
        assert!(build_list_customers_query(&args).unwrap().is_empty());
    }

    fn update_args() -> UpdateCustomerArgs {
        UpdateCustomerArgs {
            customer_id: "cus_1".into(),
            ..Default::default()
        }
    }

    #[test]
    fn every_update_flag_reaches_the_body_under_its_wire_name() {
        let args = UpdateCustomerArgs {
            customer_id: "cus_1".into(),
            first_name: Some("Ada".into()),
            last_name: Some("Lovelace".into()),
            company_name: Some("Analytical".into()),
            email: Some("ada@example.com".into()),
            mobile_phone_number: Some("+14155552309".into()),
            has_sms_consent: Some(true),
            should_use_billing_as_shipping_address: Some(true),
            billing: BillingArgs {
                city: Some("Austin".into()),
                ..Default::default()
            },
            shipping: ShippingArgs {
                city: Some("Dallas".into()),
                ..Default::default()
            },
            clear: vec![],
        };
        let body = build_update_customer_body(&args).unwrap();
        assert_eq!(body["firstName"], "Ada");
        assert_eq!(body["lastName"], "Lovelace");
        assert_eq!(body["companyName"], "Analytical");
        assert_eq!(body["email"], "ada@example.com");
        assert_eq!(body["mobilePhoneNumber"], "+14155552309");
        assert_eq!(body["hasSmsConsent"], true);
        assert_eq!(body["shouldUseBillingAsShippingAddress"], true);
        assert_eq!(body["billingAddress"]["city"], "Austin");
        assert_eq!(body["shippingAddress"]["city"], "Dallas");
    }

    /// Absent is absent, never null: the schema sets
    /// `additionalProperties: false` and the API distinguishes the two.
    #[test]
    fn an_absent_update_flag_is_omitted_rather_than_nulled() {
        let mut args = update_args();
        args.email = Some("ada@example.com".into());
        let body = build_update_customer_body(&args).unwrap();
        assert_eq!(body.as_object().unwrap().len(), 1);
        for key in [
            "firstName",
            "lastName",
            "companyName",
            "mobilePhoneNumber",
            "hasSmsConsent",
            "shouldUseBillingAsShippingAddress",
            "billingAddress",
            "shippingAddress",
        ] {
            assert!(body.get(key).is_none(), "{key} should be absent");
        }
    }

    /// The asymmetry with `create` is the point: a PATCH must be able to
    /// clear a flag, and a bare switch cannot express `false`.
    #[test]
    fn an_update_can_send_a_boolean_false() {
        let mut args = update_args();
        args.has_sms_consent = Some(false);
        let body = build_update_customer_body(&args).unwrap();
        assert_eq!(body["hasSmsConsent"], false);
    }

    /// An empty address component sends nothing, and the refusal says why
    /// rather than asking for a field the caller did pass.
    #[test]
    fn an_update_with_only_an_empty_address_component_says_it_cannot_be_cleared() {
        let mut args = update_args();
        args.shipping.postal_code = Some(String::new());
        let err = build_update_customer_body(&args).unwrap_err().to_string();
        assert!(err.contains("--shipping-postal-code is empty"), "{err}");
        assert!(err.contains("cannot be cleared individually"), "{err}");
    }

    /// An empty PATCH is a round trip that cannot change anything.
    #[test]
    fn an_update_with_no_fields_is_refused() {
        let err = build_update_customer_body(&update_args())
            .unwrap_err()
            .to_string();
        assert!(err.contains("nothing to update"), "{err}");
    }
}
