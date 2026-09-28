//! The v2 error envelope, in all four shapes the API answers with.
//!
//! One envelope is documented; four are returned, and the parser handles all
//! four because a caller has no way to know which layer answered. Two further
//! properties shape the parsing as much: error bodies are PascalCase where
//! every success body and every documented example is camelCase, and the
//! messages interpolate internal type names into fields documented to carry
//! friendly text — which is why they are redacted structurally rather than
//! trusted.

use crate::api::redact::{redact_message, redact_preserving_shape};
use serde_json::Value;
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("transport error: {0}")]
    Transport(String),

    /// The correlation id is named only when there is one: an absent optional
    /// rendered as `None` reads as a value, and is passed on to support as
    /// one.
    #[error("API {status}{}: {message}",
        .correlation_id.as_deref().map_or_else(String::new, |id| format!(" (correlation_id={id})")))]
    Api {
        status: u16,
        correlation_id: Option<String>,
        message: String,
    },

    #[error("auth error: {0}")]
    Auth(String),

    #[error("invalid response: {0}")]
    Decode(String),

    /// A request this CLI refuses to build. Client-side, so it reports as
    /// `client` and exits 3 — the same classification a usage error gets,
    /// because that is what it is.
    #[error("{0}")]
    Client(String),
}

impl ApiError {
    /// Supply a correlation id the response header carried, where the body
    /// gave none.
    ///
    /// A fallback rather than an override: the body is the API's own
    /// statement about the failure. It exists because a bodyless response has
    /// no body to state it in — a 401 answered with nothing but headers would
    /// otherwise report no identifier at all, though one was sent.
    ///
    /// Applied at the call site rather than inside `parse_error_body`, whose
    /// two `Option<&str>` parameters would otherwise become three adjacent
    /// ones that compile in any order.
    #[must_use]
    pub fn or_correlation_id(self, fallback: Option<String>) -> Self {
        match self {
            Self::Api {
                status,
                correlation_id,
                message,
            } => Self::Api {
                status,
                correlation_id: correlation_id.or(fallback),
                message,
            },
            other => other,
        }
    }
}

/// The documented envelope and RFC 9110 ProblemDetails at once.
///
/// Downstream handlers answer in PascalCase and the edge model-validator in
/// camelCase, so every field carries both spellings. A `BTreeMap` keeps the
/// flattened field errors deterministic.
#[derive(Debug, serde::Deserialize)]
struct ErrorEnvelope {
    #[serde(alias = "Title")]
    title: Option<String>,
    /// The only prose that varies per refusal: `Title`, `Cause` and
    /// `Resolution` repeat across every refusal of the same status, so a
    /// response that names its own condition names it here.
    #[serde(alias = "Details")]
    details: Option<String>,
    #[serde(alias = "Cause")]
    cause: Option<String>,
    #[serde(alias = "Resolution")]
    resolution: Option<String>,
    #[serde(rename = "errorCode", alias = "ErrorCode")]
    error_code: Option<String>,
    #[serde(rename = "correlationId", alias = "CorrelationId")]
    correlation_id: Option<String>,
    #[serde(rename = "traceId", alias = "TraceId")]
    trace_id: Option<String>,
    #[serde(rename = "exceptionType", alias = "ExceptionType")]
    exception_type: Option<String>,
    #[serde(rename = "errors", alias = "Errors")]
    errors: Option<BTreeMap<String, Vec<String>>>,
}

impl ErrorEnvelope {
    /// Flatten the field-error map into `field: message; field: message`.
    ///
    /// This is where the actionable detail lives — the generic `Title` says
    /// only that validation failed. The empty-string key holds form-level
    /// messages, which carry no prefix.
    fn flatten_errors(&self) -> Option<String> {
        let map = self.errors.as_ref()?;
        let parts: Vec<String> = map
            .iter()
            .flat_map(|(field, msgs)| {
                msgs.iter().map(move |m| {
                    if field.is_empty() {
                        m.clone()
                    } else {
                        format!("{field}: {m}")
                    }
                })
            })
            .collect();
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("; "))
        }
    }
}

/// The token endpoint speaks OpenIddict, which shares no field with the API
/// envelope.
#[derive(Debug, serde::Deserialize)]
struct OpenIddictError {
    error: String,
    error_description: Option<String>,
    error_uri: Option<String>,
}

