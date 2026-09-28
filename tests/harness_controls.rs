//! Negative controls: proof that the harness can see.
//!
//! Nothing in the layer table checks the checkers the rest of the suite
//! trusts. Every "Catches" cell is a claim
//! about roughly 1,500 lines of bespoke code, and a checker that accepts
//! everything is indistinguishable from a clean suite.
//!
//! Each control feeds a deliberately broken input to a checker and asserts
//! rejection **with the expected reason**. A panic for an unrelated reason is
//! not a pass — the rule the production tests follow, turned on the harness.
//!
//! **Every control builds its own fixture.** None reads `contracts::exchange`:
//! the matrix holds no command rows at this point, and even later a control
//! consuming a real row would start failing when that row legitimately
//! changed, turning a harness check into a maintenance tax. The operation ids
//! are real, so the bundle is still the oracle; only the fixtures are
//! synthetic.

mod support;

use serde_json::json;
use support::contracts::{Contract, Live, Mapping, Variant};
use support::parity::{Capability, Parity};
use support::spec::{self, Exchange, RequestFixture, ResponseFixture, amount};
use support::surface::{Exposure, Field};

/// Assert that `f` rejects its input, and rejects it for the stated reason.
///
/// Reason matching is the point. A control that only asserts "something
/// panicked" passes when the checker panics on an unrelated line, which is how
/// a checker that has stopped checking still looks healthy.
fn assert_rejects(reason_fragment: &str, f: impl FnOnce()) {
    // The checkers panic on rejection, and a rejection here is the expected
    // outcome, so the default hook's backtraces would be pure noise. The hook
    // is restored rather than replaced for the process: tests share one, and
    // leaving it installed would silence unrelated failures in this binary.
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    std::panic::set_hook(prev);

    let payload = outcome.expect_err(&format!(
        "checker accepted an input it claims to reject ({reason_fragment})"
    ));
    let msg = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("<non-string panic>");
    assert!(
        msg.contains(reason_fragment),
        "rejected for the wrong reason.\n  expected to mention: {reason_fragment}\n  \
         actual: {msg}"
    );
}

/// A minimal, valid customer-create request, built here rather than read from
/// the matrix so this file has no dependency on which groups exist yet.
fn valid_customer_create() -> (RequestFixture, ResponseFixture) {
    (
        RequestFixture {
            method: "POST".into(),
            path: "/v2/customers".into(),
            path_params: vec![],
            query: vec![],
            content_type: None,
            body: Some(json!({"firstName": "Ada", "lastName": "Lovelace"})),
        },
        ResponseFixture {
            status: 200,
            body: Some(json!({"customerId": "cus_1"})),
        },
    )
}

/// **The control that keeps every other control honest.** If the rejection
/// helper never rejects, all of them are vacuous — one level up from the
/// failure mode this file exists to eliminate.
#[test]
fn the_rejection_helper_fails_when_the_input_is_sound() {
    let (req, resp) = valid_customer_create();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert_rejects("this input is in fact valid", || {
            spec::assert_exchange_conforms("flute-v2-post-customers", &req, &resp);
        });
    }));
    let payload = outcome.expect_err("assert_rejects accepted a sound input");
    let msg = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .unwrap_or("");
    assert!(
        msg.contains("checker accepted an input it claims to reject"),
        "unexpected message: {msg}"
    );
}

/// And the baseline it depends on: the valid exchange must be *accepted*, or
/// every rejection below could be the harness failing on everything.
#[test]
fn conformance_accepts_a_valid_exchange() {
    let (req, resp) = valid_customer_create();
    spec::assert_exchange_conforms("flute-v2-post-customers", &req, &resp);
}

// ── Exchange conformance ─────────────────────────────────────────────────────

/// The claim that makes exchange conformance worth more than schema
/// validation: a body that is internally valid but attached to the wrong
/// operation must fail. If this control ever passes, the layer has silently
/// degraded to "some JSON validated against some schema".
#[test]
fn conformance_rejects_a_valid_body_on_the_wrong_operation() {
    let (req, resp) = valid_customer_create();
    assert_rejects("does not match template", || {
        spec::assert_exchange_conforms("flute-v2-post-payment-links", &req, &resp);
    });
}

/// `merchantId` is a documented filter the server ignores. An undeclared
/// query parameter is therefore invisible at runtime, and conformance is the
/// only layer that can see it.
#[test]
fn conformance_rejects_an_undeclared_query_parameter() {
    let req = RequestFixture {
        method: "GET".into(),
        path: "/v2/customers".into(),
        path_params: vec![],
        query: vec![("notAParameter".into(), "x".into())],
        content_type: None,
        body: None,
    };
    let resp = ResponseFixture {
        status: 200,
        body: Some(json!({"items": [], "pageInfo": {"hasMore": false}})),
    };
    assert_rejects("is not declared", || {
        spec::assert_exchange_conforms("flute-v2-get-customers", &req, &resp);
    });
}

#[test]
fn conformance_rejects_an_omitted_required_query_parameter() {
    let req = RequestFixture {
        method: "POST".into(),
        path: "/v2/payment-methods/pm_1/set-default".into(),
        path_params: vec![("paymentMethodId".into(), "pm_1".into())],
        query: vec![],
        content_type: None,
        body: None,
    };
    assert_rejects("required query parameter", || {
        spec::assert_exchange_conforms(
            "flute-v2-post-payment-methods-paymentMethodId-set-default",
            &req,
            &ResponseFixture {
                status: 200,
                body: None,
            },
        );
    });
}

