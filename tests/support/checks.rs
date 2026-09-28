//! The invariant bodies, each taking its input.
//!
//! `tests/coverage.rs` and `tests/surface.rs` are separate crates, so a
//! function defined in one is not importable from another and `mod support;`
//! compiles a fresh copy into each. Anything that has to be
//! called twice — notably the negative controls that verify these checkers
//! fail when they should — has to live here.

use super::contracts::{Contract, Live, Mapping};
use super::spec::{self, assert_exchange_conforms};

/// Every non-webhook operation has exactly one contract row. Adding one
/// upstream fails the build until somebody decides what it means for the CLI.
pub fn operation_set_matches_spec(contracts: &[Contract]) {
    let mut spec_ids = spec::non_webhook_operation_ids();
    spec_ids.sort();
    assert_eq!(spec_ids.len(), 50, "50 non-webhook operations expected");

    let mut accounted: Vec<String> = contracts
        .iter()
        .map(|c| c.operation_id.to_string())
        .collect();
    // A duplicate row leaves the sorted lists different lengths, so the one
    // comparison catches a missing row and a doubled one.
    accounted.sort();
    assert_eq!(
        spec_ids, accounted,
        "matrix and spec disagree on the operation set"
    );
}

/// **The invariant that makes the matrix mean something.** Every variant's
/// fixture is validated against its own operation: method, rendered path, path
/// parameters, required and undeclared query parameters, content type, request
/// body, declared status, empty-versus-JSON, and response body.
pub fn variants_conform(contracts: &[Contract]) {
    for c in contracts {
        if let Mapping::Command(cmd) = &c.mapping {
            assert!(
                !c.variants.is_empty(),
                "{} ({cmd}): no variants",
                c.operation_id
            );
        }
        for v in c.variants {
            let ex = (v.exchange)();
            assert_exchange_conforms(c.operation_id, &ex.request, &ex.response);
        }
    }
}

/// Live coverage is opt-out with a stated reason, never silent. `sources` is
/// the text of the live scenarios.
pub fn variants_have_live_coverage(contracts: &[Contract], sources: &str) {
    for c in contracts {
        for v in c.variants {
            match &v.live {
                Live::Test(name) => assert!(
                    // ponytail: a mention in a comment also passes
                    sources.contains(&format!("fn {name}(")),
                    "{} / {}: live test `{}` does not exist",
                    c.operation_id,
                    v.name,
                    name
                ),
                Live::Skip(reason) => assert!(
                    !reason.is_empty(),
                    "{} / {}: live skip needs a reason",
                    c.operation_id,
                    v.name
                ),
            }
        }
    }
}

/// Both directions. An operation that returns JSON must have at least one
/// variant carrying a body, and one that returns none must have none — a
/// one-way check would accept a fixture claiming a bodyless success for an
/// endpoint that answers with a resource.
pub fn body_expectations_match_the_spec(contracts: &[Contract]) {
    let bodyless = spec::bodyless_successes();
    for c in contracts {
        for v in c.variants {
            let ex = (v.exchange)();
            let spec_says_empty = bodyless
                .iter()
                .any(|(id, st)| id == c.operation_id && *st == ex.response.status);
            assert_eq!(
                ex.response.body.is_none(),
                spec_says_empty,
                "{} / {}: fixture says body={}, spec says body={} for status {}",
                c.operation_id,
                v.name,
                ex.response.body.is_some(),
                !spec_says_empty,
                ex.response.status
            );
        }
    }
}

/// Both sets are derived, never typed by hand. A hard-coded count is how
/// "every write endpoint -- nineteen" came to omit eleven of them.
pub fn built_writes_and_bodyless_successes_are_covered(contracts: &[Contract]) {
    for op_id in spec::write_operation_ids() {
        let c = contracts
            .iter()
            .find(|c| c.operation_id == op_id)
            .unwrap_or_else(|| panic!("{op_id} is a write with no contract row"));
        let Mapping::Command(_) = &c.mapping else {
            continue;
        };
        assert!(
            c.variants
                .iter()
                .any(|v| matches!(v.live, Live::Test(_) | Live::Skip(_))),
            "{op_id} is a write and needs live coverage or an explicit Skip reason"
        );
    }
    for (op_id, status) in spec::bodyless_successes() {
        let mapped = contracts
            .iter()
            .any(|c| c.operation_id == op_id && matches!(c.mapping, Mapping::Command(_)));
        if !mapped {
            continue;
        }
        assert!(
            contracts.iter().any(|c| c.operation_id == op_id
                && c.variants.iter().any(|v| {
                    let ex = (v.exchange)();
                    ex.response.status == status && ex.response.body.is_none()
                })),
            "{op_id} answers {status} with no body; no variant exercises that"
        );
    }
}

