#![forbid(unsafe_code)]

use std::sync::atomic::{AtomicBool, Ordering};

/// Whether stdout's consumer has stopped reading.
static STDOUT_CLOSED: AtomicBool = AtomicBool::new(false);

/// Whether stderr's consumer has stopped reading.
static STDERR_CLOSED: AtomicBool = AtomicBool::new(false);

/// One write to a standard stream, and the rule both of them follow.
///
/// Rust starts with `SIGPIPE` ignored, so a write to a pipe nobody is reading
/// returns `EPIPE` instead of ending the process — and the standard macros
/// turn that into a panic, which is a backtrace and an exit code outside the
/// published table. A consumer that stops reading is ordinary use of a CLI
/// whose output is piped, so the break is recorded and later writes are
/// dropped: the calling code has nothing to act on the error with, and the
/// process has nothing left to say to a stream nobody is reading. Any other
/// write error still panics as the standard macros do: a full disk is not a
/// consumer walking away.
fn write_stream(
    closed: &AtomicBool,
    stream: &str,
    out: &mut dyn std::io::Write,
    write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
) {
    if closed.load(Ordering::Relaxed) {
        return;
    }
    match write(out) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {
            closed.store(true, Ordering::Relaxed);
        }
        Err(e) => panic!("failed printing to {stream}: {e}"),
    }
}

/// The one place this crate writes to stdout, which carries the data a caller
/// came for — so a break is also what `main_entry` ends the process by.
fn write_stdout(write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>) {
    write_stream(
        &STDOUT_CLOSED,
        "stdout",
        &mut std::io::stdout().lock(),
        write,
    );
}

/// The one place this crate writes to stderr, which carries diagnostics and
/// no data — so a break costs the line and leaves the exit code alone.
fn write_stderr(write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>) {
    write_stream(
        &STDERR_CLOSED,
        "stderr",
        &mut std::io::stderr().lock(),
        write,
    );
}

/// One formatted write, the body of `print!` and `println!`.
///
/// The arguments are formatted by the caller rather than inside a closure, so
/// that a `println!` whose argument is `.await`ed still compiles.
pub(crate) fn print_args(args: std::fmt::Arguments<'_>) {
    write_stdout(|out| out.write_fmt(args));
}

/// One formatted write to stderr, the body of `eprintln!`.
pub(crate) fn eprint_args(args: std::fmt::Arguments<'_>) {
    write_stderr(|out| out.write_fmt(args));
}

/// One unformatted write, for output that is already a block of bytes.
pub(crate) fn print_bytes(bytes: &[u8]) {
    write_stdout(|out| out.write_all(bytes));
}

/// Whether anything written to stdout was dropped because its consumer had
/// stopped reading, which makes the caller's copy incomplete.
pub(crate) fn stdout_closed() -> bool {
    STDOUT_CLOSED.load(Ordering::Relaxed)
}

/// `print!`, `println!` and `eprintln!` for the whole crate, shadowing the
/// standard macros so that every call site in every module below routes
/// through one writer without naming it. Shadowing is what makes the rule
/// unskippable: a new `println!` or `eprintln!` anywhere in the crate is
/// covered the moment it is written.
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::print_args(::std::format_args!($($arg)*))
    };
}

macro_rules! println {
    () => {
        $crate::print_bytes(b"\n")
    };
    ($($arg:tt)*) => {
        $crate::print_args(::std::format_args!("{}\n", ::std::format_args!($($arg)*)))
    };
}

macro_rules! eprintln {
    () => {
        $crate::eprint_args(::std::format_args!("\n"))
    };
    ($($arg:tt)*) => {
        $crate::eprint_args(::std::format_args!("{}\n", ::std::format_args!($($arg)*)))
    };
}

pub mod api;
pub mod auth;
pub mod cli;
pub mod config;
pub mod groups;
pub mod update;
pub mod update_check;

use api::ApiClient;
use cli::output::{EXIT_STDOUT_CLOSED, ErrorJson, OutputFormat, exit_code_for};
use cli::{AuthCommand, Cli, Command};
use config::Profile;
use std::ffi::OsString;
use std::process::ExitCode;

