//! Layer 7: golden files over the settled command tree and the settled
//! renderer.
//!
//! Deferred until the tree stopped growing. A snapshot taken while eleven
//! groups were still arriving would have been re-baselined on almost every
//! commit, and a snapshot that is routinely re-baselined catches nothing — the
//! one diff that mattered would arrive among thirty that did not.
//!
//! Nothing here is a *new* assertion about behaviour. Every claim these files
//! carry is already made by an ordinary test somewhere; what they add is that
//! a change to any of it has to be looked at and accepted by a person.
//!
//! **Both halves enumerate their own subject.** The command paths come from
//! clap's own tree and the renderer's inputs from the contract matrix, because
//! a hand-written list of either would omit exactly the newly added thing this
//! layer exists to notice.
//!
//! **The two shared tables are the subject**, not every table the CLI can
//! print. `transactions inspect` and `api-keys create` compose their own from
//! the same machinery, and each is asserted by its group's command test
//! against a response written for it — a fixture from the contract matrix
//! carries none of the fields that distinguish those two views, so a golden
//! file here could only ever copy the one above it.

mod support;

use clap::CommandFactory;
use flute_cli2::cli::render::{self, Resource};

/// `--help` for one path through the tree.
///
/// It goes through the compiled binary rather than `Command::render_help`, so
/// what is snapshotted is what a user sees. Help must succeed and must be on
/// stdout; a leaf that started failing would otherwise snapshot as an empty
/// string and look like a deliberate change.
///
/// **The env-backed globals are cleared, and that is load-bearing.** clap
/// prints an `env`-backed argument's *current* value into `--help`, so
/// `--profile` renders as `[env: FLUTE2_PROFILE=sandbox]` for anyone whose
/// environment sets it and as `[env: FLUTE2_PROFILE=]` for anyone whose does
/// not. A golden file cannot depend on the developer running it.
fn help_for(args: &[&str]) -> String {
    let out = support::bin_without_credentials()
        .env_remove("FLUTE2_PROFILE")
        .env_remove("FLUTE2_OUTPUT")
        .args(args)
        .arg("--help")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "`flute2 {} --help` exited {:?}: {}",
        args.join(" "),
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

/// Every path through the tree, root first, as `["customers", "create"]`.
fn every_command_path() -> Vec<Vec<String>> {
    fn walk(cmd: &clap::Command, prefix: Vec<String>, out: &mut Vec<Vec<String>>) {
        out.push(prefix.clone());
        for sub in cmd.get_subcommands() {
            // clap's own generated `help` subcommand is not part of the
            // surface this repository owns.
            if sub.get_name() == "help" {
                continue;
            }
            let mut next = prefix.clone();
            next.push(sub.get_name().to_string());
            walk(sub, next, out);
        }
    }
    let mut out = Vec::new();
    walk(&flute_cli2::cli::Cli::command(), vec![], &mut out);
    out
}

/// **Every leaf, not only root and group.** A flag-heavy leaf is where a help
/// regression actually hurts, and it is the level no earlier layer snapshots:
/// `surface` checks that a flag exists, and nothing checks what it says.
#[test]
fn every_command_help_is_stable() {
    let paths = every_command_path();
    assert!(
        paths.len() > 40,
        "the tree walk found only {} paths, so it is not walking the tree",
        paths.len()
    );
    for path in paths {
        let args: Vec<&str> = path.iter().map(String::as_str).collect();
        let name = if args.is_empty() {
            "root".to_string()
        } else {
            args.join("_")
        };
        insta::assert_snapshot!(name, help_for(&args));
    }
}

/// Pagination help must state that the index is zero-based, because the
/// obvious reading of "page" is one-based and would be off by one.
///
/// Over **every** command that offers the flag, not one of them. Eleven groups
/// flatten the same `PaginationArgs`, and a command reached by a different
/// route — `api-keys list` deliberately offers no pagination at all — is
/// exactly where an inconsistency would sit.
#[test]
fn every_paginated_help_states_the_index_is_zero_based() {
    let mut checked = 0;
    for path in every_command_path() {
        let args: Vec<&str> = path.iter().map(String::as_str).collect();
        let help = help_for(&args);
        if !help.contains("--page-index") {
            continue;
        }
        assert!(
            help.contains("Zero-based"),
            "`flute2 {} --help` offers --page-index and does not say it is \
             zero-based",
            args.join(" ")
        );
        checked += 1;
    }
    assert!(checked > 5, "only {checked} paginated commands were found");
}

/// **No `--help` may explain the implementation to a user.**
///
/// A `///` on a clap type is user-facing text, and the difference between a
/// doc comment and an ordinary one is invisible while reading the struct. The
/// root's long help opened with a justification for an
/// `#[allow(clippy::large_enum_variant)]`, because `Cli` declared no
/// `long_about` and clap fell back to the subcommand enum's doc comment —
/// so `flute2 --help` explained `#[derive(Subcommand)]` to whoever ran it
/// while `flute2 -h` was correct.
///
/// The whole tree is walked rather than the root alone: every group enum
/// carries the same `#[allow]` and the same shape of doc comment, and today
/// they are shadowed by their variant's own text. A variant losing its doc
/// comment would put the rationale on that group's help instead.
#[test]
fn no_help_text_explains_the_implementation() {
    // Vocabulary that cannot appear in prose meant for a caller: Rust
    // attributes, lint names, the derive machinery, the bundle's own
    // vocabulary, and the reasoning behind how an argument is declared.
    //
    // The predecessor CLI is matched as ` v1` rather than `v1` so that a
    // version string or a path segment is not a false positive.
    const INTERNAL: [&str; 15] = [
        "clippy",
        "clap",
        "#[derive",
        "#[allow",
        "enum_variant",
        "Subcommand",
        "default_value",
        "no-op in v1",
        " v1",
        "Option<",
        "sortOrder",
        "additionalProperties",
        "schema",
        "merge-patch",
        "wire name",
    ];
    for path in every_command_path() {
        let args: Vec<&str> = path.iter().map(String::as_str).collect();
        let help = help_for(&args);
        for needle in INTERNAL {
            assert!(
                !help.contains(needle),
                "`flute2 {} --help` contains {needle:?}, which is \
                 implementation vocabulary a caller has no use for",
                args.join(" ")
            );
        }
    }
}

/// A command carried over from the predecessor CLI keeps its wording.
///
/// Two commands stand for the whole tree: one flag-bearing write and one
/// positional read. They are the cheapest possible guard against the help
/// being paraphrased back into its own voice, which is what this repository's
/// `--help` had drifted into.
#[test]
fn carried_over_commands_keep_their_original_one_liners() {
    for (args, one_liner) in [
        (
            ["transactions", "capture"],
            "Capture a previously authorised transaction \
             (POST /v2/transactions/{transactionId}/capture)",
        ),
        (
            ["customers", "get"],
            "Fetch a single customer by ID (GET /v2/customers/{customerId})",
        ),
    ] {
        let help = help_for(&args);
        assert!(
            help.contains(one_liner),
            "`flute2 {} --help` does not carry {one_liner:?}:\n{help}",
            args.join(" ")
        );
    }
}

/// Dropped v1 commands must fail as unrecognised, never silently succeed.
///
/// The renames are an enumerated v2 change and there are no aliases, so a
/// script written against v1 has to break loudly rather than charge something
/// else. `devices` and `subscriptions` have no v2 endpoints at all.
#[test]
fn removed_v1_commands_are_not_accepted() {
    for args in [
        vec!["transactions", "sale"],
        vec!["transactions", "void"],
        vec!["transactions", "refund"],
        vec!["transactions", "settle"],
        vec!["ach", "debit"],
        vec!["ach", "credit"],
        vec!["keys", "list"],
        vec!["devices", "list"],
        vec!["subscriptions", "list"],
    ] {
        support::bin_without_credentials()
            .args(&args)
            .assert()
            .code(3);
    }
}

/// One conformance-validated response object per resource, and where inside
/// the response body it sits.
///
/// The object comes from the contract matrix rather than being written here:
/// a fixture the bundle has already validated is a better input than invented
/// JSON, and it cannot drift from what the command tests mount.
struct Rendered {
    resource: &'static Resource,
    /// The declaration's own identifier. It is the join key for
    /// `every_declared_resource_is_snapshotted` and, lowercased, the snapshot
    /// file name.
    ident: &'static str,
    operation_id: &'static str,
    variant: &'static str,
    /// A pointer to the object inside the response body — empty for a
    /// response that *is* the object.
    at: &'static str,
}

/// Every resource the CLI declares, with something real to render.
///
/// Each is fed the **richest** conformance-validated object the matrix holds
/// for it, which is not always the `get`: a `customers get` fixture carries
/// four leaves and no `billingAddress`, so it would snapshot almost none of
/// the ordering the descriptor declares, while a list item carries the nested
/// block. A thin fixture makes a golden file that agrees with itself.
///
/// Every resource gets a list rendering too, singletons included: a descriptor
/// declares its columns whether or not an endpoint returns a collection today,
/// and an unexercised column is where a wrong header or width survives.
/// Four render empty, and deliberately. `AMOUNT_CALCULATION`'s response is a
/// quote per instrument with no single total, so there is no honest column to
/// put one in; the three write descriptors describe what a write answers
/// with, and no write answers with a collection. All four are POSTs returning
/// one object, with no collection endpoint to need a column.
const RENDERED: &[Rendered] = &[
    Rendered {
        resource: &flute_cli2::groups::customers::CUSTOMER,
        ident: "CUSTOMER",
        operation_id: "flute-v2-get-customers",
        variant: "first page, server defaults",
        at: "/items/0",
    },
    Rendered {
        resource: &flute_cli2::groups::payment_methods::PAYMENT_METHOD,
        ident: "PAYMENT_METHOD",
        operation_id: "flute-v2-get-payment-methods-paymentMethodId",
        variant: "card",
        at: "",
    },
    Rendered {
        resource: &flute_cli2::groups::transactions::TRANSACTION,
        ident: "TRANSACTION",
        operation_id: "flute-v2-get-transactions",
        variant: "first page, server defaults",
        at: "/items/0",
    },
    Rendered {
        resource: &flute_cli2::groups::transactions::TRANSACTION_WRITE,
        ident: "TRANSACTION_WRITE",
        operation_id: "flute-v2-post-transactions",
        variant: "new card, automatic capture",
        at: "",
    },
    Rendered {
        resource: &flute_cli2::groups::transactions::TRANSACTION_WRITE_SHORT,
        ident: "TRANSACTION_WRITE_SHORT",
        operation_id: "flute-v2-post-transactions-transactionId-reversal",
        variant: "full",
        at: "",
    },
    Rendered {
        resource: &flute_cli2::groups::transactions::TRANSACTION_ACH_ACTION,
        ident: "TRANSACTION_ACH_ACTION",
        operation_id: "flute-v2-post-transactions-transactionId-ach-hold",
        variant: "default",
        at: "",
    },
    Rendered {
        resource: &flute_cli2::groups::transactions::AMOUNT_CALCULATION,
        ident: "AMOUNT_CALCULATION",
        operation_id: "flute-v2-post-transactions-calculate-amount",
        variant: "default",
        at: "",
    },
    Rendered {
        resource: &flute_cli2::groups::pos::POS_TRANSACTION,
        ident: "POS_TRANSACTION",
        operation_id: "flute-v2-get-pos-transactions-posTransactionId",
        variant: "default",
        at: "",
    },
    Rendered {
        resource: &flute_cli2::groups::terminals::TERMINAL,
        ident: "TERMINAL",
        operation_id: "flute-v2-get-terminals",
        variant: "first page, server defaults",
        at: "/items/0",
    },
    Rendered {
        resource: &flute_cli2::groups::terminals::TERMINAL_STATUS,
        ident: "TERMINAL_STATUS",
        operation_id: "flute-v2-get-terminals-terminalId-status",
        variant: "default",
        at: "",
    },
    Rendered {
        resource: &flute_cli2::groups::settlements::SETTLEMENT,
        ident: "SETTLEMENT",
        operation_id: "flute-v2-get-settlements-batches",
        variant: "first page, server defaults",
        at: "/items/0",
    },
    Rendered {
        resource: &flute_cli2::groups::settlements::BATCH_CLOSURE,
        ident: "BATCH_CLOSURE",
        operation_id: "flute-v2-post-settlements-batches-close",
        variant: "default",
        at: "",
    },
    Rendered {
        resource: &flute_cli2::groups::settings::PAYMENT_CONFIG,
        ident: "PAYMENT_CONFIG",
        operation_id: "flute-v2-get-settings-payment-config",
        variant: "default",
        at: "",
    },
    Rendered {
        resource: &flute_cli2::groups::settings::CONTACT_INFO,
        ident: "CONTACT_INFO",
        operation_id: "flute-v2-get-settings-contact-information",
        variant: "default",
        at: "",
    },
    Rendered {
        resource: &flute_cli2::groups::settings::TRANSACTION_AUTOFILL,
        ident: "TRANSACTION_AUTOFILL",
        operation_id: "flute-v2-get-settings-transaction-autofill",
        variant: "default",
        at: "",
    },
    Rendered {
        resource: &flute_cli2::groups::payment_links::PAYMENT_LINK,
        ident: "PAYMENT_LINK",
        operation_id: "flute-v2-get-payment-links-paymentLinkId",
        variant: "default",
        at: "",
    },
    Rendered {
        resource: &flute_cli2::groups::payment_sessions::PAYMENT_SESSION,
        ident: "PAYMENT_SESSION",
        operation_id: "flute-v2-get-payment-sessions-paymentSessionId",
        variant: "default",
        at: "",
    },
    Rendered {
        resource: &flute_cli2::groups::api_keys::API_KEY,
        ident: "API_KEY",
        operation_id: "flute-v2-get-api-keys",
        variant: "every key",
        at: "/apiKeys/0",
    },
];

fn object_for(r: &Rendered) -> serde_json::Value {
    let body = support::contracts::exchange(r.operation_id, r.variant)
        .response
        .body
        .unwrap_or_else(|| panic!("{} / {} declares no body", r.operation_id, r.variant));
    if r.at.is_empty() {
        body
    } else {
        body.pointer(r.at)
            .unwrap_or_else(|| panic!("{} has nothing at {}", r.operation_id, r.at))
            .clone()
    }
}

#[test]
fn every_resource_detail_rendering_is_stable() {
    for r in RENDERED {
        insta::assert_snapshot!(
            format!("detail_{}", r.ident.to_lowercase()),
            render::detail_table(r.resource, &object_for(r))
        );
    }
}

#[test]
fn every_resource_list_rendering_is_stable() {
    for r in RENDERED {
        insta::assert_snapshot!(
            format!("list_{}", r.ident.to_lowercase()),
            render::list_table(r.resource, &[object_for(r)])
        );
    }
}

/// **A new resource cannot escape the two snapshots above.**
///
/// `RENDERED` is a hand-written list, and the thing a hand-written list omits
/// is whatever arrived last. The declarations are the oracle: every
/// `pub static …: Resource` under `src/groups/` must be named here, so adding
/// a twelfth group's resource fails this test rather than quietly rendering
/// unsnapshotted.
#[test]
fn every_declared_resource_is_snapshotted() {
    let mut declared: Vec<String> = Vec::new();
    for (group, path) in support::group_sources() {
        for line in std::fs::read_to_string(&path).unwrap().lines() {
            let Some(rest) = line.strip_prefix("pub static ") else {
                continue;
            };
            if !rest.contains(": Resource") {
                continue;
            }
            let ident = rest.split(':').next().unwrap();
            declared.push(format!("{group}::{ident}"));
        }
    }
    assert!(
        declared.len() > 10,
        "the scan found only {} resource declarations, so it is not scanning",
        declared.len()
    );

    let missing: Vec<&String> = declared
        .iter()
        .filter(|d| {
            let ident = d.split("::").nth(1).unwrap();
            !RENDERED.iter().any(|r| r.ident == ident)
        })
        .collect();
    assert!(
        missing.is_empty(),
        "{} resource(s) are declared and never snapshotted: {:?}",
        missing.len(),
        missing
    );
}

/// Every pointer one descriptor declares, whatever slot it sits in.
///
/// A `Derived` column computes its text from whatever the object holds, so
/// there is no pointer to bind.
fn declared_pointers(resource: &Resource) -> Vec<&'static str> {
    let mut out = vec![resource.id];
    for slot in [resource.detail, resource.amounts, resource.yes_no] {
        out.extend(slot.iter().copied());
    }
    out.extend(resource.columns.iter().filter_map(|c| match c.cell {
        render::Cell::Path(p) => Some(p),
        render::Cell::Derived(_) => None,
    }));
    out.sort();
    out.dedup();
    out
}