/// A divergence's `evidence` must name a live scenario in `sources`, and it
/// must state the condition under which it is deleted. A divergence suspends a
/// declared constraint, and the scenario it names is the only thing behind it.
pub fn divergences_name_real_tests(divergences: &[spec::Divergence], sources: &str) {
    for d in divergences {
        assert!(
            // ponytail: a mention in a comment also passes
            sources.contains(&format!("fn {}(", d.evidence)),
            "{}: evidence `{}` does not exist, so the exemption has no oracle",
            d.name,
            d.evidence
        );
        assert!(
            !d.removal.is_empty(),
            "{}: a divergence needs a condition under which it is deleted",
            d.name
        );
    }
}

/// Every `FLUTE2_LIVE_*` variable the scenarios read is in the template, and
/// every one in the template is read.
///
/// Both directions, because they fail differently. A variable a scenario reads
/// and the template omits is a value nobody knows to supply: the scenario
/// panics naming a name that appears nowhere in the setup instructions. A
/// variable in the template that nothing reads is worse in a quieter way — it
/// is a value somebody looked up, pasted, and got no use from, and it makes
/// the setup look longer than it is.
///
/// The scan is a literal prefix search rather than a pattern: the prefix is
/// fixed, the characters after it are an obvious set, and a regex here would
/// be a harder thing to read for no gain.
pub fn live_variables_are_documented(sources: &str, template: &str) {
    let read = live_variables_in(sources);
    let documented = live_variables_in(template);

    for var in &read {
        assert!(
            documented.contains(var),
            "{var} is read by a live scenario and is not in \
             .flute2-live.env.example, so nobody running the suite is told to \
             set it"
        );
    }
    for var in &documented {
        assert!(
            read.contains(var),
            "{var} is in .flute2-live.env.example and no live scenario reads \
             it, so it is setup work that buys nothing"
        );
    }
}

/// Every distinct `FLUTE2_LIVE_*` name in `text`.
fn live_variables_in(text: &str) -> std::collections::BTreeSet<String> {
    const PREFIX: &str = "FLUTE2_LIVE_";
    let mut out = std::collections::BTreeSet::new();
    let mut rest = text;
    while let Some(at) = rest.find(PREFIX) {
        let tail = &rest[at + PREFIX.len()..];
        let end = tail
            .find(|c: char| !c.is_ascii_uppercase() && !c.is_ascii_digit() && c != '_')
            .unwrap_or(tail.len());
        if end > 0 {
            out.insert(format!("{PREFIX}{}", &tail[..end]));
        }
        rest = &tail[end..];
    }
    out
}

// ── Layer 4a: API surface ────────────────────────────────────────────────────

use super::surface::{Exposure, Field};

pub fn every_request_field_is_accounted_for(surface: &[Field]) {
    for (op_id, field) in spec::request_fields_of_mapped_operations() {
        let row = surface
            .iter()
            .find(|f| f.operation_id == op_id && f.field == field)
            .unwrap_or_else(|| {
                panic!(
                    "{op_id}: {field} is in the spec and unaccounted for. Add a \
                     flag, fix it deliberately, or exclude it with a reason."
                )
            });
        match &row.exposure {
            Exposure::Flag(f) => assert!(f.starts_with("--"), "{op_id}/{field}: not a flag"),
            Exposure::Fixed(v) => assert!(!v.is_empty(), "{op_id}/{field}: fixed to nothing"),
            Exposure::Excluded(r) => assert!(!r.is_empty(), "{op_id}/{field}: needs a reason"),
        }
    }
}

pub fn no_surface_row_is_stale(surface: &[Field]) {
    let known = spec::request_fields_of_mapped_operations();
    for f in surface {
        assert!(
            known
                .iter()
                .any(|(o, x)| o == f.operation_id && x == f.field),
            "{}: {} is not in the spec",
            f.operation_id,
            f.field
        );
    }
}

pub fn exposed_flags_appear_in_help(surface: &[Field]) {
    for f in surface {
        if let Exposure::Flag(flags) = &f.exposure {
            let help = super::help_for_operation(f.operation_id);
            for flag in flags.split('/') {
                assert!(
                    help.contains(flag),
                    "{}: {flag} is claimed but absent from --help",
                    f.operation_id
                );
            }
        }
    }
}
