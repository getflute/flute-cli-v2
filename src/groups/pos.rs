//! `pos`: create, get, list, cancel, print-receipt.
//!
//! The group with the only long-poll in the CLI. `--wait` drives **two**
//! independent API controls — `waitForAcceptanceByTerminal` on the create body
//! and `waitForTransactionProcessing` on each get — because a caller waiting
//! for a terminal wants both halves: the first holds the create until the
//! device answers, the second holds each poll open until the state moves.

use crate::Ctx;
use crate::api::ApiError;
use crate::api::ApiPath;
use crate::cli::common::{self, CaptureMethod, PaginationArgs, PricingType};
use crate::cli::money::{self, parse_amount, parse_rate, to_amount_number};
use crate::cli::output::Reported;
use crate::cli::render::{self, Cell, Column, Resource};
use anyhow::Result;
use reqwest::Method;
use rust_decimal::Decimal;
use serde_json::{Map, Value};
use std::time::{Duration, Instant};

/// The status the poll waits out. The enum is exactly `InProgress`,
/// `Completed`, `Cancelled` and `Failed`, so every other value is terminal.
const IN_PROGRESS: &str = "InProgress";

/// How long the loop pauses between polls.
///
/// Each poll already blocks server-side, so this is only a guard against a
/// server that answers instantly — not the thing that paces the wait.
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// The `--wait-timeout` default, and the bound `pos get --wait` is given.
const DEFAULT_WAIT_TIMEOUT_SECS: u64 = 120;

/// Added to the budget each poll and a waiting create are given, so an
/// answer that arrives as the deadline passes is read rather than cut off in
/// transit.
const POLL_TIMEOUT_MARGIN: Duration = Duration::from_secs(5);

/// `Cloud` or `Deeplink`, capitalised on the wire.
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum, serde::Serialize)]
pub enum InitiationChannel {
    Cloud,
    Deeplink,
}

/// `KeyedEntry` or `Regular`, capitalised on the wire.
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum, serde::Serialize)]
pub enum ReadingMethod {
    KeyedEntry,
    Regular,
}

/// The four declared POS transaction statuses, capitalised on the wire.
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum, serde::Serialize)]
pub enum PosTransactionStatus {
    InProgress,
    Completed,
    Cancelled,
    Failed,
}

/// One variant per command, and the arg-bearing ones are large.
///
/// `large_enum_variant` is allowed rather than fixed: boxing a variant breaks
/// `#[derive(Subcommand)]`, which needs the args struct inline, and the enum
/// is constructed exactly once per process.
#[allow(clippy::large_enum_variant)]
#[derive(clap::Subcommand, Debug)]
pub enum PosCommand {
    /// Create a POS transaction (POST /v2/pos/transactions).
    ///
    /// Use `--wait` to long-poll until the terminal completes or rejects the
    /// transaction. Ctrl-C interrupts the create or the poll and exits 130;
    /// during the poll it prints the last-known status.
    Create(CreatePosTransactionArgs),
    /// Fetch a single POS transaction by ID
    /// (GET /v2/pos/transactions/{posTransactionId}).
    Get {
        /// POS transaction UUID to retrieve (positional).
        pos_transaction_id: String,
        /// Hold the response open until the transaction state changes.
        #[arg(long, id = "pos_get_wait")]
        wait: bool,
    },
    /// List POS transactions (GET /v2/pos/transactions).
    List(ListPosTransactionsArgs),
    /// Cancel a POS transaction
    /// (POST /v2/pos/transactions/{posTransactionId}/cancel).
    ///
    /// Requires `--yes` to prevent accidental cancellation.
    Cancel {
        /// POS transaction UUID to cancel (positional).
        pos_transaction_id: String,
        /// Confirm the cancellation (required).
        #[arg(long)]
        yes: bool,
    },
    /// Print a transaction receipt on a terminal
    /// (POST /v2/pos/transactions/{posTransactionId}/print-receipt).
    PrintReceipt {
        /// POS transaction UUID to print a receipt for (positional).
        pos_transaction_id: String,
        /// Terminal UUID to print on (required).
        #[arg(long, id = "receipt_terminal_id", value_name = "TERMINAL_ID")]
        terminal_id: String,
    },
}