/// An **array** query parameter is repeated pairs on the wire, so each value
/// has to validate against the parameter's `items` — never against the array
/// schema itself, which no single value can satisfy.
///
/// Without this, `batchIds=<uuid>&batchIds=<uuid>` is rejected as "not an
/// array" and the only two array parameters in the API become unreachable
/// through a conformant fixture.
#[test]
fn conformance_accepts_a_repeated_array_query_parameter() {
    let req = RequestFixture {
        method: "GET".into(),
        path: "/v2/settlements/batches".into(),
        path_params: vec![],
        query: vec![
            (
                "batchIds".into(),
                "21c75430-a316-456f-9126-365760dca33a".into(),
            ),
            (
                "batchIds".into(),
                "0f19d58c-3d4e-4f5a-6b7c-8d9e0f1a2b58".into(),
            ),
        ],
        content_type: None,
        body: None,
    };
    let resp = ResponseFixture {
        status: 200,
        body: Some(json!({"items": [], "pageInfo": {"hasMore": false}})),
    };
    spec::assert_exchange_conforms("flute-v2-get-settlements-batches", &req, &resp);
}

/// The array arm must not skip the declared-parameter lookup on its way to
/// the item schema: a repeated pair under a name the operation does not
/// declare is still undeclared.
#[test]
fn conformance_rejects_a_repeated_parameter_the_operation_does_not_declare() {
    let req = RequestFixture {
        method: "GET".into(),
        path: "/v2/settlements/batches".into(),
        path_params: vec![],
        query: vec![
            ("batchIdentifiers".into(), "a".into()),
            ("batchIdentifiers".into(), "b".into()),
        ],
        content_type: None,
        body: None,
    };
    let resp = ResponseFixture {
        status: 200,
        body: Some(json!({"items": [], "pageInfo": {"hasMore": false}})),
    };
    assert_rejects("is not declared", || {
        spec::assert_exchange_conforms("flute-v2-get-settlements-batches", &req, &resp);
    });
}

/// And a **scalar** parameter is still validated against its own type, so the
/// array arm has not disabled query validation wholesale. A query value is a
/// string on the wire and is coerced before validating; a value that cannot
/// be coerced has to fail.
#[test]
fn conformance_rejects_a_query_value_that_is_not_its_declared_type() {
    let req = RequestFixture {
        method: "GET".into(),
        path: "/v2/settlements/batches".into(),
        path_params: vec![],
        query: vec![("pageSize".into(), "many".into())],
        content_type: None,
        body: None,
    };
    let resp = ResponseFixture {
        status: 200,
        body: Some(json!({"items": [], "pageInfo": {"hasMore": false}})),
    };
    assert_rejects("does not conform", || {
        spec::assert_exchange_conforms("flute-v2-get-settlements-batches", &req, &resp);
    });
}

#[test]
fn conformance_rejects_a_request_field_with_the_wrong_casing() {
    let (mut req, resp) = valid_customer_create();
    req.body = Some(json!({"firstname": "Ada", "lastName": "Lovelace"}));
    assert_rejects("does not conform", || {
        spec::assert_exchange_conforms("flute-v2-post-customers", &req, &resp);
    });
}

#[test]
fn conformance_rejects_a_content_type_the_operation_does_not_declare() {
    let (mut req, resp) = valid_customer_create();
    req.content_type = Some("application/xml".into());
    assert_rejects("not declared", || {
        spec::assert_exchange_conforms("flute-v2-post-customers", &req, &resp);
    });
}

#[test]
fn conformance_rejects_an_invented_body_on_a_bodyless_success() {
    let req = RequestFixture {
        method: "GET".into(),
        path: "/v2/ping".into(),
        path_params: vec![],
        query: vec![],
        content_type: None,
        body: None,
    };
    assert_rejects("declares no body, fixture invents one", || {
        spec::assert_exchange_conforms(
            "flute-v2-get-ping",
            &req,
            &ResponseFixture {
                status: 200,
                body: Some(json!({"ok": true})),
            },
        );
    });
}

#[test]
fn conformance_rejects_no_body_where_the_status_declares_one() {
    let (req, mut resp) = valid_customer_create();
    resp.body = None;
    assert_rejects("declares a JSON body, fixture has none", || {
        spec::assert_exchange_conforms("flute-v2-post-customers", &req, &resp);
    });
}

/// The flag says every body is optional, so the body-presence rule has to
/// come from the schema. This is the control that would have caught an
/// empty-bodied charge being accepted as conformant.
#[test]
fn conformance_rejects_a_missing_body_when_the_schema_requires_fields() {
    let (mut req, resp) = valid_customer_create();
    req.body = None;
    assert_rejects("requestBody is required", || {
        spec::assert_exchange_conforms("flute-v2-post-customers", &req, &resp);
    });
}

/// The same rule, aimed at the request that matters most: `requestBody`
/// carries no `required` flag on `POST /v2/transactions`, and its schema
/// declares three required properties.
#[test]
fn conformance_rejects_an_empty_bodied_charge() {
    let req = RequestFixture {
        method: "POST".into(),
        path: "/v2/transactions".into(),
        path_params: vec![],
        query: vec![],
        content_type: None,
        body: None,
    };
    assert_rejects("requestBody is required", || {
        spec::assert_exchange_conforms(
            "flute-v2-post-transactions",
            &req,
            &ResponseFixture {
                status: 200,
                body: Some(json!({"transactionId": "t", "transactionStatus": "Captured"})),
            },
        );
    });
}

// ── Divergence scoping ───────────────────────────────────────────────────────

fn transaction_create(body: serde_json::Value) -> (RequestFixture, ResponseFixture) {
    (
        RequestFixture {
            method: "POST".into(),
            path: "/v2/transactions".into(),
            path_params: vec![],
            query: vec![],
            content_type: None,
            body: Some(body),
        },
        ResponseFixture {
            status: 200,
            body: Some(json!({
                "transactionId": "txn_1", "transactionStatus": "Captured",
                "processedAmount": amount("1.00"), "currencyCode": "USD"})),
        },
    )
}

