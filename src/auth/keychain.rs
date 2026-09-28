//! Client credentials in the OS keychain, with an environment fallback.

use anyhow::{Context, Result, bail};
use keyring::Entry;
use serde::{Deserialize, Serialize};

/// Deliberately not `flute-cli`, the entry v1 uses. Two binaries must not
/// read each other's credentials, even where the values would be identical.
const SERVICE: &str = "flute-cli-v2";

#[derive(Debug, Serialize, Deserialize)]
struct StoredCreds {
    client_id: String,
    client_secret: String,
}

/// The one place an OS keychain handle is obtained — and therefore the one
/// place the seam can be enforced for every path at once.
///
/// **Fail closed.** Refusing here rather than in each caller means a path
/// added later cannot reach the store by forgetting a guard: the callers that
/// legitimately no-op under the seam say so themselves, and anything else
/// gets an error instead of a developer's credentials.
fn entry(profile: &str) -> Result<Entry> {
    if keychain_disabled() {
        bail!(
            "FLUTE2_NO_KEYCHAIN is set, so the OS keychain is not available to \
             this process. Unset it, or supply FLUTE2_CLIENT_ID and \
             FLUTE2_CLIENT_SECRET instead."
        );
    }
    Entry::new(SERVICE, profile).with_context(|| format!("keyring entry for profile {profile}"))
}

pub fn store_client_credentials(profile: &str, client_id: &str, client_secret: &str) -> Result<()> {
    let json = serde_json::to_string(&StoredCreds {
        client_id: client_id.to_string(),
        client_secret: client_secret.to_string(),
    })
    .context("serialising credentials")?;
    entry(profile)?.set_password(&json)?;
    Ok(())
}

