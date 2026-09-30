//! How each transaction response is drawn in `table` mode.

use crate::cli::render::{self, Cell, Column, Resource};
use serde_json::Value;

/// A transaction at a glance: what it is, whether it went through, and why
/// not when it did not.
///
/// Every single-transaction response has this shape: the `GET` and all seven
/// writes — create, capture, tip-adjust, reversal, credit, ach-hold and
/// ach-release — answer with the same object. A write answers for the
/// transaction it addressed, so a reversal's `processedAmount` is the original
/// charge: a void says so in `transactionStatus`, and a refund leaves the
/// status `Settled` and says so only in `refundDetails.refundedAmount`, the
/// running total refunded. The list item is the same shape with fewer fields,
/// so it reads through this one too.
/// `inspect` is the fuller view, and `--output json` the whole response.
pub static TRANSACTION: Resource = Resource {
    object: "transaction",
    object_list: "transaction_list",
    id: "/transactionId",
    detail: &[
        "/transactionId",
        "/transactionStatus",
        "/processedAmount",
        "/refundDetails/refundedAmount",
        "/processorDetails/authCode",
        "/declineDetails/message",
    ],
    columns: TRANSACTION_COLUMNS,
    amounts: TRANSACTION_AMOUNTS,
    yes_no: &[],
};

// CUSTOMER reads `customerId`: the list item carries no `customerName`.
const TRANSACTION_COLUMNS: &[Column] = &[
    Column {
        header: "ID",
        width: 36,
        cell: Cell::Path("/transactionId"),
    },
    Column {
        header: "DATE",
        width: 10,
        cell: Cell::Derived(|v| render::date_only(v, "/transactionDateTime")),
    },
    Column {
        header: "STATUS",
        width: 12,
        cell: Cell::Path("/transactionStatus"),
    },
    Column {
        header: "TYPE",
        width: 14,
        cell: Cell::Path("/transactionType"),
    },
    Column {
        header: "AMOUNT",
        width: 10,
        cell: Cell::Path("/processedAmount"),
    },
    Column {
        header: "CUSTOMER",
        width: 36,
        cell: Cell::Path("/customerId"),
    },
];

// The summary and `inspect`'s breakdown both read these.
const TRANSACTION_AMOUNTS: &[&str] = &[
    "/processedAmount",
    "/amountBreakdown/baseAmount",
    "/amountBreakdown/tipAmount",
    "/amountBreakdown/discountAmount",
    "/amountBreakdown/surchargeAmount",
    "/refundDetails/refundedAmount",
];

/// The rows of `inspect`'s header: what the transaction is and, when it was
/// refused, why.
///
/// `declineDetails` is the one code-and-message pair
/// `GetTransactionResponseDtoFull` declares for a payment that did not go
/// through.
const INSPECT_HEADER: &[(&str, &str)] = &[
    ("transactionId", "/transactionId"),
    ("transactionStatus", "/transactionStatus"),
    ("transactionType", "/transactionType"),
    ("paymentMethodType", "/paymentMethodType"),
    ("currencyCode", "/currencyCode"),
    ("declineDetails.code", "/declineDetails/code"),
    ("declineDetails.message", "/declineDetails/message"),
];

/// A card's rows, followed by the address-verification line: the
/// authorization code is the evidence a card charge worked.
const INSPECT_CARD: &[(&str, &str)] = &[
    ("processorDetails.authCode", "/processorDetails/authCode"),
    ("cardDetails.cardDataSource", "/cardDetails/cardDataSource"),
    (
        "cardDetails.maskedCardNumber",
        "/cardDetails/maskedCardNumber",
    ),
];

/// A bank account's rows. An ACH response carries no authorization code or
/// address-verification answer.
const INSPECT_ACH: &[(&str, &str)] = &[
    (
        "achDetails.maskedAccountNumber",
        "/achDetails/maskedAccountNumber",
    ),
    ("achDetails.secCode", "/achDetails/secCode"),
];

