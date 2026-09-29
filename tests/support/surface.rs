//! The API surface matrix (layer 4a).
//!
//! The contract matrix proves every *operation* is reachable. It says nothing
//! about whether every *field* is: `customers list` can hold a passing
//! contract row while `fullName`, `email`, `companyName`, `mobilePhoneNumber`,
//! `createdFrom`, `createdTo`, `sortBy` and `asc` are all unreachable. The
//! endpoint is covered and most of its capability is not.

use Exposure::{Excluded, Flag};
use std::sync::LazyLock;

#[derive(Clone)]
pub enum Exposure {
    /// Reachable from the CLI through this flag. Several spellings for one
    /// field are separated by `/`.
    Flag(&'static str),
    /// Always sent with a value the CLI chooses, never user-supplied.
    Fixed(&'static str),
    /// Deliberately not exposed, with a reason.
    Excluded(&'static str),
}

#[derive(Clone)]
pub struct Field {
    pub operation_id: &'static str,
    /// A **leaf** request-body JSON pointer, or `?name` for a query
    /// parameter. Array element steps are written `/[]`, so a product's name
    /// is `/transactionEnhancedData/products/[]/name`.
    ///
    /// Containers are not rows. A row for `/transactionDetails` would account
    /// for the whole card and ACH surface at once, which is the miss this
    /// matrix exists to catch.
    pub field: &'static str,
    pub exposure: Exposure,
}

/// One row: the operation, the field, and how the CLI reaches it.
type Row = (&'static str, &'static str, Exposure);

/// Every row: [`ROWS`], plus the billing and shipping address leaves of each
/// operation in [`ADDRESSED`].
pub static SURFACE: LazyLock<Vec<Field>> = LazyLock::new(|| {
    let addresses = ADDRESSED.iter().flat_map(|op| address_rows(op));
    ROWS.iter()
        .cloned()
        .chain(addresses)
        .map(|(operation_id, field, exposure)| Field {
            operation_id,
            field,
            exposure,
        })
        .collect()
});

/// The operations that take a billing and a shipping address, each through
/// the same six `--billing-*` and `--shipping-*` flags.
const ADDRESSED: &[&str] = &[
    "flute-v2-post-transactions-credit",
    "flute-v2-patch-customers-customerId",
    "flute-v2-post-customers",
    "flute-v2-post-transactions",
];

/// The twelve address leaves of one operation in [`ADDRESSED`].
fn address_rows(op: &'static str) -> Vec<Row> {
    let mut rows = Vec::new();
    for (object, prefix) in [
        ("billingAddress", "--billing"),
        ("shippingAddress", "--shipping"),
    ] {
        for (leaf, suffix) in [
            ("addressLine1", "line1"),
            ("addressLine2", "line2"),
            ("city", "city"),
            ("countryCode", "country"),
            ("postalCode", "postal-code"),
            ("stateCode", "state"),
        ] {
            let field: &'static str = format!("/{object}/{leaf}").leak();
            let flag: &'static str = format!("{prefix}-{suffix}").leak();
            rows.push((op, field, Flag(flag)));
        }
    }
    rows
}

#[rustfmt::skip]
const ROWS: &[Row] = &[
    ("flute-v2-post-transactions-credit", "/paymentProcessorId", Flag("--payment-processor-id")),
    ("flute-v2-post-transactions-credit", "/baseAmount", Flag("--amount")),
    // Required here, unlike on `create`.
    ("flute-v2-post-transactions-credit", "/referenceId", Flag("--reference-id")),
    ("flute-v2-post-transactions-credit", "/currencyCode", Flag("--currency-code")),
    ("flute-v2-post-transactions-credit", "/customerId", Flag("--customer-id")),
    ("flute-v2-post-transactions-credit", "/contactInfo/firstName", Flag("--contact-first-name")),
    ("flute-v2-post-transactions-credit", "/contactInfo/lastName", Flag("--contact-last-name")),
    ("flute-v2-post-transactions-credit", "/contactInfo/companyName", Flag("--contact-company")),
    ("flute-v2-post-transactions-credit", "/contactInfo/email", Flag("--contact-email")),
    ("flute-v2-post-transactions-credit", "/contactInfo/mobilePhoneNumber", Flag("--contact-phone")),
    ("flute-v2-post-transactions-credit", "/contactInfo/hasSmsConsent", Flag("--contact-sms-consent")),
    ("flute-v2-post-transactions-credit", "/creditDetails/cardData/paymentMethodId", Flag("--payment-method-id/--instrument")),
    ("flute-v2-post-transactions-credit", "/creditDetails/cardData/paymentMethodDetails/cardNumber", Flag("--card")),
    ("flute-v2-post-transactions-credit", "/creditDetails/cardData/paymentMethodDetails/securityCode", Flag("--cvv")),
    ("flute-v2-post-transactions-credit", "/creditDetails/cardData/paymentMethodDetails/expirationMonth", Flag("--exp")),
    ("flute-v2-post-transactions-credit", "/creditDetails/cardData/paymentMethodDetails/expirationYear", Flag("--exp")),
    ("flute-v2-post-transactions-credit", "/creditDetails/achData/paymentMethodId", Flag("--payment-method-id/--instrument")),
    ("flute-v2-post-transactions-credit", "/creditDetails/achData/paymentMethodDetails/accountNumber", Flag("--ach-account-number")),
    ("flute-v2-post-transactions-credit", "/creditDetails/achData/paymentMethodDetails/routingNumber", Flag("--ach-routing-number")),
    ("flute-v2-post-transactions-credit", "/creditDetails/achData/paymentMethodDetails/accountType", Flag("--ach-account-type")),
    ("flute-v2-post-transactions-credit", "/creditDetails/achData/paymentMethodDetails/accountHolderType", Flag("--ach-account-holder-type")),
    ("flute-v2-post-transactions-credit", "/creditDetails/achData/paymentMethodDetails/taxId", Flag("--ach-tax-id")),
    ("flute-v2-post-transactions-credit", "/creditDetails/achData/secCode", Flag("--sec-code")),
    ("flute-v2-post-transactions-credit", "/creditDetails/achData/requesterIpAddress", Flag("--requester-ip")),
    ("flute-v2-post-transactions-credit", "/creditDetails/achData/isSameDayProcessing", Flag("--same-day")),
    ("flute-v2-get-transactions", "?pageIndex", Flag("--page-index")),
    ("flute-v2-get-transactions", "?pageSize", Flag("--page-size")),
    ("flute-v2-get-transactions", "?sortBy", Flag("--sort-by")),
    ("flute-v2-get-transactions", "?sortOrder", Flag("--asc/--desc")),
    ("flute-v2-get-transactions", "?fromDate", Flag("--from")),
    ("flute-v2-get-transactions", "?toDate", Flag("--to")),
    ("flute-v2-get-transactions", "?sourceType", Flag("--source-type")),
    ("flute-v2-get-transactions", "?sourceId", Flag("--source-id")),
    ("flute-v2-get-transactions", "?batchId", Flag("--batch-id")),
    ("flute-v2-get-transactions", "?transactionStatus", Flag("--status")),
    ("flute-v2-get-transactions", "?paymentMethodType", Flag("--payment-method-type")),
    ("flute-v2-get-transactions", "?customerId", Flag("--customer-id")),
    ("flute-v2-get-transactions", "?merchantId", Flag("--merchant-id")),
    ("flute-v2-get-transactions", "?minAmount", Flag("--min-amount")),
    ("flute-v2-get-transactions", "?maxAmount", Flag("--max-amount")),
    ("flute-v2-get-transactions", "?referenceId", Flag("--reference-id")),
    // The schema declares `captureAmount` and the operation's own
    // request example sends `amount`. The schema is normative, so this is
    // what the flag reaches.
    ("flute-v2-post-transactions-transactionId-capture", "/captureAmount", Flag("--amount")),
    ("flute-v2-post-transactions-transactionId-reversal", "/reversalAmount", Flag("--amount")),
    ("flute-v2-post-transactions-transactionId-tip-adjustment", "/tipAmount", Flag("--tip-amount")),
    ("flute-v2-post-transactions-transactionId-tip-adjustment", "/tipRate", Flag("--tip-rate")),
    ("flute-v2-post-transactions-transactionId-share-receipt", "/shareBy", Flag("--share-by")),
    ("flute-v2-post-transactions-transactionId-share-receipt", "/recipient", Flag("--recipient")),
    ("flute-v2-post-transactions-transactionId-share-receipt", "/hasCustomerConsent", Flag("--consent")),
    ("flute-v2-post-transactions-calculate-amount", "/baseAmount", Flag("--amount")),
    ("flute-v2-post-transactions-calculate-amount", "/currencyCode", Flag("--currency-code")),
    ("flute-v2-post-transactions-calculate-amount", "/pricingType", Flag("--pricing-type")),
    ("flute-v2-post-transactions-calculate-amount", "/tipAmount", Flag("--tip-amount")),
    ("flute-v2-post-transactions-calculate-amount", "/tipRate", Flag("--tip-rate")),
    ("flute-v2-post-transactions-calculate-amount", "/discountAmount", Flag("--discount-amount")),
    ("flute-v2-post-transactions-calculate-amount", "/discountRate", Flag("--discount-rate")),
    ("flute-v2-post-transactions-calculate-amount", "/surchargeRate", Flag("--surcharge-rate")),
    ("flute-v2-get-payment-methods", "?pageIndex", Flag("--page-index")),
    ("flute-v2-get-payment-methods", "?pageSize", Flag("--page-size")),
    // As on `customers list`: the API defaults `asc` to true, so the
    // flag names the non-default direction.
    ("flute-v2-get-payment-methods", "?asc", Flag("--desc")),
    ("flute-v2-get-payment-methods", "?sortBy", Flag("--sort-by")),
    ("flute-v2-get-payment-methods", "?search", Flag("--search")),
    ("flute-v2-get-payment-methods", "?createdFrom", Flag("--created-from")),
    ("flute-v2-get-payment-methods", "?createdTo", Flag("--created-to")),
    ("flute-v2-get-payment-methods", "?customerId", Flag("--customer-id")),
    ("flute-v2-post-payment-methods-cards", "/cardNumber", Flag("--card")),
    ("flute-v2-post-payment-methods-cards", "/securityCode", Flag("--cvv")),
    // One MM/YY flag fills both halves, as on `transactions create`.
    ("flute-v2-post-payment-methods-cards", "/expirationMonth", Flag("--exp")),
    ("flute-v2-post-payment-methods-cards", "/expirationYear", Flag("--exp")),
    ("flute-v2-post-payment-methods-cards", "/customerId", Flag("--customer-id")),
    // The label is `paymentName` here and `name` on the ACH route. One
    // flag covers both, so an inconsistency in the API does not become
    // one in the CLI.
    ("flute-v2-post-payment-methods-cards", "/paymentName", Flag("--name")),
    ("flute-v2-post-payment-methods-ach", "/accountNumber", Flag("--account")),
    ("flute-v2-post-payment-methods-ach", "/routingNumber", Flag("--routing")),
    ("flute-v2-post-payment-methods-ach", "/accountType", Flag("--account-type")),
    ("flute-v2-post-payment-methods-ach", "/accountHolderType", Flag("--account-holder-type")),
    ("flute-v2-post-payment-methods-ach", "/taxId", Flag("--tax-id")),
    ("flute-v2-post-payment-methods-ach", "/customerId", Flag("--customer-id")),
    ("flute-v2-post-payment-methods-ach", "/name", Flag("--name")),
    ("flute-v2-post-payment-methods-ach", "/companyName", Flag("--company-name")),
    ("flute-v2-patch-payment-methods-paymentMethodId", "/paymentName", Flag("--name")),
    // Required by the API, so clap requires it too.
    ("flute-v2-post-payment-methods-paymentMethodId-set-default", "?customerId", Flag("--customer-id")),
    ("flute-v2-get-customers", "?pageIndex", Flag("--page-index")),
    ("flute-v2-get-customers", "?pageSize", Flag("--page-size")),
    // The API defaults `asc` to true and a bare clap switch cannot
    // express false, so the flag names the non-default direction and the
    // parameter is sent only when it is asked for.
    ("flute-v2-get-customers", "?asc", Flag("--desc")),
    ("flute-v2-get-customers", "?sortBy", Flag("--sort-by")),
    ("flute-v2-get-customers", "?fullName", Flag("--full-name")),
    ("flute-v2-get-customers", "?email", Flag("--email")),
    ("flute-v2-get-customers", "?companyName", Flag("--company-name")),
    ("flute-v2-get-customers", "?mobilePhoneNumber", Flag("--mobile")),
    ("flute-v2-get-customers", "?createdFrom", Flag("--created-from")),
    ("flute-v2-get-customers", "?createdTo", Flag("--created-to")),
    ("flute-v2-patch-customers-customerId", "/companyName", Flag("--company")),
    ("flute-v2-patch-customers-customerId", "/email", Flag("--email")),
    ("flute-v2-patch-customers-customerId", "/firstName", Flag("--first-name")),
    ("flute-v2-patch-customers-customerId", "/hasSmsConsent", Flag("--sms-consent")),
    ("flute-v2-patch-customers-customerId", "/lastName", Flag("--last-name")),
    ("flute-v2-patch-customers-customerId", "/mobilePhoneNumber", Flag("--mobile")),
    ("flute-v2-patch-customers-customerId", "/shouldUseBillingAsShippingAddress", Flag("--use-billing-as-shipping")),
    ("flute-v2-post-customers", "/firstName", Flag("--first-name")),
    ("flute-v2-post-customers", "/lastName", Flag("--last-name")),
    ("flute-v2-post-customers", "/companyName", Flag("--company")),
    ("flute-v2-post-customers", "/email", Flag("--email")),
    ("flute-v2-post-customers", "/mobilePhoneNumber", Flag("--mobile")),
    ("flute-v2-post-customers", "/hasSmsConsent", Flag("--sms-consent")),
    ("flute-v2-post-customers", "/shouldUseBillingAsShippingAddress", Flag("--use-billing-as-shipping")),
    ("flute-v2-post-customers", "/paymentMethodsAchAccounts/[]/accountHolderType", Excluded(
        "an instrument is added with `payment-methods add-card` / `add-ach`; \
        inlining the whole instrument surface onto a customer command would \
        duplicate it, and v1 keeps them separate too",
        )),
    ("flute-v2-post-customers", "/paymentMethodsAchAccounts/[]/accountNumber", Excluded(
        "an instrument is added with `payment-methods add-card` / `add-ach`; \
        inlining the whole instrument surface onto a customer command would \
        duplicate it, and v1 keeps them separate too",
        )),
    ("flute-v2-post-customers", "/paymentMethodsAchAccounts/[]/accountType", Excluded(
        "an instrument is added with `payment-methods add-card` / `add-ach`; \
        inlining the whole instrument surface onto a customer command would \
        duplicate it, and v1 keeps them separate too",
        )),
    ("flute-v2-post-customers", "/paymentMethodsAchAccounts/[]/paymentName", Excluded(
        "an instrument is added with `payment-methods add-card` / `add-ach`; \
        inlining the whole instrument surface onto a customer command would \
        duplicate it, and v1 keeps them separate too",
        )),
    ("flute-v2-post-customers", "/paymentMethodsAchAccounts/[]/routingNumber", Excluded(
        "an instrument is added with `payment-methods add-card` / `add-ach`; \
        inlining the whole instrument surface onto a customer command would \
        duplicate it, and v1 keeps them separate too",
        )),
    ("flute-v2-post-customers", "/paymentMethodsAchAccounts/[]/taxId", Excluded(
        "an instrument is added with `payment-methods add-card` / `add-ach`; \
        inlining the whole instrument surface onto a customer command would \
        duplicate it, and v1 keeps them separate too",
        )),
    ("flute-v2-post-customers", "/paymentMethodsCards/[]/cardNumber", Excluded(
        "an instrument is added with `payment-methods add-card` / `add-ach`; \
        inlining the whole instrument surface onto a customer command would \
        duplicate it, and v1 keeps them separate too",
        )),
    ("flute-v2-post-customers", "/paymentMethodsCards/[]/expirationMonth", Excluded(
        "an instrument is added with `payment-methods add-card` / `add-ach`; \
        inlining the whole instrument surface onto a customer command would \
        duplicate it, and v1 keeps them separate too",
        )),
    ("flute-v2-post-customers", "/paymentMethodsCards/[]/expirationYear", Excluded(
        "an instrument is added with `payment-methods add-card` / `add-ach`; \
        inlining the whole instrument surface onto a customer command would \
        duplicate it, and v1 keeps them separate too",
        )),
    ("flute-v2-post-customers", "/paymentMethodsCards/[]/paymentName", Excluded(
        "an instrument is added with `payment-methods add-card` / `add-ach`; \
        inlining the whole instrument surface onto a customer command would \
        duplicate it, and v1 keeps them separate too",
        )),
    ("flute-v2-post-customers", "/paymentMethodsCards/[]/securityCode", Excluded(
        "an instrument is added with `payment-methods add-card` / `add-ach`; \
        inlining the whole instrument surface onto a customer command would \
        duplicate it, and v1 keeps them separate too",
        )),
    ("flute-v2-post-transactions", "/paymentProcessorId", Flag("--payment-processor-id")),
    ("flute-v2-post-transactions", "/baseAmount", Flag("--amount")),
    ("flute-v2-post-transactions", "/referenceId", Flag("--reference-id")),
    ("flute-v2-post-transactions", "/currencyCode", Flag("--currency-code")),
    ("flute-v2-post-transactions", "/transactionDetails/cardData/paymentMethodDetails/cardNumber", Flag("--card")),
    ("flute-v2-post-transactions", "/transactionDetails/cardData/paymentMethodDetails/securityCode", Flag("--cvv")),
    ("flute-v2-post-transactions", "/transactionDetails/cardData/paymentMethodDetails/expirationMonth", Flag("--exp")),
    ("flute-v2-post-transactions", "/transactionDetails/cardData/paymentMethodDetails/expirationYear", Flag("--exp")),
    ("flute-v2-post-transactions", "/appVersion", Excluded("SDK telemetry; meaningless from a CLI")),
    ("flute-v2-post-transactions", "/sdkVersion", Excluded("SDK telemetry; meaningless from a CLI")),
    ("flute-v2-post-transactions", "/platform", Excluded(
        "SDK telemetry. Not fixed to a value either: the enum is not documented, \
        so any constant would be a guess sent on every charge",
        )),
    ("flute-v2-post-transactions", "/deviceId", Excluded(
        "identifies a POS terminal; `pos create` is the command that owns a device",
        )),
    ("flute-v2-post-transactions", "/transactionDetails/cardData/paymentMethodId", Flag("--payment-method-id/--instrument")),
    ("flute-v2-post-transactions", "/transactionDetails/achData/isSameDayProcessing", Flag("--same-day")),
    ("flute-v2-post-transactions", "/transactionDetails/achData/paymentMethodDetails/accountHolderType", Flag("--ach-account-holder-type")),
    ("flute-v2-post-transactions", "/transactionDetails/achData/paymentMethodDetails/accountNumber", Flag("--ach-account-number")),
    ("flute-v2-post-transactions", "/transactionDetails/achData/paymentMethodDetails/accountType", Flag("--ach-account-type")),
    ("flute-v2-post-transactions", "/transactionDetails/achData/paymentMethodDetails/routingNumber", Flag("--ach-routing-number")),
    ("flute-v2-post-transactions", "/transactionDetails/achData/paymentMethodDetails/taxId", Flag("--ach-tax-id")),
    ("flute-v2-post-transactions", "/transactionDetails/achData/paymentMethodId", Flag("--payment-method-id/--instrument")),
    ("flute-v2-post-transactions", "/transactionDetails/achData/requesterIpAddress", Flag("--requester-ip")),
    ("flute-v2-post-transactions", "/transactionDetails/achData/secCode", Flag("--sec-code")),
    ("flute-v2-post-transactions", "/contactInfo/companyName", Flag("--contact-company")),
    ("flute-v2-post-transactions", "/contactInfo/email", Flag("--contact-email")),
    ("flute-v2-post-transactions", "/contactInfo/firstName", Flag("--contact-first-name")),
    ("flute-v2-post-transactions", "/contactInfo/hasSmsConsent", Flag("--contact-sms-consent")),
    ("flute-v2-post-transactions", "/contactInfo/lastName", Flag("--contact-last-name")),
    ("flute-v2-post-transactions", "/contactInfo/mobilePhoneNumber", Flag("--contact-phone")),
    ("flute-v2-post-transactions", "/extraAmounts/discountAmount", Flag("--discount-amount")),
    ("flute-v2-post-transactions", "/extraAmounts/discountRate", Flag("--discount-rate")),
    ("flute-v2-post-transactions", "/extraAmounts/surchargeRate", Flag("--surcharge-rate")),
    ("flute-v2-post-transactions", "/extraAmounts/tipAmount", Flag("--tip-amount")),
    ("flute-v2-post-transactions", "/extraAmounts/tipRate", Flag("--tip-rate")),
    ("flute-v2-post-transactions", "/isCustomerInitiatedTransaction", Flag("--customer-initiated")),
    ("flute-v2-post-transactions", "/pricingType", Flag("--pricing-type")),
    // The server has no member for it. A charge carrying the field is
    // rejected outright — not ignored — so a flag here could only ever
    // break the request it was added to. The declaration is doubly
    // suspect: it also puts `maxLength` on a `number`. Restore the flag
    // when a live charge carrying `dutyCharges` succeeds.
    ("flute-v2-post-transactions", "/transactionEnhancedData/dutyCharges", Excluded(
        "the API rejects it: no member of TransactionEnhancedDataDto maps to              dutyCharges, though the schema declares it",
        )),
    ("flute-v2-post-transactions", "/transactionEnhancedData/invoiceNumber", Flag("--l3-invoice")),
    ("flute-v2-post-transactions", "/transactionEnhancedData/products/[]/discountRate", Flag("--l3-product")),
    ("flute-v2-post-transactions", "/transactionEnhancedData/products/[]/measurementUnit", Flag("--l3-product")),
    ("flute-v2-post-transactions", "/transactionEnhancedData/products/[]/productCode", Flag("--l3-product")),
    ("flute-v2-post-transactions", "/transactionEnhancedData/products/[]/productDescription", Flag("--l3-product")),
    ("flute-v2-post-transactions", "/transactionEnhancedData/products/[]/productName", Flag("--l3-product")),
    ("flute-v2-post-transactions", "/transactionEnhancedData/products/[]/quantity", Flag("--l3-product")),
    ("flute-v2-post-transactions", "/transactionEnhancedData/products/[]/taxAmount", Flag("--l3-product")),
    ("flute-v2-post-transactions", "/transactionEnhancedData/products/[]/unitPrice", Flag("--l3-product")),
    ("flute-v2-post-transactions", "/transactionEnhancedData/purchaseOrder", Flag("--l3-po")),
    ("flute-v2-post-transactions", "/transactionEnhancedData/salesTaxRate", Flag("--l2-tax-rate")),
    ("flute-v2-post-transactions", "/transactionEnhancedData/shippingCharges", Flag("--l3-shipping")),
    ("flute-v2-post-pos-transactions", "/terminalId", Flag("--terminal-id")),
    ("flute-v2-post-pos-transactions", "/posDeviceId", Flag("--pos-device-id")),
    ("flute-v2-post-pos-transactions", "/baseAmount", Flag("--amount")),
    ("flute-v2-post-pos-transactions", "/currencyCode", Flag("--currency-code")),
    // One of the two controls --wait drives. Always sent, because `false` is
    // what "answer immediately" has to say.
    ("flute-v2-post-pos-transactions", "/waitForAcceptanceByTerminal", Flag("--wait")),
    ("flute-v2-post-pos-transactions", "/captureMethod", Flag("--capture-method")),
    ("flute-v2-post-pos-transactions", "/initiationChannel", Flag("--initiation-channel")),
    ("flute-v2-post-pos-transactions", "/readingMethod", Flag("--reading-method")),
    ("flute-v2-post-pos-transactions", "/pricingType", Flag("--pricing-type")),
    // Optional here, unlike on a card transaction: the POS schema does not
    // declare it required and the account's default processor governs.
    ("flute-v2-post-pos-transactions", "/paymentProcessorId", Flag("--payment-processor-id")),
    ("flute-v2-post-pos-transactions", "/customerId", Flag("--customer-id")),
    ("flute-v2-post-pos-transactions", "/referenceId", Flag("--reference-id")),
    ("flute-v2-post-pos-transactions", "/requestPaymentMethodStorageConsent", Flag("--request-storage-consent")),
    ("flute-v2-post-pos-transactions", "/extraAmounts/tipAmount", Flag("--tip-amount")),
    ("flute-v2-post-pos-transactions", "/extraAmounts/tipRate", Flag("--tip-rate")),
    ("flute-v2-post-pos-transactions-posTransactionId-print-receipt", "/terminalId", Flag("--terminal-id")),
    ("flute-v2-get-pos-transactions", "?pageIndex", Flag("--page-index")),
    ("flute-v2-get-pos-transactions", "?pageSize", Flag("--page-size")),
    ("flute-v2-get-pos-transactions", "?sortBy", Flag("--sort-by")),
    ("flute-v2-get-pos-transactions", "?sortOrder", Flag("--asc/--desc")),
    ("flute-v2-get-pos-transactions", "?terminalId", Flag("--terminal-id")),
    ("flute-v2-get-pos-transactions", "?fromDate", Flag("--from")),
    ("flute-v2-get-pos-transactions", "?toDate", Flag("--to")),
    ("flute-v2-get-pos-transactions", "?posTransactionStatus", Flag("--status")),
    // The other control --wait drives, and a flag of `pos get` in its own
    // right. Omitted when absent, so the declared default governs.
    ("flute-v2-get-pos-transactions-posTransactionId", "?waitForTransactionProcessing", Flag("--wait")),
    ("flute-v2-get-terminals", "?pageIndex", Flag("--page-index")),
    ("flute-v2-get-terminals", "?pageSize", Flag("--page-size")),
    ("flute-v2-get-terminals", "?sortBy", Flag("--sort-by")),
    ("flute-v2-get-terminals", "?sortOrder", Flag("--asc/--desc")),
    // The **query** enum: `Ready`, `Busy`, `Offline`. The response field of
    // the same name declares `Active` instead of `Ready`.
    ("flute-v2-get-terminals", "?terminalStatus", Flag("--status")),
    ("flute-v2-get-terminals", "?terminalMode", Flag("--mode")),
    ("flute-v2-get-terminals", "?connectionStatus", Flag("--connection")),
    ("flute-v2-get-terminals", "?serialNumber", Flag("--serial-number")),
    ("flute-v2-get-terminals", "?search", Flag("--search")),
    ("flute-v2-get-settlements-batches", "?pageIndex", Flag("--page-index")),
    ("flute-v2-get-settlements-batches", "?pageSize", Flag("--page-size")),
    ("flute-v2-get-settlements-batches", "?sortBy", Flag("--sort-by")),
    // `desc` is the declared default **here**, unlike every other list, so
    // the flag names ascending rather than descending.
    ("flute-v2-get-settlements-batches", "?sortOrder", Flag("--asc")),
    ("flute-v2-get-settlements-batches", "?fromDate", Flag("--from")),
    ("flute-v2-get-settlements-batches", "?toDate", Flag("--to")),
    // Array-typed and repeatable. `settlements get` sends the same filter
    // with one value, which is why it needs no endpoint of its own.
    ("flute-v2-get-settlements-batches", "?batchIds", Flag("--batch-ids")),
    ("flute-v2-get-settlements-batches", "?paymentProcessorIds", Flag("--processor-ids")),
    ("flute-v2-get-settlements-batches", "?batchStatus", Flag("--status")),
    ("flute-v2-post-settlements-batches-close", "/paymentProcessorId", Flag("--payment-processor-id")),
    // Declared 0 to 22. `transactions create` carries the same idea as
    // `salesTaxRate` with a declared 0.01 to 100, and each is taken from
    // its own schema rather than harmonised.
    ("flute-v2-patch-settings-transaction-autofill", "/level2Settings/taxRate", Flag("--l2-tax-rate")),
    // A **rate** here, where `transactions create` sends an amount under
    // `shippingCharges` — hence the suffix rather than a bare --l3-shipping.
    ("flute-v2-patch-settings-transaction-autofill", "/level3Settings/shippingChargeRate", Flag("--l3-shipping-rate")),
    ("flute-v2-patch-settings-transaction-autofill", "/level3Settings/dutyChargeRate", Flag("--l3-duty-rate")),
    ("flute-v2-patch-settings-transaction-autofill", "/level3Settings/product/productName", Flag("--product-name")),
    // `code`, not the transaction product's `productCode`.
    ("flute-v2-patch-settings-transaction-autofill", "/level3Settings/product/code", Flag("--product-code")),
    // The server has no member for it. A patch carrying the field is
    // rejected outright — not ignored — so a flag here could only ever
    // break the request it was added to, and the read carries no
    // description either. Restore the flag when a live patch setting one
    // succeeds.
    ("flute-v2-patch-settings-transaction-autofill", "/level3Settings/product/description", Excluded(
        "the API rejects it: no member of the autofill product patch maps \
        to description, though the schema declares it",
        )),
    ("flute-v2-patch-settings-transaction-autofill", "/level3Settings/product/measurementUnit", Flag("--product-unit")),
    ("flute-v2-patch-settings-transaction-autofill", "/level3Settings/product/unitPrice", Flag("--product-unit-price")),
    ("flute-v2-patch-settings-transaction-autofill", "/level3Settings/product/quantity", Flag("--product-quantity")),
    // `discountPercentage`, not the transaction product's `discountRate`.
    ("flute-v2-patch-settings-transaction-autofill", "/level3Settings/product/discountPercentage", Flag("--product-discount")),
    ("flute-v2-post-payment-links", "/paymentMethods/card/enabled", Flag("--card-enabled")),
    ("flute-v2-post-payment-links", "/paymentMethods/card/processorId", Flag("--card-processor-id")),
    ("flute-v2-post-payment-links", "/paymentMethods/ach/enabled", Flag("--ach-enabled")),
    ("flute-v2-post-payment-links", "/paymentMethods/ach/processorId", Flag("--ach-processor-id")),
    // Omitted entirely for a flexible-amount link the payer fills in, which
    // is what the schema documents an absent amount as meaning.
    ("flute-v2-post-payment-links", "/baseAmount", Flag("--amount")),
    ("flute-v2-post-payment-links", "/currencyCode", Flag("--currency-code")),
    ("flute-v2-post-payment-links", "/linkType", Flag("--link-type")),
    ("flute-v2-post-payment-links", "/customerId", Flag("--customer-id")),
    ("flute-v2-post-payment-links", "/referenceId", Flag("--reference-id")),
    ("flute-v2-post-payment-links", "/name", Flag("--name")),
    ("flute-v2-post-payment-links", "/description", Flag("--description")),
    ("flute-v2-post-payment-links", "/expiresOn", Flag("--expires-on")),
    // Valued here, where `create`'s is a bare switch: a PATCH has to be
    // able to turn a method off.
    ("flute-v2-patch-payment-links-paymentLinkId", "/paymentMethods/card/enabled", Flag("--card-enabled")),
    ("flute-v2-patch-payment-links-paymentLinkId", "/paymentMethods/card/processorId", Flag("--card-processor-id")),
    ("flute-v2-patch-payment-links-paymentLinkId", "/paymentMethods/ach/enabled", Flag("--ach-enabled")),
    ("flute-v2-patch-payment-links-paymentLinkId", "/paymentMethods/ach/processorId", Flag("--ach-processor-id")),
    // Both directions: a value sets it, and either clearing spelling sends
    // the explicit null the merge patch reads as "make this flexible".
    ("flute-v2-patch-payment-links-paymentLinkId", "/baseAmount", Flag("--amount/--clear")),
    // Neither clearing spelling is offered: the schema says it cannot be
    // cleared.
    ("flute-v2-patch-payment-links-paymentLinkId", "/currencyCode", Flag("--currency-code")),
    ("flute-v2-patch-payment-links-paymentLinkId", "/linkType", Flag("--link-type")),
    ("flute-v2-patch-payment-links-paymentLinkId", "/paymentLinkStatus", Flag("--status")),
    ("flute-v2-patch-payment-links-paymentLinkId", "/customerId", Flag("--customer-id/--clear")),
    ("flute-v2-patch-payment-links-paymentLinkId", "/referenceId", Flag("--reference-id/--clear")),
    // Likewise unclearable by declaration.
    ("flute-v2-patch-payment-links-paymentLinkId", "/name", Flag("--name")),
    ("flute-v2-patch-payment-links-paymentLinkId", "/description", Flag("--description/--clear")),
    ("flute-v2-patch-payment-links-paymentLinkId", "/expiresOn", Flag("--expires-on/--clear")),
    // Two values here — `Email` and `Sms`. The transaction receipt's field
    // of the same name declares a third, so the enums are not shared.
    ("flute-v2-post-payment-links-paymentLinkId-share", "/shareBy", Flag("--share-by")),
    ("flute-v2-post-payment-links-paymentLinkId-share", "/recipient", Flag("--recipient")),
    // Required, so it is always sent — `false` included, which is what lets
    // the API refuse an unconsented share.
    ("flute-v2-post-payment-links-paymentLinkId-share", "/hasCustomerConsent", Flag("--consent")),
    ("flute-v2-get-payment-links", "?pageIndex", Flag("--page-index")),
    ("flute-v2-get-payment-links", "?pageSize", Flag("--page-size")),
    ("flute-v2-get-payment-links", "?search", Flag("--search")),
    ("flute-v2-get-payment-links", "?linkType", Flag("--link-type")),
    ("flute-v2-get-payment-links", "?paymentLinkStatus", Flag("--status")),
    ("flute-v2-get-payment-links", "?sortBy", Flag("--sort-by")),
    ("flute-v2-get-payment-links", "?sortOrder", Flag("--asc/--desc")),
    ("flute-v2-post-payment-sessions", "/mode", Flag("--mode")),
    // Three documented meanings, none of them expressible in the schema:
    // greater than zero for a paying session, exactly zero for a vault-only
    // one, and absent for a flexible amount set at checkout.
    ("flute-v2-post-payment-sessions", "/amount", Flag("--amount")),
    ("flute-v2-post-payment-sessions", "/tipAmount", Flag("--tip-amount")),
    ("flute-v2-post-payment-sessions", "/customerId", Flag("--customer-id")),
    ("flute-v2-post-payment-sessions", "/customerHandling", Flag("--customer-handling")),
    ("flute-v2-post-payment-sessions", "/referenceId", Flag("--reference-id")),
    ("flute-v2-post-payment-sessions", "/returnUrl", Flag("--return-url")),
    ("flute-v2-post-payment-sessions", "/skipAddressVerification", Flag("--skip-address-verification")),
    ("flute-v2-post-payment-sessions", "/pageName", Flag("--page-name")),
    ("flute-v2-post-payment-sessions", "/paymentNotes", Flag("--payment-notes")),
    ("flute-v2-post-payment-sessions", "/afterCompletionMessage", Flag("--after-completion-message")),
    ("flute-v2-post-payment-sessions", "/expiresAt", Flag("--expires-at")),
    // A free-form string map, so one repeatable `key=value` flag rather than
    // a flag per key. Its leaf is the container, because the schema names
    // no properties inside it.
    ("flute-v2-post-payment-sessions", "/metadata", Flag("--metadata")),
    ("flute-v2-post-payment-sessions", "/paymentMethods/card/enabled", Flag("--card-enabled")),
    ("flute-v2-post-payment-sessions", "/paymentMethods/card/processorId", Flag("--card-processor-id")),
    ("flute-v2-post-payment-sessions", "/paymentMethods/ach/enabled", Flag("--ach-enabled")),
    ("flute-v2-post-payment-sessions", "/paymentMethods/ach/processorId", Flag("--ach-processor-id")),
    ("flute-v2-post-api-keys", "/merchantId", Flag("--merchant-id")),
    ("flute-v2-post-api-keys", "/apiKeyName", Flag("--name")),
    // The only query parameter this group has. `GetApiKeysResponseDto`
    // declares no `pageInfo`, so there is no pagination to expose here and
    // the shared flags are rejected as unknown.
    ("flute-v2-get-api-keys", "?merchantId", Flag("--merchant-id")),
];
