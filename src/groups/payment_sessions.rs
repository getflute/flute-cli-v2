//! `payment-sessions`: create, get, cancel.
//!
//! `CreatePaymentSessionRequestDto` declares **nothing** required, and its
//! `amount` carries three mutually exclusive rules that live only in the
//! description: greater than zero for a paying session, exactly zero for a
//! vault-only one, and null for a flexible-amount session whose amount is set
//! at checkout. OpenAPI cannot express any of the three, so they are enforced
//! here — the alternative is a round trip that returns the same answer with a
//! worse message.
//!
//! `cancel` is the one destructive verb in the API that is neither a delete
//! nor a revoke, and it is not idempotent: a repeat is a 400 and an unknown id
//! is a 404, exit 4.

use crate::Ctx;
use crate::api::ApiPath;
use crate::cli::common;
use crate::cli::money::{parse_amount, to_amount_number};
use crate::cli::render::{self, Cell, Column, Resource};
use anyhow::Result;
use reqwest::Method;
use rust_decimal::Decimal;
use serde_json::{Map, Value};

/// What the session is for.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum SessionMode {
    /// Take a payment (default).
    #[default]
    Payment,
    /// Store a payment method and take nothing.
    SaveMethod,
    /// Take a payment and store the payment method.
    PaymentAndSave,
}

impl SessionMode {
    pub fn wire(self) -> &'static str {
        match self {
            Self::Payment => "Payment",
            Self::SaveMethod => "SaveMethod",
            Self::PaymentAndSave => "PaymentAndSave",
        }
    }

    /// Whether this mode takes money, which is what decides the amount rule.
    fn charges(self) -> bool {
        matches!(self, Self::Payment | Self::PaymentAndSave)
    }
}

/// What happens to the payer's details once the session completes.
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum CustomerHandling {
    CreateCustomer,
    TokenOnly,
}

impl CustomerHandling {
    pub fn wire(self) -> &'static str {
        match self {
            Self::CreateCustomer => "CreateCustomer",
            Self::TokenOnly => "TokenOnly",
        }
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(clap::Subcommand, Debug)]
pub enum PaymentSessionsCommand {
    /// Create a new payment session (POST /v2/payment-sessions).
    Create(CreatePaymentSessionArgs),
    /// Fetch a single payment session by ID
    /// (GET /v2/payment-sessions/{paymentSessionId}).
    Get {
        /// Payment session UUID to retrieve (positional).
        payment_session_id: String,
    },
    /// Cancel a payment session
    /// (POST /v2/payment-sessions/{paymentSessionId}/cancel).
    ///
    /// Requires `--yes` to prevent accidental cancellation.
    Cancel {
        /// Payment session UUID to cancel (positional).
        payment_session_id: String,
        /// Confirm the cancellation (required).
        #[arg(long)]
        yes: bool,
    },
}

