//! The API surface matrix (layer 4a).
//!
//! The contract matrix proves every *operation* is reachable. It says nothing
//! about whether every *field* is: `customers list` can hold a passing
//! contract row while `fullName`, `email`, `companyName`, `mobilePhoneNumber`,
//! `createdFrom`, `createdTo`, `sortBy` and `asc` are all unreachable. The
//! endpoint is covered and most of its capability is not.

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

pub static SURFACE: &[Field] = &[
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/paymentProcessorId",
        exposure: Exposure::Flag("--payment-processor-id"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/baseAmount",
        exposure: Exposure::Flag("--amount"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/referenceId",
        // Required here, unlike on `create`.
        exposure: Exposure::Flag("--reference-id"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/currencyCode",
        exposure: Exposure::Flag("--currency-code"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/customerId",
        exposure: Exposure::Flag("--customer-id"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/billingAddress/addressLine1",
        exposure: Exposure::Flag("--billing-line1"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/billingAddress/addressLine2",
        exposure: Exposure::Flag("--billing-line2"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/billingAddress/city",
        exposure: Exposure::Flag("--billing-city"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/billingAddress/countryCode",
        exposure: Exposure::Flag("--billing-country"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/billingAddress/postalCode",
        exposure: Exposure::Flag("--billing-postal-code"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/billingAddress/stateCode",
        exposure: Exposure::Flag("--billing-state"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/shippingAddress/addressLine1",
        exposure: Exposure::Flag("--shipping-line1"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/shippingAddress/addressLine2",
        exposure: Exposure::Flag("--shipping-line2"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/shippingAddress/city",
        exposure: Exposure::Flag("--shipping-city"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/shippingAddress/countryCode",
        exposure: Exposure::Flag("--shipping-country"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/shippingAddress/postalCode",
        exposure: Exposure::Flag("--shipping-postal-code"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/shippingAddress/stateCode",
        exposure: Exposure::Flag("--shipping-state"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/contactInfo/firstName",
        exposure: Exposure::Flag("--contact-first-name"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/contactInfo/lastName",
        exposure: Exposure::Flag("--contact-last-name"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/contactInfo/companyName",
        exposure: Exposure::Flag("--contact-company"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/contactInfo/email",
        exposure: Exposure::Flag("--contact-email"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/contactInfo/mobilePhoneNumber",
        exposure: Exposure::Flag("--contact-phone"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/contactInfo/hasSmsConsent",
        exposure: Exposure::Flag("--contact-sms-consent"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/creditDetails/cardData/paymentMethodId",
        exposure: Exposure::Flag("--payment-method-id/--instrument"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/creditDetails/cardData/paymentMethodDetails/cardNumber",
        exposure: Exposure::Flag("--card"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/creditDetails/cardData/paymentMethodDetails/securityCode",
        exposure: Exposure::Flag("--cvv"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/creditDetails/cardData/paymentMethodDetails/expirationMonth",
        exposure: Exposure::Flag("--exp"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/creditDetails/cardData/paymentMethodDetails/expirationYear",
        exposure: Exposure::Flag("--exp"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/creditDetails/achData/paymentMethodId",
        exposure: Exposure::Flag("--payment-method-id/--instrument"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/creditDetails/achData/paymentMethodDetails/accountNumber",
        exposure: Exposure::Flag("--ach-account-number"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/creditDetails/achData/paymentMethodDetails/routingNumber",
        exposure: Exposure::Flag("--ach-routing-number"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/creditDetails/achData/paymentMethodDetails/accountType",
        exposure: Exposure::Flag("--ach-account-type"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/creditDetails/achData/paymentMethodDetails/accountHolderType",
        exposure: Exposure::Flag("--ach-account-holder-type"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/creditDetails/achData/paymentMethodDetails/taxId",
        exposure: Exposure::Flag("--ach-tax-id"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/creditDetails/achData/secCode",
        exposure: Exposure::Flag("--sec-code"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/creditDetails/achData/requesterIpAddress",
        exposure: Exposure::Flag("--requester-ip"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-credit",
        field: "/creditDetails/achData/isSameDayProcessing",
        exposure: Exposure::Flag("--same-day"),
    },
    Field {
        operation_id: "flute-v2-get-transactions",
        field: "?pageIndex",
        exposure: Exposure::Flag("--page-index"),
    },
    Field {
        operation_id: "flute-v2-get-transactions",
        field: "?pageSize",
        exposure: Exposure::Flag("--page-size"),
    },
    Field {
        operation_id: "flute-v2-get-transactions",
        field: "?sortBy",
        exposure: Exposure::Flag("--sort-by"),
    },
    Field {
        operation_id: "flute-v2-get-transactions",
        field: "?sortOrder",
        exposure: Exposure::Flag("--asc/--desc"),
    },
    Field {
        operation_id: "flute-v2-get-transactions",
        field: "?fromDate",
        exposure: Exposure::Flag("--from"),
    },
    Field {
        operation_id: "flute-v2-get-transactions",
        field: "?toDate",
        exposure: Exposure::Flag("--to"),
    },
    Field {
        operation_id: "flute-v2-get-transactions",
        field: "?sourceType",
        exposure: Exposure::Flag("--source-type"),
    },
    Field {
        operation_id: "flute-v2-get-transactions",
        field: "?sourceId",
        exposure: Exposure::Flag("--source-id"),
    },
    Field {
        operation_id: "flute-v2-get-transactions",
        field: "?batchId",
        exposure: Exposure::Flag("--batch-id"),
    },
    Field {
        operation_id: "flute-v2-get-transactions",
        field: "?transactionStatus",
        exposure: Exposure::Flag("--status"),
    },
    Field {
        operation_id: "flute-v2-get-transactions",
        field: "?paymentMethodType",
        exposure: Exposure::Flag("--payment-method-type"),
    },
    Field {
        operation_id: "flute-v2-get-transactions",
        field: "?customerId",
        exposure: Exposure::Flag("--customer-id"),
    },
    Field {
        operation_id: "flute-v2-get-transactions",
        field: "?merchantId",
        exposure: Exposure::Flag("--merchant-id"),
    },
    Field {
        operation_id: "flute-v2-get-transactions",
        field: "?minAmount",
        exposure: Exposure::Flag("--min-amount"),
    },
    Field {
        operation_id: "flute-v2-get-transactions",
        field: "?maxAmount",
        exposure: Exposure::Flag("--max-amount"),
    },
    Field {
        operation_id: "flute-v2-get-transactions",
        field: "?referenceId",
        exposure: Exposure::Flag("--reference-id"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-transactionId-capture",
        field: "/captureAmount",
        // The schema declares `captureAmount` and the operation's own
        // request example sends `amount`. The schema is normative, so this is
        // what the flag reaches.
        exposure: Exposure::Flag("--amount"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-transactionId-reversal",
        field: "/reversalAmount",
        exposure: Exposure::Flag("--amount"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-transactionId-tip-adjustment",
        field: "/tipAmount",
        exposure: Exposure::Flag("--tip-amount"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-transactionId-tip-adjustment",
        field: "/tipRate",
        exposure: Exposure::Flag("--tip-rate"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-transactionId-share-receipt",
        field: "/shareBy",
        exposure: Exposure::Flag("--share-by"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-transactionId-share-receipt",
        field: "/recipient",
        exposure: Exposure::Flag("--recipient"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-transactionId-share-receipt",
        field: "/hasCustomerConsent",
        exposure: Exposure::Flag("--consent"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-calculate-amount",
        field: "/baseAmount",
        exposure: Exposure::Flag("--amount"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-calculate-amount",
        field: "/currencyCode",
        exposure: Exposure::Flag("--currency-code"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-calculate-amount",
        field: "/pricingType",
        exposure: Exposure::Flag("--pricing-type"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-calculate-amount",
        field: "/tipAmount",
        exposure: Exposure::Flag("--tip-amount"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-calculate-amount",
        field: "/tipRate",
        exposure: Exposure::Flag("--tip-rate"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-calculate-amount",
        field: "/discountAmount",
        exposure: Exposure::Flag("--discount-amount"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-calculate-amount",
        field: "/discountRate",
        exposure: Exposure::Flag("--discount-rate"),
    },
    Field {
        operation_id: "flute-v2-post-transactions-calculate-amount",
        field: "/surchargeRate",
        exposure: Exposure::Flag("--surcharge-rate"),
    },
    Field {
        operation_id: "flute-v2-get-payment-methods",
        field: "?pageIndex",
        exposure: Exposure::Flag("--page-index"),
    },
    Field {
        operation_id: "flute-v2-get-payment-methods",
        field: "?pageSize",
        exposure: Exposure::Flag("--page-size"),
    },
    Field {
        operation_id: "flute-v2-get-payment-methods",
        field: "?asc",
        // As on `customers list`: the API defaults `asc` to true, so the
        // flag names the non-default direction.
        exposure: Exposure::Flag("--desc"),
    },
    Field {
        operation_id: "flute-v2-get-payment-methods",
        field: "?sortBy",
        exposure: Exposure::Flag("--sort-by"),
    },
    Field {
        operation_id: "flute-v2-get-payment-methods",
        field: "?search",
        exposure: Exposure::Flag("--search"),
    },
    Field {
        operation_id: "flute-v2-get-payment-methods",
        field: "?createdFrom",
        exposure: Exposure::Flag("--created-from"),
    },
    Field {
        operation_id: "flute-v2-get-payment-methods",
        field: "?createdTo",
        exposure: Exposure::Flag("--created-to"),
    },
    Field {
        operation_id: "flute-v2-get-payment-methods",
        field: "?customerId",
        exposure: Exposure::Flag("--customer-id"),
    },
    Field {
        operation_id: "flute-v2-post-payment-methods-cards",
        field: "/cardNumber",
        exposure: Exposure::Flag("--card"),
    },
    Field {
        operation_id: "flute-v2-post-payment-methods-cards",
        field: "/securityCode",
        exposure: Exposure::Flag("--cvv"),
    },
    Field {
        operation_id: "flute-v2-post-payment-methods-cards",
        field: "/expirationMonth",
        // One MM/YY flag fills both halves, as on `transactions create`.
        exposure: Exposure::Flag("--exp"),
    },
    Field {
        operation_id: "flute-v2-post-payment-methods-cards",
        field: "/expirationYear",
        exposure: Exposure::Flag("--exp"),
    },
    Field {
        operation_id: "flute-v2-post-payment-methods-cards",
        field: "/customerId",
        exposure: Exposure::Flag("--customer-id"),
    },
    Field {
        operation_id: "flute-v2-post-payment-methods-cards",
        field: "/paymentName",
        // The label is `paymentName` here and `name` on the ACH route. One
        // flag covers both, so an inconsistency in the API does not become
        // one in the CLI.
        exposure: Exposure::Flag("--name"),
    },
    Field {
        operation_id: "flute-v2-post-payment-methods-ach",
        field: "/accountNumber",
        exposure: Exposure::Flag("--account"),
    },
    Field {
        operation_id: "flute-v2-post-payment-methods-ach",
        field: "/routingNumber",
        exposure: Exposure::Flag("--routing"),
    },
    Field {
        operation_id: "flute-v2-post-payment-methods-ach",
        field: "/accountType",
        exposure: Exposure::Flag("--account-type"),
    },
    Field {
        operation_id: "flute-v2-post-payment-methods-ach",
        field: "/accountHolderType",
        exposure: Exposure::Flag("--account-holder-type"),
    },
    Field {
        operation_id: "flute-v2-post-payment-methods-ach",
        field: "/taxId",
        exposure: Exposure::Flag("--tax-id"),
    },
    Field {
        operation_id: "flute-v2-post-payment-methods-ach",
        field: "/customerId",
        exposure: Exposure::Flag("--customer-id"),
    },
    Field {
        operation_id: "flute-v2-post-payment-methods-ach",
        field: "/name",
        exposure: Exposure::Flag("--name"),
    },
    Field {
        operation_id: "flute-v2-post-payment-methods-ach",
        field: "/companyName",
        exposure: Exposure::Flag("--company-name"),
    },
    Field {
        operation_id: "flute-v2-patch-payment-methods-paymentMethodId",
        field: "/paymentName",
        exposure: Exposure::Flag("--name"),
    },
    Field {
        operation_id: "flute-v2-post-payment-methods-paymentMethodId-set-default",
        field: "?customerId",
        // Required by the API, so clap requires it too.
        exposure: Exposure::Flag("--customer-id"),
    },
    Field {
        operation_id: "flute-v2-get-customers",
        field: "?pageIndex",
        exposure: Exposure::Flag("--page-index"),
    },
    Field {
        operation_id: "flute-v2-get-customers",
        field: "?pageSize",
        exposure: Exposure::Flag("--page-size"),
    },
    Field {
        operation_id: "flute-v2-get-customers",
        field: "?asc",
        // The API defaults `asc` to true and a bare clap switch cannot
        // express false, so the flag names the non-default direction and the
        // parameter is sent only when it is asked for.
        exposure: Exposure::Flag("--desc"),
    },
    Field {
        operation_id: "flute-v2-get-customers",
        field: "?sortBy",
        exposure: Exposure::Flag("--sort-by"),
    },
    Field {
        operation_id: "flute-v2-get-customers",
        field: "?fullName",
        exposure: Exposure::Flag("--full-name"),
    },
    Field {
        operation_id: "flute-v2-get-customers",
        field: "?email",
        exposure: Exposure::Flag("--email"),
    },
    Field {
        operation_id: "flute-v2-get-customers",
        field: "?companyName",
        exposure: Exposure::Flag("--company-name"),
    },
    Field {
        operation_id: "flute-v2-get-customers",
        field: "?mobilePhoneNumber",
        exposure: Exposure::Flag("--mobile"),
    },
    Field {
        operation_id: "flute-v2-get-customers",
        field: "?createdFrom",
        exposure: Exposure::Flag("--created-from"),
    },
    Field {
        operation_id: "flute-v2-get-customers",
        field: "?createdTo",
        exposure: Exposure::Flag("--created-to"),
    },
    Field {
        operation_id: "flute-v2-patch-customers-customerId",
        field: "/billingAddress/addressLine1",
        exposure: Exposure::Flag("--billing-line1"),
    },
    Field {
        operation_id: "flute-v2-patch-customers-customerId",
        field: "/billingAddress/addressLine2",
        exposure: Exposure::Flag("--billing-line2"),
    },
    Field {
        operation_id: "flute-v2-patch-customers-customerId",
        field: "/billingAddress/city",
        exposure: Exposure::Flag("--billing-city"),
    },
    Field {
        operation_id: "flute-v2-patch-customers-customerId",
        field: "/billingAddress/countryCode",
        exposure: Exposure::Flag("--billing-country"),
    },
    Field {
        operation_id: "flute-v2-patch-customers-customerId",
        field: "/billingAddress/postalCode",
        exposure: Exposure::Flag("--billing-postal-code"),
    },
    Field {
        operation_id: "flute-v2-patch-customers-customerId",
        field: "/billingAddress/stateCode",
        exposure: Exposure::Flag("--billing-state"),
    },
    Field {
        operation_id: "flute-v2-patch-customers-customerId",
        field: "/companyName",
        exposure: Exposure::Flag("--company"),
    },
    Field {
        operation_id: "flute-v2-patch-customers-customerId",
        field: "/email",
        exposure: Exposure::Flag("--email"),
    },
    Field {
        operation_id: "flute-v2-patch-customers-customerId",
        field: "/firstName",
        exposure: Exposure::Flag("--first-name"),
    },
    Field {
        operation_id: "flute-v2-patch-customers-customerId",
        field: "/hasSmsConsent",
        exposure: Exposure::Flag("--sms-consent"),
    },
    Field {
        operation_id: "flute-v2-patch-customers-customerId",
        field: "/lastName",
        exposure: Exposure::Flag("--last-name"),
    },
    Field {
        operation_id: "flute-v2-patch-customers-customerId",
        field: "/mobilePhoneNumber",
        exposure: Exposure::Flag("--mobile"),
    },
    Field {
        operation_id: "flute-v2-patch-customers-customerId",
        field: "/shippingAddress/addressLine1",
        exposure: Exposure::Flag("--shipping-line1"),
    },
    Field {
        operation_id: "flute-v2-patch-customers-customerId",
        field: "/shippingAddress/addressLine2",
        exposure: Exposure::Flag("--shipping-line2"),
    },
    Field {
        operation_id: "flute-v2-patch-customers-customerId",
        field: "/shippingAddress/city",
        exposure: Exposure::Flag("--shipping-city"),
    },
    Field {
        operation_id: "flute-v2-patch-customers-customerId",
        field: "/shippingAddress/countryCode",
        exposure: Exposure::Flag("--shipping-country"),
    },
    Field {
        operation_id: "flute-v2-patch-customers-customerId",
        field: "/shippingAddress/postalCode",
        exposure: Exposure::Flag("--shipping-postal-code"),
    },
    Field {
        operation_id: "flute-v2-patch-customers-customerId",
        field: "/shippingAddress/stateCode",
        exposure: Exposure::Flag("--shipping-state"),
    },
    Field {
        operation_id: "flute-v2-patch-customers-customerId",
        field: "/shouldUseBillingAsShippingAddress",
        exposure: Exposure::Flag("--use-billing-as-shipping"),
    },
    Field {
        operation_id: "flute-v2-post-customers",
        field: "/billingAddress/addressLine1",
        exposure: Exposure::Flag("--billing-line1"),
    },
    Field {
        operation_id: "flute-v2-post-customers",
        field: "/billingAddress/addressLine2",
        exposure: Exposure::Flag("--billing-line2"),
    },
    Field {
        operation_id: "flute-v2-post-customers",
        field: "/billingAddress/city",
        exposure: Exposure::Flag("--billing-city"),
    },
    Field {
        operation_id: "flute-v2-post-customers",
        field: "/billingAddress/countryCode",
        exposure: Exposure::Flag("--billing-country"),
    },
    Field {
        operation_id: "flute-v2-post-customers",
        field: "/billingAddress/postalCode",
        exposure: Exposure::Flag("--billing-postal-code"),
    },
    Field {
        operation_id: "flute-v2-post-customers",
        field: "/billingAddress/stateCode",
        exposure: Exposure::Flag("--billing-state"),
    },
    Field {
        operation_id: "flute-v2-post-customers",
        field: "/shippingAddress/addressLine1",
        exposure: Exposure::Flag("--shipping-line1"),
    },
    Field {
        operation_id: "flute-v2-post-customers",
        field: "/shippingAddress/addressLine2",
        exposure: Exposure::Flag("--shipping-line2"),
    },
    Field {
        operation_id: "flute-v2-post-customers",
        field: "/shippingAddress/city",
        exposure: Exposure::Flag("--shipping-city"),
    },
    Field {
        operation_id: "flute-v2-post-customers",
        field: "/shippingAddress/countryCode",
        exposure: Exposure::Flag("--shipping-country"),
    },
    Field {
        operation_id: "flute-v2-post-customers",
        field: "/shippingAddress/postalCode",
        exposure: Exposure::Flag("--shipping-postal-code"),
    },
    Field {
        operation_id: "flute-v2-post-customers",
        field: "/shippingAddress/stateCode",
        exposure: Exposure::Flag("--shipping-state"),
    },
    Field {
        operation_id: "flute-v2-post-customers",
        field: "/firstName",
        exposure: Exposure::Flag("--first-name"),
    },
    Field {
        operation_id: "flute-v2-post-customers",
        field: "/lastName",
        exposure: Exposure::Flag("--last-name"),
    },
    Field {
        operation_id: "flute-v2-post-customers",
        field: "/companyName",
        exposure: Exposure::Flag("--company"),
    },
    Field {
        operation_id: "flute-v2-post-customers",
        field: "/email",
        exposure: Exposure::Flag("--email"),
    },
    Field {
        operation_id: "flute-v2-post-customers",
        field: "/mobilePhoneNumber",
        exposure: Exposure::Flag("--mobile"),
    },
    Field {
        operation_id: "flute-v2-post-customers",
        field: "/hasSmsConsent",
        exposure: Exposure::Flag("--sms-consent"),
    },
    Field {
        operation_id: "flute-v2-post-customers",
        field: "/shouldUseBillingAsShippingAddress",
        exposure: Exposure::Flag("--use-billing-as-shipping"),
    },
    Field {
        operation_id: "flute-v2-post-customers",
        field: "/paymentMethodsAchAccounts/[]/accountHolderType",
        exposure: Exposure::Excluded(
            "an instrument is added with `payment-methods add-card` / `add-ach`; \
             inlining the whole instrument surface onto a customer command would \
             duplicate it, and v1 keeps them separate too",
        ),
    },
    Field {
        operation_id: "flute-v2-post-customers",
        field: "/paymentMethodsAchAccounts/[]/accountNumber",
        exposure: Exposure::Excluded(
            "an instrument is added with `payment-methods add-card` / `add-ach`; \
             inlining the whole instrument surface onto a customer command would \
             duplicate it, and v1 keeps them separate too",
        ),
    },
    Field {
        operation_id: "flute-v2-post-customers",
        field: "/paymentMethodsAchAccounts/[]/accountType",
        exposure: Exposure::Excluded(
            "an instrument is added with `payment-methods add-card` / `add-ach`; \
             inlining the whole instrument surface onto a customer command would \
             duplicate it, and v1 keeps them separate too",
        ),
    },
    Field {
        operation_id: "flute-v2-post-customers",
        field: "/paymentMethodsAchAccounts/[]/paymentName",
        exposure: Exposure::Excluded(
            "an instrument is added with `payment-methods add-card` / `add-ach`; \
             inlining the whole instrument surface onto a customer command would \
             duplicate it, and v1 keeps them separate too",
        ),
    },
    Field {
        operation_id: "flute-v2-post-customers",
        field: "/paymentMethodsAchAccounts/[]/routingNumber",
        exposure: Exposure::Excluded(
            "an instrument is added with `payment-methods add-card` / `add-ach`; \
             inlining the whole instrument surface onto a customer command would \
             duplicate it, and v1 keeps them separate too",
        ),
    },
    Field {
        operation_id: "flute-v2-post-customers",
        field: "/paymentMethodsAchAccounts/[]/taxId",
        exposure: Exposure::Excluded(
            "an instrument is added with `payment-methods add-card` / `add-ach`; \
             inlining the whole instrument surface onto a customer command would \
             duplicate it, and v1 keeps them separate too",
        ),
    },
    Field {
        operation_id: "flute-v2-post-customers",
        field: "/paymentMethodsCards/[]/cardNumber",
        exposure: Exposure::Excluded(
            "an instrument is added with `payment-methods add-card` / `add-ach`; \
             inlining the whole instrument surface onto a customer command would \
             duplicate it, and v1 keeps them separate too",
        ),
    },
    Field {
        operation_id: "flute-v2-post-customers",
        field: "/paymentMethodsCards/[]/expirationMonth",
        exposure: Exposure::Excluded(
            "an instrument is added with `payment-methods add-card` / `add-ach`; \
             inlining the whole instrument surface onto a customer command would \
             duplicate it, and v1 keeps them separate too",
        ),
    },
    Field {
        operation_id: "flute-v2-post-customers",
        field: "/paymentMethodsCards/[]/expirationYear",
        exposure: Exposure::Excluded(
            "an instrument is added with `payment-methods add-card` / `add-ach`; \
             inlining the whole instrument surface onto a customer command would \
             duplicate it, and v1 keeps them separate too",
        ),
    },
    Field {
        operation_id: "flute-v2-post-customers",
        field: "/paymentMethodsCards/[]/paymentName",
        exposure: Exposure::Excluded(
            "an instrument is added with `payment-methods add-card` / `add-ach`; \
             inlining the whole instrument surface onto a customer command would \
             duplicate it, and v1 keeps them separate too",
        ),
    },
    Field {
        operation_id: "flute-v2-post-customers",
        field: "/paymentMethodsCards/[]/securityCode",
        exposure: Exposure::Excluded(
            "an instrument is added with `payment-methods add-card` / `add-ach`; \
             inlining the whole instrument surface onto a customer command would \
             duplicate it, and v1 keeps them separate too",
        ),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/paymentProcessorId",
        exposure: Exposure::Flag("--payment-processor-id"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/baseAmount",
        exposure: Exposure::Flag("--amount"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/referenceId",
        exposure: Exposure::Flag("--reference-id"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/currencyCode",
        exposure: Exposure::Flag("--currency-code"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/transactionDetails/cardData/paymentMethodDetails/cardNumber",
        exposure: Exposure::Flag("--card"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/transactionDetails/cardData/paymentMethodDetails/securityCode",
        exposure: Exposure::Flag("--cvv"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/transactionDetails/cardData/paymentMethodDetails/expirationMonth",
        exposure: Exposure::Flag("--exp"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/transactionDetails/cardData/paymentMethodDetails/expirationYear",
        exposure: Exposure::Flag("--exp"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/billingAddress/addressLine1",
        exposure: Exposure::Flag("--billing-line1"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/billingAddress/addressLine2",
        exposure: Exposure::Flag("--billing-line2"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/billingAddress/city",
        exposure: Exposure::Flag("--billing-city"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/billingAddress/countryCode",
        exposure: Exposure::Flag("--billing-country"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/billingAddress/postalCode",
        exposure: Exposure::Flag("--billing-postal-code"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/billingAddress/stateCode",
        exposure: Exposure::Flag("--billing-state"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/shippingAddress/addressLine1",
        exposure: Exposure::Flag("--shipping-line1"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/shippingAddress/addressLine2",
        exposure: Exposure::Flag("--shipping-line2"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/shippingAddress/city",
        exposure: Exposure::Flag("--shipping-city"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/shippingAddress/countryCode",
        exposure: Exposure::Flag("--shipping-country"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/shippingAddress/postalCode",
        exposure: Exposure::Flag("--shipping-postal-code"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/shippingAddress/stateCode",
        exposure: Exposure::Flag("--shipping-state"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/appVersion",
        exposure: Exposure::Excluded("SDK telemetry; meaningless from a CLI"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/sdkVersion",
        exposure: Exposure::Excluded("SDK telemetry; meaningless from a CLI"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/platform",
        exposure: Exposure::Excluded(
            "SDK telemetry. Not fixed to a value either: the enum is not documented, \
             so any constant would be a guess sent on every charge",
        ),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/deviceId",
        exposure: Exposure::Excluded(
            "identifies a POS terminal; `pos create` is the command that owns a device",
        ),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/transactionDetails/cardData/paymentMethodId",
        exposure: Exposure::Flag("--payment-method-id/--instrument"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/transactionDetails/achData/isSameDayProcessing",
        exposure: Exposure::Flag("--same-day"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/transactionDetails/achData/paymentMethodDetails/accountHolderType",
        exposure: Exposure::Flag("--ach-account-holder-type"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/transactionDetails/achData/paymentMethodDetails/accountNumber",
        exposure: Exposure::Flag("--ach-account-number"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/transactionDetails/achData/paymentMethodDetails/accountType",
        exposure: Exposure::Flag("--ach-account-type"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/transactionDetails/achData/paymentMethodDetails/routingNumber",
        exposure: Exposure::Flag("--ach-routing-number"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/transactionDetails/achData/paymentMethodDetails/taxId",
        exposure: Exposure::Flag("--ach-tax-id"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/transactionDetails/achData/paymentMethodId",
        exposure: Exposure::Flag("--payment-method-id/--instrument"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/transactionDetails/achData/requesterIpAddress",
        exposure: Exposure::Flag("--requester-ip"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/transactionDetails/achData/secCode",
        exposure: Exposure::Flag("--sec-code"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/contactInfo/companyName",
        exposure: Exposure::Flag("--contact-company"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/contactInfo/email",
        exposure: Exposure::Flag("--contact-email"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/contactInfo/firstName",
        exposure: Exposure::Flag("--contact-first-name"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/contactInfo/hasSmsConsent",
        exposure: Exposure::Flag("--contact-sms-consent"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/contactInfo/lastName",
        exposure: Exposure::Flag("--contact-last-name"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/contactInfo/mobilePhoneNumber",
        exposure: Exposure::Flag("--contact-phone"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/extraAmounts/discountAmount",
        exposure: Exposure::Flag("--discount-amount"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/extraAmounts/discountRate",
        exposure: Exposure::Flag("--discount-rate"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/extraAmounts/surchargeRate",
        exposure: Exposure::Flag("--surcharge-rate"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/extraAmounts/tipAmount",
        exposure: Exposure::Flag("--tip-amount"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/extraAmounts/tipRate",
        exposure: Exposure::Flag("--tip-rate"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/isCustomerInitiatedTransaction",
        exposure: Exposure::Flag("--customer-initiated"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/pricingType",
        exposure: Exposure::Flag("--pricing-type"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/transactionEnhancedData/dutyCharges",
        // The server has no member for it. A charge carrying the field is
        // rejected outright — not ignored — so a flag here could only ever
        // break the request it was added to. The declaration is doubly
        // suspect: it also puts `maxLength` on a `number`. Restore the flag
        // when a live charge carrying `dutyCharges` succeeds.
        exposure: Exposure::Excluded(
            "the API rejects it: no member of TransactionEnhancedDataDto maps to              dutyCharges, though the schema declares it",
        ),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/transactionEnhancedData/invoiceNumber",
        exposure: Exposure::Flag("--l3-invoice"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/transactionEnhancedData/products/[]/discountRate",
        exposure: Exposure::Flag("--l3-product"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/transactionEnhancedData/products/[]/measurementUnit",
        exposure: Exposure::Flag("--l3-product"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/transactionEnhancedData/products/[]/productCode",
        exposure: Exposure::Flag("--l3-product"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/transactionEnhancedData/products/[]/productDescription",
        exposure: Exposure::Flag("--l3-product"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/transactionEnhancedData/products/[]/productName",
        exposure: Exposure::Flag("--l3-product"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/transactionEnhancedData/products/[]/quantity",
        exposure: Exposure::Flag("--l3-product"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/transactionEnhancedData/products/[]/taxAmount",
        exposure: Exposure::Flag("--l3-product"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/transactionEnhancedData/products/[]/unitPrice",
        exposure: Exposure::Flag("--l3-product"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/transactionEnhancedData/purchaseOrder",
        exposure: Exposure::Flag("--l3-po"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/transactionEnhancedData/salesTaxRate",
        exposure: Exposure::Flag("--l2-tax-rate"),
    },
    Field {
        operation_id: "flute-v2-post-transactions",
        field: "/transactionEnhancedData/shippingCharges",
        exposure: Exposure::Flag("--l3-shipping"),
    },
    Field {
        operation_id: "flute-v2-post-pos-transactions",
        field: "/terminalId",
        exposure: Exposure::Flag("--terminal-id"),
    },
    Field {
        operation_id: "flute-v2-post-pos-transactions",
        field: "/posDeviceId",
        exposure: Exposure::Flag("--pos-device-id"),
    },
    Field {
        operation_id: "flute-v2-post-pos-transactions",
        field: "/baseAmount",
        exposure: Exposure::Flag("--amount"),
    },
    Field {
        operation_id: "flute-v2-post-pos-transactions",
        field: "/currencyCode",
        exposure: Exposure::Flag("--currency-code"),
    },
    Field {
        operation_id: "flute-v2-post-pos-transactions",
        field: "/waitForAcceptanceByTerminal",
        // One of the two controls --wait drives. Always sent, because `false` is
        // what "answer immediately" has to say.
        exposure: Exposure::Flag("--wait"),
    },
    Field {
        operation_id: "flute-v2-post-pos-transactions",
        field: "/captureMethod",
        exposure: Exposure::Flag("--capture-method"),
    },
    Field {
        operation_id: "flute-v2-post-pos-transactions",
        field: "/initiationChannel",
        exposure: Exposure::Flag("--initiation-channel"),
    },
    Field {
        operation_id: "flute-v2-post-pos-transactions",
        field: "/readingMethod",
        exposure: Exposure::Flag("--reading-method"),
    },
    Field {
        operation_id: "flute-v2-post-pos-transactions",
        field: "/pricingType",
        exposure: Exposure::Flag("--pricing-type"),
    },
    Field {
        operation_id: "flute-v2-post-pos-transactions",
        field: "/paymentProcessorId",
        // Optional here, unlike on a card transaction: the POS schema does not
        // declare it required and the account's default processor governs.
        exposure: Exposure::Flag("--payment-processor-id"),
    },
    Field {
        operation_id: "flute-v2-post-pos-transactions",
        field: "/customerId",
        exposure: Exposure::Flag("--customer-id"),
    },
    Field {
        operation_id: "flute-v2-post-pos-transactions",
        field: "/referenceId",
        exposure: Exposure::Flag("--reference-id"),
    },
    Field {
        operation_id: "flute-v2-post-pos-transactions",
        field: "/requestPaymentMethodStorageConsent",
        exposure: Exposure::Flag("--request-storage-consent"),
    },
    Field {
        operation_id: "flute-v2-post-pos-transactions",
        field: "/extraAmounts/tipAmount",
        exposure: Exposure::Flag("--tip-amount"),
    },
    Field {
        operation_id: "flute-v2-post-pos-transactions",
        field: "/extraAmounts/tipRate",
        exposure: Exposure::Flag("--tip-rate"),
    },
    Field {
        operation_id: "flute-v2-post-pos-transactions-posTransactionId-print-receipt",
        field: "/terminalId",
        exposure: Exposure::Flag("--terminal-id"),
    },
    Field {
        operation_id: "flute-v2-get-pos-transactions",
        field: "?pageIndex",
        exposure: Exposure::Flag("--page-index"),
    },
    Field {
        operation_id: "flute-v2-get-pos-transactions",
        field: "?pageSize",
        exposure: Exposure::Flag("--page-size"),
    },
    Field {
        operation_id: "flute-v2-get-pos-transactions",
        field: "?sortBy",
        exposure: Exposure::Flag("--sort-by"),
    },
    Field {
        operation_id: "flute-v2-get-pos-transactions",
        field: "?sortOrder",
        exposure: Exposure::Flag("--asc/--desc"),
    },
    Field {
        operation_id: "flute-v2-get-pos-transactions",
        field: "?terminalId",
        exposure: Exposure::Flag("--terminal-id"),
    },
    Field {
        operation_id: "flute-v2-get-pos-transactions",
        field: "?fromDate",
        exposure: Exposure::Flag("--from"),
    },
    Field {
        operation_id: "flute-v2-get-pos-transactions",
        field: "?toDate",
        exposure: Exposure::Flag("--to"),
    },
    Field {
        operation_id: "flute-v2-get-pos-transactions",
        field: "?posTransactionStatus",
        exposure: Exposure::Flag("--status"),
    },
    Field {
        operation_id: "flute-v2-get-pos-transactions-posTransactionId",
        field: "?waitForTransactionProcessing",
        // The other control --wait drives, and a flag of `pos get` in its own
        // right. Omitted when absent, so the declared default governs.
        exposure: Exposure::Flag("--wait"),
    },
    Field {
        operation_id: "flute-v2-get-terminals",
        field: "?pageIndex",
        exposure: Exposure::Flag("--page-index"),
    },
    Field {
        operation_id: "flute-v2-get-terminals",
        field: "?pageSize",
        exposure: Exposure::Flag("--page-size"),
    },
    Field {
        operation_id: "flute-v2-get-terminals",
        field: "?sortBy",
        exposure: Exposure::Flag("--sort-by"),
    },
    Field {
        operation_id: "flute-v2-get-terminals",
        field: "?sortOrder",
        exposure: Exposure::Flag("--asc/--desc"),
    },
    Field {
        operation_id: "flute-v2-get-terminals",
        field: "?terminalStatus",
        // The **query** enum: `Ready`, `Busy`, `Offline`. The response field of
        // the same name declares `Active` instead of `Ready`.
        exposure: Exposure::Flag("--status"),
    },
    Field {
        operation_id: "flute-v2-get-terminals",
        field: "?terminalMode",
        exposure: Exposure::Flag("--mode"),
    },
    Field {
        operation_id: "flute-v2-get-terminals",
        field: "?connectionStatus",
        exposure: Exposure::Flag("--connection"),
    },
    Field {
        operation_id: "flute-v2-get-terminals",
        field: "?serialNumber",
        exposure: Exposure::Flag("--serial-number"),
    },
    Field {
        operation_id: "flute-v2-get-terminals",
        field: "?search",
        exposure: Exposure::Flag("--search"),
    },
    Field {
        operation_id: "flute-v2-get-settlements-batches",
        field: "?pageIndex",
        exposure: Exposure::Flag("--page-index"),
    },
    Field {
        operation_id: "flute-v2-get-settlements-batches",
        field: "?pageSize",
        exposure: Exposure::Flag("--page-size"),
    },
    Field {
        operation_id: "flute-v2-get-settlements-batches",
        field: "?sortBy",
        exposure: Exposure::Flag("--sort-by"),
    },
    Field {
        operation_id: "flute-v2-get-settlements-batches",
        field: "?sortOrder",
        // `desc` is the declared default **here**, unlike every other list, so
        // the flag names ascending rather than descending.
        exposure: Exposure::Flag("--asc"),
    },
    Field {
        operation_id: "flute-v2-get-settlements-batches",
        field: "?fromDate",
        exposure: Exposure::Flag("--from"),
    },
    Field {
        operation_id: "flute-v2-get-settlements-batches",
        field: "?toDate",
        exposure: Exposure::Flag("--to"),
    },
    Field {
        operation_id: "flute-v2-get-settlements-batches",
        field: "?batchIds",
        // Array-typed and repeatable. `settlements get` sends the same filter
        // with one value, which is why it needs no endpoint of its own.
        exposure: Exposure::Flag("--batch-ids"),
    },
    Field {
        operation_id: "flute-v2-get-settlements-batches",
        field: "?paymentProcessorIds",
        exposure: Exposure::Flag("--processor-ids"),
    },
    Field {
        operation_id: "flute-v2-get-settlements-batches",
        field: "?batchStatus",
        exposure: Exposure::Flag("--status"),
    },
    Field {
        operation_id: "flute-v2-post-settlements-batches-close",
        field: "/paymentProcessorId",
        exposure: Exposure::Flag("--payment-processor-id"),
    },
    Field {
        operation_id: "flute-v2-patch-settings-transaction-autofill",
        field: "/level2Settings/taxRate",
        // Declared 0 to 22. `transactions create` carries the same idea as
        // `salesTaxRate` with a declared 0.01 to 100, and each is taken from
        // its own schema rather than harmonised.
        exposure: Exposure::Flag("--l2-tax-rate"),
    },
    Field {
        operation_id: "flute-v2-patch-settings-transaction-autofill",
        field: "/level3Settings/shippingChargeRate",
        // A **rate** here, where `transactions create` sends an amount under
        // `shippingCharges` — hence the suffix rather than a bare --l3-shipping.
        exposure: Exposure::Flag("--l3-shipping-rate"),
    },
    Field {
        operation_id: "flute-v2-patch-settings-transaction-autofill",
        field: "/level3Settings/dutyChargeRate",
        exposure: Exposure::Flag("--l3-duty-rate"),
    },
    Field {
        operation_id: "flute-v2-patch-settings-transaction-autofill",
        field: "/level3Settings/product/productName",
        exposure: Exposure::Flag("--product-name"),
    },
    Field {
        operation_id: "flute-v2-patch-settings-transaction-autofill",
        field: "/level3Settings/product/code",
        // `code`, not the transaction product's `productCode`.
        exposure: Exposure::Flag("--product-code"),
    },
    Field {
        operation_id: "flute-v2-patch-settings-transaction-autofill",
        field: "/level3Settings/product/description",
        // The server has no member for it. A patch carrying the field is
        // rejected outright — not ignored — so a flag here could only ever
        // break the request it was added to, and the read carries no
        // description either. Restore the flag when a live patch setting one
        // succeeds.
        exposure: Exposure::Excluded(
            "the API rejects it: no member of the autofill product patch maps \
             to description, though the schema declares it",
        ),
    },
    Field {
        operation_id: "flute-v2-patch-settings-transaction-autofill",
        field: "/level3Settings/product/measurementUnit",
        exposure: Exposure::Flag("--product-unit"),
    },
    Field {
        operation_id: "flute-v2-patch-settings-transaction-autofill",
        field: "/level3Settings/product/unitPrice",
        exposure: Exposure::Flag("--product-unit-price"),
    },
    Field {
        operation_id: "flute-v2-patch-settings-transaction-autofill",
        field: "/level3Settings/product/quantity",
        exposure: Exposure::Flag("--product-quantity"),
    },
    Field {
        operation_id: "flute-v2-patch-settings-transaction-autofill",
        field: "/level3Settings/product/discountPercentage",
        // `discountPercentage`, not the transaction product's `discountRate`.
        exposure: Exposure::Flag("--product-discount"),
    },
    Field {
        operation_id: "flute-v2-post-payment-links",
        field: "/paymentMethods/card/enabled",
        exposure: Exposure::Flag("--card-enabled"),
    },
    Field {
        operation_id: "flute-v2-post-payment-links",
        field: "/paymentMethods/card/processorId",
        exposure: Exposure::Flag("--card-processor-id"),
    },
    Field {
        operation_id: "flute-v2-post-payment-links",
        field: "/paymentMethods/ach/enabled",
        exposure: Exposure::Flag("--ach-enabled"),
    },
    Field {
        operation_id: "flute-v2-post-payment-links",
        field: "/paymentMethods/ach/processorId",
        exposure: Exposure::Flag("--ach-processor-id"),
    },
    Field {
        operation_id: "flute-v2-post-payment-links",
        field: "/baseAmount",
        // Omitted entirely for a flexible-amount link the payer fills in, which
        // is what the schema documents an absent amount as meaning.
        exposure: Exposure::Flag("--amount"),
    },
    Field {
        operation_id: "flute-v2-post-payment-links",
        field: "/currencyCode",
        exposure: Exposure::Flag("--currency-code"),
    },
    Field {
        operation_id: "flute-v2-post-payment-links",
        field: "/linkType",
        exposure: Exposure::Flag("--link-type"),
    },
    Field {
        operation_id: "flute-v2-post-payment-links",
        field: "/customerId",
        exposure: Exposure::Flag("--customer-id"),
    },
    Field {
        operation_id: "flute-v2-post-payment-links",
        field: "/referenceId",
        exposure: Exposure::Flag("--reference-id"),
    },
    Field {
        operation_id: "flute-v2-post-payment-links",
        field: "/name",
        exposure: Exposure::Flag("--name"),
    },
    Field {
        operation_id: "flute-v2-post-payment-links",
        field: "/description",
        exposure: Exposure::Flag("--description"),
    },
    Field {
        operation_id: "flute-v2-post-payment-links",
        field: "/expiresOn",
        exposure: Exposure::Flag("--expires-on"),
    },
    Field {
        operation_id: "flute-v2-patch-payment-links-paymentLinkId",
        field: "/paymentMethods/card/enabled",
        // Valued here, where `create`'s is a bare switch: a PATCH has to be
        // able to turn a method off.
        exposure: Exposure::Flag("--card-enabled"),
    },
    Field {
        operation_id: "flute-v2-patch-payment-links-paymentLinkId",
        field: "/paymentMethods/card/processorId",
        exposure: Exposure::Flag("--card-processor-id"),
    },
    Field {
        operation_id: "flute-v2-patch-payment-links-paymentLinkId",
        field: "/paymentMethods/ach/enabled",
        exposure: Exposure::Flag("--ach-enabled"),
    },
    Field {
        operation_id: "flute-v2-patch-payment-links-paymentLinkId",
        field: "/paymentMethods/ach/processorId",
        exposure: Exposure::Flag("--ach-processor-id"),
    },
    Field {
        operation_id: "flute-v2-patch-payment-links-paymentLinkId",
        field: "/baseAmount",
        // Both directions: a value sets it, and either clearing spelling sends
        // the explicit null the merge patch reads as "make this flexible".
        exposure: Exposure::Flag("--amount/--clear"),
    },
    Field {
        operation_id: "flute-v2-patch-payment-links-paymentLinkId",
        field: "/currencyCode",
        // Neither clearing spelling is offered: the schema says it cannot be
        // cleared.
        exposure: Exposure::Flag("--currency-code"),
    },
    Field {
        operation_id: "flute-v2-patch-payment-links-paymentLinkId",
        field: "/linkType",
        exposure: Exposure::Flag("--link-type"),
    },
    Field {
        operation_id: "flute-v2-patch-payment-links-paymentLinkId",
        field: "/paymentLinkStatus",
        exposure: Exposure::Flag("--status"),
    },
    Field {
        operation_id: "flute-v2-patch-payment-links-paymentLinkId",
        field: "/customerId",
        exposure: Exposure::Flag("--customer-id/--clear"),
    },
    Field {
        operation_id: "flute-v2-patch-payment-links-paymentLinkId",
        field: "/referenceId",
        exposure: Exposure::Flag("--reference-id/--clear"),
    },
    Field {
        operation_id: "flute-v2-patch-payment-links-paymentLinkId",
        field: "/name",
        // Likewise unclearable by declaration.
        exposure: Exposure::Flag("--name"),
    },
    Field {
        operation_id: "flute-v2-patch-payment-links-paymentLinkId",
        field: "/description",
        exposure: Exposure::Flag("--description/--clear"),
    },
    Field {
        operation_id: "flute-v2-patch-payment-links-paymentLinkId",
        field: "/expiresOn",
        exposure: Exposure::Flag("--expires-on/--clear"),
    },
    Field {
        operation_id: "flute-v2-post-payment-links-paymentLinkId-share",
        field: "/shareBy",
        // Two values here — `Email` and `Sms`. The transaction receipt's field
        // of the same name declares a third, so the enums are not shared.
        exposure: Exposure::Flag("--share-by"),
    },
    Field {
        operation_id: "flute-v2-post-payment-links-paymentLinkId-share",
        field: "/recipient",
        exposure: Exposure::Flag("--recipient"),
    },
    Field {
        operation_id: "flute-v2-post-payment-links-paymentLinkId-share",
        field: "/hasCustomerConsent",
        // Required, so it is always sent — `false` included, which is what lets
        // the API refuse an unconsented share.
        exposure: Exposure::Flag("--consent"),
    },
    Field {
        operation_id: "flute-v2-get-payment-links",
        field: "?pageIndex",
        exposure: Exposure::Flag("--page-index"),
    },
    Field {
        operation_id: "flute-v2-get-payment-links",
        field: "?pageSize",
        exposure: Exposure::Flag("--page-size"),
    },
    Field {
        operation_id: "flute-v2-get-payment-links",
        field: "?search",
        exposure: Exposure::Flag("--search"),
    },
    Field {
        operation_id: "flute-v2-get-payment-links",
        field: "?linkType",
        exposure: Exposure::Flag("--link-type"),
    },
    Field {
        operation_id: "flute-v2-get-payment-links",
        field: "?paymentLinkStatus",
        exposure: Exposure::Flag("--status"),
    },
    Field {
        operation_id: "flute-v2-get-payment-links",
        field: "?sortBy",
        exposure: Exposure::Flag("--sort-by"),
    },
    Field {
        operation_id: "flute-v2-get-payment-links",
        field: "?sortOrder",
        exposure: Exposure::Flag("--asc/--desc"),
    },
    Field {
        operation_id: "flute-v2-post-payment-sessions",
        field: "/mode",
        exposure: Exposure::Flag("--mode"),
    },
    Field {
        operation_id: "flute-v2-post-payment-sessions",
        field: "/amount",
        // Three documented meanings, none of them expressible in the schema:
        // greater than zero for a paying session, exactly zero for a vault-only
        // one, and absent for a flexible amount set at checkout.
        exposure: Exposure::Flag("--amount"),
    },
    Field {
        operation_id: "flute-v2-post-payment-sessions",
        field: "/tipAmount",
        exposure: Exposure::Flag("--tip-amount"),
    },
    Field {
        operation_id: "flute-v2-post-payment-sessions",
        field: "/customerId",
        exposure: Exposure::Flag("--customer-id"),
    },
    Field {
        operation_id: "flute-v2-post-payment-sessions",
        field: "/customerHandling",
        exposure: Exposure::Flag("--customer-handling"),
    },
    Field {
        operation_id: "flute-v2-post-payment-sessions",
        field: "/referenceId",
        exposure: Exposure::Flag("--reference-id"),
    },
    Field {
        operation_id: "flute-v2-post-payment-sessions",
        field: "/returnUrl",
        exposure: Exposure::Flag("--return-url"),
    },
    Field {
        operation_id: "flute-v2-post-payment-sessions",
        field: "/skipAddressVerification",
        exposure: Exposure::Flag("--skip-address-verification"),
    },
    Field {
        operation_id: "flute-v2-post-payment-sessions",
        field: "/pageName",
        exposure: Exposure::Flag("--page-name"),
    },
    Field {
        operation_id: "flute-v2-post-payment-sessions",
        field: "/paymentNotes",
        exposure: Exposure::Flag("--payment-notes"),
    },
    Field {
        operation_id: "flute-v2-post-payment-sessions",
        field: "/afterCompletionMessage",
        exposure: Exposure::Flag("--after-completion-message"),
    },
    Field {
        operation_id: "flute-v2-post-payment-sessions",
        field: "/expiresAt",
        exposure: Exposure::Flag("--expires-at"),
    },
    Field {
        operation_id: "flute-v2-post-payment-sessions",
        field: "/metadata",
        // A free-form string map, so one repeatable `key=value` flag rather than
        // a flag per key. Its leaf is the container, because the schema names
        // no properties inside it.
        exposure: Exposure::Flag("--metadata"),
    },
    Field {
        operation_id: "flute-v2-post-payment-sessions",
        field: "/paymentMethods/card/enabled",
        exposure: Exposure::Flag("--card-enabled"),
    },
    Field {
        operation_id: "flute-v2-post-payment-sessions",
        field: "/paymentMethods/card/processorId",
        exposure: Exposure::Flag("--card-processor-id"),
    },
    Field {
        operation_id: "flute-v2-post-payment-sessions",
        field: "/paymentMethods/ach/enabled",
        exposure: Exposure::Flag("--ach-enabled"),
    },
    Field {
        operation_id: "flute-v2-post-payment-sessions",
        field: "/paymentMethods/ach/processorId",
        exposure: Exposure::Flag("--ach-processor-id"),
    },
    Field {
        operation_id: "flute-v2-post-api-keys",
        field: "/merchantId",
        exposure: Exposure::Flag("--merchant-id"),
    },
    Field {
        operation_id: "flute-v2-post-api-keys",
        field: "/apiKeyName",
        exposure: Exposure::Flag("--name"),
    },
    Field {
        operation_id: "flute-v2-get-api-keys",
        field: "?merchantId",
        // The only query parameter this group has. `GetApiKeysResponseDto`
        // declares no `pageInfo`, so there is no pagination to expose here and
        // the shared flags are rejected as unknown.
        exposure: Exposure::Flag("--merchant-id"),
    },
];
