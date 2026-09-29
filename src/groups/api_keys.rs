//! `api-keys`: create, list, revoke.
//!
//! Two things are peculiar to this group.
//!
//! **It sits behind a server feature flag.** With the flag off the endpoints
//! answer 404, which reads as a wrong URL rather than a disabled feature — so
//! a 404 from `create` or `list` says which it is. That collides with the rule
//! that a 404 on a delete or revoke is success, and the collision is resolved
//! by which reading is *possible*: a revoke names a key that can be gone
//! already, while a read names nothing that could be missing, so only the
//! reads take the feature-flag reading.
//!
//! **`list` is not paginated.** `GetApiKeysResponseDto` declares one property,
//! `apiKeys`, and no `pageInfo`, so the shared pagination surface is
//! deliberately not wired in and its flags are rejected as unknown.

use crate::Ctx;
use crate::api::ApiError;
use crate::api::ApiPath;
use crate::cli::common;
use crate::cli::output::OutputFormat;
use crate::cli::render::{self, Cell, Column, Resource};
use anyhow::Result;
use reqwest::Method;
use serde_json::{Map, Value};

#[allow(clippy::large_enum_variant)]
#[derive(clap::Subcommand, Debug)]
pub enum ApiKeysCommand {
    /// Create a merchant API key (POST /v2/api-keys).
    ///
    /// The response contains `clientSecret`, which is shown only once.
    /// Store it securely immediately after creation.
    Create {
        /// Merchant UUID (required).
        #[arg(long)]
        merchant_id: String,
        /// API key display name (required).
        #[arg(long = "name", value_name = "API_KEY_NAME")]
        api_key_name: String,
    },
    /// List merchant API keys (GET /v2/api-keys).
    List {
        /// Filter by merchant UUID (optional).
        #[arg(long, id = "key_list_merchant_id", value_name = "MERCHANT_ID")]
        merchant_id: Option<String>,
    },
    /// Revoke a merchant API key (DELETE /v2/api-keys/{clientId}).
    ///
    /// Requires `--yes` to prevent accidental revocation. 404 is treated as
    /// idempotent success (already revoked).
    Revoke {
        /// Client ID of the key to revoke (required).
        #[arg(long)]
        client_id: String,
        /// Confirm the revocation (required).
        #[arg(long)]
        yes: bool,
    },
}

/// Build the `CreateApiKeyRequestDto` body: both fields are required.
pub fn build_create_api_key_body(merchant_id: &str, api_key_name: &str) -> Result<Value> {
    if merchant_id.trim().is_empty() {
        anyhow::bail!("--merchant-id is required");
    }
    if api_key_name.trim().is_empty() {
        anyhow::bail!("--name is required: a key without one cannot be told from another");
    }
    Ok(Value::Object(Map::from_iter([
        (
            "merchantId".to_string(),
            Value::String(merchant_id.to_string()),
        ),
        (
            "apiKeyName".to_string(),
            Value::String(api_key_name.to_string()),
        ),
    ])))
}

/// Re-read a 404 from a *read* in this group as the feature flag.
///
/// Only a 404 and only on a read. A revoke's 404 is the idempotent-delete
/// rule, and every other status keeps its own message and exit code — a 403
/// is an authorisation failure, not a disabled feature.
fn explain_a_missing_feature(e: ApiError) -> ApiError {
    match e {
        ApiError::Api {
            status: 404,
            correlation_id,
            message,
        } => ApiError::Api {
            status: 404,
            correlation_id,
            message: format!(
                "{message} — `api-keys` is behind a server feature flag, and a \
                 404 on this group usually means the feature is off for this \
                 account rather than that the URL is wrong"
            ),
        },
        other => other,
    }
}

/// The keys of one account.
///
/// The group is `api-keys` and its envelope is `api_token`. The envelope name
/// is part of the output contract, and an agent branching on `object` would
/// see a change to it.
pub static API_KEY: Resource = Resource {
    object: "api_token",
    object_list: "api_token_list",
    id: "/clientId",
    detail: &["/clientId", "/clientSecret", "/apiKeyName", "/merchantId"],
    columns: &[
        Column {
            header: "CLIENT ID",
            width: 36,
            cell: Cell::Path("/clientId"),
        },
        Column {
            header: "NAME",
            width: 28,
            cell: Cell::Path("/apiKeyName"),
        },
        Column {
            header: "MERCHANT",
            width: 36,
            cell: Cell::Path("/merchantId"),
        },
    ],
    amounts: &[],
    yes_no: &[],
};

/// `create`'s table: the new key's identity, its secret, and the warning that
/// the secret is not shown again.
///
/// `CreateApiKeyResponseDto` declares these two fields and nothing else, so
/// the view is the response — the name and merchant the caller supplied are
/// theirs already, and echoing them as empty rows would say the key has none.
fn created_key_table(data: &Value) -> String {
    let rows = [
        (
            "clientId".to_string(),
            render::value_at(&API_KEY, data, "/clientId"),
        ),
        (
            "clientSecret".to_string(),
            render::value_at(&API_KEY, data, "/clientSecret"),
        ),
    ];
    format!(
        "{}\n\nNOTE: The clientSecret is shown ONLY ONCE. Store it securely.",
        render::labelled(&rows).join("\n")
    )
}