#[derive(clap::Args, Debug, Default)]
pub struct CreatePaymentSessionArgs {
    /// Session mode: `payment` (default), `save-method` or
    /// `payment-and-save`.
    #[arg(long, value_enum)]
    pub mode: Option<SessionMode>,
    /// Amount to charge. Plain decimal, e.g. `100.00`. Omit for a session the
    /// payer fills in at checkout; a `save-method` session needs no flag.
    #[arg(long = "amount", value_parser = parse_amount,
          id = "session_amount", value_name = "AMOUNT",
          allow_negative_numbers = true)]
    pub amount: Option<Decimal>,
    /// Tip amount added to the amount charged.
    #[arg(long, value_parser = parse_amount,
          id = "session_tip_amount", value_name = "TIP_AMOUNT",
          allow_negative_numbers = true)]
    pub tip_amount: Option<Decimal>,
    /// Customer UUID to link the session to.
    #[arg(long, id = "session_customer_id", value_name = "CUSTOMER_ID")]
    pub customer_id: Option<String>,
    /// What to keep once the session completes: `create-customer` or
    /// `token-only`.
    #[arg(long, value_enum)]
    pub customer_handling: Option<CustomerHandling>,
    /// Merchant-assigned reference ID, part of the duplicate-check key.
    #[arg(long, id = "session_reference_id", value_name = "REFERENCE_ID")]
    pub reference_id: Option<String>,
    /// Where to send the payer after a successful payment.
    #[arg(long)]
    pub return_url: Option<String>,
    /// Bypass address verification. Only meaningful on a paying session.
    #[arg(long)]
    pub skip_address_verification: bool,
    /// Display name shown on the checkout page.
    #[arg(long)]
    pub page_name: Option<String>,
    /// Notes shown to the payer on the checkout page.
    #[arg(long)]
    pub payment_notes: Option<String>,
    /// Message shown to the payer once the session completes.
    #[arg(long)]
    pub after_completion_message: Option<String>,
    /// UTC expiry (ISO 8601). Omit for a session that never expires.
    #[arg(long)]
    pub expires_at: Option<String>,
    /// Arbitrary `key=value` pair to attach, handed back on the read. Repeat
    /// the flag for several pairs.
    #[arg(long = "metadata", value_name = "KEY=VALUE")]
    pub metadata: Vec<String>,
    /// Accept card payments.
    #[arg(long = "card-enabled", id = "session_card_enabled")]
    pub card_enabled: bool,
    /// Charge card payments through this processor.
    #[arg(
        long = "card-processor-id",
        id = "session_card_processor_id",
        value_name = "CARD_PROCESSOR_ID"
    )]
    pub card_processor_id: Option<String>,
    /// Accept ACH payments.
    #[arg(long = "ach-enabled", id = "session_ach_enabled")]
    pub ach_enabled: bool,
    /// Charge ACH payments through this processor.
    #[arg(
        long = "ach-processor-id",
        id = "session_ach_processor_id",
        value_name = "ACH_PROCESSOR_ID"
    )]
    pub ach_processor_id: Option<String>,
}

/// Parse one `--metadata` value into a key and a value.
///
/// Only the **first** `=` separates them: metadata is arbitrary, and a query
/// string or a URL as a value is ordinary rather than exotic.
fn metadata_pair(raw: &str) -> Result<(String, String)> {
    let Some((key, value)) = raw.split_once('=') else {
        anyhow::bail!("--metadata takes key=value pairs; '{raw}' has no '='");
    };
    let key = key.trim();
    if key.is_empty() {
        anyhow::bail!("--metadata '{raw}' has an empty key");
    }
    Ok((key.to_string(), value.to_string()))
}

/// The amount rules the description carries and the schema cannot.
///
/// Each refusal spends no round trip on an answer the CLI could already give,
/// and each names the alternative — a zero and an absent amount mean
/// different things here, which is exactly the pair a caller confuses.
fn resolved_amount(args: &CreatePaymentSessionArgs) -> Result<Option<Decimal>> {
    let mode = args.mode.unwrap_or_default();
    match (mode.charges(), args.amount) {
        (true, Some(v)) if v <= Decimal::ZERO => anyhow::bail!(
            "a {} session must charge more than zero. Omit --amount entirely for \
             a session the payer fills in at checkout.",
            mode.wire()
        ),
        (false, Some(v)) if v != Decimal::ZERO => anyhow::bail!(
            "a save-method session stores a payment method and charges nothing, \
             so its amount must be zero (got {v})"
        ),
        // A vault-only session's amount is zero rather than absent: absent is
        // documented as a flexible amount, which a session that takes no
        // payment cannot have.
        (false, None) => Ok(Some(Decimal::ZERO)),
        (_, other) => Ok(other),
    }
}