/// `captureMethod` and `customerId` are exempt by name, so the exchange that
/// uses both must pass — otherwise the strip is not doing its job and the
/// controls below prove nothing.
#[test]
fn conformance_accepts_the_two_exempt_undocumented_fields() {
    let (req, resp) = transaction_create(json!({
        "paymentProcessorId": "pp-1", "baseAmount": amount("1.00"),
        "customerId": "cus_1",
        "transactionDetails": {"cardData": {
            "captureMethod": "Auto", "paymentMethodId": "pm_1"}}}));
    spec::assert_exchange_conforms("flute-v2-post-transactions", &req, &resp);
}

/// **`RequestField` strips only the named pointer.** A second undocumented
/// field must still fail, or the divergence has become a blanket hole.
#[test]
fn conformance_rejects_a_second_undocumented_field_beside_an_exempt_one() {
    let (req, resp) = transaction_create(json!({
        "paymentProcessorId": "pp-1", "baseAmount": amount("1.00"),
        "transactionDetails": {"cardData": {
            "captureMethod": "Auto", "paymentMethodId": "pm_1"}},
        "totallyInvented": "x"}));
    assert_rejects("does not conform", || {
        spec::assert_exchange_conforms("flute-v2-post-transactions", &req, &resp);
    });
}

/// A divergence must not leak onto its neighbours. The exemption covers
/// `captureMethod` on create only; the same field elsewhere is still an
/// undocumented field.
#[test]
fn the_capture_method_exemption_does_not_apply_to_a_neighbouring_operation() {
    let req = RequestFixture {
        method: "POST".into(),
        path: "/v2/transactions/txn_1/capture".into(),
        path_params: vec![("transactionId".into(), "txn_1".into())],
        query: vec![],
        content_type: None,
        body: Some(json!({"captureAmount": amount("5.25"), "captureMethod": "Auto"})),
    };
    let resp = ResponseFixture {
        status: 200,
        body: Some(json!({"transactionId": "txn_1", "transactionStatus": "Captured"})),
    };
    assert_rejects("does not conform", || {
        spec::assert_exchange_conforms(
            "flute-v2-post-transactions-transactionId-capture",
            &req,
            &resp,
        );
    });
}

/// **The control that keeps the response-shape exemption off the one
/// transaction endpoint whose paged schema is right.** `GET /v2/transactions`
/// declares the same schema and its declared example genuinely is a page.
#[test]
fn the_response_shape_exemption_cannot_swallow_the_genuinely_paged_operation() {
    let (req, _) = transaction_create(json!({
        "paymentProcessorId": "pp-1", "baseAmount": amount("1.00"),
        "transactionDetails": {"cardData": {"paymentMethodId": "pm_1"}}}));
    let page = ResponseFixture {
        status: 200,
        body: Some(json!({
            "items": [{"transactionId": "t"}], "pageInfo": {"hasMore": false}})),
    };
    assert_rejects("the fixture is a page", || {
        spec::assert_exchange_conforms("flute-v2-post-transactions", &req, &page);
    });
}

/// A response field no declared example carries is a shape the operation has
/// never been observed to return, and the examples are the only oracle the
/// response-shape exemption leaves.
#[test]
fn the_example_oracle_rejects_a_field_no_example_declares() {
    let (req, _) = transaction_create(json!({
        "paymentProcessorId": "pp-1", "baseAmount": amount("1.00"),
        "transactionDetails": {"cardData": {"paymentMethodId": "pm_1"}}}));
    let resp = ResponseFixture {
        status: 200,
        body: Some(json!({"transactionId": "t", "inventedByTheFixture": true})),
    };
    assert_rejects("no declared example carries", || {
        spec::assert_exchange_conforms("flute-v2-post-transactions", &req, &resp);
    });
}

// ── The bundle hash gate ─────────────────────────────────────────────────────

/// The gate is only a gate if a mismatch fails. Hashing a mutated copy proves
/// the comparison runs, without touching the vendored file.
#[test]
fn the_hash_gate_rejects_an_altered_bundle() {
    let mut bytes = include_bytes!("../docs/reference/openapi-v2.json").to_vec();
    bytes.extend_from_slice(b"\n");
    let recorded = include_str!("../docs/reference/openapi-v2.sha256").trim();
    assert_rejects("does not match", || {
        spec::assert_bundle_hash(&bytes, recorded);
    });
}

// ── The source scan ──────────────────────────────────────────────────────────

/// `test_fn_exists` pointed at the wrong directory would report full coverage
/// forever, which is the clearest case of a checker failing invisibly.
#[test]
fn the_source_scan_rejects_a_name_that_does_not_exist() {
    assert!(support::test_fn_exists(
        "the_source_scan_rejects_a_name_that_does_not_exist"
    ));
    assert!(!support::test_fn_exists(
        "no_such_test_function_anywhere_1f4c"
    ));
}

/// A prefix must not satisfy a longer name, or a row could name
/// `create_posts_everything` and be satisfied by `create_posts`.
#[test]
fn the_source_scan_does_not_match_on_a_prefix() {
    assert!(!support::test_fn_exists("the_source_scan_rejects_a_name"));
}

/// The subject is a *different* function on purpose. Asserting about this
/// one would put both ids in its own body, so the negative case could never
/// fail — the trap the citation scan itself exists to catch, one level up.
#[test]
fn the_citation_scan_rejects_a_test_that_never_names_its_operation() {
    const SUBJECT: &str = "ping_exchange_matches_the_contract";
    assert!(
        support::test_fn_cites(SUBJECT, "flute-v2-get-ping"),
        "the subject does mount the ping operation"
    );
    assert!(
        !support::test_fn_cites(SUBJECT, "flute-v2-post-customers"),
        "the subject does not cite customer create, so the scan must say so"
    );
}

