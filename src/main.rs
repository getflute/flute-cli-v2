#![forbid(unsafe_code)]

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    flute_cli2::main_entry(std::env::args_os()).await
}
