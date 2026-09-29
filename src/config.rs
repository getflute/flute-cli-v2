//! Profiles and `~/.flute2/config.toml`.
//!
//! Everything here is namespaced away from v1: two binaries on one machine
//! must not share mutable state, and one variable meaning two things is the
//! failure that avoids.

use std::path::PathBuf;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct Config {
    pub default_profile: String,
    pub output: String,
    pub auto_update_check: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_profile: "sandbox".into(),
            output: "table".into(),
            auto_update_check: true,
        }
    }
}

pub fn config_dir() -> PathBuf {
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".flute2")
}

/// The home directory, from the environment first.
///
/// `dirs` reads `HOME` on unix but never `USERPROFILE`, consulting the Windows
/// shell API instead — so on Windows a process that sets the variable, a test
/// harness above all, is answered with the real user's profile and writes a
/// config file into it.
fn home_dir() -> Option<PathBuf> {
    ["HOME", "USERPROFILE"]
        .into_iter()
        .find_map(|var| std::env::var_os(var).filter(|value| !value.is_empty()))
        .map(PathBuf::from)
        .or_else(dirs::home_dir)
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

/// A malformed or absent config is not an error: the CLI still has to work
/// for someone whose config file has a typo in it.
pub fn load_or_default() -> Config {
    match std::fs::read_to_string(config_path()) {
        Ok(text) => toml::from_str(&text).unwrap_or_default(),
        Err(_) => Config::default(),
    }
}

pub fn save(cfg: &Config) -> anyhow::Result<()> {
    std::fs::create_dir_all(config_dir())?;
    std::fs::write(config_path(), toml::to_string_pretty(cfg)?)?;
    Ok(())
}

/// The endpoints for one environment.
///
/// These four constants are fixed here and asserted by test rather than
/// derived from the vendored bundle, whose `servers` block puts the token
/// endpoint on the API host where it answers 404.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub name: String,
    pub api_base_url: String,
    pub oauth_url: String,
}

impl Profile {
    pub fn sandbox() -> Self {
        Self {
            name: "sandbox".into(),
            api_base_url: "https://sandbox.api.flute.com".into(),
            oauth_url: "https://sandbox.oauth.api.flute.com/oauth2/token".into(),
        }
    }

    pub fn production() -> Self {
        Self {
            name: "production".into(),
            api_base_url: "https://api.flute.com".into(),
            oauth_url: "https://oauth.api.flute.com/oauth2/token".into(),
        }
    }

    pub fn by_name(name: &str) -> Option<Self> {
        match name {
            "sandbox" => Some(Self::sandbox()),
            "production" | "prod" => Some(Self::production()),
            _ => None,
        }
    }

    pub fn is_production(&self) -> bool {
        self.name == "production"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_dir_is_namespaced_away_from_v1() {
        assert!(config_dir().ends_with(".flute2"));
    }

    /// The environment names the home directory, and `HOME` wins where both
    /// are set. A test harness that redirects a config write has only these
    /// variables to do it with, on every platform.
    #[test]
    fn the_home_directory_comes_from_home_then_userprofile() {
        temp_env::with_vars(
            [
                ("HOME", Some("/tmp/from-home")),
                ("USERPROFILE", Some("/tmp/from-userprofile")),
            ],
            || assert_eq!(config_dir(), PathBuf::from("/tmp/from-home/.flute2")),
        );
        temp_env::with_vars(
            [
                ("HOME", None::<&str>),
                ("USERPROFILE", Some("/tmp/from-userprofile")),
            ],
            || assert_eq!(config_dir(), PathBuf::from("/tmp/from-userprofile/.flute2")),
        );
    }

    #[test]
    fn by_name_resolves_aliases_and_rejects_unknown() {
        assert_eq!(Profile::by_name("prod").unwrap().name, "production");
        assert!(Profile::by_name("staging").is_none());
        assert!(!Profile::by_name("sandbox").unwrap().is_production());
    }

    /// A wrong constant here aims payment traffic at the wrong environment,
    /// and every request would still be perfectly well-formed. Nothing else
    /// in the suite would notice.
    #[test]
    fn hosts_are_exactly_these_four() {
        let s = Profile::by_name("sandbox").unwrap();
        assert_eq!(s.api_base_url, "https://sandbox.api.flute.com");
        assert_eq!(
            s.oauth_url,
            "https://sandbox.oauth.api.flute.com/oauth2/token"
        );

        let p = Profile::by_name("production").unwrap();
        assert_eq!(p.api_base_url, "https://api.flute.com");
        assert_eq!(p.oauth_url, "https://oauth.api.flute.com/oauth2/token");
    }

    /// The OAuth host is a separate hostname, not a path on the API host.
    #[test]
    fn oauth_host_is_not_the_api_host() {
        for name in ["sandbox", "production"] {
            let p = Profile::by_name(name).unwrap();
            assert!(
                !p.oauth_url.starts_with(&p.api_base_url),
                "{name}: oauth_url must not sit under api_base_url"
            );
        }
    }

    /// The config carries the three keys v1 uses, under the same names.
    /// `default_profile` is the one `auth switch` writes and `--profile`
    /// absence reads.
    #[test]
    fn config_round_trips_v1_key_names() {
        let cfg: Config = toml::from_str(
            "default_profile = \"production\"\noutput = \"json\"\nauto_update_check = false\n",
        )
        .unwrap();
        assert_eq!(cfg.default_profile, "production");
        assert_eq!(cfg.output, "json");
        assert!(!cfg.auto_update_check);
        let text = toml::to_string_pretty(&cfg).unwrap();
        assert!(text.contains("default_profile"));
    }

    #[test]
    fn default_config_is_sandbox_table_and_checks_for_updates() {
        let d = Config::default();
        assert_eq!(d.default_profile, "sandbox");
        assert_eq!(d.output, "table");
        assert!(d.auto_update_check);
    }
}