#[derive(clap::Args, Debug, Default)]
pub struct CreatePosTransactionArgs {
    /// Terminal UUID to send the transaction to (required).
    #[arg(long)]
    pub terminal_id: String,
    /// POS device ID (required).
    #[arg(long)]
    pub pos_device_id: String,
    /// Transaction amount (required). Plain decimal, e.g. `100.00`.
    #[arg(long = "amount", value_parser = parse_amount, value_name = "AMOUNT", allow_negative_numbers = true)]
    pub base_amount: Decimal,
    /// Currency code, e.g. `USD` (required).
    #[arg(long)]
    pub currency_code: String,
    /// Capture method: `auto` charges immediately, `manual` authorises for
    /// later capture.
    #[arg(long, value_enum, default_value_t = CaptureMethod::Auto)]
    pub capture_method: CaptureMethod,
    /// Initiation channel: `cloud` or `deeplink`. `deeplink` cannot be
    /// combined with `--wait`.
    #[arg(long, value_enum)]
    pub initiation_channel: Option<InitiationChannel>,
    /// Reading method: `keyed-entry` or `regular`. Omit to let the server
    /// decide.
    #[arg(long, value_enum)]
    pub reading_method: Option<ReadingMethod>,
    /// Pricing type: `card` or `cash`, for accounts with dual pricing enabled.
    #[arg(long, value_enum)]
    pub pricing_type: Option<PricingType>,
    /// Payment processor UUID. Defaults to the account's own.
    #[arg(long)]
    pub payment_processor_id: Option<String>,
    /// Customer UUID for vault-linked transactions.
    #[arg(long)]
    pub customer_id: Option<String>,
    /// Merchant-assigned reference ID, part of the duplicate-check key.
    #[arg(long)]
    pub reference_id: Option<String>,
    /// Ask on the terminal whether to save the payment method.
    #[arg(long = "request-storage-consent")]
    pub request_payment_method_storage_consent: bool,
    /// Tip amount. Plain decimal, e.g. `5.00`. At least 0.01.
    #[arg(long, value_parser = parse_amount, allow_negative_numbers = true)]
    pub tip_amount: Option<Decimal>,
    /// Tip rate as a percentage, e.g. `18.5` for 18.5%.
    #[arg(long, value_parser = parse_rate, allow_negative_numbers = true)]
    pub tip_rate: Option<Decimal>,
    /// Long-poll for terminal acceptance, then poll
    /// GET /v2/pos/transactions/{posTransactionId} every 2 seconds until the
    /// transaction leaves `InProgress` or the timeout expires.
    #[arg(long)]
    pub wait: bool,
    /// Seconds to wait for the create and the poll together before giving up
    /// (default 120, at most 86400). Requires `--wait`.
    #[arg(long, requires = "wait", default_value_t = DEFAULT_WAIT_TIMEOUT_SECS, value_parser = clap::value_parser!(u64).range(0..=86_400))]
    pub wait_timeout: u64,
}

#[derive(clap::Args, Debug, Default)]
pub struct ListPosTransactionsArgs {
    #[command(flatten)]
    pub pagination: PaginationArgs,
    /// Sort results by this field name.
    #[arg(long)]
    pub sort_by: Option<String>,
    /// Sort ascending. With neither `--asc` nor `--desc`, the server's default
    /// order applies.
    #[arg(long, id = "pos_asc", conflicts_with = "pos_desc")]
    pub asc: bool,
    /// Sort descending.
    #[arg(long, id = "pos_desc")]
    pub desc: bool,
    /// Filter by terminal UUID.
    #[arg(long, id = "pos_list_terminal_id", value_name = "TERMINAL_ID")]
    pub terminal_id: Option<String>,
    /// Filter results from this date-time inclusive (ISO 8601).
    #[arg(long = "from", value_name = "FROM_DATE")]
    pub from_date: Option<String>,
    /// Filter results up to this date-time inclusive (ISO 8601).
    #[arg(long = "to", value_name = "TO_DATE")]
    pub to_date: Option<String>,
    /// Filter results by status.
    #[arg(long = "status", value_enum, value_name = "POS_TRANSACTION_STATUS")]
    pub pos_transaction_status: Option<PosTransactionStatus>,
}

