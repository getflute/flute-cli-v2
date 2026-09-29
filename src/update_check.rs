//! Lightweight startup check that pings GitHub Releases at most once per
//! 24 h and reports whether a newer version of flute2 is available.
//!
//! Three opt-out paths so this never gets in the way:
//!   1. `auto_update_check = false` in `~/.flute2/config.toml`
//!   2. `FLUTE2_NO_UPDATE_CHECK` env var set to anything
//!   3. `CI` env var set (typical Actions/Buildkite/Jenkins indicator)
//!
//! Callers are also expected to bail when stderr isn't a TTY — there is no
//! point printing an update notice to a piped or redirected stream — but
//! `IsTerminal` is a per-call concern so that check lives at the call site.
//!
//! The cache file lives at `~/.flute2/update-check.json`. Anything we can't
//! parse, write, or fetch is treated as "no notice" rather than a hard
//! error: this code path is best-effort, not part of the user's task.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::config::{Config, config_dir};

const CACHE_FILE: &str = "update-check.json";
const CACHE_TTL_SECS: u64 = 24 * 60 * 60;
const NETWORK_TIMEOUT_SECS: u64 = 3;

#[derive(serde::Serialize, serde::Deserialize)]
struct Cache {
    checked_at_unix_secs: u64,
    /// The latest release seen by the check, newer than this binary or not,
    /// so a check that found no update still suppresses the next 24 h of
    /// network calls. `None` reads as no newer version.
    latest_version: Option<String>,
}

/// True if any of the documented opt-outs apply.
pub fn opt_out(cfg: &Config) -> bool {
    if !cfg.auto_update_check {
        return true;
    }
    if std::env::var_os("FLUTE2_NO_UPDATE_CHECK").is_some() {
        return true;
    }
    if std::env::var_os("CI").is_some() {
        return true;
    }
    false
}

/// Returns `Some(latest_version)` if a newer version exists, otherwise `None`.
/// Uses the on-disk cache when fresh; falls back to a network query (bounded
/// by `NETWORK_TIMEOUT_SECS`) when the cache is stale or unreadable.
pub async fn check_for_update() -> Option<String> {
    check_for_update_at(
        &cache_path(),
        now_unix(),
        crate::update::query_latest_silently(),
    )
    .await
}

/// Path- and query-injectable body of [`check_for_update`]. A query that
/// fails or times out writes nothing, so the next run asks again rather than
/// reading the failure as "on latest" for 24 h.
async fn check_for_update_at(
    path: &std::path::Path,
    now_secs: u64,
    query: impl std::future::Future<Output = Option<String>>,
) -> Option<String> {
    if let Some(cached) = read_fresh_cache_at(path, now_secs) {
        return cached.latest_version.filter(|v| is_newer_than_current(v));
    }

    let latest = tokio::time::timeout(Duration::from_secs(NETWORK_TIMEOUT_SECS), query)
        .await
        .ok()
        .flatten()?;

    let _ = write_cache_at(
        path,
        &Cache {
            checked_at_unix_secs: now_secs,
            latest_version: Some(latest.clone()),
        },
    );

    Some(latest).filter(|v| is_newer_than_current(v))
}

/// Returns true only if `v` is *strictly newer* than the compiled binary
/// version, compared as semver. A bare string `!=` check would (incorrectly)
/// say "update available" when a stale cache held an older version than the
/// running binary — e.g. cache "0.5.3" after the user upgraded to 0.5.4.
/// If either side fails to parse as semver, fall back to false so a parse
/// glitch can't trigger a spurious update prompt.
pub(crate) fn is_newer_than_current(v: &str) -> bool {
    let Ok(current) = env!("CARGO_PKG_VERSION").parse::<axoupdater::Version>() else {
        return false;
    };
    let Ok(candidate) = v.parse::<axoupdater::Version>() else {
        return false;
    };
    candidate > current
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn cache_path() -> std::path::PathBuf {
    config_dir().join(CACHE_FILE)
}

/// Path-injectable cache reader so tests don't have to touch `~/.flute2`.
/// Returns `None` if the file is missing, corrupt, or older than the TTL.
fn read_fresh_cache_at(path: &std::path::Path, now_secs: u64) -> Option<Cache> {
    let raw = std::fs::read_to_string(path).ok()?;
    let cache: Cache = serde_json::from_str(&raw).ok()?;
    let age = now_secs.saturating_sub(cache.checked_at_unix_secs);
    (age < CACHE_TTL_SECS).then_some(cache)
}

/// Path-injectable cache writer. Creates the parent directory if needed.
fn write_cache_at(path: &std::path::Path, c: &Cache) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string(c)?)
}

