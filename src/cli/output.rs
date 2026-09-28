//! Output contract: JSON envelope, agent error envelope, exit codes.

use crate::api::ApiError;
use serde::Serialize;
use serde_json::Value;

#[derive(Copy, Clone, Debug, clap::ValueEnum, PartialEq, Eq)]
pub enum OutputFormat {
    Table,
    Json,
    Quiet,
}

impl OutputFormat {
    /// Read a config-file value. `None` for anything unrecognised, so a
    /// caller falls through to the next precedence layer rather than
    /// inheriting a typo as a mode.
    pub fn from_config_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "table" => Some(Self::Table),
            "json" => Some(Self::Json),
            "quiet" => Some(Self::Quiet),
            _ => None,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Meta {
    pub environment: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    /// The API's `pageInfo`, reproduced field for field on collection reads so
    /// an agent can paginate without parsing table output. Skipped rather than
    /// nulled: a write carries no pages, and a consumer branching on key
    /// presence must see one shape.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_info: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct Envelope<T: Serialize> {
    pub object: &'static str,
    pub data: T,
    pub meta: Meta,
}

impl<T: Serialize> Envelope<T> {
    pub fn new(
        object: &'static str,
        data: T,
        environment: &str,
        correlation_id: Option<String>,
        page_info: Option<Value>,
    ) -> Self {
        Self {
            object,
            data,
            meta: Meta {
                environment: environment.to_string(),
                correlation_id,
                page_info,
            },
        }
    }
}

/// The failure envelope, printed to stdout under `--output json`.
///
/// A machine consumer must never see an empty stdout, so every failure
/// classifies into one `kind`: `api` for an HTTP error carrying a status,
/// `transport` for a connection failure, `auth` for OAuth or keychain,
/// `decode` for a body that could not be read, and `client` for everything
/// else — config, usage, and client-side validation.
#[derive(Debug, Serialize)]
pub struct ErrorJson {
    pub kind: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
}

impl ErrorJson {
    /// `downcast_ref` searches the whole chain, so a typed error still
    /// classifies after a caller has added context with `?`.
    pub fn from_anyhow(err: &anyhow::Error) -> Self {
        match err.downcast_ref::<ApiError>() {
            Some(ApiError::Api {
                status,
                correlation_id,
                message,
            }) => Self {
                kind: "api",
                message: message.clone(),
                status: Some(*status),
                correlation_id: correlation_id.clone(),
            },
            Some(ApiError::Transport(m)) => Self {
                kind: "transport",
                message: m.clone(),
                status: None,
                correlation_id: None,
            },
            Some(ApiError::Auth(m)) => Self {
                kind: "auth",
                message: m.clone(),
                status: None,
                correlation_id: None,
            },
            Some(ApiError::Decode(m)) => Self {
                kind: "decode",
                message: m.clone(),
                status: None,
                correlation_id: None,
            },
            Some(ApiError::Client(m)) => Self {
                kind: "client",
                message: m.clone(),
                status: None,
                correlation_id: None,
            },
            None => Self {
                kind: "client",
                message: err.to_string(),
                status: None,
                correlation_id: None,
            },
        }
    }
}

/// Map an HTTP status to its exit code.
///
/// 402, 409 and 429 are new in v2 and deliberately fall through the general
/// arm: nothing can depend on a code the CLI has never emitted, so claiming
/// one for them later stays backwards compatible.
pub fn exit_code_for_api(status: u16) -> i32 {
    match status {
        401 | 403 => 2,
        400 | 422 => 3,
        404 => 4,
        _ => 1,
    }
}

/// The consumer of stdout stopped reading. `128 + SIGPIPE` is the value a
/// shell reports for a process a `SIGPIPE` killed, so a `pipefail` pipeline
/// reads the same code from this CLI as from every other tool in it.
pub const EXIT_STDOUT_CLOSED: u8 = 141;

/// An outcome the command has already written out, carrying only its exit
/// code.
///
/// The `pos create --wait` endings are why it exists. An interrupt must leave
/// stdout empty in every output mode, and an expired timeout or a failed poll
/// has already printed the last-known envelope there — so the general failure
/// path must add nothing in either direction, and the exit code travels here
/// instead.
#[derive(Debug, thiserror::Error)]
#[error("already reported")]
pub struct Reported {
    pub code: i32,
}

/// Derive the exit code from a failed command.
pub fn exit_code_for(err: &anyhow::Error) -> i32 {
    if let Some(Reported { code }) = err.downcast_ref::<Reported>() {
        return *code;
    }
    match err.downcast_ref::<ApiError>() {
        Some(ApiError::Api { status, .. }) => exit_code_for_api(*status),
        Some(ApiError::Auth(_)) => 2,
        Some(ApiError::Decode(_) | ApiError::Transport(_)) => 1,
        Some(ApiError::Client(_)) => 3,
        // An untyped error is client-side — bad input, usage, config — which
        // the contract classifies as validation, parallel to a server 400.
        None => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_map_status_classes() {
        assert_eq!(exit_code_for_api(401), 2);
        assert_eq!(exit_code_for_api(403), 2);
        assert_eq!(exit_code_for_api(400), 3);
        assert_eq!(exit_code_for_api(422), 3);
        assert_eq!(exit_code_for_api(404), 4);
        assert_eq!(exit_code_for_api(500), 1);
    }

    /// The two `pos create --wait` endings need codes the status map cannot
    /// reach: an interrupt is 130 and an expired timeout is 1, where an
    /// untyped error would be 3.
    #[test]
    fn an_already_reported_outcome_carries_its_own_exit_code() {
        assert_eq!(
            exit_code_for(&anyhow::Error::new(Reported { code: 130 })),
            130
        );
        assert_eq!(exit_code_for(&anyhow::Error::new(Reported { code: 1 })), 1);
    }

    /// The context a `?` adds must not lose the code: `downcast_ref` searches
    /// the whole chain, and an interrupt reported through one more layer is
    /// still an interrupt.
    #[test]
    fn a_reported_outcome_survives_added_context() {
        let wrapped = anyhow::Error::new(Reported { code: 130 }).context("while polling");
        assert_eq!(exit_code_for(&wrapped), 130);
    }

    /// v2 introduces these three; they fall through the general arm rather
    /// than claiming new codes.
    #[test]
    fn new_v2_statuses_fall_through_to_general() {
        assert_eq!(exit_code_for_api(402), 1);
        assert_eq!(exit_code_for_api(409), 1);
        assert_eq!(exit_code_for_api(429), 1);
    }

    #[test]
    fn envelope_carries_meta_and_page_info() {
        let page = serde_json::json!({
            "pageIndex": 0, "pageSize": 20, "totalItems": 17,
            "totalPages": 1, "hasMore": false });
        let env = Envelope::new(
            "customer",
            serde_json::json!({"id": "c1"}),
            "sandbox",
            Some("corr-1".into()),
            Some(page),
        );
        let v = serde_json::to_value(&env).unwrap();
        assert_eq!(v["object"], "customer");
        assert_eq!(v["meta"]["environment"], "sandbox");
        assert_eq!(v["meta"]["correlation_id"], "corr-1");
        assert_eq!(v["meta"]["page_info"]["hasMore"], false);
    }

    /// A write carries no page_info, and the key is absent rather than null:
    /// an agent branching on key presence must see the same shape every time.
    #[test]
    fn envelope_omits_page_info_when_absent() {
        let env = Envelope::new(
            "transaction",
            serde_json::json!({"transactionId": "txn_1"}),
            "sandbox",
            None,
            None,
        );
        let json = serde_json::to_string(&env).unwrap();
        assert!(!json.contains("page_info"));
        assert!(!json.contains("correlation_id"));
    }

    #[test]
    fn error_json_classifies_kinds() {
        let api = anyhow::Error::from(crate::api::ApiError::Api {
            status: 422,
            message: "bad".into(),
            correlation_id: Some("x".into()),
        });
        let e = ErrorJson::from_anyhow(&api);
        assert_eq!(e.kind, "api");
        assert_eq!(e.status, Some(422));
        assert_eq!(e.correlation_id.as_deref(), Some("x"));

        let plain = anyhow::anyhow!("garbage argument");
        assert_eq!(ErrorJson::from_anyhow(&plain).kind, "client");

        let refused = anyhow::Error::from(crate::api::ApiError::Client("bad id".into()));
        let e = ErrorJson::from_anyhow(&refused);
        assert_eq!(e.kind, "client");
        assert_eq!(e.status, None);
    }

    /// Callers add context with `?`, so the typed error has to be found
    /// through a wrapper or the envelope loses status and correlation id.
    #[test]
    fn error_json_finds_the_api_error_through_a_context_wrapper() {
        let inner = anyhow::Error::from(crate::api::ApiError::Api {
            status: 500,
            message: "boom".into(),
            correlation_id: Some("xyz".into()),
        });
        let e = ErrorJson::from_anyhow(&inner.context("while creating a customer"));
        assert_eq!(e.kind, "api");
        assert_eq!(e.status, Some(500));
    }

    #[test]
    fn from_config_str_is_case_insensitive_and_rejects_nonsense() {
        assert_eq!(
            OutputFormat::from_config_str("JSON"),
            Some(OutputFormat::Json)
        );
        assert_eq!(
            OutputFormat::from_config_str("quiet"),
            Some(OutputFormat::Quiet)
        );
        assert_eq!(OutputFormat::from_config_str("nonsense"), None);
    }

    #[test]
    fn exit_code_for_routes_every_api_error_variant() {
        let auth = anyhow::Error::from(crate::api::ApiError::Auth("no token".into()));
        assert_eq!(exit_code_for(&auth), 2);
        let decode = anyhow::Error::from(crate::api::ApiError::Decode("bad json".into()));
        assert_eq!(exit_code_for(&decode), 1);
        let transport = anyhow::Error::from(crate::api::ApiError::Transport("dns".into()));
        assert_eq!(exit_code_for(&transport), 1);
        // Client-side validation and usage errors are validation errors,
        // parallel to a server 400.
        assert_eq!(exit_code_for(&anyhow::anyhow!("bad flag")), 3);
        let refused = anyhow::Error::from(crate::api::ApiError::Client("bad id".into()));
        assert_eq!(exit_code_for(&refused), 3);
    }
}
