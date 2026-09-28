//! OAuth2 client-credentials token store with in-memory caching.

use crate::api::{ApiError, parse_error_body};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// The longest token lifetime the store will believe. A server's claim about
/// its own token becomes clock arithmetic here, and an absurd one is a
/// needless refresh at worst.
const MAX_TOKEN_TTL_SECS: u64 = 86_400;

#[derive(Clone)]
pub struct TokenStore {
    inner: Arc<Mutex<Option<CachedToken>>>,
    fetcher: Arc<dyn Fetcher + Send + Sync>,
}

#[derive(Debug, Clone)]
struct CachedToken {
    bearer: String,
    expires_at: Instant,
}

#[async_trait::async_trait]
pub trait Fetcher {
    async fn fetch(&self) -> anyhow::Result<(String, Duration)>;
}

impl TokenStore {
    pub fn new(fetcher: Arc<dyn Fetcher + Send + Sync>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
            fetcher,
        }
    }

    pub async fn bearer(&self) -> anyhow::Result<String> {
        let mut guard = self.inner.lock().await;
        if let Some(cached) = guard.as_ref() {
            // Refresh inside a safety margin rather than at expiry, so a
            // request issued just before the boundary does not race it.
            if cached.expires_at.saturating_duration_since(Instant::now()) > Duration::from_secs(60)
            {
                return Ok(cached.bearer.clone());
            }
        }
        let (bearer, ttl) = self.fetcher.fetch().await?;
        let now = Instant::now();
        *guard = Some(CachedToken {
            bearer: bearer.clone(),
            // A `Fetcher` may hand back any duration, and the clock it lands
            // on is finite; a lifetime it cannot represent expires at once, so
            // the next call fetches rather than trusting an unrepresentable one.
            expires_at: now.checked_add(ttl).unwrap_or(now),
        });
        Ok(bearer)
    }

    /// Drop the cached token so the next `bearer` call fetches a fresh one.
    ///
    /// A 401 can mean clock skew, a revocation, or a server restart rather
    /// than bad credentials, so one stale cache entry must not keep failing
    /// every request.
    pub async fn invalidate(&self) {
        let mut guard = self.inner.lock().await;
        *guard = None;
    }
}

/// The fetcher for a profile whose credentials did not resolve.
///
/// A client is built either way, so the absence is reported when a token is
/// first needed rather than before the command has run at all — every refusal
/// the CLI can make on its own comes first, and a wrong invocation on a
/// machine that has never logged in is reported as the wrong invocation.
pub struct MissingCredentials {
    profile: String,
}

impl MissingCredentials {
    pub fn new(profile: impl Into<String>) -> Self {
        Self {
            profile: profile.into(),
        }
    }
}

#[async_trait::async_trait]
impl Fetcher for MissingCredentials {
    async fn fetch(&self) -> anyhow::Result<(String, Duration)> {
        Err(ApiError::Auth(format!(
            "no credentials for [{}]; run `flute2 auth login`",
            self.profile
        ))
        .into())
    }
}

pub struct OAuth2Fetcher {
    oauth_url: String,
    client_id: String,
    client_secret: String,
    http: reqwest::Client,
}

impl OAuth2Fetcher {
    pub fn new(
        oauth_url: impl Into<String>,
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        http: reqwest::Client,
    ) -> Self {
        Self {
            oauth_url: oauth_url.into(),
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            http,
        }
    }
}

#[derive(serde::Deserialize)]
struct TokenResp {
    access_token: String,
    expires_in: u64,
}

