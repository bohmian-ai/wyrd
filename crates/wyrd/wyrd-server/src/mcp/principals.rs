//! Tenant principal and credential capabilities over MCP.
//!
//! The adapter owns exactly three things: parsing a closed request, projecting
//! an answer an agent can act on, and translating failure onto the protocol.
//! Authorization, tenancy, audit, and credential retirement all stay with the
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

use serde_json::Map as JsonMap;
use std::sync::Arc;

use rmcp::model::{CallToolResult, Tool, ToolAnnotations};
use schemars::JsonSchema;
use schemars::SchemaGenerator;
use serde::de::DeserializeOwned;
use serde_json::Value as JsonValue;
use wyrd_runtime::Permission;
use wyrd_spec::auth::{
    CredentialListResponse, CredentialRevoked, ListCredentialsArgs, PrincipalSummary,
    RevokeCredentialArgs, SetWorkloadRestrictionArgs,
};
use wyrd_spec::error::WyrdError;

use super::WyrdMcpHandler;
use crate::components::auth::Caller;
use crate::http::error::internal_failure;

/// Wire name of the credential-metadata listing.
pub(super) const LIST_CREDENTIALS: &str = "principals.list_credentials";

/// Wire name of the credential revocation.
pub(super) const REVOKE_CREDENTIAL: &str = "principals.revoke_credential";

/// Wire name of the Service account workload-restriction replacement.
pub(super) const SET_WORKLOAD_RESTRICTION: &str = "principals.set_workload_restriction";

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

/// The tools shown only to a caller holding tenant administration (`*`).
///
/// A family restriction needs more than principal administration, so it is
/// advertised separately; the operation still authorizes and audits it.
#[must_use]
pub(super) fn admin_descriptors() -> Vec<Tool> {
    vec![set_workload_restriction_tool()]
}

