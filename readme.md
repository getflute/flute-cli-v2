# flute2

[![CI](https://github.com/getflute/flute-cli-v2/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/getflute/flute-cli-v2/actions/workflows/ci.yml)

`flute2` is a cross-platform CLI for the Flute payments platform, targeting the v2 API. It is built in Rust and MIT licensed.

## Install

### Homebrew (macOS, Linux)

```sh
brew install getflute/flute-cli-v2/flute2
```

### Shell script (macOS, Linux)

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/getflute/flute-cli-v2/releases/latest/download/flute2-installer.sh | sh
```

### PowerShell (Windows)

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/getflute/flute-cli-v2/releases/latest/download/flute2-installer.ps1 | iex"
```

Both installers download from GitHub Releases, put `flute2` in `~/.cargo/bin` (`%USERPROFILE%\.cargo\bin` on Windows), and add that directory to your `PATH`. Open a new terminal afterwards and check the install with `flute2 version`.

### From source

```sh
cargo install --locked --path .

# Or just build
cargo build --release
# Binary lands at target/release/flute2
```

On Linux, building needs `libdbus-1-dev` and `pkg-config` for the keychain.

## Quick start

Log in. The command prompts for a client ID and secret and stores them in the OS keychain:

```sh
flute2 auth login
```

In CI or another non-interactive environment, set environment variables instead of using the keychain:

```sh
export FLUTE2_CLIENT_ID=<your-client-id>
export FLUTE2_CLIENT_SECRET=<your-client-secret>
```

Check the connection and find your processor ids, which every charge needs:

```sh
flute2 ping
flute2 version
flute2 settings payment-config
```

Charge a card:

```sh
flute2 transactions create \
  --payment-processor-id <card-processor-id> \
  --amount 10.50 --reference-id order-1001 \
  --card 4111111111111111 --exp 12/2032 --cvv 123 \
  --billing-line1 "123 Test St" --billing-postal-code 10001 --billing-country US
```

A declined charge exits 0, so read `transactionStatus` to see whether it was paid.

`--output table|json|quiet` sets the output mode. The `FLUTE2_OUTPUT` environment variable and the `output` key in `~/.flute2/config.toml` set it too.

- `table` (the default) prints a human-readable summary.
- `json` prints a structured envelope for scripts and agents.
- `quiet` prints only the resource id, one per line, for shell capture: `TXN=$(flute2 --output quiet transactions create …)`

## Profiles and environments

| Profile | Alias | API |
|---|---|---|
| `sandbox` | | `https://sandbox.api.flute.com` |
| `production` | `prod` | `https://api.flute.com` |

`sandbox` is the default. Any command run against `production` prints a warning banner to stderr.

Choose a profile in one of three ways, highest precedence first:

1. Flag: `--profile production`
2. Environment variable: `FLUTE2_PROFILE=production`
3. Config file default: `default_profile = "production"` in `~/.flute2/config.toml`

The OS keychain holds credentials for each profile separately. The config file holds only settings that are not secret, such as `default_profile` and `output`.

Every setting follows the same precedence: flag, then environment variable, then `config.toml`, then the built-in default.

## Command overview

| Group | Commands |
|---|---|
| `auth` | `login`, `logout`, `status`, `switch`, `token`: credentials and profiles |
| `transactions` | `create`, `capture`, `reversal`, `credit`, `tip-adjust`, `ach-hold`, `ach-release`, `share-receipt`, `calculate-amount`, `get`, `list`, `inspect`: card and ACH payments |
| `customers` | `create`, `get`, `list`, `update`, `delete`: customer records |
| `payment-methods` | `list`, `get`, `add-card`, `add-ach`, `update`, `delete`, `set-default`: stored cards and bank accounts |
| `payment-links` | `create`, `get`, `list`, `update`, `delete`, `share`: hosted payment links |
| `payment-sessions` | `create`, `get`, `cancel`: hosted checkout sessions |
| `terminals` | `list`, `status`: POS terminals |
| `pos` | `create` and `reversal` (each with a `--wait` long poll), `get`, `list`, `cancel`, `print-receipt`: POS transactions |
| `settlements` | `list`, `get`, `close`: settlement batches |
| `settings` | `payment-config`, `contact-info`, `autofill`, `update-autofill`: account settings |
| `api-keys` | `create`, `list`, `revoke`: API keys |
| `ping` | API health check |
| `version` | Print the CLI version and active profile |
| `update` | Update to the latest GitHub Release |
| `completion` | Print a shell completion script |

Run `flute2 <group> --help` for the flags on any command.

## Output and scripting

`--output json` wraps every successful response in the same envelope:

```json
{ "object": "<type>", "data": <api response>, "meta": { "environment": "sandbox", "correlation_id": "…" } }
```

Under `--output json`, errors are written to stdout as JSON too, with a non-zero exit:

```json
{ "kind": "api"|"transport"|"auth"|"decode"|"client", "message": "…", "status": 422, "correlation_id": "…" }
```

Parse stdout only. Data goes to stdout; tracing, the production banner and update notices go to stderr.

### Exit codes

| Code | Meaning |
|---|---|
| `0` | Success, including a declined payment |
| `1` | General failure: transport, decode, or a server 5xx |
| `2` | Authentication failure: 401, 403, or missing credentials |
| `3` | Bad input: a server 400 or 422, client-side validation, or a CLI usage error |
| `4` | Not found (404) |

## Logging and debugging

`flute2` writes logs to stderr and command output to stdout, with or without `--debug`.

The global `--debug` flag prints full HTTP request and response traces to stderr:

```sh
flute2 --debug transactions get <txn-id>
```

Traces mask card and bank account numbers to the last 4 digits and remove security codes, secrets and tokens. They still show amounts and request metadata, so don't keep `--debug` output from `production`.

`RUST_LOG` replaces the built-in filters and takes standard [`EnvFilter` syntax](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html):

```sh
RUST_LOG=flute_cli2=debug,reqwest=debug,hyper=debug flute2 ping
```

## Shell completions

`flute2` generates completions for `bash`, `zsh`, `fish`, `powershell` and `elvish`:

```sh
# Bash
flute2 completion bash > /etc/bash_completion.d/flute2

# Zsh (into the first directory on $fpath)
flute2 completion zsh > "${fpath[1]}/_flute2"

# Fish
flute2 completion fish > ~/.config/fish/completions/flute2.fish

# PowerShell
flute2 completion powershell | Out-String | Invoke-Expression

# Elvish
flute2 completion elvish > ~/.config/elvish/lib/flute2.elv
```

## Self-update

```sh
flute2 update
```

This downloads the latest GitHub Release and replaces the installed binary.

## For AI agents and MCP

[`agents.md`](agents.md) is the machine-readable contract. It covers the output format, the error schema, exit codes, which commands are safe to retry, and commands for common agent tasks.

## Coming from `flute` (v1)

`flute2` installs alongside `flute` and does not replace it. See [`docs/migrating-from-v1.md`](docs/migrating-from-v1.md).

## Development

```sh
# Everything CI runs
scripts/ci.sh

# Run all tests
cargo test

# Lint (zero warnings enforced)
cargo clippy --all-targets -- -D warnings

# Format check
cargo fmt --check
```

The codebase enforces `#![forbid(unsafe_code)]`. The live sandbox suite is in `tests/live/`; the module docs in `tests/live.rs` explain how to run it.

MIT licensed. See [`LICENSE`](LICENSE).
