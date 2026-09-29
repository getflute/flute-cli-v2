//! The `transactions` command tree: subcommands, their flags, and the value
//! enums the flags take.

use crate::cli::address::{BillingArgs, ShippingArgs};
use crate::cli::common::{
    AccountHolderType, AccountType, CaptureMethod, PaginationArgs, PricingType,
};
use crate::cli::money::{parse_amount, parse_rate};
use rust_decimal::Decimal;

/// Which container a saved `--payment-method-id` belongs in.
///
/// A stored instrument id says nothing about its own type, and `cardData` and
/// `achData` are different objects, so the caller has to say which.
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Instrument {
    Card,
    Ach,
}

/// Case-sensitive on the wire: the enum is exactly `Web`, `PPD` and `CCD`,
/// which is neither all-caps nor all-title-case.
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum, serde::Serialize)]
pub enum SecCode {
    Web,
    #[serde(rename = "PPD")]
    Ppd,
    #[serde(rename = "CCD")]
    Ccd,
}

/// The declared `sourceType` values, capitalised on the wire.
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum, serde::Serialize)]
pub enum SourceType {
    ApiKey,
    Invoice,
    MobileApp,
    Portal,
    QuickPayment,
    Subscription,
    TapToPay,
    Terminal,
    WebComponent,
}

/// The declared `transactionStatus` values, capitalised on the wire.
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum, serde::Serialize)]
pub enum TransactionStatus {
    Authorized,
    Cancelled,
    Captured,
    ChargedBack,
    Cleared,
    Declined,
    Failed,
    Held,
    HeldByProcessor,
    Informational,
    InProgress,
    PartiallyAuthorized,
    Pending,
    Refunded,
    Scheduled,
    Settled,
    Verified,
    Voided,
}

/// How a receipt is shared.
///
/// **SMS is the only channel the endpoint accepts.** Its validator answers
/// `ShareBy must be Sms; it is the only supported channel.` for anything else,
/// so the other two are refused here rather than on a round trip. The
/// published `shareBy` documents an Email/None/Sms table anyway, and declares
/// an E.164 pattern its own `Sms` example fails — the conformance harness
/// carries a narrow exemption for the pattern.
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum, serde::Serialize)]
pub enum ShareBy {
    Sms,
}

