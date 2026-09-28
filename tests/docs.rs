//! The committed documents, checked where they state a fact something else
//! owns: a command path the binary parses, a release target the build
//! produces, an installer URL the binary prints. Prose is reviewed, not tested.

mod support;

/// **The documentation filenames are lowercase**, matching v1's.
///
/// Both documents are cross-linked from each other, and a link to
/// `readme.md` is broken on a case-sensitive filesystem by a file named
/// `README.md` — which is what most repositories have, so the wrong name is
/// the one a contributor reaches for.
///
/// **Read from the directory listing, never by probing a path.** macOS is
/// case-insensitive by default, so `metadata("README.md")` succeeds for a file
/// actually named `readme.md`, and a probe would pass on the machine most
/// likely to have introduced the problem.
#[test]
fn the_documentation_filenames_are_lowercase() {
    let names: Vec<String> = std::fs::read_dir(".")
        .expect("the package root")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    support::checks::documentation_filenames_are_lowercase(&names, &["readme.md", "agents.md"]);
}

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
    for file in ["agents.md", "readme.md"] {
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

/// **The manifest names the readme and the project page.**
///
/// Read as text rather than through `CARGO_PKG_*`, because an absent key
/// compiles to an empty string and an empty string is what a passing test
/// would then be asserting against. crates.io renders neither key when it is
/// missing, and the readme is the package's whole front page.
#[test]
fn the_manifest_names_the_readme_and_the_homepage() {
    let text = std::fs::read_to_string("Cargo.toml").expect("Cargo.toml");
    const EXPECTED: [&str; 3] = [
        "readme = \"readme.md\"",
        "homepage = \"https://github.com/getflute/flute-cli-v2\"",
        "description = \"Cross-platform CLI for the Flute payments platform (v2 API)\"",
    ];
    let missing: Vec<&str> = EXPECTED
        .iter()
        .copied()
        .filter(|needle| !text.contains(needle))
        .collect();
    assert!(
        missing.is_empty(),
        "Cargo.toml does not carry: {}",
        missing.join(", ")
    );
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

    // The `brew install` line names the tap the release publishes to, so a
    // formula the readme or the hint points at is one the release put there.
    let manifest: toml::Value = std::fs::read_to_string("dist-workspace.toml")
        .expect("dist-workspace.toml")
        .parse()
        .expect("dist-workspace.toml is valid TOML");
    let tap = manifest["dist"]["tap"].as_str().expect("dist.tap");
    let (owner, repo) = tap.split_once('/').expect("tap is owner/repo");
    let short = repo
        .strip_prefix("homebrew-")
        .expect("tap repo is homebrew-*");
    let brew = format!("brew install {owner}/{short}/flute2");
    for (document, text) in [
        ("the reinstall hint", hint.as_str()),
        ("readme.md", readme.as_str()),
    ] {
        assert!(text.contains(&brew), "{document} does not carry `{brew}`");
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