/// The bounds the schemas declare, refused here rather than spent on a 400.
///
/// The deeplink rule is not a bound but the same kind of saving: the API
/// requires `waitForAcceptanceByTerminal` to be false on that channel, and a
/// caller who asked for both has asked for a contradiction.
pub fn validate_pos_create(args: &CreatePosTransactionArgs) -> Result<()> {
    if args.terminal_id.trim().is_empty() {
        anyhow::bail!(
            "--terminal-id is required: a transaction has to name the terminal to run on"
        );
    }
    if args.pos_device_id.trim().is_empty() {
        anyhow::bail!("--pos-device-id is required");
    }
    if args.currency_code.trim().is_empty() {
        anyhow::bail!("--currency-code is required, e.g. `--currency-code USD`");
    }
    if args.base_amount <= Decimal::ZERO {
        anyhow::bail!("--amount must be greater than zero");
    }
    common::reject_empty_processor_id(
        "--payment-processor-id",
        args.payment_processor_id.as_deref(),
    )?;
    money::refuse_pair(
        ("--tip-amount", args.tip_amount),
        ("--tip-rate", args.tip_rate),
    )?;
    if let Some(tip) = args.tip_amount {
        if tip < Decimal::new(1, 2) {
            anyhow::bail!("--tip-amount must be at least 0.01 (got {tip})");
        }
    }
    if args.wait && args.initiation_channel == Some(InitiationChannel::Deeplink) {
        anyhow::bail!(
            "the API requires waitForAcceptanceByTerminal to be false when \
             initiationChannel is Deeplink, so --wait cannot be combined with \
             --initiation-channel deeplink"
        );
    }
    Ok(())
}

/// Build the `CreatePosTransactionRequestDto` body.
///
/// `waitForAcceptanceByTerminal` is always present, unlike every other
/// optional field: `false` is what "answer immediately" has to say, and the
/// flag that sets it is a switch, so its absence is a decision rather than an
/// unknown.
pub fn build_pos_create_body(args: &CreatePosTransactionArgs) -> Result<Value> {
    validate_pos_create(args)?;

    let mut body = Map::new();
    body.insert("terminalId".into(), Value::String(args.terminal_id.clone()));
    body.insert(
        "posDeviceId".into(),
        Value::String(args.pos_device_id.clone()),
    );
    body.insert("baseAmount".into(), to_amount_number(args.base_amount)?);
    body.insert(
        "currencyCode".into(),
        Value::String(args.currency_code.clone()),
    );
    body.insert("waitForAcceptanceByTerminal".into(), Value::Bool(args.wait));

    body.insert(
        "captureMethod".into(),
        serde_json::json!(args.capture_method),
    );
    if let Some(v) = args.initiation_channel {
        body.insert("initiationChannel".into(), serde_json::json!(v));
    }
    if let Some(v) = args.reading_method {
        body.insert("readingMethod".into(), serde_json::json!(v));
    }
    if let Some(v) = args.pricing_type {
        body.insert("pricingType".into(), serde_json::json!(v));
    }
    common::put_str(&mut body, "paymentProcessorId", &args.payment_processor_id);
    common::put_str(&mut body, "customerId", &args.customer_id);
    common::put_str(&mut body, "referenceId", &args.reference_id);

    // A bare switch cannot distinguish "not passed" from "passed false", so
    // only the true case is sent and the server's default governs otherwise.
    if args.request_payment_method_storage_consent {
        body.insert(
            "requestPaymentMethodStorageConsent".into(),
            Value::Bool(true),
        );
    }
    if let Some(extra) = extra_amounts(args)? {
        body.insert("extraAmounts".into(), extra);
    }
    Ok(Value::Object(body))
}

/// `None` when neither component was supplied, so an absent object is an
/// absent key rather than an empty one the API would have to interpret.
fn extra_amounts(args: &CreatePosTransactionArgs) -> Result<Option<Value>> {
    let mut map = Map::new();
    for (key, value) in [("tipAmount", args.tip_amount), ("tipRate", args.tip_rate)] {
        if let Some(v) = value {
            map.insert(key.to_string(), to_amount_number(v)?);
        }
    }
    Ok((!map.is_empty()).then_some(Value::Object(map)))
}

/// The `PrintTransactionReceiptRequestDto` body: one required field.
pub fn build_print_receipt_body(terminal_id: &str) -> Result<Value> {
    common::reject_empty_id("--terminal-id", terminal_id)?;
    Ok(Value::Object(Map::from_iter([(
        "terminalId".to_string(),
        Value::String(terminal_id.to_string()),
    )])))
}

/// The `GET /v2/pos/transactions` query, omitting every absent flag.
pub fn build_list_pos_transactions_query(
    args: &ListPosTransactionsArgs,
) -> Result<Vec<(&'static str, String)>> {
    args.pagination.validate()?;
    let mut query = args.pagination.query();
    query.extend(common::sort_order(args.asc, args.desc));
    if let Some(status) = args.pos_transaction_status {
        query.push(("posTransactionStatus", common::wire(status)));
    }
    common::push_str(&mut query, "sortBy", &args.sort_by);
    common::push_id(&mut query, "--terminal-id", "terminalId", &args.terminal_id)?;
    common::push_str(&mut query, "fromDate", &args.from_date);
    common::push_str(&mut query, "toDate", &args.to_date);
    Ok(query)
}