/// One variant per command, and the arg-bearing ones are large.
///
/// `large_enum_variant` is allowed rather than fixed: boxing a variant breaks
/// `#[derive(Subcommand)]`, which needs the args struct inline, and the enum
/// is constructed exactly once per process. The lint is measuring a cost that
/// is not paid here.
#[allow(clippy::large_enum_variant)]
#[derive(clap::Subcommand, Debug)]
pub enum TransactionsCommand {
    /// Charge a card or bank account immediately (POST /v2/transactions).
    ///
    /// Use `--capture-method manual` to authorise without capturing.
    Create(CreateTransactionArgs),
    /// Capture a previously authorised transaction
    /// (POST /v2/transactions/{transactionId}/capture).
    Capture {
        /// UUID of the transaction to capture (required).
        #[arg(long)]
        transaction_id: String,
        /// Capture amount for a partial capture. Omit for a full capture.
        #[arg(long, value_parser = parse_amount, allow_negative_numbers = true)]
        amount: Option<Decimal>,
    },
    /// Void or refund a transaction
    /// (POST /v2/transactions/{transactionId}/reversal).
    ///
    /// One endpoint for both: whether the transaction has settled is detected
    /// server-side.
    Reversal {
        /// UUID of the transaction to reverse (required).
        #[arg(long)]
        transaction_id: String,
        /// Refund part of a settled card transaction. Refused for an unsettled
        /// or ACH transaction, which the API reverses in full. Omit for a full
        /// reversal.
        #[arg(long, value_parser = parse_amount, allow_negative_numbers = true)]
        amount: Option<Decimal>,
    },
    /// Card or ACH credit payment (POST /v2/transactions/credit).
    Credit(CreditArgs),
    /// Adjust the tip on a transaction
    /// (POST /v2/transactions/{transactionId}/tip-adjustment).
    TipAdjust {
        /// UUID of the transaction to adjust (required).
        #[arg(long)]
        transaction_id: String,
        /// New tip amount. Plain decimal, e.g. `3.50`.
        #[arg(long, value_parser = parse_amount, id = "adjust_tip_amount", value_name = "TIP_AMOUNT", allow_negative_numbers = true)]
        tip_amount: Option<Decimal>,
        /// New tip rate as a percentage, e.g. `18.5` for 18.5%.
        #[arg(long, value_parser = parse_rate, id = "adjust_tip_rate", value_name = "TIP_RATE", allow_negative_numbers = true)]
        tip_rate: Option<Decimal>,
    },
    /// Place an ACH transaction on hold
    /// (POST /v2/transactions/{transactionId}/ach-hold).
    AchHold {
        /// ACH transaction UUID to hold (positional).
        transaction_id: String,
    },
    /// Release an ACH transaction from hold
    /// (POST /v2/transactions/{transactionId}/ach-release).
    AchRelease {
        /// ACH transaction UUID to release (positional).
        transaction_id: String,
    },
    /// Send a transaction receipt to a customer
    /// (POST /v2/transactions/{transactionId}/share-receipt).
    ShareReceipt(ShareReceiptArgs),
    /// Calculate the totals for an amount without creating a transaction
    /// (POST /v2/transactions/calculate-amount).
    CalculateAmount(CalculateAmountArgs),
    /// Fetch a single transaction by ID (GET /v2/transactions/{transactionId}).
    Get {
        /// Transaction UUID to retrieve (positional).
        transaction_id: String,
    },
    /// List transactions for the current merchant (GET /v2/transactions).
    List(ListTransactionsArgs),
    /// Show rich details for a single transaction
    /// (GET /v2/transactions/{transactionId}).
    ///
    /// Displays the transaction and payment method types, the decline details,
    /// the instrument — a card with its authorization code and address check,
    /// or a bank account — and the amount breakdown. `transactions get` prints
    /// the whole response instead.
    Inspect {
        /// Transaction UUID to inspect (positional).
        transaction_id: String,
    },
}

#[derive(clap::Args, Debug, Default)]
pub struct ListTransactionsArgs {
    #[command(flatten)]
    pub pagination: PaginationArgs,
    /// Sort results by this field.
    #[arg(
        long,
        id = "txn_sort_by",
        value_name = "SORT_BY",
        value_parser = [
            "cardTokenType",
            "customerCompanyName",
            "customerName",
            "maskedCardNumber",
            "merchantCompanyName",
            "paymentMethodType",
            "processedAmount",
            "transactionDateTime",
            "transactionId",
            "transactionStatus",
        ]
    )]
    pub sort_by: Option<String>,
    /// Sort ascending. With neither `--asc` nor `--desc`, results come back
    /// newest first.
    #[arg(long, id = "txn_asc", conflicts_with = "txn_desc")]
    pub asc: bool,
    /// Sort descending.
    #[arg(long, id = "txn_desc")]
    pub desc: bool,
    /// Filter results from this date-time inclusive (ISO 8601).
    #[arg(long = "from", id = "txn_from", value_name = "FROM_DATE")]
    pub from_date: Option<String>,
    /// Filter results up to this date-time inclusive (ISO 8601).
    #[arg(long = "to", id = "txn_to", value_name = "TO_DATE")]
    pub to_date: Option<String>,
    /// Filter by the transaction source.
    #[arg(long, value_enum, id = "txn_source_type", value_name = "SOURCE_TYPE")]
    pub source_type: Option<SourceType>,
    /// Filter by the source identifier.
    #[arg(long, id = "txn_source_id", value_name = "SOURCE_ID")]
    pub source_id: Option<String>,
    /// Filter by settlement batch ID.
    #[arg(long, id = "txn_batch_id", value_name = "BATCH_ID")]
    pub batch_id: Option<String>,
    /// Filter results by status.
    #[arg(
        long = "status",
        value_enum,
        ignore_case = true,
        id = "txn_status",
        value_name = "TRANSACTION_STATUS"
    )]
    pub transaction_status: Option<TransactionStatus>,
    /// Filter by payment method type.
    #[arg(
        long,
        id = "txn_payment_method_type",
        value_name = "PAYMENT_METHOD_TYPE"
    )]
    pub payment_method_type: Option<String>,
    /// Filter by customer UUID.
    #[arg(long, id = "txn_list_customer_id", value_name = "CUSTOMER_ID")]
    pub customer_id: Option<String>,
    /// Filter by merchant UUID.
    #[arg(long, id = "txn_merchant_id", value_name = "MERCHANT_ID")]
    pub merchant_id: Option<String>,
    /// Filter results at or above this amount.
    #[arg(long, value_parser = parse_amount, id = "txn_min_amount", value_name = "MIN_AMOUNT", allow_negative_numbers = true)]
    pub min_amount: Option<Decimal>,
    /// Filter results at or below this amount.
    #[arg(long, value_parser = parse_amount, id = "txn_max_amount", value_name = "MAX_AMOUNT", allow_negative_numbers = true)]
    pub max_amount: Option<Decimal>,
    /// Filter by merchant-assigned reference ID.
    #[arg(long, id = "txn_reference_id", value_name = "REFERENCE_ID")]
    pub reference_id: Option<String>,
}

