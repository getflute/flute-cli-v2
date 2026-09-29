//! Self-update, wrapping `axoupdater`.
//!
//! Two callers: the `update` subcommand, which installs, and the startup
//! check, which only queries.
//!
//! Users who installed through a cargo-dist installer have an install receipt
//! that tells `axoupdater` which release source and installer to use. A
//! build from source has none, so the version *check* falls back to an
//! explicit GitHub Releases source and the install itself reports how to
//! reinstall rather than failing cryptically.

use anyhow::Result;
use axoupdater::{AxoUpdater, ReleaseSource, ReleaseSourceType};

/// These three identify **v2**, and getting one wrong is not a wrong result
/// but a wrong target: `flute2 update` would query the v1 project's releases
/// and, on an installer-managed machine, hand the v1 installer a v2 update.
/// Every request would be perfectly well-formed, so only an asserted
/// constant catches it.
pub const APP_NAME: &str = "flute2";
pub const REPO_OWNER: &str = "getflute";
pub const REPO_NAME: &str = "flute-cli-v2";

/// The latest release's version, newer or not. Every error maps to `None` so
/// the silent startup check can never break the foreground command.
pub async fn query_latest_silently() -> Option<String> {
    let (mut updater, _) = make_updater();
    let latest = updater.query_new_version().await.ok().flatten()?;
    Some(latest.to_string())
}

fn make_updater() -> (AxoUpdater, bool) {
    let mut updater = AxoUpdater::new_for(APP_NAME);
    let has_receipt = updater.load_receipt().is_ok();
    if !has_receipt {
        updater.set_release_source(ReleaseSource {
            release_type: ReleaseSourceType::GitHub,
            owner: REPO_OWNER.into(),
            name: REPO_NAME.into(),
            app_name: APP_NAME.into(),
        });
        if let Ok(v) = env!("CARGO_PKG_VERSION").parse() {
            let _ = updater.set_current_version(v);
        }
    }
    if let Ok(token) = std::env::var("FLUTE2_GITHUB_TOKEN") {
        updater.set_github_token(&token);
    }
    (updater, has_receipt)
}

/// How to reinstall when there is no receipt to update through.
pub fn reinstall_hint() -> String {
    format!(
        "Reinstall using one of:\n  \
         curl -LsSf https://github.com/{REPO_OWNER}/{REPO_NAME}/releases/latest/download/{APP_NAME}-installer.sh | sh\n  \
         irm https://github.com/{REPO_OWNER}/{REPO_NAME}/releases/latest/download/{APP_NAME}-installer.ps1 | iex"
    )
}

/// What `update` says without a receipt, given the latest release. Only a
/// release strictly newer than this binary is offered, so a source build
/// ahead of the latest release is not told to install an older one.
fn no_receipt_notice(latest: &str) -> String {
    if crate::update_check::is_newer_than_current(latest) {
        format!(
            "A newer version ({latest}) is available, but this binary was not \
             installed via a cargo-dist installer, so `update` cannot \
             replace it in place.\n{}",
            reinstall_hint()
        )
    } else {
        format!(
            "Already on the latest version ({}).",
            env!("CARGO_PKG_VERSION")
        )
    }
}

/// "Already on latest" and "no receipt" are informational, not failures.
pub async fn run() -> Result<()> {
    let (mut updater, has_receipt) = make_updater();

    if !has_receipt {
        let latest = updater.query_new_version().await.map_err(|e| {
            crate::api::ApiError::Transport(format!(
                "failed to query GitHub Releases for the latest version: {}",
                crate::api::with_causes(e)
            ))
        })?;
        match latest {
            Some(v) => println!("{}", no_receipt_notice(&v.to_string())),
            None => println!(
                "Already on the latest version ({}).",
                env!("CARGO_PKG_VERSION")
            ),
        }
        return Ok(());
    }

    println!("Checking for updates\u{2026}");
    let updated = updater
        .run()
        .await
        .map_err(|e| anyhow::anyhow!(crate::api::with_causes(e)))?;
    match updated {
        Some(result) => println!("Updated to {}.", result.new_version),
        None => println!(
            "Already on the latest version ({}).",
            env!("CARGO_PKG_VERSION")
        ),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A wrong constant here is a well-formed request to the wrong project:
    /// `flute2 update` would read the v1 project's releases and, on an
    /// installer-managed machine, hand the v1 installer a v2 update. Nothing
    /// else in the suite would see it.
    #[test]
    fn update_targets_v2_everywhere_and_never_v1() {
        assert_eq!(APP_NAME, "flute2");
        assert_eq!(REPO_OWNER, "getflute");
        assert_eq!(REPO_NAME, "flute-cli-v2");

        let hint = reinstall_hint();
        assert!(hint.contains("flute2-installer.sh"));
        assert!(hint.contains("flute2-installer.ps1"));

        // `flute2` contains `flute`, so the check is for the v1 names
        // specifically rather than for the substring.
        for s in [hint.as_str(), REPO_NAME, APP_NAME] {
            assert!(!s.contains("flute-installer"), "{s}: v1 installer");
        }
        assert_ne!(REPO_NAME, "flute-cli");
    }

    #[test]
    fn a_newer_release_is_offered_without_a_receipt() {
        assert!(no_receipt_notice("999.0.0").contains("A newer version (999.0.0)"));
    }

    #[test]
    fn an_older_release_is_not_offered_without_a_receipt() {
        let n = no_receipt_notice("0.0.1");
        assert!(n.starts_with("Already on the latest version"), "{n}");
    }
}
