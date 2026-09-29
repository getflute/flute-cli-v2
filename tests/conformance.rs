mod support;

/// The vendored bundle is the oracle three layers validate against. Editing it
/// to make a test pass would be indistinguishable from fixing the code, so the
/// file is pinned to a hash recorded beside it.
///
/// This proves only that the vendored file has not been edited locally.
/// Detecting that the *published* bundle has moved needs a fetch, which
/// `cargo test` must not do.
#[test]
fn vendored_bundle_matches_its_recorded_hash() {
    support::spec::assert_bundle_hash(
        include_bytes!("../docs/reference/openapi-v2.json"),
        include_str!("../docs/reference/openapi-v2.sha256").trim(),
    );
}

/// The bundle must hold exactly the operation set the design counted.
#[test]
fn the_bundle_declares_fifty_non_webhook_operations() {
    assert_eq!(support::spec::non_webhook_routes().len(), 50);
}

/// The generated fact sheet and the surface walker must mean the same thing by
/// "leaf".
///
/// The sheet is planning input and the walker is the test oracle. Nothing else
/// connects them, so they can drift silently — and a sheet that under-counts
/// would send a group task off to write the wrong number of surface rows,
/// which the walker would then reject for reasons the author cannot see.
#[test]
fn the_generated_facts_agree_with_the_surface_walker() {
    #[derive(serde::Deserialize)]
    struct Facts {
        bundle_sha256: String,
        operation_count: usize,
        operations: Vec<Op>,
    }
    #[derive(serde::Deserialize)]
    struct Op {
        method: String,
        path: String,
        leaves: Vec<String>,
    }

    let facts: Facts = serde_json::from_str(include_str!("../docs/reference/group-facts.json"))
        .expect("group-facts.json is not valid JSON; regenerate it");

    // Generated from this bundle, not an older one.
    support::spec::assert_bundle_hash(
        include_bytes!("../docs/reference/openapi-v2.json"),
        &facts.bundle_sha256,
    );
    let count = support::spec::non_webhook_routes().len();
    assert_eq!(facts.operation_count, count);
    assert_eq!(facts.operations.len(), count);

    for op in &facts.operations {
        let route = format!("{} {}", op.method.to_ascii_uppercase(), op.path);
        let label = support::contracts::CONTRACTS
            .iter()
            .find(|c| c.route == route)
            .unwrap_or_else(|| panic!("{route}: in the fact sheet, with no contract row"))
            .operation_id;
        let mut walker = support::spec::request_leaves(label);
        walker.sort();
        let mut sheet = op.leaves.clone();
        sheet.sort();
        assert_eq!(
            sheet, walker,
            "{}: the fact sheet and the surface walker disagree on the leaf set. \
             Regenerate with `python3 docs/reference/group-facts.py`, and if they \
             still differ, one of the two walks is wrong.",
            route
        );
    }
}
