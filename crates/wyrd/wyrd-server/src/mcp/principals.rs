//! Tenant principal and credential capabilities over MCP.
//!
//! The adapter owns exactly three things: parsing a closed request, projecting
//! an answer an agent can act on, and translating failure onto the protocol.
//! Authorization, tenancy, audit, and the revocation epoch all stay with the
//! server operations this module calls, so there is one implementation of each
//! rather than an MCP-shaped copy.
//!
//! Read tools are always advertised. Write tools appear only when the caller's
//! token carries the permission they need, so an agent never discovers a
//! capability it cannot use and an under-scoped agent cannot invoke one by
//! naming it directly.
//!
//! Credential issuance is deliberately absent. A plaintext credential returned
//! into an agent's context window would be copied into a transcript, a log, and
//! a model provider's request body in one step — the one place where a
//! once-returned secret is least defensible. Agents observe and revoke; a human
//! or a deployment pipeline issues.

use std::sync::Arc;

use rmcp::model::{CallToolResult, Tool, ToolAnnotations};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use uuid::Uuid;
use wyrd_runtime::Permission;
use wyrd_spec::error::WyrdError;

use super::WyrdMcpHandler;
use crate::components::auth::Caller;
use crate::http::error::internal_failure;

/// Wire name of the credential-metadata listing.
pub(super) const LIST_CREDENTIALS: &str = "principals.list_credentials";

/// Wire name of the credential revocation.
pub(super) const REVOKE_CREDENTIAL: &str = "principals.revoke_credential";

/// The tools every authenticated caller may be shown.
///
/// Read tools are unconditional, which is the MCP contract: an agent can always
/// discover and observe. Nothing here returns secret material.
#[must_use]
pub(super) fn descriptors_unscoped() -> Vec<Tool> {
    vec![list_credentials_tool()]
}

/// The tools shown only to a caller holding principal administration.
///
/// Advertised per caller so an agent never discovers a capability it cannot
/// use. This is a planning convenience, not the boundary: the operation behind
/// the tool authorizes the same permission and audits that decision, and it
/// refuses an under-scoped caller that names the tool directly. Re-checking
/// here would refuse identically while auditing nothing.
#[must_use]
pub(super) fn write_descriptors() -> Vec<Tool> {
    vec![revoke_credential_tool()]
}

/// Whether this caller holds tenant principal administration.
///
/// Reads the same permission the HTTP surface requires, from the verified
/// token, so the two cannot disagree about who may write.
pub(super) fn may_administer(caller: &Caller) -> bool {
    caller
        .principal
        .effective_permissions
        .contains(&Permission::service_accounts_write())
}

/// Build one tool descriptor from its static input schema.
///
/// # Panics
/// Panics if `schema` is not a JSON object, which every caller below passes.
fn tool(
    name: &'static str,
    title: &str,
    description: &'static str,
    read_only: bool,
    schema: JsonValue,
) -> Tool {
    let JsonValue::Object(schema) = schema else {
        unreachable!("every principal tool input schema is a JSON object")
    };
    Tool::new(name, description, Arc::new(schema))
        .with_title(title)
        .annotate({
            let mut annotations = ToolAnnotations::default().read_only(read_only);
            annotations.destructive_hint = Some(!read_only);
            annotations
        })
}

/// Descriptor for the credential-metadata listing.
fn list_credentials_tool() -> Tool {
    tool(
        LIST_CREDENTIALS,
        "List a principal's credentials",
        "List the non-secret credential metadata for one principal in this caller's tenant: \
         identifier, lookup prefix, creation, expiry, revocation, and last use. Never returns \
         secret material. Includes revoked and expired credentials, so a rotation can be \
         confirmed complete.",
        true,
        serde_json::json!({
            "type": "object",
            "properties": {
                "principal_id": {
                    "type": "string",
                    "description": "Principal whose credentials to list, as a UUID."
                }
            },
            "required": ["principal_id"],
            "additionalProperties": false
        }),
    )
}