/// `--output` → `FLUTE2_OUTPUT` → config `output` → table.
///
/// clap fills the flag from the environment, so this sees only flag and
/// config. An unrecognised config value falls through rather than becoming a
/// mode, so a typo in a config file cannot silently change what is printed.
pub(crate) fn resolve_output(flag: Option<OutputFormat>, config_value: &str) -> OutputFormat {
    flag.or_else(|| OutputFormat::from_config_str(config_value))
        .unwrap_or(OutputFormat::Table)
}

/// `--profile` → `FLUTE2_PROFILE` (via clap) → config `default_profile` → sandbox.
pub(crate) fn resolve_profile(flag: Option<String>, config_value: &str) -> String {
    flag.filter(|s| !s.is_empty())
        .or_else(|| {
            Some(config_value)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "sandbox".into())
}

/// Whether a *parse failure* should be reported as a JSON envelope.
///
/// A usage error is raised before clap has produced a `Cli`, so this one
/// decision walks the same precedence by hand: argv, the environment, then the
/// config file. Both spellings of the flag are accepted because clap accepts
/// both.
pub(crate) fn wants_json_output(args: &[OsString]) -> bool {
    let mut iter = args.iter().map(|a| a.to_string_lossy());
    while let Some(arg) = iter.next() {
        if let Some(value) = arg.strip_prefix("--output=") {
            return value.eq_ignore_ascii_case("json");
        }
        if arg == "--output" {
            return iter.next().is_some_and(|v| v.eq_ignore_ascii_case("json"));
        }
    }
    match std::env::var("FLUTE2_OUTPUT")
        .ok()
        .filter(|v| !v.is_empty())
    {
        Some(value) => value.eq_ignore_ascii_case("json"),
        None => config::load_or_default()
            .output
            .eq_ignore_ascii_case("json"),
    }
}

/// The single place an error becomes output and an exit code.
///
/// Library code returns `Result` and never calls `process::exit`: an exit
/// buried in a dispatch arm cannot be unit-tested and skips destructors.
/// `run` decides *what happened* and hands back the format it resolved;
/// `main_entry` decides *how to say it and what to exit with*. A parse failure
/// never reaches `run`, so its format is resolved separately.
pub async fn main_entry<I, T>(args: I) -> ExitCode
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let argv: Vec<OsString> = args.into_iter().map(Into::into).collect();
    for note in flute_prefixed_variable_notes(|name| std::env::var_os(name).is_some()) {
        eprintln!("{note}");
    }
    let exit = match parse_cli(&argv) {
        Ok(cli) => {
            let (output, result) = run(cli).await;
            match result {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => report(&e, output == OutputFormat::Json),
            }
        }
        Err(e) => report_clap(&e, wants_json_output(&argv)),
    };
    // A consumer that stopped reading outranks whatever the command was going
    // to say, in every output mode: the caller's copy of stdout is incomplete,
    // and it is not listening to be told so. The check follows `report`
    // because an error envelope is one of the writes that can break.
    if stdout_closed() {
        return ExitCode::from(EXIT_STDOUT_CLOSED);
    }
    exit
}

/// Parse argv, with an empty `FLUTE2_OUTPUT` read as unset.
///
/// clap fills an env-backed flag before this crate's own precedence runs, and
/// an exported-but-empty variable carries no format to select — so the binding
/// is dropped rather than refused as a value nobody typed, which would make
/// even an explicit `--output json` a usage error.
fn parse_cli(argv: &[OsString]) -> Result<Cli, clap::Error> {
    use clap::{CommandFactory, FromArgMatches};
    let mut command = Cli::command();
    if std::env::var_os("FLUTE2_OUTPUT").is_some_and(|value| value.is_empty()) {
        command = command.mut_arg("output", |arg| arg.env(None::<&str>));
    }
    Cli::from_arg_matches(&command.try_get_matches_from(argv)?)
}