#[derive(clap::Args, Debug, Default)]
pub struct CreditArgs {
    /// Transaction amount (required). Plain decimal, e.g. `500.00`.
    #[arg(long = "amount", value_parser = parse_amount, id = "credit_amount",
          value_name = "AMOUNT", allow_negative_numbers = true)]
    pub base_amount: Decimal,
    /// Payment processor UUID (required).
    #[arg(
        long,
        id = "credit_payment_processor_id",
        value_name = "PAYMENT_PROCESSOR_ID"
    )]
    pub payment_processor_id: String,
    /// Merchant-assigned reference ID for idempotency tracking (required).
    #[arg(long, id = "credit_reference_id", value_name = "REFERENCE_ID")]
    pub reference_id: String,
    /// Currency code, e.g. `USD`.
    #[arg(long, id = "credit_currency_code", value_name = "CURRENCY_CODE")]
    pub currency_code: Option<String>,
    /// Customer UUID for vault-linked credits.
    #[arg(long, id = "credit_customer_id", value_name = "CUSTOMER_ID")]
    pub customer_id: Option<String>,
    #[command(flatten)]
    pub instrument: InstrumentArgs,
    #[command(flatten)]
    pub contact: ContactArgs,
    #[command(flatten)]
    pub billing: BillingArgs,
    #[command(flatten)]
    pub shipping: ShippingArgs,
}

#[derive(clap::Args, Debug)]
pub struct ShareReceiptArgs {
    /// Transaction UUID (positional).
    pub transaction_id: String,
    /// How to send the receipt. `sms` is the only channel the API accepts.
    #[arg(long, value_enum)]
    pub share_by: ShareBy,
    /// Customer mobile number in E.164 form, for an SMS receipt.
    #[arg(long)]
    pub recipient: String,
    /// The customer has consented to receive the receipt (required for SMS).
    #[arg(long = "consent")]
    pub has_customer_consent: bool,
}

