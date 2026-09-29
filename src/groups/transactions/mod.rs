//! `transactions`: every command under the group, dispatched from here.

use crate::Ctx;
use crate::api::ApiPath;
use crate::cli::common::{self};
use crate::cli::money::{self};
use crate::cli::output::OutputFormat;
use crate::cli::render;
use anyhow::Result;
use reqwest::Method;
use serde_json::Value;

mod args;
mod create;
mod requests;
mod tables;

pub use args::*;
pub use create::*;
pub use requests::*;
pub use tables::*;

pub async fn dispatch(ctx: &Ctx, command: TransactionsCommand) -> Result<()> {
    match command {
        TransactionsCommand::Create(args) => {
            let body = build_create_transaction_body(&args)?;
            money::note_fractional_rates(&[
                ("--tip-rate", args.tip_rate),
                ("--discount-rate", args.discount_rate),
                ("--surcharge-rate", args.surcharge_rate),
                ("--l2-tax-rate", args.sales_tax_rate),
            ]);
            let resp = ctx
                .api
                .request(Method::POST, "/v2/transactions", &[], Some(body))
                .await?;
            let data = unwrap_single_transaction(common::body_of(resp.body)?)?;
            render_transaction(ctx, &data, resp.correlation_id)
        }
        TransactionsCommand::Get { transaction_id } => {
            let resp = ctx
                .api
                .request(
                    Method::GET,
                    ApiPath::from("/v2/transactions").id(&transaction_id)?,
                    &[],
                    None,
                )
                .await?;
            render_transaction(ctx, &common::body_of(resp.body)?, resp.correlation_id)
        }
        // No endpoint of its own: the same read, curated.
        TransactionsCommand::Inspect { transaction_id } => {
            let resp = ctx
                .api
                .request(
                    Method::GET,
                    ApiPath::from("/v2/transactions").id(&transaction_id)?,
                    &[],
                    None,
                )
                .await?;
            let data = common::body_of(resp.body)?;
            match ctx.output {
                OutputFormat::Table => {
                    println!("{}", inspect_table(&data));
                    Ok(())
                }
                // The API's own data, as `get` reports it: curating JSON
                // would make two commands disagree about what the API said.
                _ => render::one(ctx, &TRANSACTION, &data, resp.correlation_id),
            }
        }
        TransactionsCommand::List(args) => {
            let query = build_list_transactions_query(&args)?;
            common::list(
                ctx,
                &TRANSACTION,
                "/v2/transactions",
                &query,
                &args.pagination,
            )
            .await
        }
        TransactionsCommand::Capture {
            transaction_id,
            amount,
        } => {
            let body = build_capture_body(amount)?;
            action(ctx, &transaction_id, "capture", body).await
        }
        TransactionsCommand::Reversal {
            transaction_id,
            amount,
        } => {
            let body = build_reversal_body(amount)?;
            // The state can still change between this read and the reversal;
            // the check stops the common case, not a race.
            if amount.is_some() {
                let resp = ctx
                    .api
                    .request(
                        Method::GET,
                        ApiPath::from("/v2/transactions").id(&transaction_id)?,
                        &[],
                        None,
                    )
                    .await?;
                refuse_a_partial_reversal_the_api_ignores(&common::body_of(resp.body)?)?;
            }
            action(ctx, &transaction_id, "reversal", body).await
        }
        TransactionsCommand::TipAdjust {
            transaction_id,
            tip_amount,
            tip_rate,
        } => {
            money::note_fractional_rates(&[("--tip-rate", tip_rate)]);
            let body = build_tip_adjustment_body(tip_amount, tip_rate)?;
            action(ctx, &transaction_id, "tip-adjustment", Some(body)).await
        }
        TransactionsCommand::AchHold { transaction_id } => {
            action(ctx, &transaction_id, "ach-hold", None).await
        }
        TransactionsCommand::AchRelease { transaction_id } => {
            action(ctx, &transaction_id, "ach-release", None).await
        }
        TransactionsCommand::ShareReceipt(args) => {
            let body = build_share_receipt_body(&args)?;
            let resp = ctx
                .api
                .request(
                    Method::POST,
                    ApiPath::from("/v2/transactions")
                        .id(&args.transaction_id)?
                        .seg("share-receipt"),
                    &[],
                    Some(body),
                )
                .await?;
            // 200 with no body, so the confirmation comes from the request.
            render::confirmed(
                ctx,
                &TRANSACTION,
                &args.transaction_id,
                "shared",
                &format!("Shared receipt for transaction {}.", args.transaction_id),
                resp.correlation_id,
            )
        }
        TransactionsCommand::Credit(args) => {
            let body = build_credit_body(&args)?;
            let resp = ctx
                .api
                .request(Method::POST, "/v2/transactions/credit", &[], Some(body))
                .await?;
            let data = unwrap_single_transaction(common::body_of(resp.body)?)?;
            render_transaction(ctx, &data, resp.correlation_id)
        }
        TransactionsCommand::CalculateAmount(args) => {
            money::note_fractional_rates(&[
                ("--tip-rate", args.tip_rate),
                ("--discount-rate", args.discount_rate),
                ("--surcharge-rate", args.surcharge_rate),
            ]);
            let body = build_calculate_amount_body(&args)?;
            let resp = ctx
                .api
                .request(
                    Method::POST,
                    "/v2/transactions/calculate-amount",
                    &[],
                    Some(body),
                )
                .await?;
            render::one(
                ctx,
                &AMOUNT_CALCULATION,
                &common::body_of(resp.body)?,
                resp.correlation_id,
            )
        }
    }
}

/// The five lifecycle verbs share a request and a response: one POST under
/// the transaction, answered with that transaction in the read's shape
/// rather than the declared page, rendered like `get`.
async fn action(
    ctx: &Ctx,
    transaction_id: &str,
    verb: &'static str,
    body: Option<Value>,
) -> Result<()> {
    let resp = ctx
        .api
        .request(
            Method::POST,
            ApiPath::from("/v2/transactions")
                .id(transaction_id)?
                .seg(verb),
            &[],
            body,
        )
        .await?;
    let data = unwrap_single_transaction(common::body_of(resp.body)?)?;
    note_assigned_reference(verb, &data);
    render_transaction(ctx, &data, resp.correlation_id)
}

/// The stderr line an action earns when the API answers with a `referenceId`
/// of its own.
///
/// The hold, the release and a reversal on the ACH side each mint one, and the
/// merchant's reference stops selecting the transaction — so a caller told to
/// reconcile before re-issuing has to reconcile by transaction id. The action
/// is given only that id, never the reference the transaction was created
/// with, so the note says whose the returned value is rather than claiming a
/// comparison. A card reversal keeps its reference and earns nothing.
fn note_assigned_reference(verb: &str, data: &Value) {
    let ach = data.get("paymentMethodType").and_then(Value::as_str) == Some("ACH");
    if matches!(verb, "ach-hold" | "ach-release") || (verb == "reversal" && ach) {
        eprintln!(
            "`{verb}` assigns its own `referenceId`; the value in this answer is the API's, \
             not the merchant reference. Reconcile by transaction id."
        );
    }
}