/// The variable names that exist under both a `FLUTE_` and a `FLUTE2_`
/// prefix. Only the suffix is listed, so the two spellings cannot drift apart.
const DUAL_PREFIX_VARIABLES: [&str; 6] = [
    "PROFILE",
    "OUTPUT",
    "CLIENT_ID",
    "CLIENT_SECRET",
    "NO_UPDATE_CHECK",
    "GITHUB_TOKEN",
];

/// One note per `FLUTE_` variable set with no `FLUTE2_` counterpart.
///
/// A `FLUTE_` name belongs to the other Flute CLI, which holds separate
/// credentials and separate configuration — so adopting one would point this
/// binary at another account. Presence is the whole question, and the caller
/// supplies a lookup that answers it without reading a value. That leaves a
/// set variable silently inert, and the note is the only thing that tells the
/// caller which name is read instead.
///
/// The lookup arrives as an argument so the decision can be tested without
/// mutating the process environment.
fn flute_prefixed_variable_notes(is_set: impl Fn(&str) -> bool) -> Vec<String> {
    DUAL_PREFIX_VARIABLES
        .iter()
        .filter(|suffix| is_set(&format!("FLUTE_{suffix}")) && !is_set(&format!("FLUTE2_{suffix}")))
        .map(|suffix| format!("FLUTE_{suffix} is not read; flute2 reads FLUTE2_{suffix}."))
        .collect()
}

/// Everything a command that talks to the API is handed.
///
/// Built once, here, rather than resolved inside every dispatch arm as v1
/// does — which is what lets a test point the compiled binary at a mock
/// server.
pub struct Ctx {
    pub api: ApiClient,
    pub profile: Profile,
    pub output: OutputFormat,
}

/// Resolve the environment the command will run in, then dispatch.
///
/// Commands fall into **three** categories, not two:
///
/// - **No credential resolution** — `completion`, `version`, `update`, and
///   `auth login/logout/switch`. They are not all offline and not all
///   keychain-free: `auth login` writes the keychain, `auth logout` deletes
///   from it, and `update` reaches GitHub. What unites them is that nothing
///   resolves a credential *on their behalf*, so none can fail for want of
///   one. A login that fails because you are not logged in is the failure
///   this split exists to prevent.
/// - **Credentials optional** — `auth status` alone.
/// - **Credentials required** — everything else, which receives a `Ctx`.
///   Missing credentials is exit 2, reported when a token is first needed:
///   after the command's own refusals and before any request is sent.
///
/// The resolved output format travels back with the result because a failure
/// is reported in the format the config file asked for, and only this function
/// has read it.
pub async fn run(mut cli: Cli) -> (OutputFormat, anyhow::Result<()>) {
    // A bare invocation asks for nothing, so it is answered with help on
    // stdout and success — before a config file is read or a credential is
    // resolved, neither of which help needs.
    let Some(command) = cli.command.take() else {
        use clap::CommandFactory;
        println!("{}", Cli::command().render_help());
        return (OutputFormat::Table, Ok(()));
    };

    let config = config::load_or_default();
    let output = resolve_output(cli.output, &config.output);

    // Confirmation is a client-side gate, so it is answered before a
    // credential is resolved: a machine with no login must still be told what
    // is missing is `--yes`.
    if let Some(refusal) = unconfirmed_destructive_command(&command) {
        return (output, Err(anyhow::anyhow!(refusal)));
    }

    let notify = should_check_for_update(&command, &config, output, stderr_is_terminal());
    let result = dispatch(&cli, command, &config, output).await;

    // After the command, never instead of it: a notice must not turn a
    // successful charge into a failure, and it is pointless on a failed one.
    if notify && result.is_ok() {
        if let Some(latest) = update_check::check_for_update().await {
            // Blank line first: the notice is an aside, not the last line of
            // the command's own output. stderr, so it never contaminates the
            // data.
            eprintln!("\n{}", update_check::notice_for(&latest));
        }
    }
    (output, result)
}

