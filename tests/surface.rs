mod support;

use std::collections::BTreeSet;
use support::checks;
use support::spec;
use support::surface::SURFACE;

/// Every request property and query parameter of every mapped operation is
/// reachable, fixed deliberately, or excluded with a reason. Endpoint
/// coverage cannot see this.
#[test]
fn every_request_field_is_exposed_fixed_or_excluded() {
    checks::every_request_field_is_accounted_for(&SURFACE);
}

/// The reverse: a row naming a field the spec does not have is stale.
#[test]
fn no_surface_row_names_a_field_that_does_not_exist() {
    checks::no_surface_row_is_stale(&SURFACE);
}

/// Every flag named here must appear in that command's `--help`.
#[test]
fn every_exposed_flag_appears_in_help() {
    checks::exposed_flags_appear_in_help(&SURFACE);
}

/// One client-side `wire()` map, and the field whose bundle enum it must
/// equal.
struct Vocabulary {
    /// The CLI type the row binds, for the failure message.
    name: &'static str,
    values: fn() -> BTreeSet<String>,
    operation_id: &'static str,
    /// The surface matrix's spelling: `?name` for a query parameter, a JSON
    /// pointer for a request-body field.
    field: &'static str,
}

/// The values a clap value-enum puts on the wire.
///
/// Read from the type rather than restated here, so a wrong arm in the map and
/// a variant nobody mapped both reach this comparison.
fn vocabulary<T: clap::ValueEnum + Copy>(wire: fn(T) -> &'static str) -> BTreeSet<String> {
    T::value_variants()
        .iter()
        .map(|v| wire(*v).to_string())
        .collect()
}

/// Every `wire()` map the CLI offers a flag for, against the field it is sent
/// as.
///
/// A map used at several operations is bound once: the bundle declares one
/// component per vocabulary, so a second row would re-read the same enum.
/// `transactions share-receipt --share-by` is absent because
/// `SendReceiptRequestDto.shareBy` declares a pattern where its description
/// documents an enum — a conformance exemption holds that suspension and
/// fails when the enum arrives. `settlements list` is below, where the departures are.
static VOCABULARIES: &[Vocabulary] = &[
    Vocabulary {
        name: "transactions::SecCode",
        values: || vocabulary(flute_cli2::groups::transactions::SecCode::wire),
        operation_id: "flute-v2-post-transactions",
        field: "/transactionDetails/achData/secCode",
    },
    Vocabulary {
        name: "transactions::SourceType",
        values: || vocabulary(flute_cli2::groups::transactions::SourceType::wire),
        operation_id: "flute-v2-get-transactions",
        field: "?sourceType",
    },
    Vocabulary {
        name: "transactions::TransactionStatus",
        values: || vocabulary(flute_cli2::groups::transactions::TransactionStatus::wire),
        operation_id: "flute-v2-get-transactions",
        field: "?transactionStatus",
    },
    Vocabulary {
        name: "terminals::TerminalStatusFilter",
        values: || vocabulary(flute_cli2::groups::terminals::TerminalStatusFilter::wire),
        operation_id: "flute-v2-get-terminals",
        field: "?terminalStatus",
    },
    Vocabulary {
        name: "terminals::TerminalMode",
        values: || vocabulary(flute_cli2::groups::terminals::TerminalMode::wire),
        operation_id: "flute-v2-get-terminals",
        field: "?terminalMode",
    },
    Vocabulary {
        name: "terminals::ConnectionStatus",
        values: || vocabulary(flute_cli2::groups::terminals::ConnectionStatus::wire),
        operation_id: "flute-v2-get-terminals",
        field: "?connectionStatus",
    },
    Vocabulary {
        name: "pos::InitiationChannel",
        values: || vocabulary(flute_cli2::groups::pos::InitiationChannel::wire),
        operation_id: "flute-v2-post-pos-transactions",
        field: "/initiationChannel",
    },
    Vocabulary {
        name: "pos::ReadingMethod",
        values: || vocabulary(flute_cli2::groups::pos::ReadingMethod::wire),
        operation_id: "flute-v2-post-pos-transactions",
        field: "/readingMethod",
    },
    Vocabulary {
        name: "pos::PosTransactionStatus",
        values: || vocabulary(flute_cli2::groups::pos::PosTransactionStatus::wire),
        operation_id: "flute-v2-get-pos-transactions",
        field: "?posTransactionStatus",
    },
    Vocabulary {
        name: "payment_links::LinkType",
        values: || vocabulary(flute_cli2::groups::payment_links::LinkType::wire),
        operation_id: "flute-v2-post-payment-links",
        field: "/linkType",
    },
    Vocabulary {
        name: "payment_links::PaymentLinkStatus",
        values: || vocabulary(flute_cli2::groups::payment_links::PaymentLinkStatus::wire),
        operation_id: "flute-v2-patch-payment-links-paymentLinkId",
        field: "/paymentLinkStatus",
    },
    Vocabulary {
        name: "payment_links::ShareChannel",
        values: || vocabulary(flute_cli2::groups::payment_links::ShareChannel::wire),
        operation_id: "flute-v2-post-payment-links-paymentLinkId-share",
        field: "/shareBy",
    },
    Vocabulary {
        name: "payment_sessions::SessionMode",
        values: || vocabulary(flute_cli2::groups::payment_sessions::SessionMode::wire),
        operation_id: "flute-v2-post-payment-sessions",
        field: "/mode",
    },
    Vocabulary {
        name: "payment_sessions::CustomerHandling",
        values: || vocabulary(flute_cli2::groups::payment_sessions::CustomerHandling::wire),
        operation_id: "flute-v2-post-payment-sessions",
        field: "/customerHandling",
    },
    Vocabulary {
        name: "common::AccountType",
        values: || vocabulary(flute_cli2::cli::common::AccountType::wire),
        operation_id: "flute-v2-post-payment-methods-ach",
        field: "/accountType",
    },
    Vocabulary {
        name: "common::AccountHolderType",
        values: || vocabulary(flute_cli2::cli::common::AccountHolderType::wire),
        operation_id: "flute-v2-post-payment-methods-ach",
        field: "/accountHolderType",
    },
    Vocabulary {
        name: "common::CaptureMethod",
        values: || vocabulary(flute_cli2::cli::common::CaptureMethod::wire),
        operation_id: "flute-v2-post-pos-transactions",
        field: "/captureMethod",
    },
    Vocabulary {
        name: "common::PricingType",
        values: || vocabulary(flute_cli2::cli::common::PricingType::wire),
        operation_id: "flute-v2-post-transactions",
        field: "/pricingType",
    },
];