/// Build the `CreatePaymentSessionRequestDto` body.
pub fn build_create_payment_session_body(args: &CreatePaymentSessionArgs) -> Result<Value> {
    let mut body = Map::new();
    if let Some(mode) = args.mode {
        body.insert("mode".into(), Value::String(mode.wire().into()));
    }
    if let Some(v) = resolved_amount(args)? {
        body.insert("amount".into(), to_amount_number(v)?);
    }
    if let Some(v) = args.tip_amount {
        if v <= Decimal::ZERO {
            anyhow::bail!("--tip-amount must be greater than zero");
        }
        body.insert("tipAmount".into(), to_amount_number(v)?);
    }
    if let Some(v) = args.customer_handling {
        body.insert("customerHandling".into(), Value::String(v.wire().into()));
    }
    // A bare switch cannot distinguish "not passed" from "passed false", so
    // only the true case is sent and the server's default governs otherwise.
    if args.skip_address_verification {
        body.insert("skipAddressVerification".into(), Value::Bool(true));
    }
    common::put_str(&mut body, "customerId", &args.customer_id);
    common::put_str(&mut body, "referenceId", &args.reference_id);
    common::put_str(&mut body, "returnUrl", &args.return_url);
    common::put_str(&mut body, "pageName", &args.page_name);
    common::put_str(&mut body, "paymentNotes", &args.payment_notes);
    common::put_str(
        &mut body,
        "afterCompletionMessage",
        &args.after_completion_message,
    );
    common::put_str(&mut body, "expiresAt", &args.expires_at);

    let mut metadata = Map::new();
    for raw in &args.metadata {
        let (key, value) = metadata_pair(raw)?;
        metadata.insert(key, Value::String(value));
    }
    if !metadata.is_empty() {
        body.insert("metadata".into(), Value::Object(metadata));
    }

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
    if !methods.is_empty() {
        body.insert("paymentMethods".into(), Value::Object(methods));
    }
    Ok(Value::Object(body))
}

/// What a payment session is worth saying: what it is for, then what it
/// carries, then what it became.
///
/// **The identifier is on `create` only.**
/// `CreatePaymentSessionResponseDto` declares `id` and
/// `GetPaymentSessionResponseDto` declares no identifier at all, so a read
/// has nothing for `quiet` to print.
pub static PAYMENT_SESSION: Resource = Resource {
    object: "payment_session",
    object_list: "payment_session_list",
    id: "/id",
    detail: &[
        "/id",
        "/status",
        "/mode",
        // The amount a session charges is reported on the transaction it
        // produced; the request field of the same name is not echoed back.
        "/transactionDetails/amount/baseAmount",
        "/tipAmount",
        "/surchargeAmount",
        "/customerId",
        "/vaultedPaymentMethodId",
        "/referenceId",
        "/pageName",
        "/paymentNotes",
        "/afterCompletionMessage",
        "/returnUrl",
        "/expiresAt",
        "/skipAddressVerification",
        "/paymentLinkId",
        "/paymentMethods/card/enabled",
        "/paymentMethods/card/processorId",
        "/paymentMethods/ach/enabled",
        "/paymentMethods/ach/processorId",
        "/achAccountLast2",
        "/achRoutingLast2",
        "/transactionDetails/transactionReceiptUrl",
        "/transactionDetails/transactionReceipt/transactionId",
        "/transactionDetails/transactionReceipt/status",
        "/transactionDetails/transactionReceipt/amount/totalAmount",
    ],
    // No list endpoint, so these columns are never reached today. They name
    // what a collection of sessions would be worth showing if one arrives.
    columns: &[
        Column {
            header: "ID",
            width: 36,
            cell: Cell::Path("/id"),
        },
        Column {
            header: "STATUS",
            width: 12,
            cell: Cell::Path("/status"),
        },
        Column {
            header: "MODE",
            width: 16,
            cell: Cell::Path("/mode"),
        },
    ],
    amounts: &[
        "/transactionDetails/amount/baseAmount",
        "/tipAmount",
        "/surchargeAmount",
        "/transactionDetails/transactionReceipt/amount/totalAmount",
    ],
    yes_no: &[],
};