#[derive(clap::Args, Debug, Default)]
pub struct CalculateAmountArgs {
    /// Amount to calculate totals for (required). Plain decimal, e.g. `100.00`.
    #[arg(long = "amount", value_parser = parse_amount, id = "calc_amount", value_name = "AMOUNT", allow_negative_numbers = true)]
    pub base_amount: Decimal,
    /// Currency code, e.g. `USD`.
    #[arg(long, id = "calc_currency_code", value_name = "CURRENCY_CODE")]
    pub currency_code: Option<String>,
    /// Pricing type: `card` or `cash`.
    #[arg(
        long,
        value_enum,
        id = "calc_pricing_type",
        value_name = "PRICING_TYPE"
    )]
    pub pricing_type: Option<PricingType>,
    /// Tip amount. Plain decimal, e.g. `5.00`.
    #[arg(long, value_parser = parse_amount, id = "calc_tip_amount", value_name = "TIP_AMOUNT", allow_negative_numbers = true)]
    pub tip_amount: Option<Decimal>,
    /// Tip rate as a percentage, e.g. `18.5` for 18.5%.
    #[arg(long, value_parser = parse_rate, id = "calc_tip_rate", value_name = "TIP_RATE", allow_negative_numbers = true)]
    pub tip_rate: Option<Decimal>,
    /// Discount amount. Plain decimal, e.g. `5.00`.
    #[arg(long, value_parser = parse_amount, id = "calc_discount_amount", value_name = "DISCOUNT_AMOUNT", allow_negative_numbers = true)]
    pub discount_amount: Option<Decimal>,
    /// Discount rate as a percentage, e.g. `10` for 10%.
    #[arg(long, value_parser = parse_rate, id = "calc_discount_rate", value_name = "DISCOUNT_RATE", allow_negative_numbers = true)]
    pub discount_rate: Option<Decimal>,
    /// Surcharge rate as a percentage, e.g. `3.5` for 3.5%.
    #[arg(long, value_parser = parse_rate, id = "calc_surcharge_rate", value_name = "SURCHARGE_RATE", allow_negative_numbers = true)]
    pub surcharge_rate: Option<Decimal>,
}

/// The payment instrument, shared by `transactions create` and
/// `transactions credit`.
///
/// One declaration rather than two: the four shapes, their mutual exclusion
/// and the ACH rules are identical on both commands, and a second copy is a
/// second place for a wire name to drift.
#[derive(clap::Args, Debug, Default)]
pub struct InstrumentArgs {
    /// Card PAN (primary account number), e.g. `4111111111111111`.
    #[arg(long)]
    pub card: Option<String>,
    /// Card expiry in MM/YY or MM/YYYY format, e.g. `12/26`.
    #[arg(long)]
    pub exp: Option<String>,
    /// Card CVV/security code.
    #[arg(long)]
    pub cvv: Option<String>,
    /// Payment method UUID (stored card or bank account) to charge instead of
    /// raw details.
    #[arg(long)]
    pub payment_method_id: Option<String>,
    /// Which container `--payment-method-id` belongs in: `card` or `ach`.
    #[arg(long, value_enum)]
    pub instrument: Option<Instrument>,
    /// Bank account number.
    #[arg(long)]
    pub ach_account_number: Option<String>,
    /// Routing number (ABA).
    #[arg(long)]
    pub ach_routing_number: Option<String>,
    /// Account type: `checking` or `savings`.
    #[arg(long, value_enum)]
    pub ach_account_type: Option<AccountType>,
    /// Account holder type: `business` or `personal`.
    #[arg(long, value_enum)]
    pub ach_account_holder_type: Option<AccountHolderType>,
    /// Tax ID (optional).
    #[arg(long)]
    pub ach_tax_id: Option<String>,
    /// ACH SEC code: `web`, `ppd` or `ccd`. Required for ACH.
    #[arg(long, value_enum)]
    pub sec_code: Option<SecCode>,
    /// End-customer IP address. Default `127.0.0.1`. Required for ACH.
    #[arg(long = "requester-ip", value_name = "REQUESTER_IP")]
    pub requester_ip_address: Option<String>,
    /// Request same-day settlement.
    #[arg(long = "same-day")]
    pub is_same_day_processing: bool,
}

/// The `contactInfo` block, shared for the same reason.
#[derive(clap::Args, Debug, Default)]
pub struct ContactArgs {
    /// Contact first name.
    #[arg(long = "contact-first-name", value_name = "FIRST_NAME")]
    pub first_name: Option<String>,
    /// Contact last name.
    #[arg(long = "contact-last-name", value_name = "LAST_NAME")]
    pub last_name: Option<String>,
    /// Contact company name.
    #[arg(long = "contact-company", value_name = "COMPANY_NAME")]
    pub company_name: Option<String>,
    /// Contact email address.
    #[arg(long = "contact-email", value_name = "EMAIL")]
    pub email: Option<String>,
    /// Contact mobile/phone number in E.164 form. Required for new bank
    /// details.
    #[arg(long = "contact-phone", value_name = "MOBILE_PHONE_NUMBER")]
    pub mobile_phone_number: Option<String>,
    /// Record the contact's consent to receive SMS.
    #[arg(long = "contact-sms-consent")]
    pub has_sms_consent: bool,
}

