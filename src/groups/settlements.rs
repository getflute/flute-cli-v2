//! `settlements`: list, get, close.
//!
//! `get` has **no endpoint of its own.** It is the list with the documented
//! `batchIds` filter applied server-side — not page zero fetched and scanned,
//! which would report a batch beyond the first page as missing. That is a
//! wrong answer rather than a slow one, so an empty result here is exit 4.

use crate::Ctx;
use crate::cli::common::{self, PaginationArgs};
use crate::cli::render::{self, Cell, Column, Resource};
use anyhow::Result;
use reqwest::Method;
use serde_json::{Map, Value};

/// The four statuses a batch reports.
///
/// The **query** parameter declares no enum at all, so this vocabulary comes
/// from the response field of the same name. Offering it as an enum rather
/// than a free string is what turns a misspelling into a rejected flag
/// instead of a silently empty page.
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum BatchStatus {
    Open,
    PendingSettlement,
    Settled,
    Declined,
}

impl BatchStatus {
    pub fn wire(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::PendingSettlement => "PendingSettlement",
            Self::Settled => "Settled",
            Self::Declined => "Declined",
        }
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(clap::Subcommand, Debug)]
pub enum SettlementsCommand {
    /// List settlement batches (GET /v2/settlements/batches).
    List(ListBatchesArgs),
    /// Fetch a single settlement batch by ID.
    ///
    /// There is no single-batch endpoint. This reads the batch list
    /// under its own `batchIds` filter and returns the one match.
    Get {
        /// Settlement batch ID (positional).
        batch_id: String,
    },
    /// Settle the open batch for a payment processor
    /// (POST /v2/settlements/batches/close).
    ///
    /// This is a batch-level operation, not a per-transaction
    /// one. It closes and settles the processor's entire open batch. Use
    /// `--payment-processor-id` to identify the processor.
    Close {
        /// UUID of the payment processor whose open batch should be settled
        /// (required).
        #[arg(long)]
        payment_processor_id: String,
    },
}

#[derive(clap::Args, Debug, Default)]
pub struct ListBatchesArgs {
    #[command(flatten)]
    pub pagination: PaginationArgs,
    /// Sort results by this field name.
    #[arg(long, id = "batch_sort_by", value_name = "SORT_BY")]
    pub sort_by: Option<String>,
    // The one list whose server-side default is `desc`, so the flag here
    // names the opposite direction from every other group's.
    /// Sort ascending. The default here is descending.
    #[arg(long)]
    pub asc: bool,
    /// Filter results created from this date-time inclusive (ISO 8601).
    #[arg(long = "from", id = "batch_from", value_name = "FROM_DATE")]
    pub from_date: Option<String>,
    /// Filter results created up to this date-time inclusive (ISO 8601).
    #[arg(long = "to", id = "batch_to", value_name = "TO_DATE")]
    pub to_date: Option<String>,
    /// Filter by settlement batch ID. Repeat the flag for several batches.
    #[arg(long = "batch-ids", value_name = "BATCH_ID")]
    pub batch_ids: Vec<String>,
    /// Filter by payment processor UUID. Repeat the flag for several
    /// processors.
    #[arg(long = "processor-ids", value_name = "PAYMENT_PROCESSOR_ID")]
    pub payment_processor_ids: Vec<String>,
    /// Filter results by status.
    #[arg(
        long = "status",
        value_enum,
        id = "batch_status",
        value_name = "BATCH_STATUS"
    )]
    pub batch_status: Option<BatchStatus>,
}

/// The `GET /v2/settlements/batches` query, omitting every absent flag.
///
/// An array parameter becomes one pair per value, which is the OpenAPI
/// default for a `form`-style array. A comma-joined single value is the other
/// possibility and nothing in the bundle chooses between them.
pub fn build_list_batches_query(args: &ListBatchesArgs) -> Result<Vec<(&'static str, String)>> {
    args.pagination.validate()?;
    let mut query = args.pagination.query();
    if args.asc {
        query.push(("sortOrder", "asc".into()));
    }
    let mut put = |key: &'static str, value: &Option<String>| {
        if let Some(v) = value.as_ref().filter(|s| !s.is_empty()) {
            query.push((key, v.clone()));
        }
    };
    put("sortBy", &args.sort_by);
    put("fromDate", &args.from_date);
    put("toDate", &args.to_date);
    for id in args.batch_ids.iter().filter(|s| !s.is_empty()) {
        query.push(("batchIds", id.clone()));
    }
    for id in args.payment_processor_ids.iter().filter(|s| !s.is_empty()) {
        query.push(("paymentProcessorIds", id.clone()));
    }
    if let Some(status) = args.batch_status {
        query.push(("batchStatus", status.wire().into()));
    }
    Ok(query)
}

