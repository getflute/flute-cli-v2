mod support;

#[test]
fn version_flag_prints_crate_version() {
    support::bin_without_credentials()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicates::str::contains(env!("CARGO_PKG_VERSION")));
}

/// The exit-code boundary is reachable without spawning a binary, which is
/// what makes every later exit-code assertion cheap.
#[tokio::test]
async fn main_entry_returns_success_for_version() {
    assert_eq!(
        flute_cli2::main_entry(["flute2", "--version"]).await,
        std::process::ExitCode::SUCCESS
    );
}