/// The rows of `inspect`'s amount section: the total charged, then how much
/// of it has been refunded.
const INSPECT_BREAKDOWN: &[(&str, &str)] = &[
    ("  baseAmount", "/amountBreakdown/baseAmount"),
    ("  tipAmount", "/amountBreakdown/tipAmount"),
    ("  tipRate", "/amountBreakdown/tipRate"),
    ("  discountAmount", "/amountBreakdown/discountAmount"),
    ("  discountRate", "/amountBreakdown/discountRate"),
    ("  surchargeAmount", "/amountBreakdown/surchargeAmount"),
    ("  surchargeRate", "/amountBreakdown/surchargeRate"),
    ("  processedAmount", "/processedAmount"),
    ("  refundedAmount", "/refundDetails/refundedAmount"),
];

/// `inspect`'s table: the fields that decide whether a payment worked, then
/// the amounts that make up the total.
///
/// A fixed shape per instrument, chosen by `paymentMethodType`, so a field the
/// response omits holds its row with a dash — the view answers the same
/// questions about every transaction paid the same way. A type that is
/// neither `Card` nor `ACH` gets both instruments' rows. It ends with
/// the amounts: `GetTransactionResponseDtoFull` declares no list of the
/// operations still open on a transaction, so there is none to name.
pub fn inspect_table(data: &Value) -> String {
    let row = |(label, pointer): &(&str, &str)| {
        (
            (*label).to_string(),
            render::value_at(&TRANSACTION, data, pointer),
        )
    };
    let instrument = data.get("paymentMethodType").and_then(Value::as_str);
    let mut rows: Vec<(String, String)> = INSPECT_HEADER.iter().map(row).collect();
    if instrument != Some("ACH") {
        rows.extend(INSPECT_CARD.iter().map(row));
        rows.push((
            "addressVerificationServiceResponse".to_string(),
            avs_line(data.pointer("/addressVerificationServiceResponse")),
        ));
    }
    if instrument != Some("Card") {
        rows.extend(INSPECT_ACH.iter().map(row));
    }
    let header_rows = rows.len();
    rows.extend(INSPECT_BREAKDOWN.iter().map(row));

    // One alignment across both sections, so the indented amounts line up
    // with the header above them.
    let lines = render::labelled(&rows);
    let (header, breakdown) = lines.split_at(header_rows);
    format!(
        "{}\n\nAmount breakdown:\n{}",
        header.join("\n"),
        breakdown.join("\n")
    )
}

/// The address-verification answer on one line: the code, what it means, and
/// what was done about it, each only when the API sent it.
fn avs_line(avs: Option<&Value>) -> String {
    let parts: Vec<&str> = match avs {
        Some(Value::Object(avs)) => ["responseCode", "description", "action"]
            .iter()
            .filter_map(|key| avs.get(*key).and_then(Value::as_str))
            .filter(|text| !text.is_empty())
            .collect(),
        _ => Vec::new(),
    };
    if parts.is_empty() {
        render::MISSING.to_string()
    } else {
        parts.join(" \u{2014} ")
    }
}

/// The calculated totals are not a transaction, so they get their own
/// descriptor rather than borrowing one that would drop every field.
pub static AMOUNT_CALCULATION: Resource = Resource {
    object: "amount_calculation",
    object_list: "amount_calculations",
    // Nothing is created, so the response names no resource and `quiet` has
    // nothing to print. The currency code it echoes reads back nothing.
    id: "",
    detail: &[
        "/currencyCode",
        "/pricingType",
        "/zeroCostProcessingOption",
        "/creditCard/baseAmount",
        "/creditCard/surchargeAmount",
        "/creditCard/tipAmount",
        "/creditCard/totalAmount",
        "/debitCard/totalAmount",
        "/cash/totalAmount",
        "/ach/totalAmount",
    ],
    columns: &[],
    amounts: &[
        "/creditCard/baseAmount",
        "/creditCard/discountAmount",
        "/creditCard/surchargeAmount",
        "/creditCard/tipAmount",
        "/creditCard/totalAmount",
        "/debitCard/baseAmount",
        "/debitCard/discountAmount",
        "/debitCard/surchargeAmount",
        "/debitCard/tipAmount",
        "/debitCard/totalAmount",
        "/cash/baseAmount",
        "/cash/discountAmount",
        "/cash/surchargeAmount",
        "/cash/tipAmount",
        "/cash/totalAmount",
        "/ach/baseAmount",
        "/ach/discountAmount",
        "/ach/surchargeAmount",
        "/ach/tipAmount",
        "/ach/totalAmount",
    ],
    yes_no: &[],
};
