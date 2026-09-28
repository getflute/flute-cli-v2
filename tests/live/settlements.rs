//! Live scenarios for `settlements`. **Committed.**
//!
//! `close` is the one scenario here that changes account state irreversibly —
//! a closed batch cannot be reopened, and every later transaction lands in a
//! new one. Run it last, or not at all.

use crate::*;

/// The first batch that actually carries an identifier.
///
/// **A real collection contains rows with every field null** — `batchId`,
/// `externalBatchId` and `batchStatus` all absent — alongside rows that are
/// fully populated. `batchId` is declared nullable so such a row is legal, and
/// a scenario that reaches for `data[0]` gets whichever it happens to be.
///
/// `settlements get` is unaffected: it filters by an id a caller supplies, and
/// a batch without one was never addressable. What this guards is the
/// *scenarios*, which need a batch they can name.
///
/// No such batch is a **failure**, not a quiet success. The scenarios that
/// call this are the matrix's live evidence for the filtered list and the
/// one-batch read; returning early would leave both operations unissued while
/// the run reports them covered.
fn first_identified_batch(listed: &serde_json::Value) -> serde_json::Value {
    listed["data"]
        .as_array()
        .and_then(|rows| rows.iter().find(|b| b["batchId"].is_string()))
        .cloned()
        .unwrap_or_else(|| {
            panic!(
                "no batch on the first page carries a batchId, and this \
                 scenario needs one to filter by. Settle a transaction on the \
                 sandbox processor so the account has an identified batch, \
                 then run it again.\n{listed}"
            )
        })
}

#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_settlement_list_first_page() {
    let listed = json(&["settlements", "list"]);
    assert_eq!(listed["object"], "settlement_list");
    assert!(listed["data"].is_array(), "{listed}");
}

/// **The evidence for the two array query parameters.**
///
/// `batchIds` and `paymentProcessorIds` are the only array-typed parameters
/// in the API. The CLI sends them as repeated pairs, which is the OpenAPI
/// default for a `form`-style array and an assumption until this runs: a
/// server expecting one comma-joined value would answer an empty page rather
/// than an error.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_settlement_list_filtered_by_repeated_array_parameters() {
    let listed = json(&["settlements", "list", "--page-size", "5"]);
    let first = first_identified_batch(&listed);
    let batch_id = first["batchId"].as_str().expect("checked by the finder");
    let processor = first["paymentProcessorId"]
        .as_str()
        .unwrap_or(&card_processor_id())
        .to_string();

    let filtered = json(&[
        "settlements",
        "list",
        "--batch-ids",
        batch_id,
        "--processor-ids",
        &processor,
    ]);
    let found = filtered["data"].as_array().cloned().unwrap_or_default();
    assert!(
        found.iter().any(|b| b["batchId"] == batch_id),
        "filtering by a batch id that was just listed returned {} item(s) and \
         not that batch, so the repeated-pair form is not how this parameter \
         is read",
        found.len()
    );
}

/// `settlements get` is the list endpoint with the server-side `batchIds`
/// filter, so this is the scenario that shows the filter reaches beyond the
/// first page — the reason it is not done client-side.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_settlement_get_reads_one_batch_by_id() {
    let listed = json(&["settlements", "list", "--page-size", "5"]);
    let first = first_identified_batch(&listed);
    let batch_id = first["batchId"].as_str().expect("checked by the finder");

    let one = json(&["settlements", "get", batch_id]);
    assert_eq!(one["object"], "settlement");
    assert_eq!(one["data"]["batchId"], batch_id);
}

/// An empty result from the filter is a not-found read, exit 4. The command
/// has no endpoint of its own, so this is the only place the distinction
/// between "no such batch" and "an empty page" becomes an exit code.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_settlement_get_of_an_unknown_batch_exits_four() {
    live_bin()
        .args(["settlements", "get", "00000000-0000-4000-8000-000000000000"])
        .assert()
        .code(4);
}

/// The status filter takes the four statuses the **response** field declares;
/// the query parameter itself declares no enum, so this is an assumption
/// about the vocabulary rather than a documented fact.
#[test]
#[ignore = "live sandbox; opt in with --ignored"]
fn live_settlement_list_filtered_by_status() {
    let listed = json(&["settlements", "list", "--status", "open"]);
    assert!(listed["data"].is_array(), "{listed}");
}

/// **Irreversible.** Closes the open batch on the sandbox processor; every
/// later transaction lands in a new one. Run it last.
#[test]
#[ignore = "live sandbox, and irreversible; opt in with --ignored"]
fn live_settlement_close_the_open_batch_irreversible() {
    let closed = json(&[
        "settlements",
        "close",
        "--payment-processor-id",
        &card_processor_id(),
    ]);
    assert_eq!(closed["object"], "batch_closure");
    let status = closed["data"]["batchStatus"]
        .as_str()
        .expect("close returned no batchStatus");
    assert!(
        ["Open", "PendingSettlement", "Settled", "Declined"].contains(&status),
        "batchStatus was {status}, which SettleTransactionsResponseDto does not \
         declare"
    );
}
