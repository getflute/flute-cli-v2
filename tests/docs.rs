//! The committed documents, checked where they state a fact something else
//! owns: a command path the binary parses, a release target the build
//! produces, an installer URL the binary prints. Prose is reviewed, not tested.

mod support;

/// The contract must not name what the CLI cannot do.
///
/// Both documents describe commands, flags and envelope names in prose, and
/// prose drifts. The command tree is the oracle: every `flute2 …` invocation
/// spelled out in either file has to parse as far as its subcommand path.
#[test]
fn every_documented_command_path_exists() {
    let paths = documented_command_paths();
    assert!(
        paths.len() > 15,
        "only {} invocations were found in the documents",
        paths.len()
    );
    for path in &paths {
        let args: Vec<&str> = path.iter().map(String::as_str).collect();
        // `--help` on the path proves the subcommand resolves without running
        // it, so a documented example cannot charge anything from a test.
        support::bin_without_credentials()
            .args(&args)
            .arg("--help")
            .assert()
            .success();
    }
}

/// Every `flute2 …` invocation in the two documents, reduced to its subcommand
/// path.
///
/// Two spans, because the documents write invocations two ways. Inside prose an
/// invocation is backticked, and the closing backtick is the end of it — a
/// sentence continuing "…lists the account's processors" would otherwise read
/// as three more subcommands. Inside a fenced block it runs to the end of the
/// line.
///
/// Within a span, the path is the leading run of bare lowercase words: a flag,
/// a placeholder like `<id>`, or a pipe ends it. So
/// `flute2 --output json transactions list --all` and
/// `flute2 transactions get <id>` both reduce to what a tree walk produces.
fn documented_command_paths() -> std::collections::BTreeSet<Vec<String>> {
    const GLOBAL_FLAGS: [&str; 2] = ["--output", "--profile"];
    const GLOBAL_VALUES: [&str; 6] = ["json", "table", "quiet", "sandbox", "production", "prod"];

    let mut out = std::collections::BTreeSet::new();
    for file in ["agents.md", "readme.md", "docs/migrating-from-v1.md"] {
        let text = std::fs::read_to_string(file).unwrap_or_else(|_| panic!("{file}"));
        for line in text.lines() {
            let mut from = 0;
            while let Some(at) = line[from..].find("flute2 ") {
                let start = from + at;
                let after = start + "flute2 ".len();
                let backticked = start > 0 && line.as_bytes()[start - 1] == b'`';
                let end = if backticked {
                    line[after..].find('`').map_or(line.len(), |n| after + n)
                } else {
                    line.len()
                };
                from = after;

                let mut path: Vec<String> = Vec::new();
                for word in line[after..end].split_whitespace() {
                    if GLOBAL_FLAGS.contains(&word) {
                        continue;
                    }
                    // A global flag's value, before the path has started.
                    if path.is_empty() && GLOBAL_VALUES.contains(&word) {
                        continue;
                    }
                    if !word.chars().all(|c| c.is_ascii_lowercase() || c == '-')
                        || word.starts_with('-')
                    {
                        break;
                    }
                    path.push(word.to_string());
                }
                if !path.is_empty() {
                    out.insert(path);
                }
            }
        }
    }
    out
}

/// **The reinstall hint and the readme advertise the same install paths.**
///
/// `update` prints the hint when there is no install receipt and the readme
/// reaches everyone else. An install path in one and not the other sends a
/// user to something that does not exist, and the failure surfaces on the
/// machine of whoever read the wrong document.
#[test]
fn the_reinstall_hint_and_the_readme_name_one_installer() {
    let hint = flute_cli2::update::reinstall_hint();
    let readme = std::fs::read_to_string("readme.md").expect("readme.md");

    for installer in ["flute2-installer.sh", "flute2-installer.ps1"] {
        let url = hint
            .split_whitespace()
            .find(|word| word.ends_with(installer))
            .unwrap_or_else(|| panic!("the reinstall hint names no {installer} URL"));
        assert!(
            readme.contains(url),
            "readme.md does not install from {url}"
        );
    }

    // The release publishes the formula to no tap, so a `brew install` line in
    // either document names a formula nobody can resolve.
    for (document, text) in [
        ("the reinstall hint", hint.as_str()),
        ("readme.md", readme.as_str()),
    ] {
        assert!(
            !text.contains("brew install"),
            "{document} installs from a Homebrew tap"
        );
    }
}

/// **Every target the release builds is named in the readme.**
///
/// One direction only: each triple in `dist-workspace.toml` must appear in
/// `readme.md`. The triples are what a reader checks their machine against
/// before running an installer, so a built platform the readme omits is a
/// download nobody knows to ask for. `dist-workspace.toml` is the oracle
/// because it is what cargo-dist actually builds from.
#[test]
fn the_readme_names_every_release_target() {
    let manifest: toml::Value = std::fs::read_to_string("dist-workspace.toml")
        .expect("dist-workspace.toml")
        .parse()
        .expect("dist-workspace.toml is valid TOML");
    let targets = manifest["dist"]["targets"]
        .as_array()
        .expect("dist.targets is an array");
    assert!(!targets.is_empty(), "dist.targets is empty");

    let readme = std::fs::read_to_string("readme.md").expect("readme.md");
    for target in targets {
        let triple = target.as_str().expect("a target triple");
        assert!(
            readme.contains(triple),
            "readme.md does not name the release target {triple}"
        );
    }
}