/// The refusal a destructive command earns when `--yes` is absent.
///
/// One place, consulted before anything is resolved, because a gate that
/// fires only after authentication cannot protect an unconfigured machine.
fn unconfirmed_destructive_command(command: &Command) -> Option<String> {
    use groups::api_keys::ApiKeysCommand;
    use groups::customers::CustomersCommand;
    use groups::payment_links::PaymentLinksCommand;
    use groups::payment_methods::PaymentMethodsCommand;
    use groups::payment_sessions::PaymentSessionsCommand;
    use groups::pos::PosCommand;

    // The noun names what is about to be lost, and the example is the caller's
    // own invocation with `--yes` on the end: a refusal worth reading is one
    // that can be re-run from what it printed.
    let (noun, invocation) = match command {
        Command::Customers {
            command:
                CustomersCommand::Delete {
                    customer_id,
                    yes: false,
                },
        } => ("deletion", format!("customers delete {customer_id}")),
        Command::PaymentMethods {
            command:
                PaymentMethodsCommand::Delete {
                    payment_method_id,
                    yes: false,
                },
        } => (
            "removal",
            format!("payment-methods delete {payment_method_id}"),
        ),
        Command::PaymentLinks {
            command:
                PaymentLinksCommand::Delete {
                    payment_link_id,
                    yes: false,
                },
        } => (
            "deletion",
            format!("payment-links delete {payment_link_id}"),
        ),
        Command::ApiKeys {
            command:
                ApiKeysCommand::Revoke {
                    client_id,
                    yes: false,
                },
        } => (
            "revocation",
            format!("api-keys revoke --client-id {client_id}"),
        ),
        Command::PaymentSessions {
            command:
                PaymentSessionsCommand::Cancel {
                    payment_session_id,
                    yes: false,
                },
        } => (
            "cancellation",
            format!("payment-sessions cancel {payment_session_id}"),
        ),
        Command::Pos {
            command:
                PosCommand::Cancel {
                    pos_transaction_id,
                    yes: false,
                },
        } => ("cancellation", format!("pos cancel {pos_transaction_id}")),
        _ => return None,
    };
    Some(format!(
        "{noun} requires --yes to confirm (e.g. `{invocation} --yes`)"
    ))
}

fn stderr_is_terminal() -> bool {
    use std::io::IsTerminal;
    std::io::stderr().is_terminal()
}

/// The warning a production command carries, or `None` for any other profile.
///
/// The writing and the terminal check stay at the call site so the decision
/// and the wording can be tested without stderr. Colour is the caller's
/// answer to "is anyone looking at a terminal", because escape codes in a
/// redirected log are noise.
fn production_banner(profile: &Profile, color: bool) -> Option<String> {
    if !profile.is_production() {
        return None;
    }
    let message = format!("⚠ Operating on PRODUCTION ({})", profile.api_base_url);
    Some(if color {
        format!("\x1b[31m{message}\x1b[0m")
    } else {
        message
    })
}

/// Whether the post-command update notice should be attempted.
///
/// The TTY check is a *call-site* concern, so it arrives as an argument:
/// there is no point printing a notice into a pipe, and passing it in is also
/// the only way to test the decision without a terminal.
///
/// Three commands are exempt whatever the terminal says: `update` is already
/// about the version, `auth` is run when something is wrong and wants no
/// aside, and `completion` writes a script a shell reads.
fn should_check_for_update(
    command: &Command,
    config: &config::Config,
    output: OutputFormat,
    stderr_is_terminal: bool,
) -> bool {
    if matches!(
        command,
        Command::Update | Command::Auth { .. } | Command::Completion { .. }
    ) {
        return false;
    }
    stderr_is_terminal && output != OutputFormat::Json && !update_check::opt_out(config)
}