/// Every flag value the CLI can send is a value the bundle declares, and every
/// value the bundle declares is one the CLI can send.
///
/// Set equality in both directions, because the two misses are different and
/// both are silent: a renamed value leaves the CLI sending what the API now
/// rejects, and a new value leaves it unable to send one at all — on a filter,
/// an empty page rather than an error.
#[test]
fn every_wire_vocabulary_matches_the_bundle() {
    for v in VOCABULARIES {
        assert_eq!(
            (v.values)(),
            spec::request_enum(v.operation_id, v.field),
            "{} does not offer the values {} declares at {}",
            v.name,
            v.operation_id,
            v.field
        );
    }
}

/// The two `settlements list` filters the bundle does not declare where the
/// CLI sends them.
#[test]
fn the_settlements_filters_match_the_bundle_where_it_declares_them() {
    // The `batchStatus` query parameter declares no enum, so the vocabulary
    // comes from the response field of the same name.
    assert_eq!(
        vocabulary(flute_cli2::groups::settlements::BatchStatus::wire),
        spec::response_enum("flute-v2-get-settlements-batches", "/items/[]/batchStatus"),
        "settlements::BatchStatus does not offer the values the batch list \
         reports"
    );

    // `--asc` is a boolean because `sortOrder` has exactly two values; a third
    // would leave a direction the flag cannot name.
    let sort_order = spec::request_enum("flute-v2-get-settlements-batches", "?sortOrder");
    assert_eq!(
        sort_order,
        BTreeSet::from(["asc".to_string(), "desc".to_string()])
    );
    let args = flute_cli2::groups::settlements::ListBatchesArgs {
        asc: true,
        ..Default::default()
    };
    let query = flute_cli2::groups::settlements::build_list_batches_query(&args).unwrap();
    assert!(
        query.contains(&("sortOrder", "asc".to_string())),
        "--asc sends something other than the bundle's ascending value: {query:?}"
    );
}

/// A map bound somewhere other than the table above, and why.
static DEPARTURES: &[(&str, &str)] = &[
    (
        "transactions::ShareBy",
        "SendReceiptRequestDto.shareBy declares an E.164 pattern where its \
         description documents the enum, and the conformance exemption fails \
         when the enum arrives",
    ),
    (
        "settlements::BatchStatus",
        "the batchStatus query parameter declares no enum, so it is bound to \
         the response field of the same name",
    ),
];

/// **A new `wire()` map cannot escape the binding above.**
///
/// A hand-written table omits whatever arrived last, so the declarations are
/// the oracle: every `pub fn wire` the CLI declares must be bound to the
/// bundle or named as a departure, and a row naming a map that does not
/// exist is stale.
#[test]
fn every_wire_map_is_bound_or_a_named_departure() {
    let mut sources = support::group_sources();
    sources.push(("common".to_string(), "src/cli/common.rs".into()));

    let mut declared: Vec<String> = Vec::new();
    for (module, path) in sources {
        let mut implemented_for = String::new();
        for line in std::fs::read_to_string(&path).unwrap().lines() {
            if let Some(rest) = line.strip_prefix("impl ") {
                implemented_for = rest.trim_end_matches(" {").to_string();
            }
            if line.trim_start().starts_with("pub fn wire(") {
                declared.push(format!("{module}::{implemented_for}"));
            }
        }
    }
    assert!(
        declared.len() > 15,
        "the scan found only {} wire maps, so it is not scanning",
        declared.len()
    );

    let bound: BTreeSet<&str> = VOCABULARIES
        .iter()
        .map(|v| v.name)
        .chain(DEPARTURES.iter().map(|(name, _)| *name))
        .collect();
    let declared: BTreeSet<&str> = declared.iter().map(String::as_str).collect();
    assert_eq!(
        declared, bound,
        "the wire maps the CLI declares and the ones bound to the bundle differ"
    );
}