/// The declared status of one POS transaction.
///
/// A response carrying none is a malformed response rather than a state to
/// wait out: without it the poll has no stop condition, and treating it as
/// "not yet final" would spend the whole timeout discovering that.
fn declared_status(v: &Value) -> Result<&str, ApiError> {
    v.pointer("/posTransactionStatus")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            ApiError::Decode(
                "a POS transaction response carried no posTransactionStatus, so \
                 there is no status to wait for"
                    .into(),
            )
        })
}

/// How a `--wait` ended.
enum Outcome {
    Settled,
    TimedOut,
    Interrupted,
    /// A poll failed after the create succeeded, so the transaction exists
    /// and `last` is the only record of it the caller has.
    PollFailed(ApiError),
}

struct Waited {
    outcome: Outcome,
    last: Value,
    correlation_id: Option<String>,
}

/// Poll until the transaction leaves `InProgress`, the budget runs out, or
/// the caller interrupts.
///
/// The deadline and the interrupt are selected alongside the request, not only
/// around the pause: each poll blocks server-side, so a budget or a Ctrl-C
/// that only reached the pause is ignored for as long as the API chooses to
/// hold the connection — a `--wait-timeout` of zero included. The request
/// carries the remaining budget as its own bound as well, because the
/// client-wide one is shorter than a caller may ask to wait.
///
/// `deadline` is the one the create was bounded by, so the poll spends only
/// what the create left of the budget.
async fn wait_for_a_terminal_status(
    ctx: &Ctx,
    id: &str,
    deadline: Instant,
    mut last: Value,
    mut correlation_id: Option<String>,
) -> Waited {
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let resp = tokio::select! {
            // `waitForTransactionProcessing` holds the response open until the
            // state moves, which can outlast the client-wide bound.
            r = async {
                ctx.api
                    .request_within(
                        Method::GET,
                        ApiPath::from("/v2/pos/transactions").id(id)?,
                        &[("waitForTransactionProcessing", "true".into())],
                        None,
                        Some(remaining + POLL_TIMEOUT_MARGIN),
                    )
                    .await
            } => r,
            () = tokio::time::sleep(remaining) => return Waited {
                outcome: Outcome::TimedOut, last, correlation_id },
            _ = tokio::signal::ctrl_c() => return Waited {
                outcome: Outcome::Interrupted, last, correlation_id },
        };
        // `last` is replaced only by a body with a status, so a malformed
        // poll leaves the previous record in place for the failure to report.
        let polled = resp.and_then(|r| {
            let body = common::body_of(r.body)?;
            let in_progress = declared_status(&body)? == IN_PROGRESS;
            Ok((r.correlation_id, body, in_progress))
        });
        let in_progress = match polled {
            Ok((polled_id, body, in_progress)) => {
                correlation_id = polled_id.or(correlation_id);
                last = body;
                in_progress
            }
            Err(e) => {
                return Waited {
                    outcome: Outcome::PollFailed(e),
                    last,
                    correlation_id,
                };
            }
        };

        if !in_progress {
            return Waited {
                outcome: Outcome::Settled,
                last,
                correlation_id,
            };
        }
        if Instant::now() >= deadline {
            return Waited {
                outcome: Outcome::TimedOut,
                last,
                correlation_id,
            };
        }
        let nap = POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now()));
        tokio::select! {
            () = tokio::time::sleep(nap) => {}
            _ = tokio::signal::ctrl_c() => return Waited {
                outcome: Outcome::Interrupted, last, correlation_id },
        }
    }
}

