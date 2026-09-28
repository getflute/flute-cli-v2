mod support;

use support::checks;
use support::contracts::CONTRACTS;

#[test]
fn every_non_webhook_operation_has_exactly_one_contract() {
    checks::operation_set_matches_spec(CONTRACTS);
}

#[test]
fn every_variant_conforms_to_its_own_operation() {
    checks::variants_conform(CONTRACTS);
}

#[test]
fn every_variant_has_live_coverage_or_a_reason() {
    checks::variants_have_live_coverage(CONTRACTS, &live_sources());
}

#[test]
fn body_expectations_match_the_spec_in_both_directions() {
    checks::body_expectations_match_the_spec(CONTRACTS);
}

#[test]
fn every_built_write_and_bodyless_success_is_covered() {
    checks::built_writes_and_bodyless_successes_are_covered(CONTRACTS);
}

/// The live suite's setup instructions and the values it actually reads must
/// not drift apart.
#[test]
fn every_live_variable_is_documented_and_every_documented_one_is_read() {
    checks::live_variables_are_documented(&live_sources(), &live_template());
}

/// The scenario sources, read at runtime: the package root is the working
/// directory for a test binary.
fn live_sources() -> String {
    let mut buf = std::fs::read_to_string("tests/live.rs").expect("tests/live.rs");
    for entry in std::fs::read_dir("tests/live")
        .expect("tests/live/")
        .flatten()
    {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "rs") {
            buf.push_str(&std::fs::read_to_string(&path).expect("a live scenario file"));
        }
    }
    buf
}

fn live_template() -> String {
    std::fs::read_to_string(".flute2-live.env.example").expect(".flute2-live.env.example")
}

/// A divergence suspends a declared constraint, and the live scenario it
/// names is the only thing behind it.
#[test]
fn every_divergence_names_a_live_test_that_exists() {
    checks::divergences_name_real_tests(support::spec::DIVERGENCES, &live_sources());
}
