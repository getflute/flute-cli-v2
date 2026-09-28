//! `auth login`, `logout`, `status`, `switch`, `token`.
//!
//! The group spans all three credential categories, which is why its verbs
//! are dispatched individually rather than as a block. `login`, `logout` and
//! `switch` manage credentials and must not have any resolved for them;
//! `status` tolerates their absence; `token` cannot exist without them.

use crate::api::{ApiClient, ApiError};
use crate::auth::keychain;
use crate::cli::output::{Envelope, OutputFormat};
use crate::config::{self, Profile};
use anyhow::Result;
use reqwest::Method;

/// Prompt for a client id and secret and store them in the OS keychain.
///
/// The secret goes through `rpassword`, so it never reaches the shell history
/// or the process table.
pub fn login(profile: &str) -> Result<()> {
    use std::io::{self, BufRead, Write};
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    print!("client_id for [{profile}]: ");
    stdout.flush()?;
    let mut id = String::new();
    stdin.lock().read_line(&mut id)?;
    let id = id.trim().to_string();

    let secret = rpassword::prompt_password(format!("client_secret for [{profile}]: "))?;
    let secret = secret.trim().to_string();

    if id.is_empty() || secret.is_empty() {
        anyhow::bail!("client_id and client_secret are both required");
    }

    keychain::store_client_credentials(profile, &id, &secret)?;
    println!("Stored credentials for profile [{profile}] in OS keychain.");
    Ok(())
}

/// Clear stored credentials for `profile`.
pub fn logout(profile: &str) -> Result<()> {
    keychain::delete_client_credentials(profile)?;
    println!("Credentials for profile [{profile}] removed from OS keychain.");
    Ok(())
}

/// Set `default_profile` in the config file.
///
/// v1 writes this key and never reads it, because its `--profile` flag
/// carries a `default_value` and so is never absent. Here the write changes
/// what the next command targets.
pub fn switch(new_profile: &str) -> Result<()> {
    validate_switch_target(new_profile)?;
    let mut cfg = config::load_or_default();
    cfg.default_profile = new_profile.to_string();
    config::save(&cfg)?;
    println!("Default profile set to [{new_profile}].");
    Ok(())
}

pub fn validate_switch_target(profile: &str) -> Result<()> {
    Profile::by_name(profile)
        .map(|_| ())
        .ok_or_else(|| anyhow::anyhow!("unknown profile: {profile}"))
}

/// Report the active profile and whether the stored credentials authenticate
/// **right now**.
///
/// The only command in the credentials-optional category. It is what a user
/// runs precisely because authentication is not working, so their *absence*
/// is an answer rather than a failure — but a lookup that fails is not an
/// answer, and reporting `authenticated: false` for one sends the user
/// looking at their account instead of their machine.
pub async fn status(profile: &Profile, output: OutputFormat) -> Result<()> {
    let creds = keychain::load_with_env_fallback(&profile.name)?;
    let client_id = creds.as_ref().map(|(id, _)| id.clone());

    let mut authenticated = false;
    if let Some(creds) = creds {
        // A client that cannot be built is a configuration fault, not an
        // unauthenticated answer. Only a server's refusal of the credentials
        // is reported as `false`; anything else says nothing about them.
        let api = ApiClient::new(profile, Some(creds))?;
        authenticated = match api.request(Method::GET, "/v2/ping", &[], None).await {
            Ok(_) => true,
            Err(
                ApiError::Auth(_)
                | ApiError::Api {
                    status: 401 | 403, ..
                },
            ) => false,
            Err(other) => return Err(other.into()),
        };
    }

    let data = serde_json::json!({
        "profile": profile.name,
        "api_base_url": profile.api_base_url,
        "authenticated": authenticated,
        "client_id": client_id,
    });

    match output {
        OutputFormat::Json => {
            let env = Envelope::new("auth_status", data, &profile.name, None, None);
            println!("{}", serde_json::to_string_pretty(&env)?);
        }
        OutputFormat::Quiet => println!("{}", client_id.as_deref().unwrap_or("")),
        OutputFormat::Table => {
            println!("Profile:       {}", profile.name);
            println!("API base:      {}", profile.api_base_url);
            println!("Authenticated: {authenticated}");
            println!("Client ID:     {}", client_id.as_deref().unwrap_or("—"));
        }
    }
    Ok(())
}

/// Print the current bearer token.
///
/// Not credentials-optional despite sitting in this group: the entire output
/// is a bearer, which cannot be obtained without credentials.
pub async fn token(api: &ApiClient) -> Result<()> {
    println!("{}", api.bearer().await?);
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn switch_validates_the_profile_name() {
        assert!(super::validate_switch_target("garbage").is_err());
        assert!(super::validate_switch_target("production").is_ok());
        assert!(super::validate_switch_target("prod").is_ok());
        assert!(super::validate_switch_target("sandbox").is_ok());
    }
}
