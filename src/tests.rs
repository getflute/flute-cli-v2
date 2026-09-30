use super::*;
use crate::cli::output::OutputFormat;
use clap::Parser;

/// The note names the variable that *is* read. It appears only where the
/// `FLUTE2_` name is absent, because otherwise nothing needs correcting.
#[test]
fn a_v1_variable_is_noted_only_while_its_flute2_name_is_unset() {
    let notes = flute_prefixed_variable_notes(|name| name == "FLUTE_OUTPUT");
    assert_eq!(
        notes,
        vec!["FLUTE_OUTPUT is not read; flute2 reads FLUTE2_OUTPUT.".to_string()]
    );

    let both = flute_prefixed_variable_notes(|name| name.ends_with("OUTPUT"));
    assert!(both.is_empty(), "{both:?}");

    assert!(flute_prefixed_variable_notes(|_| false).is_empty());
}

/// The un-numbered spellings of every credential and configuration
/// variable.
#[test]
fn every_renamed_variable_is_covered() {
    let all: Vec<String> = flute_prefixed_variable_notes(|name| name.starts_with("FLUTE_"));
    assert_eq!(all.len(), DUAL_PREFIX_VARIABLES.len());
    for suffix in ["CLIENT_ID", "CLIENT_SECRET", "PROFILE", "OUTPUT"] {
        assert!(
            all.iter().any(|n| n.contains(&format!("FLUTE_{suffix} "))),
            "{suffix} is not noted"
        );
    }
}

/// Precedence is flag → FLUTE2_OUTPUT → config → table. clap fills the
/// flag from the environment, so this function sees only flag and config.
#[test]
fn output_precedence_prefers_flag_then_config_then_table() {
    assert_eq!(
        resolve_output(Some(OutputFormat::Json), "quiet"),
        OutputFormat::Json
    );
    assert_eq!(resolve_output(None, "quiet"), OutputFormat::Quiet);
    assert_eq!(resolve_output(None, "nonsense"), OutputFormat::Table);
    assert_eq!(resolve_output(None, ""), OutputFormat::Table);
}

/// The config step is reachable only because the flag is optional. If this
/// test can be made to pass with a `default_value` on the flag, the flag is
/// not optional and `auth switch` has been broken again.
#[test]
fn profile_precedence_consults_the_config_when_the_flag_is_absent() {
    assert_eq!(resolve_profile(Some("prod".into()), "production"), "prod");
    assert_eq!(resolve_profile(None, "production"), "production");
    assert_eq!(resolve_profile(None, ""), "sandbox");
}

/// An empty flag is an absent flag. `--profile ""` must not strand the
/// resolution on a profile name that cannot exist.
#[test]
fn an_empty_profile_flag_falls_through_to_the_config() {
    assert_eq!(
        resolve_profile(Some(String::new()), "production"),
        "production"
    );
}

/// argv is the first step of the precedence, and both spellings of the
/// flag are read.
///
/// The last case names no format at all, so it falls through to the
/// environment and then to the config file — which is why it runs under a
/// home directory of its own. Read from the developer's, it asserts
/// whatever their `~/.flute2/config.toml` happens to say.
#[test]
fn json_output_is_detected_from_argv_in_both_spellings() {
    assert!(wants_json_output(&os(&[
        "flute2", "--output", "json", "ping"
    ])));
    assert!(wants_json_output(&os(&["flute2", "--output=json", "ping"])));
    assert!(!wants_json_output(&os(&[
        "flute2", "--output", "table", "ping"
    ])));
    with_an_empty_home(|| {
        assert!(!wants_json_output(&os(&["flute2", "ping"])));
    });
}

/// Run `f` with a home directory that holds no config file, and no
/// `FLUTE2_OUTPUT`, so the fallback resolves to the built-in default.
fn with_an_empty_home(f: impl Fn()) {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().to_str().unwrap();
    temp_env::with_vars(
        [
            ("HOME", Some(path)),
            ("XDG_CONFIG_HOME", Some(path)),
            ("USERPROFILE", Some(path)),
            ("FLUTE2_OUTPUT", None),
        ],
        f,
    );
}

/// The same decision, reached through the environment.
#[test]
fn json_output_is_detected_from_the_environment() {
    temp_env::with_var("FLUTE2_OUTPUT", Some("json"), || {
        assert!(wants_json_output(&os(&["flute2", "ping"])));
    });
    temp_env::with_var("FLUTE2_OUTPUT", Some("table"), || {
        assert!(!wants_json_output(&os(&["flute2", "ping"])));
    });
}