/// Compose the one-line notice we print when a newer version is available.
pub fn notice_for(version: &str) -> String {
    format!(
        "A newer version ({version}) of flute2 is available \u{2014} run `flute2 update` to install."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opt_out_respects_config_flag() {
        let cfg = Config {
            auto_update_check: false,
            ..Config::default()
        };
        assert!(opt_out(&cfg));
    }

    #[test]
    fn opt_out_respects_env_var() {
        temp_env::with_var("FLUTE2_NO_UPDATE_CHECK", Some("1"), || {
            let cfg = Config::default();
            assert!(opt_out(&cfg));
        });
    }

    #[test]
    fn opt_out_respects_ci_env_var() {
        temp_env::with_var("CI", Some("true"), || {
            let cfg = Config::default();
            assert!(opt_out(&cfg));
        });
    }

    #[test]
    fn opt_out_false_when_all_clear() {
        temp_env::with_vars(
            [
                ("FLUTE2_NO_UPDATE_CHECK", None::<&str>),
                ("CI", None::<&str>),
            ],
            || {
                let cfg = Config {
                    auto_update_check: true,
                    ..Config::default()
                };
                assert!(!opt_out(&cfg));
            },
        );
    }

    /// The notice must say "flute2", or a v2 user is told to run a binary
    /// that updates a different one.
    #[test]
    fn the_update_notice_names_the_v2_binary() {
        let n = notice_for("2.1.0");
        assert!(n.contains("flute2 update"), "{n}");
        assert!(n.contains("2.1.0"), "{n}");
    }

    #[test]
    fn current_version_is_not_considered_newer() {
        assert!(!is_newer_than_current(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn strictly_newer_semver_counts_as_newer() {
        assert!(is_newer_than_current("999.999.999"));
    }

    #[test]
    fn strictly_older_semver_is_not_newer() {
        // A cache written before the running binary was installed holds an
        // older version than the one in use. Comparison is `>` on semver, not
        // string inequality, so that cannot surface a notice.
        assert!(!is_newer_than_current("0.0.0"));
    }

    #[test]
    fn unparseable_candidate_returns_false() {
        // If the GitHub API ever returns something we can't parse as
        // semver, prefer no notice over a spurious one.
        assert!(!is_newer_than_current("not-a-version"));
        assert!(!is_newer_than_current(""));
    }

    #[test]
    fn cache_round_trip_returns_value_when_fresh() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("update-check.json");
        let now = 1_700_000_000;
        let written = Cache {
            checked_at_unix_secs: now,
            latest_version: Some("9.9.9".into()),
        };
        write_cache_at(&path, &written).unwrap();
        let read = read_fresh_cache_at(&path, now + 60).expect("cache should still be fresh");
        assert_eq!(read.latest_version.as_deref(), Some("9.9.9"));
    }

    #[test]
    fn cache_older_than_ttl_is_treated_as_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("update-check.json");
        let then = 1_700_000_000;
        write_cache_at(
            &path,
            &Cache {
                checked_at_unix_secs: then,
                latest_version: Some("9.9.9".into()),
            },
        )
        .unwrap();
        let now = then + CACHE_TTL_SECS + 1;
        assert!(
            read_fresh_cache_at(&path, now).is_none(),
            "cache older than TTL must force a re-check"
        );
    }

    #[test]
    fn cache_with_no_version_round_trips() {
        // A fresh cache with latest_version=None still suppresses the
        // network for 24 h and reads as no newer version.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("update-check.json");
        let now = 1_700_000_000;
        write_cache_at(
            &path,
            &Cache {
                checked_at_unix_secs: now,
                latest_version: None,
            },
        )
        .unwrap();
        let read = read_fresh_cache_at(&path, now + 60).expect("fresh");
        assert!(read.latest_version.is_none());
    }

    #[tokio::test]
    async fn a_failed_query_writes_no_cache() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("update-check.json");
        assert!(
            check_for_update_at(&path, 1_700_000_000, async { None })
                .await
                .is_none()
        );
        assert!(!path.exists(), "a failed query must not be cached");
    }

    #[tokio::test]
    async fn a_failed_query_leaves_the_stale_cache_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("update-check.json");
        let then = 1_700_000_000;
        let stale = Cache {
            checked_at_unix_secs: then,
            latest_version: Some("9.9.9".into()),
        };
        write_cache_at(&path, &stale).unwrap();
        let now = then + CACHE_TTL_SECS + 1;
        check_for_update_at(&path, now, async { None }).await;
        let raw: Cache = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(raw.checked_at_unix_secs, then);
    }

    #[tokio::test]
    async fn a_successful_query_is_cached() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("update-check.json");
        let now = 1_700_000_000;
        let latest = check_for_update_at(&path, now, async { Some("999.0.0".into()) }).await;
        assert_eq!(latest.as_deref(), Some("999.0.0"));
        let read = read_fresh_cache_at(&path, now).expect("cached");
        assert_eq!(read.latest_version.as_deref(), Some("999.0.0"));
    }

    #[test]
    fn corrupt_cache_is_ignored_rather_than_panicking() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("update-check.json");
        std::fs::write(&path, "not-json").unwrap();
        assert!(read_fresh_cache_at(&path, 1_700_000_000).is_none());
    }
}
