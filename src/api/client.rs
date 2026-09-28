//! One HTTP client, built once and injected. The seam the test strategy needs.
//!
//! v1 resolved credentials inside every dispatch arm, so no test could reach a
//! request body through argv. Here `run()` builds one client and hands it
//! down, which is what makes a mock server reachable from the compiled binary.

use crate::api::error::{ApiError, parse_error_body};
use crate::auth::token::{Fetcher, MissingCredentials, OAuth2Fetcher, TokenStore};
use crate::config::Profile;
use reqwest::header::ACCEPT;
use reqwest::{Method, StatusCode};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info};

const JSON: &str = "application/json";

const USER_AGENT: &str = concat!("flute2/", env!("CARGO_PKG_VERSION"));

/// A request that has not been answered in this long is not going to be, and
/// a payment command that hangs is worse than one that fails.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// The one client this crate builds, for the API and the token exchange
/// alike.
///
/// A builder failure reaches the caller rather than yielding a client with no
/// timeout at all — the silent fallback is the one outcome a timeout exists
/// to rule out. Redirects are not followed: a 3xx is an instruction to send
/// credentials to a host the caller never configured. The user agent names
/// the version, so a report about one build is actionable in the API's logs.
fn http_client(timeout: Duration) -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none())
        .build()
}

/// A response, faithfully. `body` is `None` for a bodyless success, which is a
/// documented outcome for thirteen of the fifty operations and not a decode
/// error — reporting it as one inside transport puts it where no renderer can
/// rescue it.
#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub body: Option<Value>,
    pub correlation_id: Option<String>,
}

/// A request path built from literals and checked identifiers only.
///
/// `request` takes nothing else, so an identifier cannot reach the request
/// line without passing `id`: a `format!`ed path does not convert.
///
/// ```
/// use flute_cli2::api::ApiPath;
/// let path = ApiPath::from("/v2/customers").id("cus_1").unwrap();
/// assert_eq!(path.as_str(), "/v2/customers/cus_1");
/// ```
///
/// ```compile_fail
/// use flute_cli2::api::ApiPath;
/// let id = "cus_1";
/// let path: ApiPath = format!("/v2/customers/{id}").as_str().into();
/// ```
#[derive(Debug, Clone)]
pub struct ApiPath(String);

impl From<&'static str> for ApiPath {
    fn from(literal: &'static str) -> Self {
        Self(literal.to_string())
    }
}

impl ApiPath {
    /// Append `value` as one path segment.
    ///
    /// A path segment ends at `/`, `?` or `#`, so an identifier carrying one
    /// rewrites the request line into an operation the caller never named —
    /// and an empty one addresses the collection rather than a member, which
    /// turns a delete of one resource into a request against all of them. The
    /// URL parser reads `\` as a separator as well, so it ends a segment here
    /// too. Whitespace is refused with them: it survives percent-encoding into
    /// an id no resource has. So is `%`, which an identifier has no legitimate
    /// use for and which spells every one of the others as an escape the URL
    /// parser then decodes.
    ///
    /// `.` and `..` are refused for the same reason by a different route: URL
    /// parsing removes dot segments, so they resolve to the collection and to
    /// the path above it rather than to any resource.
    pub fn id(mut self, value: &str) -> Result<Self, ApiError> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(ApiError::Client(
                "an identifier is required and this one is empty".to_string(),
            ));
        }
        if matches!(trimmed, "." | "..") {
            return Err(ApiError::Client(format!(
                "{value:?} is a path segment rather than an identifier: URL parsing \
                 resolves it away, so the request would address another resource"
            )));
        }
        if value
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '/' | '\\' | '?' | '#' | '%'))
        {
            return Err(ApiError::Client(format!(
                "an identifier cannot contain '/', '\\', '?', '#', '%' or whitespace: {value:?}"
            )));
        }
        self.0.push('/');
        self.0.push_str(value);
        Ok(self)
    }

    /// Append a literal segment, such as the verb after an identifier.
    pub fn seg(mut self, literal: &'static str) -> Self {
        self.0.push('/');
        self.0.push_str(literal);
        self
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone)]
pub struct ApiClient {
    base_url: String,
    http: reqwest::Client,
    tokens: TokenStore,
}