async fn dispatch(
    cli: &Cli,
    command: Command,
    config: &config::Config,
    output: OutputFormat,
) -> anyhow::Result<()> {
    let profile_name = resolve_profile(cli.profile.clone(), &config.default_profile);

    init_tracing(cli.debug);

    // `completion` and `update` act on no environment at all, so they are
    // dispatched before a profile is resolved — and are the only commands
    // with no profile to warn about.
    match &command {
        Command::Completion { shell } => return emit_completion(*shell),
        Command::Update => return update::run().await,
        _ => {}
    }

    // Every command that resolves a profile announces a production one first,
    // whether or not it goes on to reach the API. `auth switch` is the
    // exception in what it names: it warns about the profile it is pointing
    // the next command at, which is the one on the command line.
    let announced = match &command {
        Command::Auth {
            command: AuthCommand::Switch { profile },
        } => profile,
        _ => &profile_name,
    };
    // The banner goes to stderr so it never contaminates the data stream.
    if let Some(banner) = Profile::by_name(announced)
        .and_then(|profile| production_banner(&profile, stderr_is_terminal()))
    {
        eprintln!("{banner}");
    }

    // Category 1: no credential is resolved on these commands' behalf.
    if let Command::Auth {
        command: AuthCommand::Switch { profile },
    } = &command
    {
        return groups::auth::switch(profile);
    }

    let profile = Profile::by_name(&profile_name)
        .ok_or_else(|| anyhow::anyhow!("unknown profile: {profile_name}"))?;

    match &command {
        Command::Auth {
            command: AuthCommand::Login,
        } => return groups::auth::login(&profile.name),
        Command::Auth {
            command: AuthCommand::Logout,
        } => return groups::auth::logout(&profile.name),
        Command::Version => return render_version(&profile, output),
        _ => {}
    }

    // Category 2: resolves credentials and tolerates their absence.
    if let Command::Auth {
        command: AuthCommand::Status,
    } = &command
    {
        return groups::auth::status(&profile, output).await;
    }

    // Category 3: one credential resolution, one client, one place to reason
    // about it.
    // Absent credentials travel into the client rather than ending the
    // command here: the validators below run first, so an invocation the CLI
    // refuses on its own is reported as that and not as a missing login.
    let creds = auth::keychain::load_with_env_fallback(&profile.name)
        .map_err(|e| api::ApiError::Auth(e.to_string()))?;
    let ctx = Ctx {
        api: ApiClient::new(&profile, creds)?,
        profile,
        output,
    };

    // Consuming here, unlike the borrowing matches above, so a group's
    // dispatch owns its arguments rather than cloning them.
    match command {
        Command::ApiKeys { command } => groups::api_keys::dispatch(&ctx, command).await,
        Command::Customers { command } => groups::customers::dispatch(&ctx, command).await,
        Command::PaymentLinks { command } => groups::payment_links::dispatch(&ctx, command).await,
        Command::PaymentMethods { command } => {
            groups::payment_methods::dispatch(&ctx, command).await
        }
        Command::PaymentSessions { command } => {
            groups::payment_sessions::dispatch(&ctx, command).await
        }
        Command::Ping => groups::ping::dispatch(&ctx).await,
        Command::Pos { command } => groups::pos::dispatch(&ctx, command).await,
        Command::Settings { command } => groups::settings::dispatch(&ctx, command).await,
        Command::Settlements { command } => groups::settlements::dispatch(&ctx, command).await,
        Command::Terminals { command } => groups::terminals::dispatch(&ctx, command).await,
        Command::Transactions { command } => groups::transactions::dispatch(&ctx, command).await,
        Command::Auth {
            command: AuthCommand::Token,
        } => groups::auth::token(&ctx.api).await,
        // Category 1 and 2 commands returned above.
        Command::Auth { .. } | Command::Version | Command::Update | Command::Completion { .. } => {
            unreachable!("handled before credentials are resolved")
        }
    }
}

/// Traces go to **stderr**, so `--debug --output json` still emits parseable
/// JSON on stdout.
///
/// A subscriber is installed for every invocation, not only under `--debug`,
/// so `RUST_LOG` can select what an operator sees without changing the
/// command. It overrides either preset when set.
///
/// Colour is spent only where something can show it: a redirected log carries
/// escape sequences a reader has to work around, and `--debug` output is read
/// from a file at least as often as from a terminal.
fn init_tracing(debug: bool) {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(if debug {
            "debug,flute_cli2=debug,reqwest=debug,hyper=info"
        } else {
            "warn,flute_cli2=info"
        })
    });
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(!debug && stderr_is_terminal())
        .try_init();
}