pub async fn dispatch(ctx: &Ctx, command: PaymentSessionsCommand) -> Result<()> {
    match command {
        PaymentSessionsCommand::Create(args) => {
            let body = build_create_payment_session_body(&args)?;
            let resp = ctx
                .api
                .request(Method::POST, "/v2/payment-sessions", &[], Some(body))
                .await?;
            render::one(
                ctx,
                &PAYMENT_SESSION,
                &common::body_of(resp.body)?,
                resp.correlation_id,
            )
        }
        PaymentSessionsCommand::Get { payment_session_id } => {
            let resp = ctx
                .api
                .request(
                    Method::GET,
                    ApiPath::from("/v2/payment-sessions").id(&payment_session_id)?,
                    &[],
                    None,
                )
                .await?;
            render::one(
                ctx,
                &PAYMENT_SESSION,
                &common::body_of(resp.body)?,
                resp.correlation_id,
            )
        }
        PaymentSessionsCommand::Cancel {
            payment_session_id, ..
        } => {
            // A cancelled session still exists and refuses a repeat with a 400,
            // so a 404 is an id the server never had: plain not-found.
            let resp = ctx
                .api
                .request(
                    Method::POST,
                    ApiPath::from("/v2/payment-sessions")
                        .id(&payment_session_id)?
                        .seg("cancel"),
                    &[],
                    None,
                )
                .await?;
            render::confirmed(
                ctx,
                &PAYMENT_SESSION,
                &payment_session_id,
                "cancelled",
                &format!("Cancelled payment session {payment_session_id}."),
                resp.correlation_id,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nothing is required, and an absent amount is a flexible-amount
    /// session — so no flags at all is a legal request.
    #[test]
    fn a_create_with_no_flags_is_an_empty_body() {
        let body = build_create_payment_session_body(&CreatePaymentSessionArgs::default()).unwrap();
        assert_eq!(body, serde_json::json!({}));
    }

    /// An empty processor id is a value that went missing, such as an unset
    /// shell variable, and sending the method without it would route payments
    /// through the account's default processor instead of the one named.
    #[test]
    fn an_empty_processor_id_is_refused() {
        for (flag, args) in [
            (
                "--card-processor-id",
                CreatePaymentSessionArgs {
                    card_enabled: true,
                    card_processor_id: Some(String::new()),
                    ..Default::default()
                },
            ),
            (
                "--ach-processor-id",
                CreatePaymentSessionArgs {
                    ach_processor_id: Some(String::new()),
                    ..Default::default()
                },
            ),
        ] {
            let err = build_create_payment_session_body(&args)
                .unwrap_err()
                .to_string();
            assert!(err.contains(flag), "{flag}: {err}");
        }
    }

    #[test]
    fn every_create_flag_reaches_the_body_under_its_wire_name() {
        let args = CreatePaymentSessionArgs {
            mode: Some(SessionMode::PaymentAndSave),
            amount: Some("25.00".parse().unwrap()),
            tip_amount: Some("14.50".parse().unwrap()),
            customer_id: Some("cus-1".into()),
            customer_handling: Some(CustomerHandling::TokenOnly),
            reference_id: Some("ORDER-10001".into()),
            return_url: Some("https://example.com/done".into()),
            skip_address_verification: true,
            page_name: Some("Checkout".into()),
            payment_notes: Some("Deposit".into()),
            after_completion_message: Some("Thank you.".into()),
            expires_at: Some("2027-02-19T20:24:52.934Z".into()),
            metadata: vec!["orderId=9921".into()],
            card_enabled: true,
            card_processor_id: Some("pp-card".into()),
            ach_enabled: true,
            ach_processor_id: Some("pp-ach".into()),
        };
        let body = build_create_payment_session_body(&args).unwrap();
        assert_eq!(body["mode"], "PaymentAndSave");
        assert_eq!(body["amount"].to_string(), "25.00");
        assert_eq!(body["tipAmount"].to_string(), "14.50");
        assert_eq!(body["customerId"], "cus-1");
        assert_eq!(body["customerHandling"], "TokenOnly");
        assert_eq!(body["referenceId"], "ORDER-10001");
        assert_eq!(body["returnUrl"], "https://example.com/done");
        assert_eq!(body["skipAddressVerification"], true);
        assert_eq!(body["pageName"], "Checkout");
        assert_eq!(body["paymentNotes"], "Deposit");
        assert_eq!(body["afterCompletionMessage"], "Thank you.");
        assert_eq!(body["expiresAt"], "2027-02-19T20:24:52.934Z");
        assert_eq!(body["metadata"]["orderId"], "9921");
        assert_eq!(body["paymentMethods"]["card"]["processorId"], "pp-card");
        assert_eq!(body["paymentMethods"]["ach"]["processorId"], "pp-ach");
    }

    /// The schema requires a supplied tip to be greater than zero, so a zero
    /// tip is refused before the wire rather than spent on a 400.
    #[test]
    fn a_zero_tip_is_refused() {
        let args = CreatePaymentSessionArgs {
            amount: Some("25.00".parse().unwrap()),
            tip_amount: Some(Decimal::ZERO),
            ..Default::default()
        };
        let err = build_create_payment_session_body(&args)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("--tip-amount must be greater than zero"),
            "{err}"
        );
    }

    /// A vault-only session's amount is **zero and present**, because absent
    /// is documented as a flexible amount and a session that charges nothing
    /// cannot have one.
    #[test]
    fn a_vault_only_session_sends_a_zero_amount_rather_than_none() {
        let args = CreatePaymentSessionArgs {
            mode: Some(SessionMode::SaveMethod),
            ..Default::default()
        };
        let body = build_create_payment_session_body(&args).unwrap();
        assert_eq!(body["mode"], "SaveMethod");
        assert_eq!(body["amount"].to_string(), "0");
    }

    /// And a vault-only session with money on it is a contradiction.
    #[test]
    fn a_vault_only_session_refuses_a_nonzero_amount() {
        let args = CreatePaymentSessionArgs {
            mode: Some(SessionMode::SaveMethod),
            amount: Some("25.00".parse().unwrap()),
            ..Default::default()
        };
        let err = build_create_payment_session_body(&args)
            .unwrap_err()
            .to_string();
        assert!(err.contains("save-method"), "{err}");
    }

    /// A paying session's zero is the other half of the same rule, and the
    /// message names the flexible alternative because that is what the caller
    /// probably meant.
    #[test]
    fn a_paying_session_refuses_a_zero_amount_and_names_the_alternative() {
        for mode in [
            None,
            Some(SessionMode::Payment),
            Some(SessionMode::PaymentAndSave),
        ] {
            let args = CreatePaymentSessionArgs {
                mode,
                amount: Some(Decimal::ZERO),
                ..Default::default()
            };
            let err = build_create_payment_session_body(&args)
                .unwrap_err()
                .to_string();
            assert!(err.contains("Omit --amount"), "{mode:?}: {err}");
        }
    }

    /// An absent amount on a paying session is a flexible-amount session, so
    /// the key is absent rather than zero.
    #[test]
    fn a_paying_session_with_no_amount_omits_the_key() {
        let args = CreatePaymentSessionArgs {
            mode: Some(SessionMode::Payment),
            ..Default::default()
        };
        let body = build_create_payment_session_body(&args).unwrap();
        assert!(body.get("amount").is_none(), "{body}");
    }

    /// Only the first `=` separates a metadata pair: a query string or a URL
    /// as a value is ordinary rather than exotic.
    #[test]
    fn metadata_splits_on_the_first_equals_only() {
        assert_eq!(
            metadata_pair("query=a=b&c=d").unwrap(),
            ("query".to_string(), "a=b&c=d".to_string())
        );
        assert_eq!(
            metadata_pair("empty=").unwrap(),
            ("empty".to_string(), String::new())
        );
    }

    #[test]
    fn metadata_needs_a_key_and_a_separator() {
        assert!(metadata_pair("orderId").is_err());
        assert!(metadata_pair("=9921").is_err());
    }

    #[test]
    fn repeated_metadata_flags_become_one_object() {
        let args = CreatePaymentSessionArgs {
            metadata: vec!["a=1".into(), "b=2".into()],
            ..Default::default()
        };
        let body = build_create_payment_session_body(&args).unwrap();
        assert_eq!(body["metadata"]["a"], "1");
        assert_eq!(body["metadata"]["b"], "2");
        assert_eq!(body["metadata"].as_object().unwrap().len(), 2);
    }

    /// A processor id implies its method, as on a payment link.
    #[test]
    fn a_processor_id_alone_enables_its_method() {
        let args = CreatePaymentSessionArgs {
            ach_processor_id: Some("pp-ach".into()),
            ..Default::default()
        };
        let body = build_create_payment_session_body(&args).unwrap();
        assert_eq!(body["paymentMethods"]["ach"]["enabled"], true);
        assert!(body["paymentMethods"].get("card").is_none(), "{body}");
    }

    /// Absent is absent, never null.
    #[test]
    fn absent_flags_are_omitted_rather_than_nulled() {
        let body = build_create_payment_session_body(&CreatePaymentSessionArgs::default()).unwrap();
        for key in [
            "mode",
            "amount",
            "tipAmount",
            "customerId",
            "customerHandling",
            "referenceId",
            "returnUrl",
            "skipAddressVerification",
            "pageName",
            "paymentNotes",
            "afterCompletionMessage",
            "expiresAt",
            "metadata",
            "paymentMethods",
        ] {
            assert!(body.get(key).is_none(), "{key} should be absent");
        }
    }
}