/// A pointer in the spelling the bundle's leaves use, so an index step and the
/// matrix's `/[]` are the same thing.
fn without_indices(pointer: &str) -> String {
    pointer
        .split('/')
        .map(|s| {
            if !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()) {
                "[]"
            } else {
                s
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Every field any response in one command group declares, as a descriptor
/// would name it.
///
/// **The group, not the one operation.** A descriptor serves every view of its
/// resource — `customers get` and `customers list` share `CUSTOMER`, and the
/// list response declares ten of its fields — so binding a pointer to the
/// operation a snapshot happens to read from would call two thirds of that
/// descriptor dead. The group is also what absorbs the seven writes whose
/// declared response shape is a divergence: what they answer with is the
/// single transaction their own group's read declares. A key secret and a
/// session identifier are likewise carried by the create response beside the
/// read, and are named by the one descriptor that serves both.
fn renderable_fields(group: &str) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    for contract in support::contracts::CONTRACTS {
        let support::contracts::Mapping::Command(path) = contract.mapping else {
            continue;
        };
        if path.split(' ').next() != Some(group) {
            continue;
        }
        // An operation whose declared response *shape* is a divergence
        // describes a page it never returns, so its own examples are what it
        // answers with.
        let declared = match support::spec::response_shape_divergence(contract.operation_id) {
            Some(_) => support::spec::response_example_pointers(contract.operation_id),
            None => support::spec::response_leaves(contract.operation_id),
        };
        for leaf in declared {
            // A page or an envelope is rendered one element at a time, so the
            // element's own fields are what a descriptor names.
            let segments: Vec<&str> = leaf.split('/').collect();
            if segments.len() > 3 && segments[2] == "[]" {
                out.insert(format!("/{}", segments[3..].join("/")));
            }
            out.insert(leaf);
        }
    }
    out
}

/// The command group one operation is mapped to.
fn group_of(operation_id: &str) -> &'static str {
    let contract = support::contracts::CONTRACTS
        .iter()
        .find(|c| c.operation_id == operation_id)
        .unwrap_or_else(|| panic!("no contract row for {operation_id}"));
    let support::contracts::Mapping::Command(path) = contract.mapping else {
        panic!("{operation_id} is not mapped to a command");
    };
    path.split(' ').next().unwrap()
}

/// **The response-side mirror of the surface matrix.**
///
/// The matrix binds every request field to the bundle, so a stale row fails
/// the build. Nothing bound the response side, and a detail pointer no
/// response carries does not render as absent — it renders as a row saying the
/// field is empty, which for an amount is a wrong answer rather than a missing
/// one.
#[test]
fn every_rendered_pointer_names_a_declared_response_field() {
    let mut wrong: Vec<String> = Vec::new();
    for r in RENDERED {
        let group = group_of(r.operation_id);
        let declared = renderable_fields(group);
        assert!(
            !declared.is_empty(),
            "{group} declares no response fields to bind against"
        );

        for pointer in declared_pointers(r.resource) {
            let wanted = without_indices(pointer);
            let resolves = declared
                .iter()
                .any(|f| *f == wanted || f.starts_with(&format!("{wanted}/")));
            if !resolves {
                wrong.push(format!(
                    "{}.{pointer} names a field no {group} response declares",
                    r.ident
                ));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