pub fn load_client_credentials(profile: &str) -> Result<Option<(String, String)>> {
    match entry(profile)?.get_password() {
        Ok(json) => {
            let creds: StoredCreds =
                serde_json::from_str(&json).context("decoding credentials JSON")?;
            Ok(Some((creds.client_id, creds.client_secret)))
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Remove the stored credentials for `profile`.
///
/// Guarded by the same seam as the read path, and for a stronger reason:
/// `auth logout` is a keychain *write*, so a test that reaches the real store
/// deletes a developer's credentials rather than merely reading them. The
/// seam refuses rather than no-ops, because a removal that could not be
/// attempted is not a removal.
pub fn delete_client_credentials(profile: &str) -> Result<()> {
    delete_outcome(entry(profile)?.delete_credential())
}

/// Which delete failures are success.
///
/// Only "there was nothing there": that is the state `logout` exists to
/// reach, so arriving already in it is not a failure. **Everything else
/// propagates** — a locked keychain or a denied permission leaves the
/// credentials on the machine, and swallowing it prints "removed" over a
/// secret that is still stored.
///
/// Split out from the call above because it is the whole decision, and a
/// keyring error cannot otherwise be produced in a test.
fn delete_outcome(result: std::result::Result<(), keyring::Error>) -> Result<()> {
    match result {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// `Ok(None)` means "no environment credentials, use the keychain".
/// `Err` means one of the pair is set and the other is not.
///
/// A half-set pair is an error rather than absent: treating it as absent
/// would let a CI job with a typo'd secret name silently authenticate as
/// whoever last logged in on that machine.
pub(crate) fn creds_from_env() -> Result<Option<(String, String)>> {
    let id = non_empty("FLUTE2_CLIENT_ID");
    let secret = non_empty("FLUTE2_CLIENT_SECRET");
    match (id, secret) {
        (Some(id), Some(secret)) => Ok(Some((id, secret))),
        (Some(_), None) => bail!("FLUTE2_CLIENT_ID is set but FLUTE2_CLIENT_SECRET is not"),
        (None, Some(_)) => bail!("FLUTE2_CLIENT_SECRET is set but FLUTE2_CLIENT_ID is not"),
        (None, None) => Ok(None),
    }
}

fn non_empty(var: &str) -> Option<String> {
    std::env::var(var).ok().filter(|v| !v.is_empty())
}

/// Environment credentials win over the keychain — the supported path for CI
/// and agents.
///
/// `FLUTE2_NO_KEYCHAIN` skips the keychain entirely. Without that seam a "no
/// credentials" test is only valid on a machine nobody has logged in on, and
/// `cargo test` would read a developer's keychain and reach the network. It is
/// safe under production because it can only remove access, never redirect it.
pub fn load_with_env_fallback(profile: &str) -> Result<Option<(String, String)>> {
    if let Some(creds) = creds_from_env()? {
        return Ok(Some(creds));
    }
    if keychain_disabled() {
        return Ok(None);
    }
    load_client_credentials(profile)
}

/// Whether the OS keychain is off limits for this process.
///
/// One definition, consulted by every path that would touch the store, so a
/// new one cannot be added without it.
pub(crate) fn keychain_disabled() -> bool {
    std::env::var("FLUTE2_NO_KEYCHAIN").is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deleting a credential that was never stored is the outcome `logout`
    /// exists to produce, so it is success.
    #[test]
    fn deleting_a_credential_that_is_not_there_is_success() {
        assert!(delete_outcome(Err(keyring::Error::NoEntry)).is_ok());
        assert!(delete_outcome(Ok(())).is_ok());
    }

    /// **Every other failure must reach the caller.** A locked keychain, a
    /// denied permission or a platform fault leaves the credentials exactly
    /// where they were, and reporting "removed" tells a user their secret is
    /// gone when it is still on the machine.
    #[test]
    fn a_keychain_failure_on_delete_is_not_reported_as_removal() {
        let locked = keyring::Error::NoStorageAccess(Box::new(std::io::Error::other("locked")));
        assert!(delete_outcome(Err(locked)).is_err());
        assert!(delete_outcome(Err(keyring::Error::TooLong("service".into(), 8))).is_err());
    }

    /// The seam that keeps `cargo test` off the developer's keychain has to
    /// cover **delete** as well as load: `auth logout` is a keychain write,
    /// and a test that runs it without the guard reaches the real store.
    ///
    /// It refuses rather than returning success, so `logout` cannot report a
    /// removal that never reached a store.
    #[test]
    fn the_keychain_seam_covers_delete_as_well_as_load() {
        temp_env::with_var("FLUTE2_NO_KEYCHAIN", Some("1"), || {
            assert!(keychain_disabled());
            let err = delete_client_credentials("sandbox")
                .unwrap_err()
                .to_string();
            assert!(err.contains("FLUTE2_NO_KEYCHAIN"), "{err}");
        });
        temp_env::with_var("FLUTE2_NO_KEYCHAIN", None::<&str>, || {
            assert!(!keychain_disabled());
        });
    }

    /// Fail closed at the single point every path goes through, so a path
    /// added later cannot reach the store by forgetting its guard.
    #[test]
    fn the_seam_refuses_a_keychain_handle_rather_than_returning_one() {
        temp_env::with_var("FLUTE2_NO_KEYCHAIN", Some("1"), || {
            let err = entry("sandbox").unwrap_err().to_string();
            assert!(err.contains("FLUTE2_NO_KEYCHAIN"), "{err}");

            // `auth login` cannot silently pretend it stored a secret.
            assert!(store_client_credentials("sandbox", "id", "secret").is_err());
        });
    }

    /// Env wins over the keychain — the supported path for CI and agents.
    #[test]
    fn env_credentials_take_precedence() {
        temp_env::with_vars(
            [
                ("FLUTE2_CLIENT_ID", Some("env-id")),
                ("FLUTE2_CLIENT_SECRET", Some("env-secret")),
            ],
            || {
                let (id, secret) = creds_from_env().unwrap().unwrap();
                assert_eq!(id, "env-id");
                assert_eq!(secret, "env-secret");
            },
        );
    }

    /// A half-set pair is a misconfiguration, and it is reported: falling
    /// through to the keychain would let a CI job with a typo'd secret name
    /// authenticate as whoever last logged in on that machine.
    #[test]
    fn partial_env_credentials_are_an_error_not_a_fallthrough() {
        temp_env::with_vars(
            [
                ("FLUTE2_CLIENT_ID", Some("env-id")),
                ("FLUTE2_CLIENT_SECRET", None::<&str>),
            ],
            || {
                let err = creds_from_env().unwrap_err();
                assert!(err.to_string().contains("FLUTE2_CLIENT_SECRET"));
            },
        );
    }

    /// The mirror case: a secret with no id is equally a misconfiguration.
    #[test]
    fn a_secret_without_an_id_is_also_an_error() {
        temp_env::with_vars(
            [
                ("FLUTE2_CLIENT_ID", None::<&str>),
                ("FLUTE2_CLIENT_SECRET", Some("env-secret")),
            ],
            || {
                let err = creds_from_env().unwrap_err();
                assert!(err.to_string().contains("FLUTE2_CLIENT_ID"));
            },
        );
    }

    /// Neither set is not a misconfiguration; it just means "use the keychain".
    #[test]
    fn absent_env_credentials_fall_through_to_the_keychain() {
        temp_env::with_vars(
            [
                ("FLUTE2_CLIENT_ID", None::<&str>),
                ("FLUTE2_CLIENT_SECRET", None::<&str>),
            ],
            || assert!(creds_from_env().unwrap().is_none()),
        );
    }

    /// The `FLUTE_` names v1 reads are not read here; two binaries, two
    /// credential sets.
    #[test]
    fn v1_env_names_are_not_read() {
        temp_env::with_vars(
            [
                ("FLUTE_CLIENT_ID", Some("v1-id")),
                ("FLUTE_CLIENT_SECRET", Some("v1-secret")),
                ("FLUTE2_CLIENT_ID", None::<&str>),
                ("FLUTE2_CLIENT_SECRET", None::<&str>),
            ],
            || assert!(creds_from_env().unwrap().is_none()),
        );
    }

    /// The invariant is that it *differs* from the entry v1 uses, so two
    /// binaries cannot read each other's credentials. Assert the constant
    /// rather than trust it.
    #[test]
    fn the_keychain_service_differs_from_v1() {
        assert_eq!(SERVICE, "flute-cli-v2");
        assert_ne!(SERVICE, "flute-cli");
    }

    /// The seam that keeps `cargo test` hermetic: without it a "no
    /// credentials" test reads the developer's keychain and passes or fails
    /// for reasons unrelated to the code.
    #[test]
    fn the_no_keychain_seam_reports_no_credentials() {
        temp_env::with_vars(
            [
                ("FLUTE2_CLIENT_ID", None::<&str>),
                ("FLUTE2_CLIENT_SECRET", None::<&str>),
                ("FLUTE2_NO_KEYCHAIN", Some("1")),
            ],
            || assert!(load_with_env_fallback("sandbox").unwrap().is_none()),
        );
    }

    /// Env credentials still resolve with the keychain disabled — the seam
    /// removes one source, not both.
    #[test]
    fn the_no_keychain_seam_still_honours_env_credentials() {
        temp_env::with_vars(
            [
                ("FLUTE2_CLIENT_ID", Some("env-id")),
                ("FLUTE2_CLIENT_SECRET", Some("env-secret")),
                ("FLUTE2_NO_KEYCHAIN", Some("1")),
            ],
            || {
                let (id, _) = load_with_env_fallback("sandbox").unwrap().unwrap();
                assert_eq!(id, "env-id");
            },
        );
    }
}
