//! The clap root and the global flags.

pub mod address;
pub mod common;
pub mod money;
pub mod output;
pub mod render;

use output::OutputFormat;

const GLOBAL_HEADING: &str = "Global options";

#[derive(clap::Parser, Debug)]
#[command(
    name = "flute2",
    version,
    about = "CLI for the Flute payments platform"
)]
pub struct Cli {
    // No `default_value`: absent is the only state in which the config file's
    // `default_profile` can be consulted, and a defaulted flag is never
    // absent.
    /// Active profile (environment). `sandbox` (default) or `production`/`prod`.
    #[arg(long, env = "FLUTE2_PROFILE", global = true, help_heading = GLOBAL_HEADING)]
    pub profile: Option<String>,

    /// Output format: table (default), json, or quiet (id only).
    /// When omitted, falls back to the `FLUTE2_OUTPUT` env var, then to the
    /// `output` key in ~/.flute2/config.toml, then to `table`.
    #[arg(long, env = "FLUTE2_OUTPUT", global = true, value_enum, ignore_case = true,
          help_heading = GLOBAL_HEADING)]
    pub output: Option<OutputFormat>,

    /// Print HTTP request/response traces to stderr. Card and bank-account
    /// numbers are masked to the last 4 digits; CVV/security codes are removed.
    #[arg(long, global = true, help_heading = GLOBAL_HEADING)]
    pub debug: bool,

    // A bare invocation prints help and succeeds, so the subcommand is
    // optional rather than a usage error.
    #[command(subcommand)]
    pub command: Option<Command>,
}

// A `///` here would be user-facing: clap takes the subcommand enum's doc
// comment as the root's `long_about` when the parent declares none, so a
// rationale written as documentation appears in `flute2 --help`.
//
// `large_enum_variant` is allowed rather than fixed: boxing a variant breaks
// `#[derive(Subcommand)]`, which needs the args struct inline, and the enum is
// constructed exactly once per process. The lint is measuring a cost that is
// not paid here.
#[allow(clippy::large_enum_variant)]
#[derive(clap::Subcommand, Debug)]
pub enum Command {
    /// Authentication and profile management.
    Auth {
        #[command(subcommand)]
        command: AuthCommand,
    },
    /// API health check.
    Ping,
    /// Print CLI version and active profile.
    Version,
    /// Transaction operations (create, capture, reversal, …).
    Transactions {
        #[command(subcommand)]
        command: crate::groups::transactions::TransactionsCommand,
    },
    /// Customer operations (create, get, list, update, delete).
    Customers {
        #[command(subcommand)]
        command: crate::groups::customers::CustomersCommand,
    },
    /// Payment-method vault operations (list, get, add-card, add-ach, update,
    /// delete, set-default).
    PaymentMethods {
        #[command(subcommand)]
        command: crate::groups::payment_methods::PaymentMethodsCommand,
    },
    /// Payment link operations (create, get, list, update, delete, share).
    PaymentLinks {
        #[command(subcommand)]
        command: crate::groups::payment_links::PaymentLinksCommand,
    },
    /// Payment session operations (create, get, cancel).
    PaymentSessions {
        #[command(subcommand)]
        command: crate::groups::payment_sessions::PaymentSessionsCommand,
    },
    /// Terminal operations (list, status).
    Terminals {
        #[command(subcommand)]
        command: crate::groups::terminals::TerminalsCommand,
    },
    /// POS transaction operations (create [--wait], get, list, cancel,
    /// print-receipt).
    Pos {
        #[command(subcommand)]
        command: crate::groups::pos::PosCommand,
    },
    /// Settlement batch operations (list, get, close).
    Settlements {
        #[command(subcommand)]
        command: crate::groups::settlements::SettlementsCommand,
    },
    /// Account settings operations (payment-config, contact-info, autofill,
    /// update-autofill).
    Settings {
        #[command(subcommand)]
        command: crate::groups::settings::SettingsCommand,
    },
    /// Merchant API key operations (create, list, revoke).
    ApiKeys {
        #[command(subcommand)]
        command: crate::groups::api_keys::ApiKeysCommand,
    },
    /// Print shell completion script for the given shell.
    Completion {
        /// Shell to generate completions for.
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// Update the flute2 CLI to the latest release.
    Update,
}

#[derive(clap::Subcommand, Debug)]
pub enum AuthCommand {
    /// Prompt for client_id + client_secret and store them in the OS keychain.
    Login,
    /// Show active profile, environment, and live authentication status.
    Status,
    /// Set the default profile in ~/.flute2/config.toml.
    Switch {
        /// `sandbox` (default) or `production`/`prod`.
        profile: String,
    },
    /// Clear stored credentials for the active profile.
    Logout,
    /// Print the current bearer token (debugging aid).
    Token,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// clap's own consistency check over the whole tree.
    ///
    /// It catches what nothing else does: a duplicate argument id, a
    /// conflicting short flag, a bad default. Two flattened structs both
    /// declaring `line1` panic *in the built binary* at parse time, which no
    /// compile check sees and every command test then reports as a mysterious
    /// exit 101.
    #[test]
    fn the_command_tree_is_internally_consistent() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }

    /// An explicit `id` — which several flattened arg structs need to avoid
    /// clap's derive-from-field-name collisions — also becomes the value name
    /// in `--help` unless a `value_name` is given. That leaks an internal
    /// disambiguator into the user-facing help, as `--email <list_email>`.
    #[test]
    fn no_value_placeholder_leaks_an_internal_argument_id() {
        use clap::CommandFactory;
        let mut offenders = Vec::new();
        walk(&Cli::command(), &mut offenders);
        assert!(
            offenders.is_empty(),
            "these placeholders are lowercase, so they are argument ids rather \
             than value names:\n  {}",
            offenders.join("\n  ")
        );
    }

    fn walk(command: &clap::Command, offenders: &mut Vec<String>) {
        for arg in command.get_arguments() {
            // `get_num_args` is unset until the command is built, so the
            // question is whether the argument takes a value at all.
            if arg.get_action().takes_values() {
                let placeholder = arg
                    .get_value_names()
                    .map(|names| {
                        names
                            .iter()
                            .map(std::string::ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                    // clap falls back to the argument id when no value name
                    // is set, which is exactly the leak being looked for.
                    .unwrap_or_else(|| arg.get_id().to_string());
                if placeholder.chars().any(char::is_lowercase) {
                    offenders.push(format!(
                        "{} {} <{placeholder}>",
                        command.get_name(),
                        arg.get_id()
                    ));
                }
            }
        }
        for sub in command.get_subcommands() {
            walk(sub, offenders);
        }
    }
}