/// Descriptor for the credential revocation.
fn revoke_credential_tool() -> Tool {
    tool(
        REVOKE_CREDENTIAL,
        "Revoke a credential",
        "Retire one credential belonging to a principal in this caller's tenant. The credential \
         can mint no further token and the tokens it already minted stop authorizing. The \
         principal, its roles, and its other credentials are untouched. Requires \
         service_accounts:write.",
        false,
        serde_json::json!({
            "type": "object",
            "properties": {
                "principal_id": {
                    "type": "string",
                    "description": "Principal that owns the credential, as a UUID."
                },
                "credential_id": {
                    "type": "string",
                    "description": "Credential to retire, as a UUID."
                }
            },
            "required": ["principal_id", "credential_id"],
            "additionalProperties": false
        }),
    )
}

/// Arguments naming one principal.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PrincipalArgs {
    /// Principal whose credentials to read.
    principal_id: String,
}

/// Arguments naming one credential and the principal that owns it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CredentialArgs {
    /// Principal that owns the credential.
    principal_id: String,
    /// Credential to retire.
    credential_id: String,
}

/// Parse a UUID argument, refusing anything else.
///
/// # Errors
/// Returns [`WyrdError::Validation`] when the value is not a UUID.
fn uuid_arg(value: &str, field: &str) -> Result<Uuid, WyrdError> {
    value.parse::<Uuid>().map_err(|_| WyrdError::Validation {
        message: format!("{field} must be a UUID"),
        details: serde_json::json!({ "field": field }),
    })
}

/// Decode a tool's closed argument object.
///
/// # Errors
/// Returns [`WyrdError::Validation`] when the arguments are absent or do not
/// match the advertised schema.
fn parse_args<T: serde::de::DeserializeOwned>(
    arguments: Option<serde_json::Map<String, JsonValue>>,
    tool: &str,
) -> Result<T, WyrdError> {
    let arguments = arguments.ok_or_else(|| WyrdError::Validation {
        message: format!("{tool} requires arguments"),
        details: serde_json::json!({ "tool": tool }),
    })?;
    serde_json::from_value(JsonValue::Object(arguments)).map_err(|error| WyrdError::Validation {
        message: format!("{tool} arguments are invalid: {error}"),
        details: serde_json::json!({ "tool": tool }),
    })
}

impl WyrdMcpHandler {
    /// List one principal's credential metadata.
    ///
    /// # Errors
    /// Returns a Wyrd error when the arguments are invalid, the caller is
    /// unauthorized, or the read fails.
    pub(super) async fn mcp_list_credentials(
        &self,
        caller: Caller,
        arguments: Option<serde_json::Map<String, JsonValue>>,
    ) -> Result<CallToolResult, WyrdError> {
        let args: PrincipalArgs = parse_args(arguments, LIST_CREDENTIALS)?;
        let principal_id = uuid_arg(&args.principal_id, "principal_id")?;
        let listing = crate::components::principals::routes::list_credentials_for(
            &self.state,
            &caller,
            principal_id,
        )
        .await?;

        Ok(CallToolResult::structured(
            serde_json::to_value(listing).map_err(|error| {
                internal_failure("credential listing could not be projected", &error)
            })?,
        ))
    }

    /// Retire one credential.
    ///
    /// # Errors
    /// Returns a Wyrd error when the arguments are invalid, the caller lacks
    /// principal administration, the credential is not found for that
    /// principal, or a write fails.
    pub(super) async fn mcp_revoke_credential(
        &self,
        caller: Caller,
        arguments: Option<serde_json::Map<String, JsonValue>>,
    ) -> Result<CallToolResult, WyrdError> {
        let args: CredentialArgs = parse_args(arguments, REVOKE_CREDENTIAL)?;
        let principal_id = uuid_arg(&args.principal_id, "principal_id")?;
        let credential_id = uuid_arg(&args.credential_id, "credential_id")?;
        crate::components::principals::routes::revoke_credential_for(
            &self.state,
            &caller,
            principal_id,
            credential_id,
        )
        .await?;

        Ok(CallToolResult::structured(serde_json::json!({
            "revoked": true,
            "credential_id": credential_id.to_string(),
        })))
    }
}