/// Shapes that name a function without defining a test anyone can run.
///
/// They are written out as source text rather than passed to the scan as
/// strings because the scan reads `tests/` off disk: a fixture is only a
/// fixture if it is really spelled this way in a file the walk reaches.
mod decoys {
    // fn a_decoy_defined_only_in_a_line_comment() {}

    /* fn a_decoy_defined_only_in_a_block_comment() {} */

    /// A definition nothing compiles, quoted so the scan meets it as text.
    pub const QUOTED: &str = "fn a_decoy_defined_only_in_a_string_literal() {}";

    /// A helper with no `#[test]` above it. `cargo test` never calls it, so a
    /// row satisfied by this name claims coverage that cannot run.
    pub fn a_decoy_that_is_not_a_test() {}

    /// A real test that a plain `cargo test` skips.
    #[test]
    #[ignore = "a decoy for the scan; it asserts nothing"]
    fn a_decoy_that_never_runs() {}

    /// Names an operation in a comment and in a loose string, and drives no
    /// mock from either.
    #[test]
    fn a_decoy_that_names_an_operation_it_never_mounts() {
        // flute-v2-get-ping
        assert_eq!("flute-v2-get-ping".len(), 17);
    }
}

/// A name a comment mentions is not a definition, and `brace_body` starts at
/// the `fn` token — so the `//` earlier on the line is invisible to it and the
/// next `{` in the file is taken as the body.
#[test]
fn the_source_scan_ignores_a_name_that_only_a_comment_mentions() {
    assert!(!support::test_fn_exists(
        "a_decoy_defined_only_in_a_line_comment"
    ));
    assert!(!support::test_fn_exists(
        "a_decoy_defined_only_in_a_block_comment"
    ));
}

/// The same for a quoted definition: a string is data, never a declaration.
#[test]
fn the_source_scan_ignores_a_name_that_only_a_string_literal_holds() {
    assert!(decoys::QUOTED.contains("a_decoy_defined_only_in_a_string_literal"));
    assert!(!support::test_fn_exists(
        "a_decoy_defined_only_in_a_string_literal"
    ));
}

/// A helper is not a test. Without the attribute check a row could name any
/// function in the tree — `sequence`, `client_for` — and read as covered.
#[test]
fn the_source_scan_rejects_a_function_that_carries_no_test_attribute() {
    decoys::a_decoy_that_is_not_a_test();
    assert!(!support::test_fn_exists("a_decoy_that_is_not_a_test"));
}

/// An `#[ignore]`d function is a scenario somebody opts into by hand, so a
/// contract row satisfied by one claims coverage a plain `cargo test` never
/// produces. The live rows are the deliberate exception and are checked with
/// the counter that keeps them.
#[test]
fn the_source_scan_separates_a_test_that_runs_from_one_that_is_ignored() {
    assert_eq!(support::test_fn_definitions("a_decoy_that_never_runs"), 1);
    assert_eq!(
        support::runnable_test_fn_definitions("a_decoy_that_never_runs"),
        0
    );
}

/// An id in a comment or in a string no mock reads is a mention, not evidence.
/// The second subject is a real test: it passes the id to
/// `assert_exchange_conforms`, which validates a fixture built by hand and
/// exercises no wire traffic at all.
#[test]
fn the_citation_scan_rejects_an_operation_id_no_mock_is_driven_from() {
    assert!(!support::test_fn_cites(
        "a_decoy_that_names_an_operation_it_never_mounts",
        "flute-v2-get-ping"
    ));
    assert!(!support::test_fn_cites(
        "conformance_rejects_an_invented_body_on_a_bodyless_success",
        "flute-v2-get-ping"
    ));
}

// ── Coverage invariants ──────────────────────────────────────────────────────

fn ping_exchange() -> Exchange {
    Exchange {
        request: RequestFixture {
            method: "GET".into(),
            path: "/v2/ping".into(),
            path_params: vec![],
            query: vec![],
            content_type: None,
            body: None,
        },
        response: ResponseFixture {
            status: 200,
            body: None,
        },
    }
}

static VARIANT_NAMING_A_MISSING_TEST: &[Variant] = &[Variant {
    name: "default",
    exchange: ping_exchange,
    mock_test: "no_such_mock_test_2b7f",
    live: Live::Test("live_ping_succeeds"),
}];

static CONTRACT_WITH_A_MISSING_TEST: &[Contract] = &[Contract {
    operation_id: "flute-v2-get-ping",
    mapping: Mapping::Command("ping"),
    variants: VARIANT_NAMING_A_MISSING_TEST,
}];

#[test]
fn coverage_rejects_a_variant_naming_a_test_that_does_not_exist() {
    assert_rejects("does not exist", || {
        support::checks::variants_name_real_tests(CONTRACT_WITH_A_MISSING_TEST);
    });
}

static VARIANT_NOT_CITING_ITS_OPERATION: &[Variant] = &[Variant {
    name: "default",
    // A real test, but one that never names flute-v2-get-ping.
    mock_test: "the_hash_gate_rejects_an_altered_bundle",
    exchange: ping_exchange,
    live: Live::Test("live_ping_succeeds"),
}];

static CONTRACT_NOT_CITING: &[Contract] = &[Contract {
    operation_id: "flute-v2-get-ping",
    mapping: Mapping::Command("ping"),
    variants: VARIANT_NOT_CITING_ITS_OPERATION,
}];

#[test]
fn coverage_rejects_a_test_that_never_mentions_its_operation_id() {
    assert_rejects("never references its operation id", || {
        support::checks::variants_name_real_tests(CONTRACT_NOT_CITING);
    });
}