/// Join the envelope's prose, introducing each part with a colon unless the
/// one before it already closed a sentence.
///
/// `Title` is a fragment and reads as a label for what follows; `Cause` and
/// `Details` are written as whole sentences, and a colon after a full stop
/// reads as a typo.
fn join_prose(parts: &[&str]) -> String {
    let mut out = String::new();
    for part in parts {
        if !out.is_empty() {
            out.push_str(if out.ends_with(['.', '!', '?']) {
                " "
            } else {
                ": "
            });
        }
        out.push_str(part);
    }
    out
}

/// Turn a failed response into an `ApiError`.
///
/// `www_authenticate` is the last resort: some 401s carry no body at all, and
/// the header is then the only statement of what went wrong.
pub fn parse_error_body(status: u16, body: &str, www_authenticate: Option<&str>) -> ApiError {
    // **Redact while the body is still structured.** A CVV is three digits and
    // an ACH account number nine, so neither is distinguishable by shape —
    // only the field name says they are sensitive, and flattening the envelope
    // into prose throws the field names away. Everything below reads the
    // redacted body, never `body`.
    let safe: Option<Value> = serde_json::from_str::<Value>(body)
        .ok()
        .map(|v| redact_preserving_shape(&v));
    let safe_text = safe
        .as_ref()
        .map_or_else(|| body.to_string(), Value::to_string);

    if body.trim().is_empty() {
        let message = match www_authenticate {
            Some(h) if !h.trim().is_empty() => h.trim().to_string(),
            _ => format!("HTTP {status} with no response body"),
        };
        return ApiError::Api {
            status,
            correlation_id: None,
            message: redact_message(&message),
        };
    }

    // OpenIddict first: `error` as a string appears in no other shape, so the
    // discrimination cannot misfire on the API envelope's `errors` map.
    if let Ok(e) = serde_json::from_str::<OpenIddictError>(&safe_text) {
        let mut message = match e.error_description.as_deref().filter(|s| !s.is_empty()) {
            Some(d) => format!("{}: {d}", e.error),
            None => e.error.clone(),
        };
        if let Some(uri) = e.error_uri.as_deref().filter(|s| !s.is_empty()) {
            message = format!("{message} ({uri})");
        }
        return ApiError::Api {
            status,
            correlation_id: None,
            message: redact_message(&message),
        };
    }

    match serde_json::from_str::<ErrorEnvelope>(&safe_text) {
        Ok(e) => {
            let title = e.title.as_deref().filter(|s| !s.is_empty());
            let details = e.details.as_deref().filter(|s| !s.is_empty());
            let cause = e.cause.as_deref().filter(|s| !s.is_empty());
            let exception = e.exception_type.as_deref().filter(|s| !s.is_empty());
            let error_code = e.error_code.as_deref().filter(|s| !s.is_empty());
            let fields = e.flatten_errors();

            // `Title` is often generic, so `Cause` has to survive alongside it,
            // and `Details` alongside both — it is the one field that varies
            // per refusal rather than per status. A generic response repeats
            // the title or the cause in it, so an exact repeat is dropped
            // rather than said twice.
            let mut parts: Vec<&str> = Vec::new();
            for part in [title, cause, details].into_iter().flatten() {
                if !parts.contains(&part) {
                    parts.push(part);
                }
            }
            let core = match parts.is_empty() {
                true if fields.is_none() => safe_text.clone(),
                true => String::new(),
                false => join_prose(&parts),
            };

            // `ExceptionType` and `ErrorCode` are diagnostic handles rather
            // than prose: the code is what a caller branches on while the
            // wording around it stays free to change.
            let stamp: Vec<&str> = [exception, error_code].into_iter().flatten().collect();
            let mut message = match stamp.is_empty() {
                true => core,
                false => {
                    let stamp = stamp.join(" ");
                    if core.is_empty() {
                        format!("[{stamp}]")
                    } else {
                        format!("{core} [{stamp}]")
                    }
                }
            };
            if let Some(f) = fields {
                message = if message.is_empty() {
                    f
                } else {
                    format!("{message}: {f}")
                };
            }
            if let Some(r) = e.resolution.as_deref().filter(|s| !s.is_empty()) {
                message = format!("{message} Resolution: {r}");
            }
            ApiError::Api {
                status,
                // ProblemDetails carries no correlation id, only a trace id.
                correlation_id: e.correlation_id.or(e.trace_id),
                // The message reaches stderr and the JSON envelope, and an
                // error body routinely quotes the value that caused the
                // failure — so a PAN can arrive here even though nothing in
                // this module put it there.
                message: redact_message(&message),
            }
        }
        Err(_) => ApiError::Api {
            status,
            correlation_id: None,
            message: redact_message(&safe_text),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The error path must redact by key, not only by digit-run length.**
    ///
    /// A CVV is three or four digits, so nothing about its shape marks it as
    /// sensitive; only the field name does. By the time the prose message is
    /// built the field name is gone, so the redaction has to happen while the
    /// body is still structured.
    #[test]
    fn a_short_secret_is_redacted_by_its_field_name_not_its_length() {
        let body = r#"{"Title":"Validation failed",
                       "Errors":{"securityCode":["123 is invalid"]}}"#;
        let ApiError::Api { message, .. } = parse_error_body(400, body, None) else {
            panic!("expected an Api error")
        };
        assert!(!message.contains("123"), "{message}");
        assert!(message.contains("securityCode"), "{message}");
        assert!(message.contains("Validation failed"), "{message}");
    }

    /// An ACH account number is shorter than a PAN and just as identifying.
    #[test]
    fn a_short_account_number_is_redacted_by_its_field_name() {
        let body = r#"{"Errors":{"accountNumber":["123456789 is not valid"]}}"#;
        let ApiError::Api { message, .. } = parse_error_body(400, body, None) else {
            panic!("expected an Api error")
        };
        assert!(!message.contains("123456789"), "{message}");
    }

    /// The long-run rule still catches what the key-based one cannot: a PAN
    /// quoted under a field name that says nothing.
    #[test]
    fn a_pan_quoted_under_an_unremarkable_field_is_still_masked() {
        let body = r#"{"Errors":{"detail":["4111111111111111 was declined"]}}"#;
        let ApiError::Api { message, .. } = parse_error_body(400, body, None) else {
            panic!("expected an Api error")
        };
        assert!(!message.contains("4111111111111111"), "{message}");
        assert!(message.contains("1111"), "{message}");
    }

    /// Redacting must not cost the message its meaning: the numbers worth
    /// reading are still there.
    #[test]
    fn redaction_leaves_the_readable_parts_of_an_error_intact() {
        let body = r#"{"Title":"Rejected","Cause":"amount 1234.56 exceeds the 5000 limit",
                       "CorrelationId":"c-1","Resolution":"Lower the amount"}"#;
        let ApiError::Api { message, .. } = parse_error_body(402, body, None) else {
            panic!("expected an Api error")
        };
        assert!(message.contains("1234.56"), "{message}");
        assert!(message.contains("5000"), "{message}");
        assert!(message.contains("Lower the amount"), "{message}");
    }

    /// A body that is not the envelope at all still goes through the
    /// free-text rules rather than being echoed whole.
    #[test]
    fn a_body_that_is_not_the_envelope_is_still_redacted() {
        let ApiError::Api { message, .. } =
            parse_error_body(500, "card 4111111111111111 blew up", None)
        else {
            panic!("expected an Api error")
        };
        assert!(!message.contains("4111111111111111"), "{message}");
    }

    /// A bodyless 401 carries no correlation id in a body it does not have,
    /// and the response header is then the only identifier that existed.
    #[test]
    fn a_header_correlation_id_fills_in_when_the_body_carries_none() {
        let e = parse_error_body(401, "", Some("Bearer error=\"invalid_token\""))
            .or_correlation_id(Some("corr-header".into()));
        let ApiError::Api { correlation_id, .. } = e else {
            panic!("expected an Api error")
        };
        assert_eq!(correlation_id.as_deref(), Some("corr-header"));
    }

    /// The body wins where it has one: it is the API's own statement about
    /// the failure, and the header is a fallback rather than an override.
    #[test]
    fn a_body_correlation_id_is_not_overwritten_by_the_header() {
        let body = r#"{"Title":"Nope","CorrelationId":"corr-body"}"#;
        let e = parse_error_body(400, body, None).or_correlation_id(Some("corr-header".into()));
        let ApiError::Api { correlation_id, .. } = e else {
            panic!("expected an Api error")
        };
        assert_eq!(correlation_id.as_deref(), Some("corr-body"));
    }

    /// The fallback is scoped to the one variant that carries an id at all.
    #[test]
    fn the_fallback_leaves_other_error_variants_alone() {
        let e = ApiError::Transport("dns".into()).or_correlation_id(Some("corr".into()));
        assert!(matches!(e, ApiError::Transport(_)));
    }

    /// `Cause` and `Resolution` repeat across every refusal of the same
    /// status, so the text naming *this* failure arrives in `Details` alone.
    #[test]
    fn details_carries_the_failure_the_constant_fields_cannot() {
        let body = r#"{"Title":"Access forbidden",
            "Details":"Merchant token is being used. Use a partner token instead.",
            "Cause":"You do not have permission to access this resource.",
            "Resolution":"Verify your credentials and permissions or contact your administrator.",
            "ExceptionType":"ForbiddenException","ErrorCode":"F0000"}"#;
        let ApiError::Api { message, .. } = parse_error_body(403, body, None) else {
            panic!("expected an Api error")
        };
        assert!(message.contains("Use a partner token instead"), "{message}");
    }

    /// A generic refusal repeats the title or the cause in `Details` — the
    /// published `403` example repeats the cause verbatim. An exact repeat is
    /// dropped rather than said twice.
    #[test]
    fn details_repeating_another_field_is_not_printed_twice() {
        let body = r#"{"Title":"Access forbidden",
            "Details":"You do not have permission to access this resource.",
            "Cause":"You do not have permission to access this resource."}"#;
        let ApiError::Api { message, .. } = parse_error_body(403, body, None) else {
            panic!("expected an Api error")
        };
        assert_eq!(
            message.matches("You do not have permission").count(),
            1,
            "{message}"
        );
    }

    /// A part that already ends a sentence is followed by a space, not by the
    /// colon that introduces one — `Cause` is written as a full sentence and
    /// `Details` continues after it.
    #[test]
    fn a_sentence_is_not_joined_to_the_next_part_by_a_colon() {
        let body = r#"{"Title":"Access forbidden",
            "Cause":"You do not have permission to access this resource.",
            "Details":"Merchant token is being used."}"#;
        let ApiError::Api { message, .. } = parse_error_body(403, body, None) else {
            panic!("expected an Api error")
        };
        assert!(!message.contains(".:"), "{message}");
        assert!(
            message.contains("Access forbidden: You do not"),
            "{message}"
        );
        assert!(message.contains("resource. Merchant token"), "{message}");
    }

    /// The error code is the stable handle a caller can branch on, where the
    /// prose around it is free to be reworded.
    #[test]
    fn the_error_code_reaches_the_message() {
        let body = r#"{"Title":"Rejected","ErrorCode":"F0000"}"#;
        let ApiError::Api { message, .. } = parse_error_body(403, body, None) else {
            panic!("expected an Api error")
        };
        assert!(message.contains("F0000"), "{message}");
    }

    /// `Details` is the whole statement when it is the only prose sent.
    #[test]
    fn details_alone_is_the_message() {
        let body = r#"{"Details":"Merchant token is being used."}"#;
        let ApiError::Api { message, .. } = parse_error_body(403, body, None) else {
            panic!("expected an Api error")
        };
        assert_eq!(message, "Merchant token is being used.");
    }

    /// The envelope answers in both casings, so every field carries both.
    #[test]
    fn reads_cause_and_resolution_from_pascal_case_envelope() {
        let body = r#"{"StatusCode":400,"Title":"Validation failed",
            "Cause":"baseAmount must be greater than zero",
            "Resolution":"Supply a positive amount",
            "CorrelationId":"c-1","ExceptionType":"ValidationException"}"#;
        let e = parse_error_body(400, body, None);
        let ApiError::Api {
            message,
            correlation_id,
            ..
        } = e
        else {
            panic!()
        };
        assert!(message.contains("baseAmount must be greater than zero"));
        assert!(message.contains("Supply a positive amount"));
        assert_eq!(correlation_id.as_deref(), Some("c-1"));
    }

    #[test]
    fn flattens_field_error_map_with_camel_case_paths() {
        let body = r#"{"Title":"Validation failed","Errors":{
            "transactionDetails.cardData.paymentMethodDetails.cardNumber":["is required"]}}"#;
        let e = parse_error_body(400, body, None);
        let ApiError::Api { message, .. } = e else {
            panic!()
        };
        assert!(message.contains("transactionDetails.cardData.paymentMethodDetails.cardNumber"));
        assert!(message.contains("is required"));
    }

    /// Edge model-validation returns RFC 9110 ProblemDetails with traceId and
    /// no correlationId.
    #[test]
    fn falls_back_to_trace_id_when_correlation_id_absent() {
        let body = r#"{"type":"https://tools.ietf.org/html/rfc9110","title":"One or more validation errors occurred.","status":400,"traceId":"00-abc-def-01","errors":{"BaseAmount":["invalid"]}}"#;
        let e = parse_error_body(400, body, None);
        let ApiError::Api { correlation_id, .. } = e else {
            panic!()
        };
        assert_eq!(correlation_id.as_deref(), Some("00-abc-def-01"));
    }

    /// The token endpoint speaks OpenIddict, not the API envelope.
    #[test]
    fn parses_openiddict_token_error() {
        let body =
            r#"{"error":"invalid_client","error_description":"Client authentication failed"}"#;
        let e = parse_error_body(400, body, None);
        let ApiError::Api { message, .. } = e else {
            panic!()
        };
        assert!(message.contains("invalid_client"));
        assert!(message.contains("Client authentication failed"));
    }

    /// An error body routinely quotes the value that caused the failure, so a
    /// PAN can reach this message even though nothing here put it there. It
    /// goes to stderr and into the JSON envelope, which makes it the same
    /// incident as a leak in the trace.
    #[test]
    fn a_pan_echoed_by_the_api_is_masked_in_the_message() {
        let body = r#"{"Title":"Validation failed","Errors":{
            "cardNumber":["4111111111111111 is not acceptable"]}}"#;
        let e = parse_error_body(400, body, None);
        let ApiError::Api { message, .. } = e else {
            panic!()
        };
        assert!(!message.contains("4111111111111111"), "{message}");
        assert!(message.contains("cardNumber"), "{message}");
        assert!(message.contains("not acceptable"), "{message}");
    }

    /// Some 401s carry no body at all.
    #[test]
    fn uses_www_authenticate_when_body_is_empty() {
        let e = parse_error_body(401, "", Some(r#"Bearer error="invalid_token""#));
        let ApiError::Api { message, .. } = e else {
            panic!()
        };
        assert!(message.contains("invalid_token"));
    }

    /// An identifier the API names in its refusal is the caller's own value
    /// coming back: altered, it matches nothing in a support ticket or a log
    /// search.
    #[test]
    fn an_identifier_the_api_names_is_reported_unaltered() {
        let id = "11111111-2222-3333-4444-555555555555";
        let body =
            format!(r#"{{"Title":"Not found","Details":"Customer with ID {id} does not exist."}}"#);
        let ApiError::Api { message, .. } = parse_error_body(404, &body, None) else {
            panic!("expected an Api error")
        };
        assert!(message.contains(id), "{message}");
    }

    /// A correlation id is the handle support searches on, and the caller has
    /// no second copy to compare it against, so it passes through verbatim.
    #[test]
    fn a_correlation_id_is_reported_exactly_as_the_api_sent_it() {
        let body = r#"{"Title":"Nope","CorrelationId":"123456789012345678"}"#;
        let ApiError::Api { correlation_id, .. } = parse_error_body(400, body, None) else {
            panic!("expected an Api error")
        };
        assert_eq!(correlation_id.as_deref(), Some("123456789012345678"));
    }

    /// A trace id stands in for the correlation id and is read the same way.
    #[test]
    fn a_trace_id_is_reported_exactly_as_the_api_sent_it() {
        let body = r#"{"title":"Nope","traceId":"00-123456789012345678901234567890ab-0123456789012345-01"}"#;
        let ApiError::Api { correlation_id, .. } = parse_error_body(400, body, None) else {
            panic!("expected an Api error")
        };
        assert_eq!(
            correlation_id.as_deref(),
            Some("00-123456789012345678901234567890ab-0123456789012345-01")
        );
    }

    /// Where an error is folded into a message, an absent correlation id has
    /// nothing to say: a rendered `None` reads as a value and is reported as
    /// one.
    #[test]
    fn an_absent_correlation_id_contributes_nothing_to_the_message() {
        let e = ApiError::Api {
            status: 401,
            correlation_id: None,
            message: "invalid_client: credentials are invalid".into(),
        };
        assert_eq!(
            e.to_string(),
            "API 401: invalid_client: credentials are invalid"
        );
    }

    #[test]
    fn a_present_correlation_id_is_named_in_the_message() {
        let e = ApiError::Api {
            status: 400,
            correlation_id: Some("c-1".into()),
            message: "Nope".into(),
        };
        assert_eq!(e.to_string(), "API 400 (correlation_id=c-1): Nope");
    }
}