/// Whether this caller holds tenant administration.
pub(super) fn may_administer_tenant(caller: &Caller) -> bool {
    caller
        .principal
        .effective_permissions
        .contains(&Permission::wildcard())
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

/// Derive one tool schema from the Rust type the tool actually speaks.
///
/// The schema comes from the workspace's default JSON Schema 2020-12
/// generator, the same one every other contract uses, and is handed to rmcp as
/// a raw object.
///
/// # Panics
/// Panics when the derived schema is not a JSON object, which the `JsonSchema`
/// derive cannot produce for a struct.
pub(super) fn schema_of<T: JsonSchema>() -> Arc<JsonMap<String, JsonValue>> {
    let root = SchemaGenerator::default().into_root_schema_for::<T>();
    let JsonValue::Object(schema) = root.to_value() else {
        panic!("a derived struct schema is a JSON object");
    };
    Arc::new(schema)
}

/// Build one tool descriptor around the types it actually speaks.
///
/// Both schemas come from the shared `wyrd-spec` DTOs the handler below
/// deserializes and returns, so the advertised contract is the parsed one. A
/// handwritten schema beside a separate struct is how the two drift.
///
/// `I` must be a struct so the input root is a plain object: MCP requires an
/// object input, and tool-calling consumers such as OpenAI function tools
/// reject a root union. A tagged union belongs under a named field.
pub(super) fn tool<I, O>(
    name: &'static str,
    title: &str,
    description: &'static str,
    read_only: bool,
) -> Tool
where
    I: JsonSchema,
    O: JsonSchema,
{
    Tool::new(name, description, schema_of::<I>())
        .with_raw_output_schema(schema_of::<O>())
        .with_title(title)
        .annotate({
            let mut annotations = ToolAnnotations::default().read_only(read_only);
            annotations.destructive_hint = Some(!read_only);
            annotations
        })
}

/// Descriptor for the credential-metadata listing.
fn list_credentials_tool() -> Tool {
    tool::<ListCredentialsArgs, CredentialListResponse>(
        LIST_CREDENTIALS,
        "List a principal's credentials",
        "List the non-secret credential metadata for one principal in this caller's tenant: \
         identifier, lookup prefix, creation, expiry, revocation, and last use. Never returns \
         secret material. Includes revoked and expired credentials, so a rotation can be \
         confirmed complete.",
        true,
    )
}

/// Descriptor for the credential revocation.
fn revoke_credential_tool() -> Tool {
    tool::<RevokeCredentialArgs, CredentialRevoked>(
        REVOKE_CREDENTIAL,
        "Revoke a credential",
        "Retire one credential belonging to a principal in this caller's tenant. The credential \
         can mint no further token; a token it already minted lapses within five minutes. The \
         principal, its roles, and its other credentials are untouched. Requires \
         service_accounts:write.",
        false,
    )
}

/// Descriptor for the workload-restriction replacement.
fn set_workload_restriction_tool() -> Tool {
    tool::<SetWorkloadRestrictionArgs, PrincipalSummary>(
        SET_WORKLOAD_RESTRICTION,
        "Set a Service account's workload restriction",
        "Replace one Service account's workload restriction in this caller's tenant with \
         {\"mode\":\"unrestricted\"} or {\"mode\":\"family\",\"kind\":\"Service\"|\"Agent\",\
         \"name\":...}. A family restriction makes the account that family's unique scheduled \
         runtime account; another live account holding it is a conflict. Idempotent for an \
         unchanged value; the id, roles, and credentials are untouched. Requires tenant \
         administration (*).",
        false,
    )
}

/// Decode a tool's closed argument object.
///
/// # Errors
/// Returns [`WyrdError::Validation`] when the arguments are absent or do not
/// match the advertised schema.
pub(super) fn parse_args<T: DeserializeOwned>(
    arguments: Option<JsonMap<String, JsonValue>>,
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
        arguments: Option<JsonMap<String, JsonValue>>,
    ) -> Result<CallToolResult, WyrdError> {
        let args: ListCredentialsArgs = parse_args(arguments, LIST_CREDENTIALS)?;
        let listing = crate::components::principals::routes::list_credentials_for(
            &self.state,
            &caller,
            args.principal_id.as_uuid(),
        )
        .await?;

        Ok(CallToolResult::structured(
            serde_json::to_value(listing).map_err(|error| {
                internal_failure("credential listing could not be projected", &error)
            })?,
        ))
    }

    /// Replace one Service account's workload restriction.
    ///
    /// # Errors
    /// Returns a Wyrd error when the arguments are invalid, the caller lacks
    /// tenant administration, the target is unknown or a User, another account
    /// holds the family, or a write fails.
    pub(super) async fn mcp_set_workload_restriction(
        &self,
        caller: Caller,
        arguments: Option<JsonMap<String, JsonValue>>,
    ) -> Result<CallToolResult, WyrdError> {
        let args: SetWorkloadRestrictionArgs = parse_args(arguments, SET_WORKLOAD_RESTRICTION)?;
        let summary = crate::components::principals::routes::set_workload_restriction_for(
            &self.state,
            &caller,
            args.principal_id.as_uuid(),
            &args.workload_restriction,
        )
        .await?;

        Ok(CallToolResult::structured(
            serde_json::to_value(summary).map_err(|error| {
                internal_failure("principal summary could not be projected", &error)
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
        arguments: Option<JsonMap<String, JsonValue>>,
    ) -> Result<CallToolResult, WyrdError> {
        let args: RevokeCredentialArgs = parse_args(arguments, REVOKE_CREDENTIAL)?;
        crate::components::principals::routes::revoke_credential_for(
            &self.state,
            &caller,
            args.principal_id.as_uuid(),
            args.credential_id,
        )
        .await?;

        Ok(CallToolResult::structured(
            serde_json::to_value(CredentialRevoked {
                revoked: true,
                credential_id: args.credential_id,
            })
            .map_err(|error| {
                internal_failure("credential revocation could not be projected", &error)
            })?,
        ))
    }
}