/// What a POS transaction is worth saying, in the order it is worth saying
/// it: identity and state, the terminal, the amounts, then the transaction it
/// became.
///
/// The **view's fixed shape** — a pointer the response does not carry still
/// holds its row, with a dash.
pub static POS_TRANSACTION: Resource = Resource {
    object: "pos_transaction",
    object_list: "pos_transaction_list",
    id: "/posTransactionId",
    detail: &[
        "/posTransactionId",
        "/posTransactionStatus",
        "/terminalId",
        "/baseAmount",
        "/processedAmount",
        "/transactionId",
    ],
    // The list item declares neither a transaction type nor an
    // `isCompleted`, so the table shows neither rather than inventing them,
    // and the creation timestamp fills the last column.
    columns: &[
        Column {
            header: "ID",
            width: 36,
            cell: Cell::Path("/posTransactionId"),
        },
        Column {
            header: "TERMINAL ID",
            width: 36,
            cell: Cell::Path("/terminalId"),
        },
        Column {
            header: "STATUS",
            width: 28,
            cell: Cell::Path("/posTransactionStatus"),
        },
        Column {
            header: "AMOUNT",
            width: 12,
            cell: Cell::Path("/processedAmount"),
        },
        Column {
            header: "CREATED",
            width: 10,
            cell: Cell::Derived(|v| render::date_only(v, "/createdOn")),
        },
    ],
    amounts: &[
        "/baseAmount",
        "/processedAmount",
        "/extraAmounts/tipAmount",
        "/linkedTransaction/processedAmount",
    ],
    yes_no: &[],
};

pub async fn dispatch(ctx: &Ctx, command: PosCommand) -> Result<()> {
    match command {
        PosCommand::Create(args) => create(ctx, args).await,
        PosCommand::Get {
            pos_transaction_id,
            wait,
        } => {
            // Omitted rather than sent as `false`, so the server's declared
            // default governs.
            let query: Vec<(&str, String)> = if wait {
                vec![("waitForTransactionProcessing", "true".into())]
            } else {
                vec![]
            };
            // `waitForTransactionProcessing` holds the response open until the
            // state moves, which can outlast the client-wide bound, so the
            // long poll gets the default wait budget a create's poll gets.
            let bound = get_wait_bound(wait);
            let resp = ctx
                .api
                .request_within(
                    Method::GET,
                    ApiPath::from("/v2/pos/transactions").id(&pos_transaction_id)?,
                    &query,
                    None,
                    bound,
                )
                .await?;
            render::one(
                ctx,
                &POS_TRANSACTION,
                &common::body_of(resp.body)?,
                resp.correlation_id,
            )
        }
        PosCommand::List(args) => {
            let query = build_list_pos_transactions_query(&args)?;
            common::list(
                ctx,
                &POS_TRANSACTION,
                "/v2/pos/transactions",
                &query,
                &args.pagination,
            )
            .await
        }
        PosCommand::Cancel {
            pos_transaction_id, ..
        } => {
            // A cancelled transaction still exists and refuses a repeat with a
            // 400, so a 404 is an id the server never had: plain not-found.
            let resp = ctx
                .api
                .request(
                    Method::POST,
                    ApiPath::from("/v2/pos/transactions")
                        .id(&pos_transaction_id)?
                        .seg("cancel"),
                    &[],
                    None,
                )
                .await?;
            render::one(
                ctx,
                &POS_TRANSACTION,
                &common::body_of(resp.body)?,
                resp.correlation_id,
            )
        }
        PosCommand::PrintReceipt {
            pos_transaction_id,
            terminal_id,
        } => {
            let body = build_print_receipt_body(&terminal_id)?;
            let resp = ctx
                .api
                .request(
                    Method::POST,
                    ApiPath::from("/v2/pos/transactions")
                        .id(&pos_transaction_id)?
                        .seg("print-receipt"),
                    &[],
                    Some(body),
                )
                .await?;
            // 200 with no body, so the confirmation comes from the request.
            render::confirmed(
                ctx,
                &POS_TRANSACTION,
                &pos_transaction_id,
                // The empty 200 means the request was accepted, not that
                // anything printed: an idle printer answers the same.
                "sent",
                &format!(
                    "Sent receipt for POS transaction {pos_transaction_id} to terminal {terminal_id}."
                ),
                resp.correlation_id,
            )
        }
    }
}