/// The API base URL for `profile`, honouring `FLUTE2_API_BASE_URL`.
///
/// Refused under production: a test hook able to silently redirect production
/// payment traffic is a liability, so combining them is a hard error.
pub fn resolve_base_url(profile: &Profile) -> anyhow::Result<String> {
    resolve(profile, "FLUTE2_API_BASE_URL", &profile.api_base_url)
}

/// The token URL for `profile`, honouring `FLUTE2_OAUTH_URL`, with the same
/// production refusal.
pub fn resolve_oauth_url(profile: &Profile) -> anyhow::Result<String> {
    resolve(profile, "FLUTE2_OAUTH_URL", &profile.oauth_url)
}

fn resolve(profile: &Profile, var: &str, fallback: &str) -> anyhow::Result<String> {
    match std::env::var(var).ok().filter(|v| !v.is_empty()) {
        Some(_) if profile.is_production() => anyhow::bail!(
            "{var} must not be combined with the production profile: a test hook \
             that redirects production payment traffic is a liability"
        ),
        Some(override_url) => Ok(override_url),
        None => Ok(fallback.to_string()),
    }
}

/// Classify a failed token exchange.
///
/// Only the token endpoint refusing the credentials is an authorisation
/// failure an operator has to go and fix. A request that never reached it is
/// a transport failure, a 5xx or a 429 from it is a server answer to retry
/// with backoff, and a success it cannot read is a decode failure: none of
/// those says anything about the credentials, so each keeps its own kind.
fn token_failure(err: anyhow::Error) -> ApiError {
    match err.downcast::<ApiError>() {
        Ok(
            refused @ ApiError::Api {
                status: 400 | 401 | 403,
                ..
            },
        ) => ApiError::Auth(refused.to_string()),
        Ok(classified) => classified,
        Err(other) => ApiError::Auth(other.to_string()),
    }
}

impl ApiClient {
    /// Resolve `profile`'s endpoints and build a client for them.
    ///
    /// Fallible: resolution reads the environment and refuses an override
    /// under production, so the refusal has to be able to reach the caller.
    ///
    /// `creds` is `None` when none resolved. The client is built regardless
    /// and the refusal lands on the first token fetch, so a command that
    /// refuses its own arguments reports that rather than a missing login.
    pub fn new(profile: &Profile, creds: Option<(String, String)>) -> anyhow::Result<Self> {
        let base_url = resolve_base_url(profile)?;
        let oauth_url = resolve_oauth_url(profile)?;
        match creds {
            Some(creds) => Self::from_endpoints(base_url, oauth_url, creds),
            None => Ok(Self::with_fetcher(
                base_url,
                http_client(REQUEST_TIMEOUT)?,
                Arc::new(MissingCredentials::new(&profile.name)),
            )),
        }
    }

    /// Build from already-resolved endpoints.
    ///
    /// This carries no `Profile`, so there is no production context for it to
    /// subvert — the refusal lives in `new`, which is the only path `run()`
    /// uses. It exists so an in-process test can aim `dispatch` at a mock
    /// server without mutating the process environment.
    pub fn from_endpoints(
        base_url: String,
        oauth_url: String,
        creds: (String, String),
    ) -> anyhow::Result<Self> {
        let http = http_client(REQUEST_TIMEOUT)?;
        let (client_id, client_secret) = creds;
        let fetcher = OAuth2Fetcher::new(oauth_url, client_id, client_secret, http.clone());
        Ok(Self::with_fetcher(base_url, http, Arc::new(fetcher)))
    }