/// Four independent reasons not to check, each asserted alone.
#[test]
fn the_update_check_is_skipped_for_each_documented_reason() {
    temp_env::with_vars(
        [
            ("FLUTE2_NO_UPDATE_CHECK", None::<&str>),
            ("CI", None::<&str>),
        ],
        || {
            let on = config::Config::default();
            assert!(should_check_for_update(
                &ping(),
                &on,
                OutputFormat::Table,
                true
            ));

            // Not a terminal: a notice in a pipe is noise in somebody's
            // data.
            assert!(!should_check_for_update(
                &ping(),
                &on,
                OutputFormat::Table,
                false
            ));

            // JSON output: a notice would not be part of the envelope, and
            // the contract is that stderr carries no data either way.
            assert!(!should_check_for_update(
                &ping(),
                &on,
                OutputFormat::Json,
                true
            ));

            // The config opt-out.
            let off = config::Config {
                auto_update_check: false,
                ..config::Config::default()
            };
            assert!(!should_check_for_update(
                &ping(),
                &off,
                OutputFormat::Table,
                true
            ));
        },
    );
    // And the two environment opt-outs.
    for var in ["FLUTE2_NO_UPDATE_CHECK", "CI"] {
        temp_env::with_var(var, Some("1"), || {
            assert!(!should_check_for_update(
                &ping(),
                &config::Config::default(),
                OutputFormat::Table,
                true
            ));
        });
    }
}

/// Three commands are exempt from the notice whatever the terminal says.
///
/// `update` is already about the version; `auth` is run when something is
/// wrong; `completion` writes a script a shell reads.
#[test]
fn the_update_check_is_skipped_for_the_three_exempt_commands() {
    temp_env::with_vars(
        [
            ("FLUTE2_NO_UPDATE_CHECK", None::<&str>),
            ("CI", None::<&str>),
        ],
        || {
            let on = config::Config::default();
            for command in [
                Command::Update,
                Command::Auth {
                    command: AuthCommand::Status,
                },
                Command::Auth {
                    command: AuthCommand::Login,
                },
                Command::Completion {
                    shell: clap_complete::Shell::Bash,
                },
            ] {
                assert!(
                    !should_check_for_update(&command, &on, OutputFormat::Table, true),
                    "{command:?} must not trigger an update check"
                );
            }
        },
    );
}

/// The banner names the environment and the host the command is about to
/// charge, and says so in red where anyone can see a colour.
#[test]
fn the_production_banner_is_printed_for_production_only() {
    let production = Profile::by_name("production").unwrap();
    let plain = production_banner(&production, false).unwrap();
    assert_eq!(
        plain,
        format!("⚠ Operating on PRODUCTION ({})", production.api_base_url)
    );
    assert!(!plain.contains('\x1b'));

    let coloured = production_banner(&production, true).unwrap();
    assert!(coloured.starts_with("\x1b[31m"), "{coloured}");
    assert!(coloured.ends_with("\x1b[0m"), "{coloured}");
    assert!(coloured.contains("PRODUCTION"), "{coloured}");

    let sandbox = Profile::by_name("sandbox").unwrap();
    assert!(production_banner(&sandbox, false).is_none());
    assert!(production_banner(&sandbox, true).is_none());
}

/// The six destructive commands, each refused by name before anything is
/// resolved; everything else passes through.
#[test]
fn every_destructive_command_is_refused_without_yes() {
    for (argv, expected) in [
        (
            vec!["flute2", "customers", "delete", "cus_1"],
            "deletion requires --yes to confirm (e.g. `customers delete cus_1 --yes`)",
        ),
        (
            vec!["flute2", "payment-methods", "delete", "pm_1"],
            "deletion requires --yes to confirm (e.g. `payment-methods delete pm_1 --yes`)",
        ),
        (
            vec!["flute2", "payment-links", "delete", "pl_1"],
            "deletion requires --yes to confirm (e.g. `payment-links delete pl_1 --yes`)",
        ),
        (
            vec!["flute2", "api-keys", "revoke", "--client-id", "key_1"],
            "revocation requires --yes to confirm (e.g. `api-keys revoke --client-id key_1 --yes`)",
        ),
        (
            vec!["flute2", "payment-sessions", "cancel", "ps_1"],
            "cancellation requires --yes to confirm (e.g. `payment-sessions cancel ps_1 --yes`)",
        ),
        (
            vec!["flute2", "pos", "cancel", "pos_1"],
            "cancellation requires --yes to confirm (e.g. `pos cancel pos_1 --yes`)",
        ),
    ] {
        let command = command_from(&argv);
        assert_eq!(
            unconfirmed_destructive_command(&command).as_deref(),
            Some(expected)
        );

        let mut confirmed = argv.clone();
        confirmed.push("--yes");
        assert!(unconfirmed_destructive_command(&command_from(&confirmed)).is_none());
    }

    assert!(unconfirmed_destructive_command(&ping()).is_none());
    assert!(
        unconfirmed_destructive_command(&command_from(&["flute2", "customers", "get", "c_1"]))
            .is_none()
    );
}