static VARIANT_NAMING_AN_IGNORED_TEST: &[Variant] = &[Variant {
    name: "default",
    exchange: ping_exchange,
    mock_test: "a_decoy_that_never_runs",
    live: Live::Test("live_ping_succeeds"),
}];

static CONTRACT_WITH_AN_IGNORED_TEST: &[Contract] = &[Contract {
    operation_id: "flute-v2-get-ping",
    mapping: Mapping::Command("ping"),
    variants: VARIANT_NAMING_AN_IGNORED_TEST,
}];

/// A mock row's whole job is to be run by `cargo test`. One naming an
/// `#[ignore]`d function is a green gate over a scenario nobody executes.
#[test]
fn coverage_rejects_a_mock_test_a_plain_run_would_skip() {
    assert_rejects("does not run", || {
        support::checks::variants_name_real_tests(CONTRACT_WITH_AN_IGNORED_TEST);
    });
}

static ONE_SOUND_VARIANT: &[Variant] = &[Variant {
    name: "default",
    exchange: ping_exchange,
    mock_test: "ping_exchange_matches_the_contract",
    live: Live::Test("live_ping_succeeds"),
}];

static ONE_SOUND_CONTRACT: &[Contract] = &[Contract {
    operation_id: "flute-v2-get-ping",
    mapping: Mapping::Command("ping"),
    variants: ONE_SOUND_VARIANT,
}];

/// The baseline: a manifest that agrees with the matrix is accepted, or the
/// two rejections below could be the checker failing on everything.
#[test]
fn the_variant_manifest_accepts_a_list_that_agrees_with_the_matrix() {
    support::checks::variants_match_the_manifest(
        ONE_SOUND_CONTRACT,
        "# a comment and a blank line are skipped\n\nflute-v2-get-ping\tdefault\n",
    );
}

/// **The control for the hole the manifest exists to close.** Deleting a whole
/// variant from `CONTRACTS` leaves every check that reads `CONTRACTS` green,
/// because each of them takes the matrix as the statement of what should
/// exist.
#[test]
fn the_variant_manifest_rejects_a_variant_the_matrix_dropped() {
    assert_rejects("is listed and the matrix does not declare it", || {
        support::checks::variants_match_the_manifest(&[], "flute-v2-get-ping\tdefault\n");
    });
}

/// And the reverse, so the manifest cannot silently fall behind a variant
/// somebody added.
#[test]
fn the_variant_manifest_rejects_a_variant_it_does_not_list() {
    assert_rejects("is declared and not listed", || {
        support::checks::variants_match_the_manifest(ONE_SOUND_CONTRACT, "");
    });
}

#[test]
fn coverage_rejects_an_operation_with_no_contract() {
    assert_rejects("matrix and spec disagree", || {
        support::checks::operation_set_matches_spec(&[]);
    });
}

static VARIANT_CLAIMING_A_BODY: &[Variant] = &[Variant {
    name: "default",
    exchange: || Exchange {
        request: ping_exchange().request,
        response: ResponseFixture {
            status: 200,
            body: Some(json!({"ok": true})),
        },
    },
    mock_test: "the_hash_gate_rejects_an_altered_bundle",
    live: Live::Test("live_ping_succeeds"),
}];

static CONTRACT_CLAIMING_A_BODY: &[Contract] = &[Contract {
    operation_id: "flute-v2-get-ping",
    mapping: Mapping::Command("ping"),
    variants: VARIANT_CLAIMING_A_BODY,
}];

/// Both directions: the spec says `GET /v2/ping` answers 200 with no body, so
/// a fixture claiming one must fail even though the earlier check would only
/// have caught the reverse.
#[test]
fn coverage_rejects_a_body_expectation_the_spec_contradicts() {
    assert_rejects("spec says body=", || {
        support::checks::body_expectations_match_the_spec(CONTRACT_CLAIMING_A_BODY);
    });
}

static VARIANT_WITH_AN_EMPTY_SKIP: &[Variant] = &[Variant {
    name: "default",
    exchange: ping_exchange,
    mock_test: "the_hash_gate_rejects_an_altered_bundle",
    live: Live::Skip(""),
}];

static CONTRACT_WITH_AN_EMPTY_SKIP: &[Contract] = &[Contract {
    operation_id: "flute-v2-get-ping",
    mapping: Mapping::Command("ping"),
    variants: VARIANT_WITH_AN_EMPTY_SKIP,
}];

#[test]
fn coverage_rejects_a_live_skip_with_no_reason() {
    assert_rejects("live skip needs a reason", || {
        support::checks::variants_have_live_coverage(CONTRACT_WITH_AN_EMPTY_SKIP);
    });
}

// ── Surface invariants ───────────────────────────────────────────────────────

/// **The accounting check is what proves a field is reachable at all**, and a
/// nested leaf is where a gap hides: a row for `/billingAddress` would account
/// for the whole address subtree at once. This one reads the real matrix
/// because that is the claim under test — remove any single leaf from it and
/// the sweep must say so.
#[test]
fn surface_rejects_a_nested_field_no_row_accounts_for() {
    const OPERATION: &str = "flute-v2-post-transactions-credit";
    const FIELD: &str = "/billingAddress/city";

    let removed = support::surface::SURFACE
        .iter()
        .find(|f| f.operation_id == OPERATION && f.field == FIELD)
        .expect("the matrix accounts for the field this control removes");
    assert!(
        matches!(removed.exposure, Exposure::Flag(_)),
        "the subject is an exposed nested field, not an exclusion"
    );

    let without: Vec<Field> = support::surface::SURFACE
        .iter()
        .filter(|f| !(f.operation_id == OPERATION && f.field == FIELD))
        .cloned()
        .collect();
    assert_eq!(without.len() + 1, support::surface::SURFACE.len());

    assert_rejects("unaccounted for", || {
        support::checks::every_request_field_is_accounted_for(&without);
    });
}