    fn with_fetcher(
        base_url: String,
        http: reqwest::Client,
        fetcher: Arc<dyn Fetcher + Send + Sync>,
    ) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http,
            tokens: TokenStore::new(fetcher),
        }
    }

    /// The current bearer, fetching one if the cache is cold.
    ///
    /// `auth token` is the only caller that wants the token rather than a
    /// response, so it goes through here rather than reaching into the store.
    pub async fn bearer(&self) -> Result<String, ApiError> {
        self.tokens.bearer().await.map_err(token_failure)
    }

    /// Issue one request, with one reactive retry on 401.
    pub async fn request(
        &self,
        method: Method,
        path: impl Into<ApiPath>,
        query: &[(&str, String)],
        body: Option<Value>,
    ) -> Result<Response, ApiError> {
        self.request_within(method, path, query, body, None).await
    }

    /// The same, bounded by `timeout` rather than by the client-wide one.
    ///
    /// A long poll is held open server-side for as long as the caller asked to
    /// wait, which is more than the shared bound allows — so a request that
    /// names its own governs itself.
    pub async fn request_within(
        &self,
        method: Method,
        path: impl Into<ApiPath>,
        query: &[(&str, String)],
        body: Option<Value>,
        timeout: Option<Duration>,
    ) -> Result<Response, ApiError> {
        let url = format!("{}{}", self.base_url, path.into().as_str());
        let traced = traced_url(&url, query);
        debug!(
            method = %method, url = %traced,
            body = %body.as_ref().map(|b| crate::api::redact::redact(&b.to_string()))
                .unwrap_or_default(),
            "HTTP request"
        );

        let first = self
            .issue(&method, &url, query, body.as_ref(), timeout)
            .await?;
        trace_response(&method, &traced, &first);
        let outcome = if first.status == StatusCode::UNAUTHORIZED.as_u16() {
            // A 401 can be clock skew, a revocation, or a server restart
            // rather than bad credentials, so the cached token is dropped and
            // the request retried exactly once.
            info!("HTTP 401 — invalidating cached token and retrying once");
            self.tokens.invalidate().await;
            let retry = self
                .issue(&method, &url, query, body.as_ref(), timeout)
                .await?;
            trace_response(&method, &traced, &retry);
            retry
        } else {
            first
        };

        if (200..300).contains(&outcome.status) {
            let body = if outcome.text.trim().is_empty() {
                None
            } else {
                match serde_json::from_str::<Value>(&outcome.text) {
                    Ok(v) => Some(v),
                    Err(e) => return Err(ApiError::Decode(e.to_string())),
                }
            };
            Ok(Response {
                status: outcome.status,
                body,
                correlation_id: outcome.correlation_id,
            })
        } else {
            // The header is the only identifier a bodyless failure has.
            Err(parse_error_body(
                outcome.status,
                &outcome.text,
                outcome.www_authenticate.as_deref(),
            )
            .or_correlation_id(outcome.correlation_id))
        }
    }

    async fn issue(
        &self,
        method: &Method,
        url: &str,
        query: &[(&str, String)],
        body: Option<&Value>,
        timeout: Option<Duration>,
    ) -> Result<Raw, ApiError> {
        let token = self.tokens.bearer().await.map_err(token_failure)?;

        let mut req = self
            .http
            .request(method.clone(), url)
            .bearer_auth(token)
            // Pin content negotiation to JSON so the server never selects a
            // different format handler.
            .header(ACCEPT, JSON);
        if !query.is_empty() {
            req = req.query(query);
        }
        match (body, method) {
            (Some(b), _) => req = req.json(b),
            // A bodyless POST/PATCH still sends an empty frame, and the frame
            // needs both headers: without `Content-Length: 0` the API rejects
            // it, and without a `Content-Type` it answers 415 Unsupported
            // Media Type. Six operations send exactly this — capture and
            // reversal without an amount, ach-hold, ach-release, set-default,
            // and the two cancels.
            (None, m) if matches!(*m, Method::POST | Method::PUT | Method::PATCH) => {
                req = req
                    .body("")
                    .header(reqwest::header::CONTENT_LENGTH, "0")
                    .header(reqwest::header::CONTENT_TYPE, JSON);
            }
            (None, _) => {}
        }
        if let Some(bound) = timeout {
            req = req.timeout(bound);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| ApiError::Transport(crate::api::with_causes(e)))?;
        let status = resp.status().as_u16();
        let correlation_id = header(&resp, "x-correlation-id");
        let www_authenticate = header(&resp, "www-authenticate");
        let text = resp
            .text()
            .await
            .map_err(|e| ApiError::Transport(crate::api::with_causes(e)))?;
        Ok(Raw {
            status,
            text,
            correlation_id,
            www_authenticate,
        })
    }
}