/// **The flag is the register of what must be confirmed, not the match.**
/// A command that offers `--yes` and has no arm parses and runs
/// unconfirmed, so the guard is derived from the command tree rather than
/// from a list kept by hand alongside it.
#[test]
fn every_command_offering_yes_is_refused_without_it() {
    use clap::CommandFactory;

    fn walk(cmd: &clap::builder::Command, path: &[String], found: &mut Vec<Vec<String>>) {
        let mut leaf = true;
        for sub in cmd.get_subcommands() {
            // clap's own generated `help` subcommand is not part of the
            // surface this repository owns.
            if sub.get_name() == "help" {
                continue;
            }
            leaf = false;
            let mut next = path.to_vec();
            next.push(sub.get_name().to_string());
            walk(sub, &next, found);
        }
        if !leaf || !cmd.get_arguments().any(|a| a.get_id() == "yes") {
            return;
        }
        let mut argv = path.to_vec();
        for arg in cmd.get_arguments() {
            if arg.get_id() == "yes" || !arg.is_required_set() {
                continue;
            }
            if let Some(long) = arg.get_long() {
                argv.push(format!("--{long}"));
            }
            argv.push("id_1".to_string());
        }
        found.push(argv);
    }

    let mut found = Vec::new();
    walk(&Cli::command(), &["flute2".to_string()], &mut found);
    assert!(
        found.len() >= 6,
        "the tree walk found only {} commands offering --yes",
        found.len()
    );
    for argv in found {
        let args: Vec<&str> = argv.iter().map(String::as_str).collect();
        assert!(
            unconfirmed_destructive_command(&command_from(&args)).is_some(),
            "`{}` offers --yes and is not refused without it",
            args[1..].join(" ")
        );
    }
}

fn ping() -> Command {
    Command::Ping
}

fn command_from(argv: &[&str]) -> Command {
    Cli::try_parse_from(argv)
        .unwrap_or_else(|e| panic!("{argv:?} did not parse: {e}"))
        .command
        .expect("a command was parsed")
}

/// The message a usage error carries into the JSON envelope: clap's
/// styling removed, and the `Usage:` block — which restates the help — cut
/// off, so a consumer reads one sentence rather than a screen.
#[test]
fn a_clap_error_message_is_one_unstyled_sentence() {
    let err = Cli::try_parse_from(["flute2", "nosuchcommand"]).unwrap_err();
    let message = clap_error_message(&err);
    assert!(!message.contains('\u{1b}'), "{message}");
    assert!(!message.contains("Usage:"), "{message}");
    assert!(message.contains("nosuchcommand"), "{message}");
    assert_eq!(message, message.trim());
}

/// An error with no `Usage:` block keeps its sentence, without the label.
#[test]
fn a_clap_error_with_no_usage_block_keeps_its_sentence() {
    assert_eq!(
        clap_error_message(&clap::Error::raw(
            clap::error::ErrorKind::ValueValidation,
            "\u{1b}[31mbad value\u{1b}[0m\n"
        )),
        "bad value"
    );
}

/// An unknown flag's message is the sentence alone: no label, no tip, no
/// pointer to `--help`.
#[test]
fn a_clap_error_message_drops_the_tip_and_the_footer() {
    let err = Cli::try_parse_from(["flute2", "ping", "--bogus"]).unwrap_err();
    assert_eq!(
        clap_error_message(&err),
        "unexpected argument '--bogus' found"
    );
}

/// The list clap attaches to the complaint stays with it, on one line.
#[test]
fn a_clap_error_message_keeps_the_list_it_names() {
    let err = Cli::try_parse_from(["flute2", "customers", "create"]).unwrap_err();
    assert_eq!(
        clap_error_message(&err),
        "the following required arguments were not provided: \
         --first-name <FIRST_NAME> --last-name <LAST_NAME>"
    );
    let err = Cli::try_parse_from(["flute2", "--output", "xml", "ping"]).unwrap_err();
    assert_eq!(
        clap_error_message(&err),
        "invalid value 'xml' for '--output <OUTPUT>' [possible values: table, json, quiet]"
    );
}

/// A literal `--output` with nothing after it must not panic on the
/// lookahead.
#[test]
fn a_trailing_output_flag_with_no_value_is_not_json() {
    assert!(!wants_json_output(&os(&["flute2", "--output"])));
}

fn os(args: &[&str]) -> Vec<std::ffi::OsString> {
    args.iter().map(std::ffi::OsString::from).collect()
}