/// A row naming a field the bundle lacks is stale, and a stale row inflates
/// the coverage claim.
#[test]
fn surface_rejects_a_row_naming_a_field_the_bundle_lacks() {
    static STALE: &[Field] = &[Field {
        operation_id: "flute-v2-post-customers",
        field: "/noSuchFieldAnywhere",
        exposure: Exposure::Flag("--nope"),
    }];
    assert_rejects("is not in the spec", || {
        support::checks::no_surface_row_is_stale(STALE);
    });
}

#[test]
fn surface_rejects_an_exclusion_with_no_reason() {
    static NO_REASON: &[Field] = &[Field {
        operation_id: "flute-v2-post-customers",
        field: "/email",
        exposure: Exposure::Excluded(""),
    }];
    // The accounting check reaches the row only for a *mapped* operation, so
    // this asserts the row shape directly rather than through the spec sweep.
    let row = &NO_REASON[0];
    match &row.exposure {
        Exposure::Excluded(reason) => assert!(reason.is_empty()),
        _ => panic!("fixture is wrong"),
    }
    assert_rejects("needs a reason", || {
        if let Exposure::Excluded(r) = &row.exposure {
            assert!(
                !r.is_empty(),
                "{}/{}: needs a reason",
                row.operation_id,
                row.field
            );
        }
    });
}

// ── Parity invariants ────────────────────────────────────────────────────────

#[test]
fn parity_rejects_a_carried_capability_naming_a_test_that_does_not_exist() {
    static ORPHAN: &[Capability] = &[Capability {
        v1_command: "ping",
        v1_flags: &[],
        parity: Parity::Preserved("ping"),
        test: Some("no_such_parity_test_9a13"),
    }];
    assert_rejects("does not exist", || {
        support::checks::carried_capabilities_name_real_tests(ORPHAN);
    });
}

/// **The completeness control.** An empty matrix must fail against the
/// vendored v1 surface, or the layer proves only that the rows it happens to
/// hold are consistent.
#[test]
fn parity_rejects_a_matrix_missing_a_v1_command() {
    assert_rejects("has no row for it", || {
        support::checks::every_v1_command_has_a_row(&[]);
    });
}

/// One flag dropped from an otherwise complete row is the realistic shape of
/// this regression, and it must fail too.
#[test]
fn parity_rejects_a_row_that_drops_one_of_its_flags() {
    static MISSING_FLAG: &[Capability] = &[Capability {
        v1_command: "customers list",
        v1_flags: &["--limit", "--page"],
        parity: Parity::Removed("a control"),
        test: None,
    }];
    assert_rejects("--search` is unaccounted for", || {
        support::checks::every_v1_flag_is_accounted_for(MISSING_FLAG);
    });
}

#[test]
fn parity_rejects_a_row_naming_a_command_v1_does_not_ship() {
    static INVENTED: &[Capability] = &[Capability {
        v1_command: "transactions teleport",
        v1_flags: &[],
        parity: Parity::Removed("never existed"),
        test: None,
    }];
    assert_rejects("v1 does not ship", || {
        support::checks::no_parity_row_is_stale(INVENTED);
    });
}

/// The leaf walker must descend a **nullable** array, which the relaxation
/// pass rewrites to `type: ["array", "null"]`. An equality test against the
/// string stops descending into every nullable collection in the bundle, and
/// the surface matrix then accounts for the container instead of its leaves —
/// the exact miss it exists to catch.
#[test]
fn leaf_derivation_descends_a_nullable_array() {
    let fields = spec::request_fields_of_mapped_operations();
    let has = |f: &str| {
        fields
            .iter()
            .any(|(o, x)| o == "flute-v2-post-customers" && x == f)
    };
    assert!(
        has("/paymentMethodsCards/[]/cardNumber"),
        "a nullable array's element leaves are missing; the walker stopped at \
         the container. Fields seen: {:?}",
        fields
            .iter()
            .filter(|(o, _)| o == "flute-v2-post-customers")
            .collect::<Vec<_>>()
    );
    assert!(
        !has("/paymentMethodsCards"),
        "the container is a row; a container never accounts for its descendants"
    );
}