async fn create(ctx: &Ctx, args: CreatePosTransactionArgs) -> Result<()> {
    let body = build_pos_create_body(&args)?;
    money::note_fractional_rates(&[("--tip-rate", args.tip_rate)]);
    // One deadline for the create and the poll together. The argument is
    // bounded, so this cannot overflow; a deadline the clock cannot represent
    // is an immediate timeout rather than a panic.
    let deadline = Instant::now()
        .checked_add(Duration::from_secs(args.wait_timeout))
        .unwrap_or_else(Instant::now);
    let bound = args
        .wait
        .then(|| deadline.saturating_duration_since(Instant::now()) + POLL_TIMEOUT_MARGIN);
    // `waitForAcceptanceByTerminal` holds the create open until the terminal
    // answers, which can outlast the client-wide bound. The interrupt is
    // selected first so its handler is in place before the request is sent:
    // a Ctrl-C while the terminal waits for a card would otherwise kill the
    // process with nothing said about a transaction that may be live.
    let resp = tokio::select! {
        biased;
        _ = tokio::signal::ctrl_c() => {
            eprintln!(
                "Interrupted while creating the POS transaction. It may exist on the \
                 terminal: reconcile with `pos list` before creating another."
            );
            return Err(Reported { code: 130 }.into());
        }
        r = ctx
            .api
            .request_within(Method::POST, "/v2/pos/transactions", &[], Some(body), bound) => r?,
    };
    let created = common::body_of(resp.body)?;
    if !args.wait {
        return render::one(ctx, &POS_TRANSACTION, &created, resp.correlation_id);
    }

    // A create that already answered with a terminal status has nothing left
    // to poll for; `waitForAcceptanceByTerminal` can settle it outright.
    if declared_status(&created)? != IN_PROGRESS {
        return render::one(ctx, &POS_TRANSACTION, &created, resp.correlation_id);
    }
    let id = render::id_of(&POS_TRANSACTION, &created).ok_or_else(|| {
        ApiError::Decode("the create response carried no posTransactionId to poll".into())
    })?;

    let waited = wait_for_a_terminal_status(ctx, &id, deadline, created, resp.correlation_id).await;

    match waited.outcome {
        Outcome::Settled => render::one(ctx, &POS_TRANSACTION, &waited.last, waited.correlation_id),
        Outcome::TimedOut => {
            render::one(ctx, &POS_TRANSACTION, &waited.last, waited.correlation_id)?;
            eprintln!(
                "Warning: --wait-timeout ({}s) expired; last status shown above.",
                args.wait_timeout
            );
            Err(Reported { code: 1 }.into())
        }
        Outcome::PollFailed(err) => {
            render::one(ctx, &POS_TRANSACTION, &waited.last, waited.correlation_id)?;
            eprintln!(
                "Error: polling POS transaction {id} failed: {err}; last status shown above."
            );
            Err(Reported {
                code: crate::cli::output::exit_code_for(&err.into()),
            }
            .into())
        }
        // Nothing to stdout, in any output mode: the transaction is still
        // running, so there is no result to report.
        Outcome::Interrupted => {
            let status = declared_status(&waited.last)?;
            eprintln!("Interrupted. Last known status: {status} (id: {id})");
            Err(Reported { code: 130 }.into())
        }
    }
}