/// The `SettleTransactionsRequestDto` body: one required field.
pub fn build_close_batch_body(payment_processor_id: &str) -> Result<Value> {
    common::reject_empty_processor_id("--payment-processor-id", Some(payment_processor_id))?;
    Ok(Value::Object(Map::from_iter([(
        "paymentProcessorId".to_string(),
        Value::String(payment_processor_id.to_string()),
    )])))
}

/// Pick the one batch a `get` asked for out of what the filter returned.
///
/// More than one match would mean `batchIds` is not the identity the command
/// is using it as, so it says so rather than picking the first — and none is
/// a not-found read, which is exit 4 through the API error path.
fn one_batch(batch_id: &str, items: Vec<Value>) -> Result<Value> {
    match items.len() {
        0 => Err(crate::api::ApiError::Api {
            status: 404,
            correlation_id: None,
            message: format!("no settlement batch {batch_id}"),
        }
        .into()),
        1 => Ok(items.into_iter().next().expect("length checked")),
        n => anyhow::bail!(
            "the batchIds filter returned {n} batches for {batch_id}, so it is \
             not identifying one batch"
        ),
    }
}

/// What a settlement batch is worth saying: identity, then the processor,
/// then the money.
pub static SETTLEMENT: Resource = Resource {
    object: "settlement",
    object_list: "settlement_list",
    id: "/batchId",
    detail: &[
        "/batchId",
        "/externalBatchId",
        "/batchStatus",
        "/paymentProcessorId",
        "/paymentProcessorName",
        "/createdOn",
        "/transactionCount",
        "/totalSalesAmount",
        "/totalRefundsAmount",
        "/totalNetAmount",
    ],
    columns: &[
        Column {
            header: "ID",
            width: 36,
            cell: Cell::Path("/batchId"),
        },
        Column {
            header: "PROCESSOR",
            width: 20,
            cell: Cell::Path("/paymentProcessorName"),
        },
        Column {
            header: "BATCH DATE",
            width: 10,
            cell: Cell::Derived(|v| render::date_only(v, "/createdOn")),
        },
        Column {
            header: "TXNS",
            width: 6,
            cell: Cell::Path("/transactionCount"),
        },
        Column {
            header: "SALES",
            width: 12,
            cell: Cell::Path("/totalSalesAmount"),
        },
        Column {
            header: "REFUNDS",
            width: 12,
            cell: Cell::Path("/totalRefundsAmount"),
        },
        Column {
            header: "NET",
            width: 12,
            cell: Cell::Path("/totalNetAmount"),
        },
        Column {
            header: "STATUS",
            width: 10,
            cell: Cell::Path("/batchStatus"),
        },
    ],
    amounts: &[
        "/totalSalesAmount",
        "/totalRefundsAmount",
        "/totalNetAmount",
    ],
    yes_no: &[],
};

/// `SettleTransactionsResponseDto` declares one property, `batchStatus`, and
/// carries no batch id — so a close cannot be rendered as a settlement, and
/// the resulting status is the only thing `quiet` has to print.
pub static BATCH_CLOSURE: Resource = Resource {
    object: "batch_closure",
    object_list: "batch_closures",
    id: "/batchStatus",
    detail: &["/batchStatus"],
    columns: &[Column {
        header: "STATUS",
        width: 18,
        cell: Cell::Path("/batchStatus"),
    }],
    amounts: &[],
    yes_no: &[],
};

