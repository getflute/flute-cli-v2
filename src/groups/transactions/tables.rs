//! How each transaction response is drawn in `table` mode.

use crate::cli::render::{self, Cell, Column, Resource};
use serde_json::Value;

/// A transaction, in the order it is worth saying it.
///
/// The **decline reason ranks immediately after the amount**: it is the field
/// a caller most needs and the one a renderer that drops containers hides.
///
/// Every single-transaction response has this shape: the `GET` and all seven
/// writes — create, capture, tip-adjust, reversal, credit, ach-hold and
/// ach-release — answer with the same object, `amountBreakdown`,
/// `declineDetails`, `processorDetails` and the address-verification answer
/// included. A write answers for the transaction it addressed, so a
/// reversal's `processedAmount` is the original charge and a refund's amount
/// is `refundDetails.refundedAmount`. The list item is the same shape with
/// fewer fields, so it reads through this one too.
pub static TRANSACTION: Resource = Resource {
    object: "transaction",
    object_list: "transaction_list",
    id: "/transactionId",
    detail: &[
        "/transactionId",
        "/transactionStatus",
        "/transactionType",
        "/transactionDateTime",
        "/processedAmount",
        "/currencyCode",
        "/declineDetails/code",
        "/declineDetails/message",
        "/amountBreakdown/baseAmount",
        "/amountBreakdown/tipAmount",
        "/amountBreakdown/tipRate",
        "/amountBreakdown/discountAmount",
        "/amountBreakdown/discountRate",
        "/amountBreakdown/surchargeAmount",
        "/amountBreakdown/surchargeRate",
        "/paymentMethodType",
        "/cardDetails/maskedCardNumber",
        "/cardDetails/cardBrand",
        "/cardDetails/cardType",
        "/cardDetails/cardDataSource",
        "/cardDetails/paymentMethodId",
        "/achDetails/maskedAccountNumber",
        "/achDetails/accountRoutingNumber",
        "/achDetails/accountType",
        "/achDetails/secCode",
        "/achDetails/paymentMethodId",
        "/referenceId",
        "/customerId",
        "/paymentProcessorId",
        "/merchantId",
        "/batchId",
        "/originalTransactionId",
        "/processorDetails/authCode",
        "/processorDetails/rrn",
        "/processorDetails/mid",
        "/processorDetails/tid",
        "/addressVerificationServiceResponse/action",
        "/addressVerificationServiceResponse/responseCode",
        "/addressVerificationServiceResponse/description",
        "/refundDetails/refundedAmount",
        "/refundDetails/availableRefundAmount",
    ],
    // CUSTOMER reads `customerId`: the list item carries no `customerName`.
    columns: &[
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
    ],
    amounts: &[
        "/processedAmount",
        "/amountBreakdown/baseAmount",
        "/amountBreakdown/tipAmount",
        "/amountBreakdown/discountAmount",
        "/amountBreakdown/surchargeAmount",
        "/refundDetails/refundedAmount",
        "/refundDetails/availableRefundAmount",
    ],
    yes_no: &[],
};

/// The rows of `inspect`'s header: what decided the payment.
///
/// Both outcomes are asked about, because only one of them is ever there: the
/// authorization code is the evidence a charge worked, and `declineDetails`
/// is the one code-and-message pair `GetTransactionResponseDtoFull` declares
/// for one that did not.
const INSPECT_HEADER: &[(&str, &str)] = &[
    ("transactionId", "/transactionId"),
    ("transactionStatus", "/transactionStatus"),
    ("currencyCode", "/currencyCode"),
    ("processorDetails.authCode", "/processorDetails/authCode"),
    ("declineDetails.code", "/declineDetails/code"),
    ("declineDetails.message", "/declineDetails/message"),
    ("cardDetails.cardDataSource", "/cardDetails/cardDataSource"),
    (
        "cardDetails.maskedCardNumber",
        "/cardDetails/maskedCardNumber",
    ),
];

/// The rows of `inspect`'s amount section, ending with the total charged.
const INSPECT_BREAKDOWN: &[(&str, &str)] = &[
    ("  baseAmount", "/amountBreakdown/baseAmount"),
    ("  tipAmount", "/amountBreakdown/tipAmount"),
    ("  tipRate", "/amountBreakdown/tipRate"),
    ("  discountAmount", "/amountBreakdown/discountAmount"),
    ("  discountRate", "/amountBreakdown/discountRate"),
    ("  surchargeAmount", "/amountBreakdown/surchargeAmount"),
    ("  surchargeRate", "/amountBreakdown/surchargeRate"),
    ("  processedAmount", "/processedAmount"),
];

/// `inspect`'s table: the fields that decide whether a payment worked, then
/// the amounts that make up the total.
///
/// A fixed shape, so a field the response omits holds its row with a dash —
/// the view answers the same questions about every transaction. It ends with
/// the amounts: `GetTransactionResponseDtoFull` declares no list of the
/// operations still open on a transaction, so there is none to name.
pub fn inspect_table(data: &Value) -> String {
    let row = |(label, pointer): &(&str, &str)| {
        (
            (*label).to_string(),
            render::value_at(&TRANSACTION, data, pointer),
        )
    };
    let mut rows: Vec<(String, String)> = INSPECT_HEADER.iter().map(row).collect();
    rows.push((
        "addressVerificationServiceResponse".to_string(),
        avs_line(data.pointer("/addressVerificationServiceResponse")),
    ));
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