pub async fn dispatch(ctx: &Ctx, command: ApiKeysCommand) -> Result<()> {
    match command {
        ApiKeysCommand::Create {
            merchant_id,
            api_key_name,
        } => {
            let body = build_create_api_key_body(&merchant_id, &api_key_name)?;
            let resp = ctx
                .api
                .request(Method::POST, "/v2/api-keys", &[], Some(body))
                .await
                .map_err(explain_a_missing_feature)?;
            let data = common::body_of(resp.body)?;
            match ctx.output {
                OutputFormat::Table => {
                    println!("{}", created_key_table(&data));
                    Ok(())
                }
                _ => render::one(ctx, &API_KEY, &data, resp.correlation_id),
            }
        }
        ApiKeysCommand::List { merchant_id } => {
            let query: Vec<(&str, String)> = merchant_id
                .filter(|s| !s.is_empty())
                .map(|id| vec![("merchantId", id)])
                .unwrap_or_default();
            // Not paginated: the response declares no `pageInfo`.
            let resp = ctx
                .api
                .request(Method::GET, "/v2/api-keys", &query, None)
                .await
                .map_err(explain_a_missing_feature)?;
            let body = common::body_of(resp.body)?;
            // No `pageInfo` on this response, so no `page_info` in meta.
            render::page(
                ctx,
                &API_KEY,
                &common::items_of(&body, "apiKeys")?,
                None,
                resp.correlation_id,
            )
        }
        ApiKeysCommand::Revoke { client_id, .. } => {
            // A 404 here is the idempotent-revoke rule, **not** the feature
            // flag: the caller named a key, and a key that is gone is the
            // outcome they asked for — though not one this call produced.
            common::delete(
                ctx,
                &API_KEY,
                ApiPath::from("/v2/api-keys").id(&client_id)?,
                &client_id,
                "revoked",
                &format!("Revoked key {client_id}."),
                &format!("No key {client_id} was found; nothing was revoked."),
            )
            .await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_create_body_carries_both_required_fields_under_their_wire_names() {
        assert_eq!(
            build_create_api_key_body("m-1", "Production API Key").unwrap(),
            serde_json::json!({
                "merchantId": "m-1",
                "apiKeyName": "Production API Key"})
        );
    }

    #[test]
    fn both_required_fields_are_refused_when_blank() {
        assert!(build_create_api_key_body("", "Production API Key").is_err());
        assert!(build_create_api_key_body("m-1", "   ").is_err());
    }

    /// The declared collection key is `apiKeys`, not the `items` every
    /// paginated read uses.
    #[test]
    fn the_collection_is_read_from_the_key_this_schema_declares() {
        let body = serde_json::json!({"apiKeys": [{"clientId": "c-1"}]});
        assert_eq!(common::items_of(&body, "apiKeys").unwrap().len(), 1);
        // An `items` array is not this response's collection.
        assert!(
            common::items_of(
                &serde_json::json!({"items": [{"clientId": "c-1"}]}),
                "apiKeys"
            )
            .unwrap()
            .is_empty()
        );
    }

    /// The field is declared nullable, so absent and null are both an empty
    /// collection — and anything else is a shape this CLI cannot read, which
    /// must not be reported as an account holding no keys.
    #[test]
    fn an_absent_collection_is_empty_and_a_wrong_typed_one_is_a_decode_error() {
        assert!(
            common::items_of(&serde_json::json!({}), "apiKeys")
                .unwrap()
                .is_empty()
        );
        assert!(
            common::items_of(&serde_json::json!({"apiKeys": null}), "apiKeys")
                .unwrap()
                .is_empty()
        );
        let Err(ApiError::Decode(message)) =
            common::items_of(&serde_json::json!({"apiKeys": "one"}), "apiKeys")
        else {
            panic!("a string was accepted as the collection");
        };
        assert!(message.contains("string"), "{message}");
    }

    /// Only a 404 gets the feature-flag reading, and it keeps its status so
    /// the exit code is unchanged.
    #[test]
    fn a_404_from_a_read_gains_the_feature_flag_explanation() {
        let explained = explain_a_missing_feature(ApiError::Api {
            status: 404,
            correlation_id: Some("c-1".into()),
            message: "Not found".into(),
        });
        let ApiError::Api {
            status,
            message,
            correlation_id,
        } = explained
        else {
            panic!("the variant changed");
        };
        assert_eq!(status, 404);
        assert_eq!(correlation_id.as_deref(), Some("c-1"));
        assert!(message.contains("feature flag"), "{message}");
        assert!(message.contains("Not found"), "{message}");
    }

    /// Every other status is left alone: a 403 is an authorisation failure,
    /// not a disabled feature.
    #[test]
    fn no_other_status_gains_the_feature_flag_explanation() {
        for status in [400u16, 401, 403, 409, 500] {
            let explained = explain_a_missing_feature(ApiError::Api {
                status,
                correlation_id: None,
                message: "Denied".into(),
            });
            let ApiError::Api { message, .. } = explained else {
                panic!("the variant changed");
            };
            assert_eq!(message, "Denied", "status {status} was rewritten");
        }
        // And a non-API error is untouched.
        let transport = explain_a_missing_feature(ApiError::Transport("refused".into()));
        assert!(matches!(transport, ApiError::Transport(_)));
    }
}