pub async fn dispatch(ctx: &Ctx, command: SettlementsCommand) -> Result<()> {
    match command {
        SettlementsCommand::List(args) => {
            let query = build_list_batches_query(&args)?;
            if args.pagination.all {
                let (items, correlation_id) = common::fetch_all(
                    &ctx.api,
                    "/v2/settlements/batches",
                    &query,
                    args.pagination.page_size,
                )
                .await?;
                render::page(ctx, &SETTLEMENT, &items, None, correlation_id)
            } else {
                let resp = ctx
                    .api
                    .request(Method::GET, "/v2/settlements/batches", &query, None)
                    .await?;
                let body = common::body_of(resp.body)?;
                render::page(
                    ctx,
                    &SETTLEMENT,
                    &common::items_of(&body)?,
                    body.get("pageInfo").cloned(),
                    resp.correlation_id,
                )
            }
        }
        SettlementsCommand::Get { batch_id } => {
            // There is no single-batch endpoint, only the list's id filter.
            common::reject_empty_id("<BATCH_ID>", &batch_id)?;
            let resp = ctx
                .api
                .request(
                    Method::GET,
                    "/v2/settlements/batches",
                    &[("batchIds", batch_id.clone())],
                    None,
                )
                .await?;
            let body = common::body_of(resp.body)?;
            let batch = one_batch(&batch_id, common::items_of(&body)?)?;
            render::one(ctx, &SETTLEMENT, &batch, resp.correlation_id)
        }
        SettlementsCommand::Close {
            payment_processor_id,
        } => {
            let body = build_close_batch_body(&payment_processor_id)?;
            let resp = ctx
                .api
                .request(
                    Method::POST,
                    "/v2/settlements/batches/close",
                    &[],
                    Some(body),
                )
                .await?;
            render::one(
                ctx,
                &BATCH_CLOSURE,
                &common::body_of(resp.body)?,
                resp.correlation_id,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list_args() -> ListBatchesArgs {
        ListBatchesArgs::default()
    }

    /// No flags means no query: the server's declared defaults govern,
    /// including a `sortOrder` of `desc`.
    #[test]
    fn an_unfiltered_batch_list_sends_nothing() {
        assert!(build_list_batches_query(&list_args()).unwrap().is_empty());
    }

    #[test]
    fn every_batch_filter_reaches_the_query_under_its_wire_name() {
        let args = ListBatchesArgs {
            pagination: PaginationArgs {
                page_index: Some(1),
                page_size: Some(5),
                all: false,
            },
            sort_by: Some("createdOn".into()),
            asc: true,
            from_date: Some("2026-01-01T00:00:00Z".into()),
            to_date: Some("2026-12-31T23:59:59Z".into()),
            batch_ids: vec!["b-1".into(), "b-2".into()],
            payment_processor_ids: vec!["pp-1".into()],
            batch_status: Some(BatchStatus::Open),
        };
        let q = build_list_batches_query(&args).unwrap();
        let names: Vec<&str> = q.iter().map(|(k, _)| *k).collect();
        assert_eq!(
            names,
            [
                "pageIndex",
                "pageSize",
                "sortOrder",
                "sortBy",
                "fromDate",
                "toDate",
                "batchIds",
                "batchIds",
                "paymentProcessorIds",
                "batchStatus"
            ]
        );
        assert_eq!(q.iter().find(|(k, _)| *k == "sortOrder").unwrap().1, "asc");
    }

    /// An array parameter is one pair per value, not one comma-joined value.
    /// Both are legal OpenAPI serialisations and the bundle chooses neither,
    /// so the choice is stated here and asserted.
    #[test]
    fn an_array_filter_becomes_one_pair_per_value() {
        let mut args = list_args();
        args.batch_ids = vec!["b-1".into(), "b-2".into(), "b-3".into()];
        let q = build_list_batches_query(&args).unwrap();
        assert_eq!(q.iter().filter(|(k, _)| *k == "batchIds").count(), 3);
        assert!(q.iter().all(|(_, v)| !v.contains(',')), "{q:?}");
    }

    /// An empty repeatable flag contributes nothing, as an empty scalar does.
    #[test]
    fn an_empty_array_filter_value_is_treated_as_absent() {
        let mut args = list_args();
        args.batch_ids = vec![String::new()];
        assert!(build_list_batches_query(&args).unwrap().is_empty());
    }

    /// **`desc` is the declared default here**, unlike every other list in the
    /// API, so the descending direction sends no parameter and the flag names
    /// ascending.
    #[test]
    fn descending_is_the_default_and_sends_no_sort_order() {
        let mut args = list_args();
        args.asc = false;
        let q = build_list_batches_query(&args).unwrap();
        assert!(q.iter().all(|(k, _)| *k != "sortOrder"), "{q:?}");
    }

    #[test]
    fn the_close_body_carries_the_processor_under_its_wire_name() {
        assert_eq!(
            build_close_batch_body("pp-1").unwrap(),
            serde_json::json!({"paymentProcessorId": "pp-1"})
        );
    }

    /// The error names the command that lists the value, which is the whole
    /// point of `settings payment-config` existing.
    #[test]
    fn an_empty_processor_id_is_refused_and_says_where_to_find_one() {
        let err = build_close_batch_body("").unwrap_err().to_string();
        assert!(err.contains("settings payment-config"), "{err}");
    }

    /// A filter that matched nothing is a not-found read, so the exit code
    /// comes from a 404 rather than from a client error.
    #[test]
    fn no_match_for_a_batch_id_is_a_404() {
        let err = one_batch("b-1", vec![]).unwrap_err();
        assert!(
            matches!(
                err.downcast_ref::<crate::api::ApiError>(),
                Some(crate::api::ApiError::Api { status: 404, .. })
            ),
            "{err}"
        );
        assert_eq!(crate::cli::output::exit_code_for(&err), 4);
    }

    /// Two matches for one id means the filter is not an identity, and
    /// picking the first would answer a question nobody asked.
    #[test]
    fn two_matches_for_a_batch_id_is_refused_rather_than_narrowed() {
        let items = vec![
            serde_json::json!({"batchId": "b-1"}),
            serde_json::json!({"batchId": "b-2"}),
        ];
        let err = one_batch("b-1", items).unwrap_err().to_string();
        assert!(err.contains('2'), "{err}");
    }

    #[test]
    fn one_match_is_the_batch_itself() {
        let batch = serde_json::json!({"batchId": "b-1", "batchStatus": "Open"});
        assert_eq!(one_batch("b-1", vec![batch.clone()]).unwrap(), batch);
    }
}