#[derive(clap::Args, Debug, Default)]
pub struct CreateTransactionArgs {
    /// Transaction amount (required). Plain decimal, e.g. `100.00`.
    #[arg(long, value_parser = crate::cli::money::parse_amount, allow_negative_numbers = true)]
    pub amount: Decimal,
    /// Payment processor UUID (required).
    #[arg(long)]
    pub payment_processor_id: String,
    /// Customer UUID for vault-linked charges.
    #[arg(long)]
    pub customer_id: Option<String>,
    /// Capture method: `auto` charges immediately, `manual` authorises for
    /// later capture.
    #[arg(long, value_enum, default_value_t = CaptureMethod::Auto)]
    pub capture_method: CaptureMethod,
    /// Merchant-assigned reference ID for idempotency tracking.
    #[arg(long)]
    pub reference_id: Option<String>,
    /// Currency code, e.g. `USD`.
    #[arg(long)]
    pub currency_code: Option<String>,

    #[command(flatten)]
    pub instrument: InstrumentArgs,
    #[command(flatten)]
    pub contact: ContactArgs,

    /// Tip amount to add to the transaction.
    #[arg(long, value_parser = parse_amount, allow_negative_numbers = true)]
    pub tip_amount: Option<Decimal>,
    /// Tip rate as a percentage, e.g. `18.5` for 18.5%.
    #[arg(long, value_parser = parse_rate, allow_negative_numbers = true)]
    pub tip_rate: Option<Decimal>,
    /// Discount amount to subtract from the transaction.
    #[arg(long, value_parser = parse_amount, allow_negative_numbers = true)]
    pub discount_amount: Option<Decimal>,
    /// Discount rate as a percentage, e.g. `10` for 10%.
    #[arg(long, value_parser = parse_rate, allow_negative_numbers = true)]
    pub discount_rate: Option<Decimal>,
    /// Surcharge rate as a percentage, e.g. `3.5` for 3.5%.
    #[arg(long, value_parser = parse_rate, allow_negative_numbers = true)]
    pub surcharge_rate: Option<Decimal>,

    /// Level-2 sales tax rate as a percentage, 0.01-100.
    #[arg(long = "l2-tax-rate", value_parser = parse_rate, value_name = "SALES_TAX_RATE", allow_negative_numbers = true)]
    pub sales_tax_rate: Option<Decimal>,
    /// Level-3 invoice number.
    #[arg(long = "l3-invoice", value_name = "INVOICE_NUMBER")]
    pub invoice_number: Option<String>,
    /// Level-3 purchase order number.
    #[arg(long = "l3-po", value_name = "PURCHASE_ORDER")]
    pub purchase_order: Option<String>,
    /// Level-3 shipping charges.
    #[arg(long = "l3-shipping", value_parser = parse_amount, value_name = "SHIPPING_CHARGES", allow_negative_numbers = true)]
    pub shipping_charges: Option<Decimal>,
    /// Level-3 product line item, as comma-separated `key=value` pairs.
    /// Repeat the flag for multiple products.
    ///
    /// Keys: `productName`, `productDescription`, `productCode`, `unitPrice`,
    /// `measurementUnit`, `quantity`, `taxAmount`, `discountRate`.
    #[arg(long = "l3-product", value_name = "PRODUCT")]
    pub products: Vec<String>,

    /// Mark the transaction as initiated by the customer.
    #[arg(long = "customer-initiated")]
    pub is_customer_initiated_transaction: bool,
    /// Pricing type: `card` or `cash`.
    #[arg(long, value_enum)]
    pub pricing_type: Option<PricingType>,

    #[command(flatten)]
    pub billing: BillingArgs,
    #[command(flatten)]
    pub shipping: ShippingArgs,
}
