//! The v1 capability-parity matrix (layer 4b).
//!
//! Operation coverage cannot see a dropped feature: every endpoint stays
//! covered while the flag that reached it disappears. v1's
//! `--payment-method-id`, its L2/L3 enhanced data, and its customer
//! `--search` are all cases where the endpoint is implemented and the
//! capability is not.
//!
//! **The oracle is a vendored snapshot of v1's own `--help` tree**, not a list
//! recalled by hand. Without it the matrix could only prove that the rows it
//! happens to contain are consistent — never that it contains every v1
//! capability, which is the one claim that matters here. Regenerate with
//! `docs/reference/snapshot-v1-capabilities.py` against a v1 checkout.

use std::collections::BTreeMap;
use std::sync::LazyLock;

pub enum Parity {
    /// v2 flag, same meaning.
    Preserved(&'static str),
    /// v2 flag, and why it differs.
    Replaced(&'static str, &'static str),
    /// v2 reason for dropping it.
    Removed(&'static str),
}

pub struct Capability {
    /// The v1 command, spelled exactly as the snapshot spells it.
    pub v1_command: &'static str,
    /// The v1 flags this row accounts for. The union of a command's rows must
    /// cover every flag the snapshot records for it.
    pub v1_flags: &'static [&'static str],
    pub parity: Parity,
    pub test: Option<&'static str>,
}

/// v1's shipped surface: leaf command to its own flags, global flags under
/// the `#globals` key.
pub static V1_SURFACE: LazyLock<BTreeMap<String, Vec<String>>> = LazyLock::new(|| {
    let text = include_str!("../../docs/reference/v1-capabilities.txt");
    let mut out = BTreeMap::new();
    for line in text.lines() {
        if line.trim().is_empty() || line.starts_with("# ") {
            continue;
        }
        let (cmd, flags) = line.split_once('\t').unwrap_or((line, ""));
        out.insert(
            cmd.to_string(),
            flags.split_whitespace().map(str::to_string).collect(),
        );
    }
    assert!(
        out.len() > 40,
        "the vendored v1 surface looks truncated: {} commands",
        out.len()
    );
    out
});

pub static CAPABILITIES: &[Capability] = &[
    Capability {
        v1_command: "#globals",
        v1_flags: &["--debug"],
        parity: Parity::Preserved("--debug"),
        test: Some("debug_traces_go_to_stderr_leaving_stdout_parseable"),
    },
    Capability {
        v1_command: "#globals",
        v1_flags: &["--output"],
        parity: Parity::Preserved("--output table|json|quiet"),
        test: Some("version_reports_the_crate_version_in_every_output_mode"),
    },
    Capability {
        v1_command: "#globals",
        v1_flags: &["--profile"],
        parity: Parity::Replaced(
            "--profile",
            "same flag and same values, but parsed as Option<String> so the config \
             file's default_profile is reachable; v1's clap default_value made \
             the flag never-absent and stranded auth switch",
        ),
        test: Some("profile_flag_overrides_the_stored_default"),
    },
    Capability {
        v1_command: "auth login",
        v1_flags: &[],
        parity: Parity::Preserved("auth login"),
        test: Some("login_is_reachable_and_documents_where_the_secret_goes"),
    },
    Capability {
        v1_command: "auth logout",
        v1_flags: &[],
        parity: Parity::Preserved("auth logout"),
        test: Some("logout_without_a_keychain_refuses_rather_than_reporting_removal"),
    },
    Capability {
        v1_command: "auth status",
        v1_flags: &[],
        parity: Parity::Preserved("auth status"),
        test: Some("status_without_credentials_reports_false_and_exits_zero"),
    },
    Capability {
        v1_command: "auth switch",
        v1_flags: &[],
        parity: Parity::Replaced(
            "auth switch",
            "same command, and profile resolution reads the value it writes; in v1 \
             nothing consulted default_profile, so the command reported success \
             and changed nothing",
        ),
        test: Some("switch_writes_default_profile_and_the_next_command_honours_it"),
    },
    Capability {
        v1_command: "auth token",
        v1_flags: &[],
        parity: Parity::Preserved("auth token"),
        test: Some("token_prints_the_bearer_and_nothing_else"),
    },
    Capability {
        v1_command: "completion",
        v1_flags: &[],
        parity: Parity::Preserved("completion"),
        test: Some("completion_emits_a_script_for_each_supported_shell"),
    },
    Capability {
        v1_command: "update",
        v1_flags: &[],
        parity: Parity::Preserved("update"),
        test: Some("update_is_reachable_and_names_the_v2_binary"),
    },
    Capability {
        v1_command: "version",
        v1_flags: &[],
        parity: Parity::Preserved("version"),
        test: Some("version_reports_the_crate_version_in_every_output_mode"),
    },
    Capability {
        v1_command: "ping",
        v1_flags: &[],
        parity: Parity::Preserved("ping"),
        test: Some("ping_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "customers get",
        v1_flags: &[],
        parity: Parity::Preserved("customers get"),
        test: Some("get_renders_json_envelope_with_object_customer"),
    },
    Capability {
        v1_command: "customers create",
        v1_flags: &[
            "--company",
            "--email",
            "--first-name",
            "--last-name",
            "--mobile",
        ],
        parity: Parity::Preserved(
            "customers create --company/--email/--first-name/--last-name/--mobile",
        ),
        test: Some("create_sends_the_company_under_the_v2_key_name"),
    },
    Capability {
        v1_command: "customers create",
        v1_flags: &[
            "--billing-city",
            "--billing-line1",
            "--billing-line2",
            "--billing-postal-code",
        ],
        parity: Parity::Preserved(
            "customers create --billing-city/--billing-line1/--billing-line2/--billing-postal-code",
        ),
        test: Some("create_sends_the_billing_address_under_v2_key_names"),
    },
    Capability {
        v1_command: "customers create",
        v1_flags: &[
            "--billing-country-id",
            "--billing-state-id",
            "--billing-state",
        ],
        parity: Parity::Replaced(
            "customers create --billing-country/--billing-state",
            "v2's AddressDto takes countryCode and stateCode, not the ids v1 sent, so one flag per component replaces v1's code-or-id pair",
        ),
        test: Some("create_sends_the_billing_address_under_v2_key_names"),
    },
    Capability {
        v1_command: "transactions get",
        v1_flags: &[],
        parity: Parity::Preserved("transactions get"),
        test: Some("get_renders_the_transaction_envelope"),
    },
    Capability {
        v1_command: "transactions sale",
        v1_flags: &["--amount", "--card", "--cvv", "--exp", "--reference-id"],
        parity: Parity::Replaced(
            "transactions create --amount/--card/--cvv/--exp/--reference-id",
            "one endpoint; a sale and an authorization differ only by captureMethod, so v1's sale and auth become create --capture-method auto|manual",
        ),
        test: Some("create_posts_the_documented_and_undocumented_fields"),
    },
    Capability {
        v1_command: "transactions sale",
        v1_flags: &["--customer-id"],
        parity: Parity::Preserved("transactions create --customer-id"),
        test: Some("create_succeeds_without_customer_id_and_omits_the_field"),
    },
    Capability {
        v1_command: "transactions sale",
        v1_flags: &[
            "--billing-city",
            "--billing-line1",
            "--billing-line2",
            "--billing-postal-code",
        ],
        parity: Parity::Preserved(
            "transactions create --billing-city/--billing-line1/--billing-line2/--billing-postal-code",
        ),
        test: Some("create_sends_the_billing_address_under_v2_key_names"),
    },
    Capability {
        v1_command: "transactions sale",
        v1_flags: &[
            "--billing-country-id",
            "--billing-state-id",
            "--billing-state",
        ],
        parity: Parity::Replaced(
            "transactions create --billing-country/--billing-state",
            "v2's AddressDto takes countryCode and stateCode, not the ids v1 sent, so one flag per component replaces v1's code-or-id pair",
        ),
        test: Some("create_sends_the_billing_address_under_v2_key_names"),
    },
    Capability {
        v1_command: "transactions sale",
        v1_flags: &["--currency-id"],
        parity: Parity::Replaced(
            "transactions create --currency-code",
            "v2 takes an ISO currency code where v1 sent an internal id",
        ),
        test: Some("create_posts_the_documented_and_undocumented_fields"),
    },
    Capability {
        v1_command: "transactions sale",
        v1_flags: &["--card-data-source"],
        parity: Parity::Removed(
            "v2's CreateTransactionRequestDto declares no cardDataSource. The field \
             exists only on the response, as cardDetails.cardDataSource, so there is \
             nothing left for the flag to set",
        ),
        test: None,
    },
    Capability {
        v1_command: "transactions sale",
        v1_flags: &["--payment-method-id"],
        parity: Parity::Replaced(
            "transactions create --payment-method-id --instrument card|ach",
            "the id alone is no longer enough: v2 declares cardData and achData as \
             separate objects and a stored id does not say which it belongs in, so \
             --instrument decides",
        ),
        test: Some("transaction_create_saved_card_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "transactions sale",
        v1_flags: &["--tip-amount", "--l2-tax-rate", "--l3-invoice", "--l3-po"],
        parity: Parity::Preserved(
            "transactions create --tip-amount/--l2-tax-rate/--l3-invoice/--l3-po",
        ),
        test: Some("transaction_create_level_three_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "transactions sale",
        v1_flags: &["--l3-product"],
        parity: Parity::Replaced(
            "transactions create --l3-product",
            "same flag, different value syntax. v1 took five positional \
             comma-separated fields; TransactionProductIsvDto declares eight, three of \
             which v1 had no position for, so the value is comma-separated key=value \
             pairs keyed by wire name",
        ),
        test: Some("transaction_create_level_three_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "transactions auth",
        v1_flags: &[
            "--amount",
            "--card",
            "--cvv",
            "--exp",
            "--reference-id",
            "--customer-id",
        ],
        parity: Parity::Replaced(
            "transactions create --capture-method manual",
            "one endpoint; a sale and an authorization differ only by captureMethod",
        ),
        test: Some("create_manual_capture_sends_the_capitalised_wire_value"),
    },
    Capability {
        v1_command: "transactions auth",
        v1_flags: &["--card-data-source"],
        parity: Parity::Removed(
            "v2's CreateTransactionRequestDto declares no cardDataSource. The field \
             exists only on the response, as cardDetails.cardDataSource, so there is \
             nothing left for the flag to set",
        ),
        test: None,
    },
    Capability {
        v1_command: "transactions auth",
        v1_flags: &["--payment-method-id"],
        parity: Parity::Replaced(
            "transactions create --payment-method-id --instrument card|ach",
            "the id alone is no longer enough: v2 declares cardData and achData as \
             separate objects and a stored id does not say which it belongs in, so \
             --instrument decides",
        ),
        test: Some("transaction_create_saved_card_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "transactions auth",
        v1_flags: &["--tip-amount", "--l2-tax-rate", "--l3-invoice", "--l3-po"],
        parity: Parity::Preserved(
            "transactions create --tip-amount/--l2-tax-rate/--l3-invoice/--l3-po",
        ),
        test: Some("transaction_create_level_three_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "transactions auth",
        v1_flags: &["--l3-product"],
        parity: Parity::Replaced(
            "transactions create --l3-product",
            "same flag, different value syntax. v1 took five positional \
             comma-separated fields; TransactionProductIsvDto declares eight, three of \
             which v1 had no position for, so the value is comma-separated key=value \
             pairs keyed by wire name",
        ),
        test: Some("transaction_create_level_three_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "transactions auth",
        v1_flags: &[
            "--billing-line1",
            "--billing-line2",
            "--billing-city",
            "--billing-postal-code",
        ],
        parity: Parity::Preserved("transactions create, the same --billing-* flags"),
        test: Some("transaction_create_new_ach_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "transactions auth",
        v1_flags: &[
            "--billing-state",
            "--billing-state-id",
            "--billing-country-id",
        ],
        parity: Parity::Replaced(
            "transactions create --billing-state/--billing-country",
            "v1 carried ids; v2's AddressDto takes stateCode and countryCode",
        ),
        test: Some("transaction_create_new_ach_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "transactions auth",
        v1_flags: &["--currency-id"],
        parity: Parity::Replaced(
            "transactions create --currency-code",
            "v1 sent a numeric currency id; v2 declares currencyCode, an ISO code",
        ),
        test: Some("create_posts_the_documented_and_undocumented_fields"),
    },
    Capability {
        v1_command: "ach credit",
        v1_flags: &[
            "--account",
            "--routing",
            "--account-type",
            "--account-holder-type",
            "--tax-id",
        ],
        parity: Parity::Replaced(
            "transactions credit --ach-account-number/--ach-routing-number/\
             --ach-account-type/--ach-account-holder-type/--ach-tax-id",
            "v2 has no ach group: a credit is a transaction verb, and it takes a card \
             as well as a bank account, which v1's `ach credit` could not",
        ),
        test: Some("transaction_credit_ach_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "ach credit",
        v1_flags: &[
            "--amount",
            "--payment-processor-id",
            "--customer-id",
            "--sec-code",
            "--requester-ip",
        ],
        parity: Parity::Replaced(
            "transactions credit --amount/--payment-processor-id/--customer-id/\
             --sec-code/--requester-ip",
            "same flags, and one new obligation: CreditRequestDto declares referenceId \
             required, where CreateTransactionRequestDto leaves it optional",
        ),
        test: Some("transaction_credit_ach_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "ach credit",
        v1_flags: &[
            "--contact-first-name",
            "--contact-last-name",
            "--contact-company",
            "--contact-email",
            "--contact-phone",
        ],
        parity: Parity::Preserved(
            "transactions credit --contact-first-name/--contact-last-name/--contact-company/--contact-email/--contact-phone",
        ),
        test: Some("transaction_credit_card_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "ach credit",
        v1_flags: &[
            "--billing-line1",
            "--billing-line2",
            "--billing-city",
            "--billing-postal-code",
        ],
        parity: Parity::Preserved("transactions credit, the same --billing-* flags"),
        test: Some("transaction_credit_card_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "ach credit",
        v1_flags: &[
            "--billing-state",
            "--billing-state-id",
            "--billing-country-id",
        ],
        parity: Parity::Replaced(
            "transactions credit --billing-state/--billing-country",
            "v1 carried ids; v2's AddressDto takes stateCode and countryCode",
        ),
        test: Some("transaction_credit_card_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "ach credit",
        v1_flags: &["--faster"],
        parity: Parity::Replaced(
            "transactions credit --same-day",
            "as on the debit side: v1's flag named a speed, v2's field is \
             isSameDayProcessing",
        ),
        test: Some("an_ach_charge_can_request_same_day_processing"),
    },
    Capability {
        v1_command: "ach credit",
        v1_flags: &["--payment-method-id"],
        parity: Parity::Replaced(
            "transactions credit --payment-method-id --instrument card|ach",
            "the id alone does not say which of cardData and achData it belongs in",
        ),
        test: Some("a_saved_card_credit_sends_no_capture_method"),
    },
    Capability {
        v1_command: "ach debit",
        v1_flags: &[
            "--account",
            "--routing",
            "--account-type",
            "--account-holder-type",
            "--tax-id",
        ],
        parity: Parity::Replaced(
            "transactions create --ach-account-number/--ach-routing-number/\
             --ach-account-type/--ach-account-holder-type/--ach-tax-id",
            "v2 has no separate ach group: an ACH debit is a transaction whose \
             transactionDetails carry achData, so the flags are prefixed to sit beside \
             the card ones on one command",
        ),
        test: Some("transaction_create_new_ach_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "ach debit",
        v1_flags: &[
            "--amount",
            "--payment-processor-id",
            "--customer-id",
            "--sec-code",
            "--requester-ip",
        ],
        parity: Parity::Preserved(
            "transactions create --amount/--payment-processor-id/--customer-id/--sec-code/--requester-ip",
        ),
        test: Some("transaction_create_new_ach_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "ach debit",
        v1_flags: &[
            "--contact-first-name",
            "--contact-last-name",
            "--contact-company",
            "--contact-email",
            "--contact-phone",
        ],
        parity: Parity::Preserved(
            "transactions create --contact-first-name/--contact-last-name/--contact-company/--contact-email/--contact-phone",
        ),
        test: Some("transaction_create_level_three_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "ach debit",
        v1_flags: &[
            "--billing-line1",
            "--billing-line2",
            "--billing-city",
            "--billing-postal-code",
        ],
        parity: Parity::Preserved("transactions create, the same --billing-* flags"),
        test: Some("transaction_create_new_ach_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "ach debit",
        v1_flags: &[
            "--billing-state",
            "--billing-state-id",
            "--billing-country-id",
        ],
        parity: Parity::Replaced(
            "transactions create --billing-state/--billing-country",
            "v1 carried ids; v2's AddressDto takes stateCode and countryCode",
        ),
        test: Some("transaction_create_new_ach_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "ach debit",
        v1_flags: &["--faster"],
        parity: Parity::Replaced(
            "transactions create --same-day",
            "v1's flag named a speed; v2's field is isSameDayProcessing, and the flag \
             names the thing it sets",
        ),
        test: Some("an_ach_charge_can_request_same_day_processing"),
    },
    Capability {
        v1_command: "ach debit",
        v1_flags: &["--payment-method-id"],
        parity: Parity::Replaced(
            "transactions create --payment-method-id --instrument card|ach",
            "the id alone is no longer enough: v2 declares cardData and achData as \
             separate objects and a stored id does not say which it belongs in, so \
             --instrument decides",
        ),
        test: Some("transaction_create_saved_ach_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "ach refund",
        v1_flags: &[],
        parity: Parity::Replaced(
            "transactions reversal",
            "v2 has one reversal endpoint where v1 had void and refund; the payment \
             method and the settled state are detected server-side, so the caller no \
             longer chooses between two commands",
        ),
        test: Some("transaction_reversal_partial_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "ach void",
        v1_flags: &[],
        parity: Parity::Replaced(
            "transactions reversal",
            "v2 has one reversal endpoint where v1 had void and refund; the payment \
             method and the settled state are detected server-side, so the caller no \
             longer chooses between two commands",
        ),
        test: Some("transaction_reversal_full_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "customers add-ach",
        v1_flags: &[
            "--account",
            "--account-holder-type",
            "--account-type",
            "--name",
            "--routing",
            "--tax-id",
        ],
        parity: Parity::Replaced(
            "payment-methods add-ach --account/--routing/--account-type/\
             --account-holder-type/--tax-id/--name",
            "same flags, moved out of `customers`: in v2 a payment method has its own \
             list endpoint, its own filters, and can exist with no customer at all, so \
             it is not a sub-resource of one",
        ),
        test: Some("payment_method_add_ach_business_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "customers add-card",
        v1_flags: &["--card", "--cvv", "--exp", "--name"],
        parity: Parity::Replaced(
            "payment-methods add-card --card/--cvv/--exp/--name",
            "same flags, moved out of `customers` for the same reason as add-ach; \
             --name reaches `paymentName`, which is what v2 calls the label on \
             the card route",
        ),
        test: Some("payment_method_add_card_full_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "customers delete",
        v1_flags: &["--yes"],
        parity: Parity::Preserved("customers delete --yes"),
        test: Some("delete_without_yes_refuses_and_issues_no_request"),
    },
    Capability {
        v1_command: "customers list",
        v1_flags: &["--limit"],
        parity: Parity::Replaced(
            "customers list --page-size",
            "same idea, different bound and default: v2 declares pageSize 1-100 with a \
             server default of 20, and the flag is omitted when absent so that default \
             governs. v1 defaulted --limit to 25 and always sent it, which made the \
             server default unreachable",
        ),
        test: Some("customer_list_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "customers list",
        v1_flags: &["--page"],
        parity: Parity::Replaced(
            "customers list --page-index",
            "0-based, mirroring the API's own pageIndex. v1's 1-based --page was \
             friendlier in isolation and introduced a silent off-by-one against every \
             example in the API documentation",
        ),
        test: Some("customer_list_filtered_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "customers list",
        v1_flags: &["--search"],
        parity: Parity::Replaced(
            "customers list --full-name/--email/--company-name/--mobile",
            "v2 declares no single search parameter; it declares four named filters, \
             so one flag would have to guess which of them the caller meant",
        ),
        test: Some("list_sends_every_named_filter_v1_search_replaces"),
    },
    Capability {
        v1_command: "customers methods",
        v1_flags: &[],
        parity: Parity::Replaced(
            "payment-methods list",
            "v1 could only list one customer's methods; v2's endpoint lists the \
             merchant's, with --customer-id as a filter, so the capability grew rather \
             than moved",
        ),
        test: Some("payment_method_list_filtered_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "customers remove-method",
        v1_flags: &["--yes"],
        parity: Parity::Replaced(
            "payment-methods delete --yes",
            "renamed with the group; the --yes gate and its no-request-issued refusal \
             are unchanged",
        ),
        test: Some("payment_method_delete_without_yes_issues_no_request"),
    },
    Capability {
        v1_command: "customers update",
        v1_flags: &["--first-name", "--last-name", "--email", "--mobile"],
        parity: Parity::Preserved("customers update --first-name/--last-name/--email/--mobile"),
        test: Some("update_sends_every_v1_field_under_its_v2_wire_name"),
    },
    Capability {
        v1_command: "customers update",
        v1_flags: &["--company"],
        parity: Parity::Preserved("customers update --company"),
        test: Some("update_sends_every_v1_field_under_its_v2_wire_name"),
    },
    Capability {
        v1_command: "customers update",
        v1_flags: &[
            "--billing-line1",
            "--billing-line2",
            "--billing-city",
            "--billing-postal-code",
        ],
        parity: Parity::Preserved(
            "customers update --billing-line1/--billing-line2/--billing-city/--billing-postal-code",
        ),
        test: Some("customer_update_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "customers update",
        v1_flags: &[
            "--billing-state",
            "--billing-state-id",
            "--billing-country-id",
        ],
        parity: Parity::Replaced(
            "customers update --billing-state/--billing-country",
            "v1 carried ids; v2's AddressDto takes stateCode and countryCode, so the \
             pair of id flags collapses into two code flags and the values differ",
        ),
        test: Some("update_sends_every_v1_field_under_its_v2_wire_name"),
    },
    Capability {
        v1_command: "devices get",
        v1_flags: &[],
        parity: Parity::Removed("no v2 endpoints; run the v1 CLI, which installs alongside"),
        test: None,
    },
    Capability {
        v1_command: "devices list",
        v1_flags: &[],
        parity: Parity::Removed("no v2 endpoints; run the v1 CLI, which installs alongside"),
        test: None,
    },
    Capability {
        v1_command: "devices register",
        v1_flags: &["--name"],
        parity: Parity::Removed("no v2 endpoints; run the v1 CLI, which installs alongside"),
        test: None,
    },
    Capability {
        v1_command: "devices ttp-activate",
        v1_flags: &[],
        parity: Parity::Removed("no v2 endpoints; run the v1 CLI, which installs alongside"),
        test: None,
    },
    Capability {
        v1_command: "devices ttp-jwt",
        v1_flags: &["--device-id"],
        parity: Parity::Removed("no v2 endpoints; run the v1 CLI, which installs alongside"),
        test: None,
    },
    Capability {
        v1_command: "keys create",
        v1_flags: &["--merchant-id", "--name"],
        parity: Parity::Replaced(
            "api-keys create --merchant-id/--name",
            "the same two flags on a renamed group: v2 calls them API keys, and \
             the design enumerates renamed commands as a v2 change. The envelope \
             name is not renamed with it — that would be a change to the output \
             contract nothing enumerates",
        ),
        test: Some("api_key_create_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "keys list",
        v1_flags: &["--merchant-id"],
        parity: Parity::Replaced(
            "api-keys list --merchant-id",
            "the same flag on a renamed group. It is also the whole query: the \
             response declares no pageInfo, so v1's absent pagination stays \
             absent rather than being wired in from the shared surface",
        ),
        test: Some("api_key_list_for_one_merchant_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "keys revoke",
        v1_flags: &["--client-id", "--yes"],
        parity: Parity::Preserved("api-keys revoke --client-id --yes"),
        test: Some("api_key_revoke_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "keys revoke",
        v1_flags: &["--merchant-id"],
        parity: Parity::Removed(
            "DELETE /v2/api-keys/{clientId} takes no merchantId, in its path or \
             its query. The client id identifies the key on its own",
        ),
        test: None,
    },
    Capability {
        v1_command: "pos cancel",
        v1_flags: &[],
        parity: Parity::Replaced(
            "pos cancel <pos-transaction-id> --yes",
            "same command, behind the confirmation gate: v2 extends --yes to \
             every destructive verb it owns, and v1 cancelled without asking",
        ),
        test: Some("pos_cancel_without_yes_issues_no_request"),
    },
    Capability {
        v1_command: "pos create",
        v1_flags: &[
            "--amount",
            "--customer-id",
            "--payment-processor-id",
            "--pos-device-id",
            "--terminal-id",
            "--tip-amount",
            "--tip-rate",
        ],
        parity: Parity::Preserved(
            "pos create --amount/--customer-id/--payment-processor-id/\
             --pos-device-id/--terminal-id/--tip-amount/--tip-rate",
        ),
        test: Some("pos_create_full_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "pos create",
        v1_flags: &["--currency-id"],
        parity: Parity::Replaced(
            "pos create --currency-code",
            "CreatePosTransactionRequestDto declares currencyCode as an ISO 4217 \
             string and requires it; v1 sent an integer currency id, which has no \
             v2 field to reach",
        ),
        test: Some("pos_create_full_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "pos create",
        v1_flags: &["--reading-method"],
        parity: Parity::Replaced(
            "pos create --reading-method keyed-entry|regular",
            "the same choice under the declared enum rather than v1's integer id, \
             so a caller need not know that 2 meant keyed entry",
        ),
        test: Some("pos_create_full_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "pos create",
        v1_flags: &["--reference-id"],
        parity: Parity::Replaced(
            "pos create --reference-id",
            "the same flag, not required in v2: v1 marked it required because the v1 \
             API rejected creates without it, and CreatePosTransactionRequestDto \
             does not declare it required",
        ),
        test: Some("pos_create_required_only_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "pos create",
        v1_flags: &["--transaction-type"],
        parity: Parity::Replaced(
            "pos create --capture-method auto|manual",
            "a v2 POS create only creates, so the five v1 transaction types are \
             gone; the one distinction it still carries, charge versus \
             authorization, is captureMethod",
        ),
        test: Some("pos_create_full_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "pos create",
        v1_flags: &["--target-transaction-id"],
        parity: Parity::Removed(
            "CreatePosTransactionRequestDto declares no targetTransactionId. v1 \
             needed one because its create also performed captures, voids and \
             refunds against an earlier transaction; those are their own v2 \
             endpoints under `transactions`",
        ),
        test: None,
    },
    Capability {
        v1_command: "pos create",
        v1_flags: &["--wait", "--wait-timeout"],
        parity: Parity::Replaced(
            "pos create --wait/--wait-timeout",
            "the same two flags driving a different mechanism: v2 declares two \
             independent waiting controls and --wait sets both, so each poll \
             blocks server-side instead of the client spinning on isCompleted",
        ),
        test: Some("pos_create_wait_polls_until_the_status_leaves_in_progress"),
    },
    Capability {
        v1_command: "pos get",
        v1_flags: &[],
        parity: Parity::Preserved("pos get <pos-transaction-id>"),
        test: Some("pos_get_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "pos list",
        v1_flags: &["--limit", "--page"],
        parity: Parity::Replaced(
            "pos list --page-size/--page-index",
            "the shared pagination surface: 0-based, bounded 1-100, and omitted \
             when absent so the server's defaults govern",
        ),
        test: Some("pos_list_every_filter_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "pos list",
        v1_flags: &["--terminal-id"],
        parity: Parity::Preserved("pos list --terminal-id"),
        test: Some("pos_list_every_filter_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "settlements get",
        v1_flags: &[],
        parity: Parity::Replaced(
            "settlements get <batch-id>",
            "same command, and still no endpoint behind it — but the batchIds \
             filter is applied server-side, where v1 fetched a hundred-item page \
             and scanned it, reporting any batch beyond that page as missing",
        ),
        test: Some("settlement_get_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "settlements list",
        v1_flags: &["--from", "--to"],
        parity: Parity::Preserved("settlements list --from/--to"),
        test: Some("settlement_list_every_filter_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "settlements list",
        v1_flags: &["--limit", "--page"],
        parity: Parity::Replaced(
            "settlements list --page-size/--page-index",
            "the shared pagination surface: 0-based, bounded 1-100, and omitted \
             when absent so the server's defaults govern",
        ),
        test: Some("settlement_list_every_filter_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "settlements list",
        v1_flags: &["--status"],
        parity: Parity::Replaced(
            "settlements list --status open|pending-settlement|settled|declined",
            "four statuses where v1 offered two, and sent as the declared string \
             rather than v1's integer id",
        ),
        test: Some("settlement_list_every_filter_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "subscriptions create",
        v1_flags: &[
            "--amount",
            "--currency-id",
            "--customer-id",
            "--faster",
            "--interval",
            "--number-of-payments",
            "--payment-frequency",
            "--payment-method-id",
            "--payment-processor-id",
            "--requester-ip",
            "--sec-code",
            "--start-date",
            "--transaction-type",
        ],
        parity: Parity::Removed("no v2 endpoints; run the v1 CLI, which installs alongside"),
        test: None,
    },
    Capability {
        v1_command: "subscriptions get",
        v1_flags: &[],
        parity: Parity::Removed("no v2 endpoints; run the v1 CLI, which installs alongside"),
        test: None,
    },
    Capability {
        v1_command: "subscriptions list",
        v1_flags: &["--customer-id", "--limit", "--page", "--search", "--status"],
        parity: Parity::Removed("no v2 endpoints; run the v1 CLI, which installs alongside"),
        test: None,
    },
    Capability {
        v1_command: "subscriptions payments",
        v1_flags: &[],
        parity: Parity::Removed("no v2 endpoints; run the v1 CLI, which installs alongside"),
        test: None,
    },
    Capability {
        v1_command: "subscriptions terminate",
        v1_flags: &["--yes"],
        parity: Parity::Removed("no v2 endpoints; run the v1 CLI, which installs alongside"),
        test: None,
    },
    Capability {
        v1_command: "terminals list",
        v1_flags: &["--limit", "--page"],
        parity: Parity::Replaced(
            "terminals list --page-size/--page-index",
            "the shared pagination surface: 0-based, bounded 1-100, and omitted \
             when absent so the server's defaults govern",
        ),
        test: Some("terminal_list_every_filter_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "terminals status",
        v1_flags: &[],
        parity: Parity::Preserved("terminals status <terminal-id>"),
        test: Some("terminal_status_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "transactions capture",
        v1_flags: &["--amount"],
        parity: Parity::Preserved("transactions capture --amount"),
        test: Some("transaction_capture_partial_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "transactions capture",
        v1_flags: &["--transaction-id"],
        parity: Parity::Preserved("transactions capture --transaction-id"),
        test: Some("transaction_capture_full_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "transactions inspect",
        v1_flags: &[],
        parity: Parity::Replaced(
            "transactions inspect",
            "same command and the same absence of an endpoint, but the pair has \
             swapped roles: v2's `get` prints the whole response, so `inspect` is \
             the curated view rather than the more detailed one",
        ),
        test: Some("inspect_reads_through_the_get_endpoint_and_curates_the_table"),
    },
    Capability {
        v1_command: "transactions list",
        v1_flags: &["--limit", "--page"],
        parity: Parity::Replaced(
            "transactions list --page-size/--page-index",
            "the shared pagination surface: 0-based, bounded 1-100, and omitted when \
             absent so the server's defaults govern",
        ),
        test: Some("transaction_list_every_filter_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "transactions list",
        v1_flags: &["--from", "--to", "--status"],
        parity: Parity::Preserved("transactions list --from/--to/--status"),
        test: Some("transaction_list_every_filter_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "transactions list",
        v1_flags: &["--unsettled"],
        parity: Parity::Removed(
            "v2 declares no unsettled parameter, and transactionStatus is an equality \
             filter over eighteen values, so \"not settled\" is not expressible \
             server-side. Filtering client-side would report a wrong answer on any \
             collection larger than one page, which is worse than not offering it",
        ),
        test: None,
    },
    Capability {
        v1_command: "transactions refund",
        v1_flags: &["--amount"],
        parity: Parity::Replaced(
            "transactions reversal --amount",
            "a partial refund is a partial reversal in v2; the amount flag survives \
             and the command it sits on does not",
        ),
        test: Some("transaction_reversal_partial_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "transactions refund",
        v1_flags: &["--transaction-id"],
        parity: Parity::Replaced(
            "transactions reversal --transaction-id",
            "the flag carries over; the command it sits on does not, because v2 \
             has one reversal endpoint where v1 had void and refund",
        ),
        test: Some("transaction_reversal_full_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "transactions refund",
        v1_flags: &["--card-data-source"],
        parity: Parity::Removed(
            "ReversalRequestDto declares one property, reversalAmount. There is no \
             cardDataSource on it, or on any v2 request",
        ),
        test: None,
    },
    Capability {
        v1_command: "transactions settle",
        v1_flags: &["--payment-processor-id"],
        parity: Parity::Replaced(
            "settlements close --payment-processor-id",
            "the same flag on a command that names what it does: settling is a \
             batch operation, and v1 filed it under transactions. The response \
             changed with it — v2 answers the resulting batch status where v1 \
             answered a transaction",
        ),
        test: Some("settlement_close_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "transactions tip-adjust",
        v1_flags: &["--tip-amount"],
        parity: Parity::Preserved("transactions tip-adjust --tip-amount"),
        test: Some("transaction_tip_adjust_amount_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "transactions tip-adjust",
        v1_flags: &["--transaction-id"],
        parity: Parity::Preserved("transactions tip-adjust --transaction-id"),
        test: Some("transaction_tip_adjust_rate_exchange_matches_the_contract"),
    },
    Capability {
        v1_command: "transactions void",
        v1_flags: &["--transaction-id"],
        parity: Parity::Replaced(
            "transactions reversal",
            "v2 has one reversal endpoint where v1 had void and refund; the payment \
             method and the settled state are detected server-side, so the caller no \
             longer chooses between two commands",
        ),
        test: Some("transaction_reversal_full_exchange_matches_the_contract"),
    },
];