/// The URL as traced, query included: what a filter or sort flag put on the
/// wire is the question a request trace is opened to answer.
///
/// Only the query is redacted, and through `redact` rather than
/// `redact_message`. A filter value is caller-supplied, and `name=value` is a
/// shape `redact` recognises, so a secret is caught by its key where a
/// digit-run rule would pass it through. The rest of the URL is left out of
/// that pass because text carrying no pair is suppressed whole, which would
/// cost the trace the endpoint it is about.
fn traced_url(url: &str, query: &[(&str, String)]) -> String {
    if query.is_empty() {
        return url.to_string();
    }
    let encoded = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(query.iter().map(|(k, v)| (k, v)))
        .finish();
    format!("{url}?{}", crate::api::redact::redact(&encoded))
}

/// Every response is traced where it arrives, so a retried request shows the
/// 401 that caused the retry as well as its outcome.
fn trace_response(method: &Method, url: &str, raw: &Raw) {
    debug!(
        method = %method, url = %url, status = raw.status,
        body = %crate::api::redact::redact(&raw.text),
        "HTTP response"
    );
}

struct Raw {
    status: u16,
    text: String,
    correlation_id: Option<String>,
    www_authenticate: Option<String>,
}

fn header(resp: &reqwest::Response, name: &str) -> Option<String> {
    resp.headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Profile;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

    #[test]
    fn an_identifier_that_would_rewrite_the_path_is_refused() {
        for bad in [
            "",
            "   ",
            "a/b",
            "a?b",
            "a#b",
            "a b",
            "a\\b",
            "..\\x",
            ".\\x",
            "a\tb",
            "a\nb",
            ".",
            "..",
            " . ",
            " .. ",
            "%2e%2e",
            "abc%2Fdef",
        ] {
            assert!(
                matches!(
                    ApiPath::from("/v2/customers").id(bad),
                    Err(ApiError::Client(_))
                ),
                "{bad:?} was accepted"
            );
        }
    }

    #[test]
    fn an_ordinary_identifier_is_appended_as_one_segment() {
        let path = ApiPath::from("/v2/customers").id("cus_1").unwrap();
        assert_eq!(path.as_str(), "/v2/customers/cus_1");
        let path = ApiPath::from("/v2/customers")
            .id("7f3c1f9e-0b1a-4a5e-9f2d-6b8c1d2e3f40")
            .unwrap();
        assert_eq!(
            path.as_str(),
            "/v2/customers/7f3c1f9e-0b1a-4a5e-9f2d-6b8c1d2e3f40"
        );
    }

    #[test]
    fn a_literal_segment_follows_the_identifier() {
        let path = ApiPath::from("/v2/pos/transactions")
            .id("p1")
            .unwrap()
            .seg("cancel");
        assert_eq!(path.as_str(), "/v2/pos/transactions/p1/cancel");
    }

    /// A test hook able to silently redirect production payment traffic is a
    /// liability, so combining them is a hard error rather than a warning.
    #[test]
    fn base_url_override_is_refused_on_production() {
        temp_env::with_var("FLUTE2_API_BASE_URL", Some("http://127.0.0.1:1"), || {
            let prod = Profile::by_name("production").unwrap();
            let err = resolve_base_url(&prod).unwrap_err();
            assert!(err.to_string().contains("production"));
        });
    }

    #[test]
    fn base_url_override_applies_on_sandbox() {
        temp_env::with_var("FLUTE2_API_BASE_URL", Some("http://127.0.0.1:9"), || {
            let sandbox = Profile::by_name("sandbox").unwrap();
            assert_eq!(resolve_base_url(&sandbox).unwrap(), "http://127.0.0.1:9");
        });
    }

    /// The OAuth override carries the same risk and gets the same refusal.
    /// Covering only the API override would leave a hook able to point
    /// production credentials at an attacker-controlled token endpoint.
    #[test]
    fn oauth_url_override_is_refused_on_production() {
        temp_env::with_var("FLUTE2_OAUTH_URL", Some("http://127.0.0.1:1/t"), || {
            let prod = Profile::by_name("production").unwrap();
            let err = resolve_oauth_url(&prod).unwrap_err();
            assert!(err.to_string().contains("production"));
        });
    }

    #[test]
    fn oauth_url_override_applies_on_sandbox() {
        temp_env::with_var("FLUTE2_OAUTH_URL", Some("http://127.0.0.1:9/t"), || {
            let sandbox = Profile::by_name("sandbox").unwrap();
            assert_eq!(resolve_oauth_url(&sandbox).unwrap(), "http://127.0.0.1:9/t");
        });
    }

    #[test]
    fn without_an_override_the_profile_constants_are_used() {
        temp_env::with_vars(
            [
                ("FLUTE2_API_BASE_URL", None::<&str>),
                ("FLUTE2_OAUTH_URL", None::<&str>),
            ],
            || {
                let p = Profile::by_name("production").unwrap();
                assert_eq!(resolve_base_url(&p).unwrap(), "https://api.flute.com");
                assert_eq!(
                    resolve_oauth_url(&p).unwrap(),
                    "https://oauth.api.flute.com/oauth2/token"
                );
            },
        );
    }

    fn token_ok(server: &MockServer) -> Mock {
        let _ = server;
        Mock::given(method("POST"))
            .and(path("/oauth2/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "tok-xyz", "expires_in": 3600, "token_type": "Bearer"})))
    }

    fn client_for(server: &MockServer) -> ApiClient {
        ApiClient::from_endpoints(
            server.uri(),
            format!("{}/oauth2/token", server.uri()),
            ("test-id".into(), "test-secret".into()),
        )
        .unwrap()
    }

    /// **The timeout is applied to the request, not merely configured.** A
    /// builder failure is an error rather than a client with no timeout at
    /// all, so a hung payment command cannot wait for the server forever.
    ///
    /// The bound is exercised through a short one: asserting the 30 s value
    /// by waiting for it would put half a minute in every run.
    #[tokio::test]
    async fn the_configured_timeout_bounds_a_request() {
        assert_eq!(REQUEST_TIMEOUT, Duration::from_secs(30));

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/slow"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(5)))
            .mount(&server)
            .await;

        let err = http_client(Duration::from_millis(200))
            .unwrap()
            .get(format!("{}/slow", server.uri()))
            .send()
            .await
            .unwrap_err();
        assert!(err.is_timeout(), "{err}");
    }

    /// **A request may carry a bound of its own.** The long poll is held open
    /// server-side for as long as the caller asked to wait, which is more than
    /// the shared bound allows — so the bound it names governs that request.
    ///
    /// Asserted through a short one for the same reason as the shared bound:
    /// waiting out a long one would put the wait in every run.
    #[tokio::test]
    async fn a_requests_own_timeout_governs_it() {
        let server = MockServer::start().await;
        token_ok(&server).mount(&server).await;
        Mock::given(method("GET"))
            .and(path("/v2/ping"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(5)))
            .mount(&server)
            .await;

        let err = client_for(&server)
            .request_within(
                reqwest::Method::GET,
                "/v2/ping",
                &[],
                None,
                Some(Duration::from_millis(200)),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ApiError::Transport(_)), "{err:?}");
    }

    /// The token request is asserted as an exact multiset with an exact
    /// content type. `body_string_contains` would accept a duplicated
    /// parameter, an extra one, or a JSON body claiming to be a form.
    #[tokio::test]
    async fn the_token_request_is_exactly_four_form_parameters() {
        let server = MockServer::start().await;
        token_ok(&server).mount(&server).await;
        Mock::given(method("GET"))
            .and(path("/v2/ping"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        client_for(&server)
            .request(reqwest::Method::GET, "/v2/ping", &[], None)
            .await
            .unwrap();

        let reqs = server.received_requests().await.unwrap();
        let tok = reqs
            .iter()
            .find(|r| r.url.path() == "/oauth2/token")
            .expect("no token request was sent");

        assert_eq!(
            tok.headers
                .get("content-type")
                .map(|v| v.to_str().unwrap().to_string()),
            Some("application/x-www-form-urlencoded".to_string())
        );

        let mut observed: Vec<(String, String)> = url::form_urlencoded::parse(&tok.body)
            .into_owned()
            .collect();
        observed.sort();
        let mut expected: Vec<(String, String)> = [
            ("client_id", "test-id"),
            ("client_secret", "test-secret"),
            ("grant_type", "client_credentials"),
            // v2 declares scope required, an enum of exactly this value. v1
            // sends the first three only, so a transplant would be malformed.
            ("scope", "offline_access"),
        ]
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
        expected.sort();
        assert_eq!(observed, expected);
    }

    /// A token that never reaches the API request is a token nobody used.
    #[tokio::test]
    async fn the_bearer_reaches_the_api_request() {
        let server = MockServer::start().await;
        token_ok(&server).mount(&server).await;
        Mock::given(method("GET"))
            .and(path("/v2/ping"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        client_for(&server)
            .request(reqwest::Method::GET, "/v2/ping", &[], None)
            .await
            .unwrap();

        let reqs = server.received_requests().await.unwrap();
        let api = reqs
            .iter()
            .find(|r| r.url.path() == "/v2/ping")
            .expect("no API request was sent");
        assert_eq!(
            api.headers
                .get("authorization")
                .map(|v| v.to_str().unwrap().to_string()),
            Some("Bearer tok-xyz".to_string())
        );
    }

    /// v1 calls `.error_for_status()`, which discards the body, so the
    /// OpenIddict shape is never parsed and `invalid_client` surfaces as a
    /// bare status code.
    #[tokio::test]
    async fn an_openiddict_failure_is_parsed_not_swallowed() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth2/token"))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                "error": "invalid_client",
                "error_description": "Client authentication failed"})))
            .mount(&server)
            .await;

        let err = client_for(&server)
            .request(reqwest::Method::GET, "/v2/ping", &[], None)
            .await
            .unwrap_err();
        let ApiError::Auth(message) = err else {
            panic!("expected Auth, got {err:?}")
        };
        assert!(message.contains("invalid_client"), "{message}");
        assert!(
            message.contains("Client authentication failed"),
            "{message}"
        );
    }

    /// A deterministic 401-then-200 sequence: one retry, not zero and not a
    /// loop. `up_to_n_times` would depend on wiremock's match ordering.
    struct FailFirst {
        calls: AtomicUsize,
        first: u16,
    }

    impl Respond for FailFirst {
        fn respond(&self, _: &Request) -> ResponseTemplate {
            if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                ResponseTemplate::new(self.first)
            } else {
                ResponseTemplate::new(200).set_body_json(json!({"ok": true}))
            }
        }
    }

    #[tokio::test]
    async fn a_401_triggers_exactly_one_retry() {
        let server = MockServer::start().await;
        token_ok(&server).mount(&server).await;
        Mock::given(method("GET"))
            .and(path("/v2/ping"))
            .respond_with(FailFirst {
                calls: AtomicUsize::new(0),
                first: 401,
            })
            .mount(&server)
            .await;

        let resp = client_for(&server)
            .request(reqwest::Method::GET, "/v2/ping", &[], None)
            .await
            .unwrap();
        assert_eq!(resp.status, 200);

        let reqs = server.received_requests().await.unwrap();
        let api_calls = reqs.iter().filter(|r| r.url.path() == "/v2/ping").count();
        assert_eq!(api_calls, 2, "one retry, not a loop");
        // The cache is invalidated before the retry, so the token is refetched.
        let tok_calls = reqs
            .iter()
            .filter(|r| r.url.path() == "/oauth2/token")
            .count();
        assert_eq!(tok_calls, 2, "the retry must use a freshly fetched token");
    }

    /// A second 401 is the server's answer, not an invitation to keep trying.
    #[tokio::test]
    async fn a_persistent_401_stops_after_one_retry_and_reports_it() {
        let server = MockServer::start().await;
        token_ok(&server).mount(&server).await;
        Mock::given(method("GET"))
            .and(path("/v2/ping"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let err = client_for(&server)
            .request(reqwest::Method::GET, "/v2/ping", &[], None)
            .await
            .unwrap_err();
        let ApiError::Api { status, .. } = err else {
            panic!("expected Api, got {err:?}")
        };
        assert_eq!(status, 401);
        let reqs = server.received_requests().await.unwrap();
        assert_eq!(
            reqs.iter().filter(|r| r.url.path() == "/v2/ping").count(),
            2
        );
    }

    /// When the refresh itself fails, the auth failure is what the caller
    /// sees — not the original 401, which would send them looking at the
    /// wrong endpoint.
    #[tokio::test]
    async fn a_failed_refresh_surfaces_the_auth_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth2/token"))
            .respond_with(FailFirstToken {
                calls: AtomicUsize::new(0),
            })
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/v2/ping"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let err = client_for(&server)
            .request(reqwest::Method::GET, "/v2/ping", &[], None)
            .await
            .unwrap_err();
        let ApiError::Auth(message) = err else {
            panic!("expected Auth, got {err:?}")
        };
        assert!(message.contains("invalid_grant"), "{message}");
    }

    /// Succeeds once so the first request can be made, then fails, which is
    /// what a revoked client looks like mid-session.
    struct FailFirstToken {
        calls: AtomicUsize,
    }

    impl Respond for FailFirstToken {
        fn respond(&self, _: &Request) -> ResponseTemplate {
            if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                ResponseTemplate::new(200).set_body_json(json!({
                    "access_token": "tok-xyz", "expires_in": 3600, "token_type": "Bearer"}))
            } else {
                ResponseTemplate::new(400).set_body_json(json!({
                    "error": "invalid_grant", "error_description": "client revoked"}))
            }
        }
    }

    /// A bodyless 200 is a documented success for thirteen of the fifty
    /// operations, `ping` among them. Reporting it as a decode error inside
    /// transport puts it where no renderer can rescue it.
    #[tokio::test]
    async fn a_bodyless_success_is_not_a_decode_error() {
        let server = MockServer::start().await;
        token_ok(&server).mount(&server).await;
        Mock::given(method("GET"))
            .and(path("/v2/ping"))
            .respond_with(ResponseTemplate::new(200).insert_header("x-correlation-id", "corr-1"))
            .mount(&server)
            .await;

        let resp = client_for(&server)
            .request(reqwest::Method::GET, "/v2/ping", &[], None)
            .await
            .unwrap();
        assert_eq!(resp.status, 200);
        assert!(resp.body.is_none());
        assert_eq!(resp.correlation_id.as_deref(), Some("corr-1"));
    }

    /// **A bodyless POST still declares its content type.**
    ///
    /// The empty frame needs `Content-Length: 0` *and*
    /// `Content-Type: application/json`: the sandbox answers 415 Unsupported
    /// Media Type to an empty POST that declares neither, which is what every
    /// capture, reversal, hold, release, cancel and set-default sends.
    ///
    /// A GET or DELETE carries no frame at all, so it declares no type — an
    /// invented one there would be a header describing a body that does not
    /// exist.
    #[tokio::test]
    async fn a_bodyless_post_declares_its_content_type_and_a_bodyless_get_does_not() {
        let server = MockServer::start().await;
        token_ok(&server).mount(&server).await;
        for (verb, route) in [("POST", "/v2/post"), ("DELETE", "/v2/delete")] {
            Mock::given(method(verb))
                .and(path(route))
                .respond_with(ResponseTemplate::new(200))
                .mount(&server)
                .await;
        }

        let client = client_for(&server);
        client
            .request(reqwest::Method::POST, "/v2/post", &[], None)
            .await
            .unwrap();
        client
            .request(reqwest::Method::DELETE, "/v2/delete", &[], None)
            .await
            .unwrap();

        let reqs = server.received_requests().await.unwrap();
        let content_type = |p: &str| -> Option<String> {
            reqs.iter()
                .find(|r| r.url.path() == p)
                .expect("the request was not recorded")
                .headers
                .get("content-type")
                .map(|v| v.to_str().unwrap().split(';').next().unwrap().to_string())
        };
        assert_eq!(
            content_type("/v2/post").as_deref(),
            Some("application/json"),
            "an empty POST that declares no content type is answered 415"
        );
        assert_eq!(
            content_type("/v2/delete"),
            None,
            "a DELETE sends no frame, so it must not describe one"
        );
    }

    /// A query value under a secret-naming key is masked by that key alone.
    /// No command builds such a query, so the trace's key-based rule is only
    /// reachable here — and a length rule would pass a short secret through.
    #[test]
    fn a_traced_query_masks_a_secret_by_its_key() {
        let traced = traced_url(
            "https://example.test/v2/customers",
            &[("securityCode", "837".to_string())],
        );
        assert!(!traced.contains("837"), "{traced}");
        assert!(traced.contains("securityCode"), "{traced}");
    }

    /// An empty slice leaves the traced URL as it is — a bare `?` would show
    /// up as a difference against the URL the request actually carried.
    #[test]
    fn a_traced_url_without_a_query_gains_no_marker() {
        assert_eq!(
            traced_url("https://example.test/v2/ping", &[]),
            "https://example.test/v2/ping"
        );
    }

    /// Query parameters reach the wire, and an empty slice adds nothing —
    /// not even a bare `?`, which would show up in a recorded-request
    /// comparison as a difference.
    #[tokio::test]
    async fn query_parameters_reach_the_url_and_an_empty_slice_adds_none() {
        let server = MockServer::start().await;
        token_ok(&server).mount(&server).await;
        Mock::given(method("GET"))
            .and(path("/v2/customers"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items": []})))
            .mount(&server)
            .await;

        let c = client_for(&server);
        c.request(
            reqwest::Method::GET,
            "/v2/customers",
            &[
                ("pageSize", "20".to_string()),
                ("email", "a@b.c".to_string()),
            ],
            None,
        )
        .await
        .unwrap();
        c.request(reqwest::Method::GET, "/v2/customers", &[], None)
            .await
            .unwrap();

        let reqs = server.received_requests().await.unwrap();
        let with: Vec<_> = reqs
            .iter()
            .filter(|r| r.url.path() == "/v2/customers")
            .collect();
        let mut pairs: Vec<(String, String)> = with[0]
            .url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        pairs.sort();
        assert_eq!(
            pairs,
            vec![
                ("email".to_string(), "a@b.c".to_string()),
                ("pageSize".to_string(), "20".to_string())
            ]
        );
        assert_eq!(
            with[1].url.query(),
            None,
            "an empty slice must add no query"
        );
    }
}