/// The bound on a `get --wait` long poll: the default wait budget plus the
/// poll margin, which outlasts the client-wide request timeout on purpose.
fn get_wait_bound(wait: bool) -> Option<Duration> {
    wait.then(|| Duration::from_secs(DEFAULT_WAIT_TIMEOUT_SECS) + POLL_TIMEOUT_MARGIN)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal() -> CreatePosTransactionArgs {
        CreatePosTransactionArgs {
            terminal_id: "term-1".into(),
            pos_device_id: "POS-DEVICE-001".into(),
            base_amount: "42.75".parse().unwrap(),
            currency_code: "USD".into(),
            wait_timeout: 120,
            ..Default::default()
        }
    }

    /// The four required fields, and the two optional ones that are always
    /// present: `false` is a decision, and the capture method defaults.
    #[test]
    fn builds_a_minimal_body_with_the_required_fields_and_the_wait_control() {
        let body = build_pos_create_body(&minimal()).unwrap();
        assert_eq!(
            body,
            serde_json::json!({
                "terminalId": "term-1",
                "posDeviceId": "POS-DEVICE-001",
                "baseAmount": serde_json::from_str::<serde_json::Number>("42.75").unwrap(),
                "currencyCode": "USD",
                "captureMethod": "Auto",
                "waitForAcceptanceByTerminal": false})
        );
    }

    /// An empty processor id is a value that went missing, such as an unset
    /// shell variable, and sending the sale without it would run it through
    /// the account's default processor instead of the one named.
    #[test]
    fn an_empty_processor_id_is_refused() {
        let args = CreatePosTransactionArgs {
            payment_processor_id: Some(String::new()),
            ..minimal()
        };
        let err = build_pos_create_body(&args).unwrap_err().to_string();
        assert!(err.contains("--payment-processor-id"), "{err}");
        assert!(err.contains("settings payment-config"), "{err}");
    }

    #[test]
    fn every_create_flag_reaches_the_body_under_its_wire_name() {
        let args = CreatePosTransactionArgs {
            capture_method: CaptureMethod::Manual,
            initiation_channel: Some(InitiationChannel::Cloud),
            reading_method: Some(ReadingMethod::KeyedEntry),
            pricing_type: Some(PricingType::Cash),
            payment_processor_id: Some("pp-1".into()),
            customer_id: Some("cus-1".into()),
            reference_id: Some("REF-1".into()),
            request_payment_method_storage_consent: true,
            tip_amount: Some("5.00".parse().unwrap()),
            ..minimal()
        };
        let body = build_pos_create_body(&args).unwrap();
        assert_eq!(body["captureMethod"], "Manual");
        assert_eq!(body["initiationChannel"], "Cloud");
        assert_eq!(body["readingMethod"], "KeyedEntry");
        assert_eq!(body["pricingType"], "Cash");
        assert_eq!(body["paymentProcessorId"], "pp-1");
        assert_eq!(body["customerId"], "cus-1");
        assert_eq!(body["referenceId"], "REF-1");
        assert_eq!(body["requestPaymentMethodStorageConsent"], true);
        assert_eq!(body["extraAmounts"]["tipAmount"].to_string(), "5.00");
    }

    /// Absent is absent, never null: the schema sets
    /// `additionalProperties: false` and the API distinguishes the two.
    #[test]
    fn an_absent_create_flag_is_omitted_rather_than_nulled() {
        let body = build_pos_create_body(&minimal()).unwrap();
        for key in [
            "initiationChannel",
            "readingMethod",
            "pricingType",
            "paymentProcessorId",
            "customerId",
            "referenceId",
            "requestPaymentMethodStorageConsent",
            "extraAmounts",
        ] {
            assert!(body.get(key).is_none(), "{key} should be absent");
        }
    }

    /// One tip component is enough to earn the container; neither means no
    /// container at all.
    #[test]
    fn extra_amounts_appears_only_when_a_tip_is_given() {
        let mut args = minimal();
        args.tip_rate = Some("15.0".parse().unwrap());
        let body = build_pos_create_body(&args).unwrap();
        assert!(body["extraAmounts"].get("tipAmount").is_none());
        assert_eq!(body["extraAmounts"]["tipRate"].to_string(), "15.0");
    }

    #[test]
    fn the_wait_flag_sets_the_acceptance_control() {
        let mut args = minimal();
        args.wait = true;
        let body = build_pos_create_body(&args).unwrap();
        assert_eq!(body["waitForAcceptanceByTerminal"], true);
    }

    #[test]
    fn rejects_a_zero_amount() {
        let mut args = minimal();
        args.base_amount = Decimal::ZERO;
        assert!(build_pos_create_body(&args).is_err());
    }

    /// The declared minimum on `extraAmounts.tipAmount`.
    #[test]
    fn rejects_a_tip_amount_below_the_declared_minimum() {
        let mut args = minimal();
        args.tip_amount = Some(Decimal::ZERO);
        let err = build_pos_create_body(&args).unwrap_err().to_string();
        assert!(err.contains("--tip-amount"), "{err}");
    }

    /// An amount and a rate set the same tip, and the API refuses both.
    #[test]
    fn a_tip_given_as_both_an_amount_and_a_rate_is_refused() {
        let mut args = minimal();
        args.tip_amount = Some("1.50".parse().unwrap());
        args.tip_rate = Some("15".parse().unwrap());
        let err = build_pos_create_body(&args).unwrap_err().to_string();
        assert!(
            err.contains("--tip-amount or --tip-rate, not both"),
            "{err}"
        );
    }

    /// The API checks presence, so a zero rate beside an amount is still both.
    #[test]
    fn a_tip_amount_beside_a_zero_rate_is_refused() {
        let mut args = minimal();
        args.tip_amount = Some("1.50".parse().unwrap());
        args.tip_rate = Some(Decimal::ZERO);
        let err = build_pos_create_body(&args).unwrap_err().to_string();
        assert!(
            err.contains("--tip-amount or --tip-rate, not both"),
            "{err}"
        );
    }

    /// The API requires the acceptance wait to be false on a deeplink, so the
    /// pair is a contradiction rather than a preference.
    #[test]
    fn wait_with_a_deeplink_channel_is_refused() {
        let mut args = minimal();
        args.wait = true;
        args.initiation_channel = Some(InitiationChannel::Deeplink);
        let err = build_pos_create_body(&args).unwrap_err().to_string();
        assert!(err.contains("Deeplink"), "{err}");

        // Either half alone is fine.
        args.wait = false;
        assert!(build_pos_create_body(&args).is_ok());
    }

    fn list_args() -> ListPosTransactionsArgs {
        ListPosTransactionsArgs::default()
    }

    /// An empty terminal id names no terminal, and dropping it would answer
    /// with every terminal's transactions.
    #[test]
    fn an_empty_terminal_id_filter_is_refused() {
        let mut args = list_args();
        args.terminal_id = Some(String::new());
        let err = build_list_pos_transactions_query(&args)
            .unwrap_err()
            .to_string();
        assert!(err.contains("--terminal-id needs a value"), "{err}");
    }

    /// No flags means no query: the server's declared defaults govern.
    #[test]
    fn an_unfiltered_pos_list_sends_nothing() {
        assert!(
            build_list_pos_transactions_query(&list_args())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn every_pos_list_filter_reaches_the_query_under_its_wire_name() {
        let args = ListPosTransactionsArgs {
            pagination: PaginationArgs {
                page_index: Some(1),
                page_size: Some(5),
                all: false,
            },
            sort_by: Some("createdOn".into()),
            asc: false,
            desc: true,
            terminal_id: Some("term-1".into()),
            from_date: Some("2026-01-01T00:00:00Z".into()),
            to_date: Some("2026-12-31T23:59:59Z".into()),
            pos_transaction_status: Some(PosTransactionStatus::Completed),
        };
        let q = build_list_pos_transactions_query(&args).unwrap();
        let names: Vec<&str> = q.iter().map(|(k, _)| *k).collect();
        assert_eq!(
            names,
            [
                "pageIndex",
                "pageSize",
                "sortOrder",
                "posTransactionStatus",
                "sortBy",
                "terminalId",
                "fromDate",
                "toDate"
            ]
        );
        assert_eq!(
            q.iter()
                .find(|(k, _)| *k == "posTransactionStatus")
                .unwrap()
                .1,
            "Completed"
        );
    }

    /// Each direction is sent explicitly, and neither flag sends nothing.
    #[test]
    fn pos_list_sends_the_sort_order_it_is_asked_for() {
        let order = |asc, desc| {
            let mut args = list_args();
            args.asc = asc;
            args.desc = desc;
            build_list_pos_transactions_query(&args)
                .unwrap()
                .into_iter()
                .find(|(k, _)| *k == "sortOrder")
                .map(|(_, v)| v)
        };
        assert_eq!(order(true, false).as_deref(), Some("asc"));
        assert_eq!(order(false, true).as_deref(), Some("desc"));
        assert_eq!(order(false, false), None);
    }

    #[test]
    fn the_print_receipt_body_carries_the_terminal_under_its_wire_name() {
        assert_eq!(
            build_print_receipt_body("term-1").unwrap(),
            serde_json::json!({"terminalId": "term-1"})
        );
        assert!(build_print_receipt_body("").is_err());
    }

    /// The poll's stop condition. Every declared status but `InProgress` is
    /// terminal, including the two that are failures.
    #[test]
    fn only_in_progress_is_a_status_worth_waiting_out() {
        for status in ["Completed", "Cancelled", "Failed"] {
            let v = serde_json::json!({"posTransactionStatus": status});
            assert_ne!(declared_status(&v).unwrap(), IN_PROGRESS, "{status}");
        }
        let v = serde_json::json!({"posTransactionStatus": IN_PROGRESS});
        assert_eq!(declared_status(&v).unwrap(), IN_PROGRESS);
    }

    /// A response with no status is malformed, not "not yet finished":
    /// waiting it out would spend the whole timeout learning nothing.
    #[test]
    fn a_response_with_no_status_is_a_decode_error() {
        let err = declared_status(&serde_json::json!({"posTransactionId": "p-1"})).unwrap_err();
        assert!(matches!(err, ApiError::Decode(_)), "{err}");
    }

    /// `get --wait` passes its own bound, and that bound outlasts the
    /// client-wide request timeout; without `--wait` the client-wide bound
    /// governs. The client's own tests prove a request's bound wins over the
    /// client-wide one.
    #[test]
    fn get_wait_is_bounded_beyond_the_client_wide_timeout() {
        let bound = get_wait_bound(true).expect("a bound with --wait");
        assert!(bound > crate::api::client::REQUEST_TIMEOUT, "{bound:?}");
        assert_eq!(get_wait_bound(false), None);
    }
}