/// **The control for the body check.** A fixture declaring no body is the one
/// place a request body goes unjudged, so a command that started sending one
/// would be weighed on its method and path alone.
#[test]
fn the_body_check_rejects_a_body_no_fixture_declares() {
    support::assert_body_matches(b"", None);
    assert_rejects("does not declare", || {
        support::assert_body_matches(br#"{"amount":"1.00"}"#, None);
    });

    let declared = json!({"amount": "1.00"});
    support::assert_body_matches(br#"{"amount":"1.00"}"#, Some(&declared));
    assert_rejects("request body", || {
        support::assert_body_matches(br#"{"amount":"2.00"}"#, Some(&declared));
    });
}

// ── Hermeticity ──────────────────────────────────────────────────────────────

/// `cargo test` stays offline, asserted rather than assumed.
///
/// Three properties make it true, and all three are mechanical: every
/// invocation comes from the isolating helpers, every test under `tests/live`
/// is `#[ignore]`d, and no other test crate clears the base-URL override that
/// pins it to a mock server. A test that spawned the binary itself, a new live
/// test someone forgot to ignore, or a command test that dropped the override
/// would send real traffic from a plain `cargo test` — and would probably
/// pass, which is why review cannot be the check.
#[test]
fn cargo_test_cannot_reach_the_network() {
    let mut checked_live = 0usize;
    let mut checked_other = 0usize;
    let files = test_sources_by_file();

    let unisolated = unisolated_spawn_sites(&files);
    assert!(
        unisolated.is_empty(),
        "these spawn the binary outside the isolating helpers, so each one \
         inherits the developer's credentials, config directory and base \
         URL: {unisolated:?}"
    );

    for (path, text) in files {
        let is_live = path.contains("live");
        if is_live {
            // Every `#[test]` in a live file carries `#[ignore]`. The
            // attributes between `#[test]` and the `fn` are the window to
            // look in — `#[ignore]` follows `#[test]`, it does not precede it.
            for (index, _) in text.match_indices("#[test]") {
                let after = &text[index..];
                let attributes = &after[..after.find("fn ").unwrap_or(after.len())];
                assert!(
                    attributes.contains("#[ignore"),
                    "{path}: a #[test] is not marked #[ignore]; a plain \
                     `cargo test` would send real traffic"
                );
                checked_live += 1;
            }
        } else {
            assert!(
                !text.contains("env_remove(\"FLUTE2_API_BASE_URL\")"),
                "{path}: clears the base-URL override outside tests/live, so \
                 this test can reach the real API"
            );
            checked_other += 1;
        }
    }

    assert!(
        checked_live > 0,
        "no live tests were examined; the scan is looking in the wrong place"
    );
    assert!(checked_other > 0, "no non-live test files were examined");
}

/// The spawn call's own name, split so this file does not read as a site of
/// the thing it is scanning for.
const SPAWN: &str = concat!("cargo_", "bin(");

/// The files allowed to spawn the binary for themselves: the helper module
/// that does the isolating, and the live scenarios a plain `cargo test` skips.
fn may_spawn_the_binary(path: &str) -> bool {
    let path = path.replace('\\', "/");
    path.contains("/live") || path.ends_with("support/mod.rs")
}

/// Every file that builds an invocation the isolating helpers never see.
fn unisolated_spawn_sites(files: &[(String, String)]) -> Vec<String> {
    files
        .iter()
        .filter(|(path, text)| text.contains(SPAWN) && !may_spawn_the_binary(path))
        .map(|(path, _)| path.clone())
        .collect()
}

/// **The control for the spawn guard.** A test that builds its own invocation
/// inherits whatever the developer's shell carries, and the credentials and
/// base URL it carries point at the real API.
///
/// The offending line is assembled here rather than written out, because the
/// guard walks this file too.
#[test]
fn the_spawn_guard_sees_an_invocation_the_helpers_never_built() {
    let line = format!("    assert_cmd::Command::{SPAWN}\"flute2\").unwrap();\n");
    let decoy = vec![("tests/cmd_decoy.rs".to_string(), line.clone())];
    assert_eq!(unisolated_spawn_sites(&decoy), ["tests/cmd_decoy.rs"]);

    let allowed = vec![
        ("tests/support/mod.rs".to_string(), line.clone()),
        ("tests/live.rs".to_string(), line.clone()),
        ("tests/live/transactions.rs".to_string(), line),
    ];
    assert!(unisolated_spawn_sites(&allowed).is_empty());
}

/// Every `.rs` under `tests/`, keyed by path.
fn test_sources_by_file() -> Vec<(String, String)> {
    fn walk(dir: &std::path::Path, out: &mut Vec<(String, String)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    out.push((path.display().to_string(), text));
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(std::path::Path::new("tests"), &mut out);
    assert!(!out.is_empty(), "no test sources found under tests/");
    out
}

// ── Documentation filenames ──────────────────────────────────────────────────

/// **The claim: a capitalised duplicate is rejected.**
///
/// It cannot be produced on a case-insensitive filesystem, so the real test
/// exercises only the missing-file arm there and this arm would otherwise ship
/// unproven — passing on every developer machine and mattering only in CI.
#[test]
fn the_filename_check_rejects_a_capitalised_duplicate() {
    let listing: Vec<String> = ["readme.md", "README.md", "agents.md"]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    assert_rejects("also spelled", || {
        support::checks::documentation_filenames_are_lowercase(&listing, &["readme.md"]);
    });
}

/// And it must still accept the real listing, or the control above proves
/// nothing about the check that runs.
#[test]
fn the_filename_check_accepts_a_correct_listing() {
    let listing: Vec<String> = ["readme.md", "agents.md", "Cargo.toml"]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    support::checks::documentation_filenames_are_lowercase(&listing, &["readme.md", "agents.md"]);
}

// ── A divergence's evidence ──────────────────────────────────────────────────

/// The real list must be accepted, or the control below proves nothing.
#[test]
fn the_divergence_evidence_check_accepts_the_real_list() {
    support::checks::divergences_name_real_tests(spec::DIVERGENCES);
}

/// **The claim: an exemption with no oracle is rejected.** A divergence
/// suspends a declared constraint and the live scenario it names is the only
/// thing standing behind it, so a pointer at a scenario nobody wrote has to
/// fail rather than read as coverage.
#[test]
fn the_divergence_evidence_check_rejects_a_scenario_nobody_wrote() {
    let orphan = &[spec::Divergence {
        name: "a rule that names nothing real",
        rule: spec::Rule::RequestField {
            operations: &["flute-v2-post-customers"],
            pointer: "/nothing",
        },
        evidence: "live_scenario_that_was_never_written",
        removal: "never",
    }];
    assert_rejects("does not exist", || {
        support::checks::divergences_name_real_tests(orphan);
    });
}

/// And a divergence with no removal condition is a permanent exemption
/// wearing a temporary one's clothes.
#[test]
fn the_divergence_evidence_check_rejects_a_missing_removal_condition() {
    let forever = &[spec::Divergence {
        name: "a rule that names nothing real",
        rule: spec::Rule::RequestField {
            operations: &["flute-v2-post-customers"],
            pointer: "/nothing",
        },
        evidence: "live_ping_succeeds",
        removal: "",
    }];
    assert_rejects("under which it is deleted", || {
        support::checks::divergences_name_real_tests(forever);
    });
}

// ── Relaxing one property's minimum, and only that ──────────────────────────

fn payment_session(body: serde_json::Value) -> (RequestFixture, ResponseFixture) {
    (
        RequestFixture {
            method: "POST".into(),
            path: "/v2/payment-sessions".into(),
            path_params: vec![],
            query: vec![],
            content_type: None,
            body: Some(body),
        },
        ResponseFixture {
            status: 200,
            body: Some(json!({"id": "3fa85f64-5717-4562-b3fc-2c963f66afa6"})),
        },
    )
}

fn assert_payment_session(body: serde_json::Value) {
    let (req, resp) = payment_session(body);
    spec::assert_exchange_conforms("flute-v2-post-payment-sessions", &req, &resp);
}

/// The exempt value must pass, or the controls below prove nothing.
/// `amount` declares `minimum: 0.01` and its own description says a
/// `SaveMethod` session's amount must be **zero**.
#[test]
fn conformance_accepts_the_documented_zero_amount_on_a_vault_only_session() {
    assert_payment_session(json!({
        "mode": "SaveMethod",
        "amount": amount("0"),
        "customerHandling": "CreateCustomer"}));
}

/// **The relaxation is one property of one operation.** A minimum declared
/// elsewhere still holds — the relaxed schema is copied rather than mutated,
/// and `RELAXED` is shared by every operation.
#[test]
fn conformance_still_enforces_a_minimum_on_another_operation() {
    let req = RequestFixture {
        method: "POST".into(),
        path: "/v2/pos/transactions".into(),
        path_params: vec![],
        query: vec![],
        content_type: None,
        body: Some(json!({
            "terminalId": "b1d4c0a7-3e18-4a6f-8c25-9f7e6d5b4a31",
            "posDeviceId": "POS-DEVICE-001",
            "baseAmount": amount("42.75"),
            "currencyCode": "USD",
            "extraAmounts": {"tipAmount": amount("0")}})),
    };
    let resp = ResponseFixture {
        status: 200,
        body: Some(json!({
            "posTransactionId": "7c9a3d1e-5b42-4f80-9a1c-6d3e2f8b4a05",
            "posTransactionStatus": "InProgress"})),
    };
    assert_rejects("does not conform", || {
        spec::assert_exchange_conforms("flute-v2-post-pos-transactions", &req, &resp);
    });
}

/// And the type still holds on the relaxed property: an amount is a number,
/// and dropping a bound is not dropping the schema.
#[test]
fn conformance_still_type_checks_the_property_whose_minimum_is_relaxed() {
    assert_rejects("does not conform", || {
        assert_payment_session(json!({"mode": "SaveMethod", "amount": "0"}));
    });
}

/// `additionalProperties: false` is untouched, so an undeclared key is still
/// rejected on the same operation.
#[test]
fn conformance_still_rejects_an_undeclared_key_on_a_payment_session() {
    assert_rejects("does not conform", || {
        assert_payment_session(json!({"amount": amount("0"), "notAField": 1}));
    });
}

// ── Relaxing one property's pattern, and only that ──────────────────────────

fn share_receipt(body: serde_json::Value) -> (RequestFixture, ResponseFixture) {
    (
        RequestFixture {
            method: "POST".into(),
            path: "/v2/transactions/txn_1/share-receipt".into(),
            path_params: vec![("transactionId".into(), "txn_1".into())],
            query: vec![],
            content_type: None,
            body: Some(body),
        },
        ResponseFixture {
            status: 200,
            body: None,
        },
    )
}

fn assert_share_receipt(body: serde_json::Value) {
    let (req, resp) = share_receipt(body);
    spec::assert_exchange_conforms(
        "flute-v2-post-transactions-transactionId-share-receipt",
        &req,
        &resp,
    );
}

/// The exempt value must pass, or the controls below prove nothing.
/// `shareBy` is described as `Email`, `None` or `Sms`, and declares a phone
/// number's pattern.
#[test]
fn conformance_accepts_the_documented_share_by_value() {
    assert_share_receipt(json!({
        "shareBy": "Sms",
        "recipient": "+14155552309",
        "hasCustomerConsent": true}));
}

/// **The relaxation is one property's pattern, not the body's.** `recipient`
/// carries its own E.164 pattern and must still be enforced.
#[test]
fn conformance_still_enforces_the_neighbouring_pattern() {
    assert_rejects("does not conform", || {
        assert_share_receipt(json!({
            "shareBy": "Sms",
            "recipient": "ada@example.com",
            "hasCustomerConsent": true}));
    });
}

/// It relaxes a constraint, not the requirement. A pointer strip could not
/// express this case, which is why the rule exists: dropping `shareBy` would
/// fail as a missing required property rather than pass.
#[test]
fn conformance_still_requires_the_relaxed_property() {
    assert_rejects("does not conform", || {
        assert_share_receipt(json!({
            "recipient": "+14155552309",
            "hasCustomerConsent": true}));
    });
}

/// And the type still holds: a number is not a share-by method.
#[test]
fn conformance_still_type_checks_the_relaxed_property() {
    assert_rejects("does not conform", || {
        assert_share_receipt(json!({
            "shareBy": 42,
            "recipient": "+14155552309",
            "hasCustomerConsent": true}));
    });
}

/// `additionalProperties: false` is untouched, so the operation's own request
/// example — which sends `mobilePhoneNumber` and omits both required fields —
/// is still rejected.
#[test]
fn conformance_rejects_the_share_receipt_operations_own_request_example() {
    assert_rejects("does not conform", || {
        assert_share_receipt(json!({
            "mobilePhoneNumber": "+15551234567",
            "hasCustomerConsent": true}));
    });
}