fn emit_completion(shell: clap_complete::Shell) -> anyhow::Result<()> {
    use clap::CommandFactory;
    let mut command = Cli::command();
    // `generate` panics on a write error of its own, so it is given memory it
    // cannot fail to write to and the script reaches stdout by the same route
    // as every other line this crate prints.
    let mut script = Vec::new();
    clap_complete::generate(shell, &mut command, "flute2", &mut script);
    print_bytes(&script);
    Ok(())
}

fn render_version(profile: &Profile, output: OutputFormat) -> anyhow::Result<()> {
    let version = env!("CARGO_PKG_VERSION");
    let api_base_url = api::client::resolve_base_url(profile)?;
    match output {
        OutputFormat::Json => {
            let data = serde_json::json!({
                "version": version,
                "profile": profile.name,
                "api_base_url": api_base_url,
            });
            let env = cli::output::Envelope::new("version", data, &profile.name, None, None);
            println!("{}", serde_json::to_string_pretty(&env)?);
        }
        OutputFormat::Quiet => println!("{version}"),
        OutputFormat::Table => {
            println!("flute2  v{version}");
            println!("Profile:  {}", profile.name);
            println!("API base: {api_base_url}");
        }
    }
    Ok(())
}

/// Data goes to stdout and everything else to stderr, so a machine consumer
/// under `--output json` never sees an empty stdout on failure.
fn report(err: &anyhow::Error, json: bool) -> ExitCode {
    let code = exit_code_for(err);
    // An outcome the command already wrote out gets nothing added to either
    // stream: a second envelope on stdout would be a second JSON document,
    // and an interrupt has to leave stdout empty in every output mode.
    if err.downcast_ref::<cli::output::Reported>().is_some() {
        return ExitCode::from(u8::try_from(code).unwrap_or(1));
    }
    if json {
        let envelope = ErrorJson::from_anyhow(err);
        println!(
            "{}",
            serde_json::to_string_pretty(&envelope).unwrap_or_default()
        );
    } else {
        eprintln!("Error: {err:#}");
    }
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}

/// `--version` and `--help` reach here as clap "errors" that are not
/// failures: clap renders them to stdout and the process succeeds. A real
/// usage error is a client error, exit 3.
fn report_clap(err: &clap::Error, json: bool) -> ExitCode {
    if !err.use_stderr() {
        let _ = err.print();
        return ExitCode::SUCCESS;
    }
    if json {
        let envelope = ErrorJson {
            kind: "client",
            message: clap_error_message(err),
            status: None,
            correlation_id: None,
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&envelope).unwrap_or_default()
        );
    } else {
        let _ = err.print();
    }
    ExitCode::from(3)
}

/// Reduce a clap error to the complaint alone for the JSON envelope.
///
/// The `Usage:` block clap appends restates the help, which a machine
/// consumer already has a command for and a log line has no room for.
fn clap_error_message(err: &clap::Error) -> String {
    let full = strip_ansi(&err.to_string());
    // A group invoked with no subcommand carries the group's help page as its
    // payload, whose first line is the group description — which describes the
    // group rather than saying what is wrong with the invocation.
    if matches!(
        err.kind(),
        clap::error::ErrorKind::MissingSubcommand
            | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    ) {
        if let Some(path) = usage_command_path(&full) {
            return format!(
                "'{path}' requires a subcommand; run `{path} --help` to see the choices"
            );
        }
    }
    match full.split_once("\n\nUsage:") {
        Some((head, _)) => head.trim().to_string(),
        None => full.trim().to_string(),
    }
}

/// The command a rendered clap message is about, read off its `Usage:` line.
///
/// The path ends at the first placeholder, so `flute2 auth [OPTIONS]
/// <COMMAND>` names `flute2 auth`.
fn usage_command_path(rendered: &str) -> Option<String> {
    let usage = rendered
        .lines()
        .find_map(|l| l.trim().strip_prefix("Usage:"))?;
    let path: Vec<&str> = usage
        .split_whitespace()
        .take_while(|w| !w.starts_with('[') && !w.starts_with('<'))
        .collect();
    (!path.is_empty()).then(|| path.join(" "))
}

/// clap styles its errors for a terminal. Escape sequences inside a JSON
/// string are technically legal and practically unreadable, so they are
/// removed on the JSON path only.
fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests;