#[async_trait::async_trait]
impl Fetcher for OAuth2Fetcher {
    async fn fetch(&self) -> anyhow::Result<(String, Duration)> {
        let resp = self
            .http
            .post(&self.oauth_url)
            .form(&[
                ("grant_type", "client_credentials"),
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                // v2 declares scope required, an enum of exactly this value.
                ("scope", "offline_access"),
            ])
            .send()
            .await
            .map_err(|e| {
                ApiError::Transport(format!(
                    "token request failed: {}",
                    crate::api::with_causes(e)
                ))
            })?;

        let status = resp.status();
        let www = resp
            .headers()
            .get(reqwest::header::WWW_AUTHENTICATE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let text = resp.text().await.map_err(|e| {
            ApiError::Transport(format!(
                "reading token response: {}",
                crate::api::with_causes(e)
            ))
        })?;

        // `error_for_status` would discard the body, and the body is where the
        // OpenIddict reason lives — without it `invalid_client` surfaces as a
        // bare status code.
        if !status.is_success() {
            return Err(parse_error_body(status.as_u16(), &text, www.as_deref()).into());
        }

        let parsed: TokenResp = serde_json::from_str(&text)
            .map_err(|e| ApiError::Decode(format!("decoding token response: {e}")))?;
        Ok((
            parsed.access_token,
            Duration::from_secs(parsed.expires_in.min(MAX_TOKEN_TTL_SECS)),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingFetcher {
        calls: AtomicUsize,
        ttl: Duration,
    }

    #[async_trait::async_trait]
    impl Fetcher for CountingFetcher {
        async fn fetch(&self) -> anyhow::Result<(String, Duration)> {
            let n = self.calls.fetch_add(1, Ordering::SeqCst);
            Ok((format!("token-{n}"), self.ttl))
        }
    }

    #[tokio::test]
    async fn caches_a_token_within_its_validity() {
        let f = Arc::new(CountingFetcher {
            calls: AtomicUsize::new(0),
            ttl: Duration::from_secs(3600),
        });
        let store = TokenStore::new(f.clone());
        assert_eq!(store.bearer().await.unwrap(), "token-0");
        assert_eq!(store.bearer().await.unwrap(), "token-0");
        assert_eq!(f.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn invalidate_forces_a_refetch() {
        let f = Arc::new(CountingFetcher {
            calls: AtomicUsize::new(0),
            ttl: Duration::from_secs(3600),
        });
        let store = TokenStore::new(f.clone());
        assert_eq!(store.bearer().await.unwrap(), "token-0");
        store.invalidate().await;
        assert_eq!(store.bearer().await.unwrap(), "token-1");
    }

    /// A server may claim any lifetime at all. The clock is monotonic and
    /// finite, so a lifetime it cannot represent expires on arrival and the
    /// next call fetches, rather than overflowing the clock or being trusted.
    #[tokio::test]
    async fn a_ttl_that_would_overflow_the_clock_expires_at_once() {
        let f = Arc::new(CountingFetcher {
            calls: AtomicUsize::new(0),
            ttl: Duration::MAX,
        });
        let store = TokenStore::new(f.clone());
        assert_eq!(store.bearer().await.unwrap(), "token-0");
        assert_eq!(store.bearer().await.unwrap(), "token-1");
        assert_eq!(f.calls.load(Ordering::SeqCst), 2);
    }

    /// `expires_in` is the server's claim about its own token, and the store
    /// turns it into clock arithmetic, so it is bounded on arrival.
    #[tokio::test]
    async fn an_absurd_expires_in_is_clamped() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/oauth2/token"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "access_token": "tok-xyz", "expires_in": u64::MAX, "token_type": "Bearer"})),
            )
            .mount(&server)
            .await;

        let fetcher = OAuth2Fetcher::new(
            format!("{}/oauth2/token", server.uri()),
            "id",
            "secret",
            reqwest::Client::new(),
        );
        let (bearer, ttl) = fetcher.fetch().await.unwrap();
        assert_eq!(bearer, "tok-xyz");
        assert!(ttl <= Duration::from_secs(MAX_TOKEN_TTL_SECS), "{ttl:?}");
    }

    #[tokio::test]
    async fn refreshes_inside_the_safety_margin() {
        let f = Arc::new(CountingFetcher {
            calls: AtomicUsize::new(0),
            ttl: Duration::from_secs(30),
        });
        let store = TokenStore::new(f.clone());
        assert_eq!(store.bearer().await.unwrap(), "token-0");
        assert_eq!(store.bearer().await.unwrap(), "token-1");
    }
}
