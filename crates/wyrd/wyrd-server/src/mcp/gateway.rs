//! Tenant gateway administration tools advertised over MCP.
//!
//! Each tool is a projection of one `/v1/admin/gateway/...` route and returns
//! the same redacted contract through [`GatewayAdministration`], which owns
//! authorization, audit, validation, and tenancy. Replacement tools take the
//! route's request body as their arguments and publish that contract's
//! derived JSON Schema; named reads, revocation, and deletions take the
//! resource `name`. Reads require `gateway:read`, replacements and revocation
//! `gateway:write`, and deletions `gateway:delete` or `gateway:write` exactly
//! as the HTTP routes do. Read tools are always advertised; a write tool is
//! advertised only to a caller already holding the permission it enforces.

use std::future::Future;
use std::sync::Arc;

use rmcp::model::{CallToolResult, ErrorData, Tool, ToolAnnotations};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value as JsonValue};
use wyrd_runtime::Permission;
use wyrd_spec::error::WyrdError;
use wyrd_spec::gateway::{
    GatewayCapturePolicyWrite, GatewayContractError, GatewayFallbackPolicy,
    GatewayGovernancePolicy, ProviderCredentialWrite, ProviderDeployment, ProviderDeploymentsApply,
};
use wyrd_spec::ids::{ProviderCredentialName, ProviderDeploymentName};

use super::WyrdMcpHandler;
use super::principals::schema_of;
use crate::components::auth::Caller;
use crate::components::gateway::{GatewayAdministration, invalid};

/// Wire name of the redacted provider credential listing.
pub(super) const LIST_CREDENTIALS: &str = "gateway.list_provider_credentials";

/// Wire name of the provider deployment listing.
pub(super) const LIST_DEPLOYMENTS: &str = "gateway.list_provider_deployments";

/// Wire name of the effective fallback policy read.
pub(super) const GET_FALLBACK: &str = "gateway.get_fallback_policy";

/// Wire name of the effective governance policy read.
pub(super) const GET_GOVERNANCE: &str = "gateway.get_governance_policy";

/// Wire name of the effective capture policy read.
pub(super) const GET_CAPTURE: &str = "gateway.get_capture_policy";

/// Wire name of the single redacted provider credential read.
pub(super) const GET_CREDENTIAL: &str = "gateway.get_provider_credential";

/// Wire name of the single provider deployment read.
pub(super) const GET_DEPLOYMENT: &str = "gateway.get_provider_deployment";

/// Wire name of the provider credential create-or-rotate.
pub(super) const PUT_CREDENTIAL: &str = "gateway.put_provider_credential";

/// Wire name of the provider deployment create-or-replace.
pub(super) const PUT_DEPLOYMENT: &str = "gateway.put_provider_deployment";

/// Wire name of the atomic credential-and-deployment apply.
pub(super) const APPLY_DEPLOYMENTS: &str = "gateway.apply_provider_deployments";

/// Wire name of the fallback policy replacement.
pub(super) const PUT_FALLBACK: &str = "gateway.put_fallback_policy";

/// Wire name of the governance policy replacement.
pub(super) const PUT_GOVERNANCE: &str = "gateway.put_governance_policy";

/// Wire name of the capture policy replacement.
pub(super) const PUT_CAPTURE: &str = "gateway.put_capture_policy";

/// Wire name of the terminal provider credential revocation.
pub(super) const REVOKE_CREDENTIAL: &str = "gateway.revoke_provider_credential";

/// Wire name of the unreferenced provider credential deletion.
pub(super) const DELETE_CREDENTIAL: &str = "gateway.delete_provider_credential";

/// Wire name of the provider deployment deletion.
pub(super) const DELETE_DEPLOYMENT: &str = "gateway.delete_provider_deployment";

/// Wire name of the fallback policy reset to its default.
pub(super) const DELETE_FALLBACK: &str = "gateway.delete_fallback_policy";

/// Wire name of the governance policy reset to its default.
pub(super) const DELETE_GOVERNANCE: &str = "gateway.delete_governance_policy";

/// Every gateway tool in catalog order: reads, replacements, then destructive
/// operations.
pub(super) const TOOLS: [&str; 18] = [
    LIST_CREDENTIALS,
    LIST_DEPLOYMENTS,
    GET_FALLBACK,
    GET_GOVERNANCE,
    GET_CAPTURE,
    GET_CREDENTIAL,
    GET_DEPLOYMENT,
    PUT_CREDENTIAL,
    PUT_DEPLOYMENT,
    APPLY_DEPLOYMENTS,
    PUT_FALLBACK,
    PUT_GOVERNANCE,
    PUT_CAPTURE,
    REVOKE_CREDENTIAL,
    DELETE_CREDENTIAL,
    DELETE_DEPLOYMENT,
    DELETE_FALLBACK,
    DELETE_GOVERNANCE,
];

/// Input schema of tools that take no arguments.
const NO_INPUT: &str = r#"{"type":"object","properties":{},"additionalProperties":false}"#;

/// Input schema of tools addressing one named tenant resource.
const NAME_INPUT: &str = r#"{"type":"object","required":["name"],"properties":{"name":{"type":"string","description":"Tenant-scoped resource name"}},"additionalProperties":false}"#;

/// Parses one of the static argument-free or named-resource input schemas.
///
/// # Panics
///
/// Panics if `schema` is not a JSON object, which [`NO_INPUT`] and
/// [`NAME_INPUT`] never are.
fn literal(schema: &'static str) -> Arc<Map<String, JsonValue>> {
    let Ok(schema) = serde_json::from_str(schema) else {
        unreachable!("every static gateway tool input schema is a JSON object")
    };
    Arc::new(schema)
}

/// How a tool's effects are annotated for agents.
#[derive(Clone, Copy)]
enum Effect {
    /// Reads without changing tenant state.
    Read,
    /// Replaces a resource; repeating the same arguments has no further effect.
    Replace,
    /// Revokes, deletes, or resets a resource; repeating is harmless.
    Destroy,
}

impl Effect {
    /// MCP annotations advertising this effect.
    fn annotations(self) -> ToolAnnotations {
        match self {
            Self::Read => ToolAnnotations::default().read_only(true),
            Self::Replace => ToolAnnotations::default()
                .read_only(false)
                .destructive(false)
                .idempotent(true)
                .open_world(false),
            Self::Destroy => ToolAnnotations::default()
                .read_only(false)
                .destructive(true)
                .idempotent(true)
                .open_world(false),
        }
    }
}

/// The gateway administration tools in [`TOOLS`] order, each paired with the
/// permission its operation enforces.
///
/// The permission decides who is shown a write tool; dispatch still authorizes
/// every call through [`GatewayAdministration`]. Replacement inputs are derived
/// from the same `wyrd_spec::gateway` request types the handler deserializes.
///
/// # Panics
///
/// Panics if an input schema is not a JSON object, which the static literals
/// and derived request-struct schemas never are.
fn gated() -> Vec<(Tool, Permission)> {
    [
        (
            LIST_CREDENTIALS,
            "List gateway provider credentials",
            "List this tenant's provider credentials as redacted views: name, provider, source \
             kind and operator reference, state, and timestamps. Never returns secret material. \
             Requires gateway read permission.",
            literal(NO_INPUT),
            Effect::Read,
            Permission::gateway_read(),
        ),
        (
            LIST_DEPLOYMENTS,
            "List gateway provider deployments",
            "List this tenant's provider deployments: model, adapter, upstream auth by credential \
             name, declared capabilities, and routing weight. Requires gateway read permission.",
            literal(NO_INPUT),
            Effect::Read,
            Permission::gateway_read(),
        ),
        (
            GET_FALLBACK,
            "Get gateway fallback policy",
            "Read this tenant's effective gateway fallback policy. Requires gateway read \
             permission.",
            literal(NO_INPUT),
            Effect::Read,
            Permission::gateway_read(),
        ),
        (
            GET_GOVERNANCE,
            "Get gateway governance policy",
            "Read this tenant's effective gateway limits, budgets, pricing, and unknown-cost \
             policy. Requires gateway read permission.",
            literal(NO_INPUT),
            Effect::Read,
            Permission::gateway_read(),
        ),
        (
            GET_CAPTURE,
            "Get gateway capture policy",
            "Read this tenant's effective gateway payload capture policy and its version. \
             Requires gateway read permission.",
            literal(NO_INPUT),
            Effect::Read,
            Permission::gateway_read(),
        ),
        (
            GET_CREDENTIAL,
            "Get gateway provider credential",
            "Read one redacted provider credential by name. Never returns secret material. \
             Requires gateway read permission.",
            literal(NAME_INPUT),
            Effect::Read,
            Permission::gateway_read(),
        ),
        (
            GET_DEPLOYMENT,
            "Get gateway provider deployment",
            "Read one provider deployment by name. Requires gateway read permission.",
            literal(NAME_INPUT),
            Effect::Read,
            Permission::gateway_read(),
        ),
        (
            PUT_CREDENTIAL,
            "Put gateway provider credential",
            "Create or rotate a provider credential from an operator-declared environment \
             binding, an external secret reference, or a write-only managed secret, and \
             return its redacted view. Environment and external-secret sources name operator \
             configuration and carry no secret value. A managed secret accepts the value once \
             as bounded plaintext, seals it into a tenant-encrypted envelope before it is \
             stored, and is never readable again: responses, listings, logs, and errors show \
             only the redacted source. Requires gateway write permission.",
            schema_of::<ProviderCredentialWrite>(),
            Effect::Replace,
            Permission::gateway_write(),
        ),
        (
            PUT_DEPLOYMENT,
            "Put gateway provider deployment",
            "Create or replace a provider deployment. Newly admitted calls observe the change. \
             Requires gateway write permission.",
            schema_of::<ProviderDeployment>(),
            Effect::Replace,
            Permission::gateway_write(),
        ),
        (
            APPLY_DEPLOYMENTS,
            "Apply gateway provider deployments",
            "Create or rotate provider credentials and create or replace the deployments that \
             use them in one tenant transaction: every named resource commits or none does, \
             and unnamed resources are untouched. Credentials take the same sources as the \
             credential put; a managed secret is sealed into a tenant-encrypted envelope \
             before it is stored and only redacted views are returned. Every submitted \
             credential must be referenced by a submitted deployment. Requires gateway write \
             permission.",
            schema_of::<ProviderDeploymentsApply>(),
            Effect::Replace,
            Permission::gateway_write(),
        ),
        (
            PUT_FALLBACK,
            "Put gateway fallback policy",
            "Replace this tenant's gateway fallback policy. Requires gateway write permission.",
            schema_of::<GatewayFallbackPolicy>(),
            Effect::Replace,
            Permission::gateway_write(),
        ),
        (
            PUT_GOVERNANCE,
            "Put gateway governance policy",
            "Replace this tenant's gateway limits, budgets, pricing, and unknown-cost policy. \
             Requires gateway write permission.",
            schema_of::<GatewayGovernancePolicy>(),
            Effect::Replace,
            Permission::gateway_write(),
        ),
        (
            PUT_CAPTURE,
            "Put gateway capture policy",
            "Replace this tenant's gateway payload capture policy and return its version. \
             Requires gateway write permission.",
            schema_of::<GatewayCapturePolicyWrite>(),
            Effect::Replace,
            Permission::gateway_write(),
        ),
        (
            REVOKE_CREDENTIAL,
            "Revoke gateway provider credential",
            "Terminally revoke a provider credential and return its redacted view; repeating \
             returns the same revocation. Requires gateway write permission.",
            literal(NAME_INPUT),
            Effect::Destroy,
            Permission::gateway_write(),
        ),
        (
            DELETE_CREDENTIAL,
            "Delete gateway provider credential",
            "Delete a provider credential no deployment references; an absent name succeeds. \
             Requires gateway delete permission.",
            literal(NAME_INPUT),
            Effect::Destroy,
            Permission::gateway_delete(),
        ),
        (
            DELETE_DEPLOYMENT,
            "Delete gateway provider deployment",
            "Delete a provider deployment; an absent name succeeds. Requires gateway delete \
             permission.",
            literal(NAME_INPUT),
            Effect::Destroy,
            Permission::gateway_delete(),
        ),
        (
            DELETE_FALLBACK,
            "Reset gateway fallback policy",
            "Restore this tenant's default gateway fallback policy. Requires gateway write \
             permission.",
            literal(NO_INPUT),
            Effect::Destroy,
            Permission::gateway_write(),
        ),
        (
            DELETE_GOVERNANCE,
            "Reset gateway governance policy",
            "Restore this tenant's default gateway governance policy. Requires gateway write \
             permission.",
            literal(NO_INPUT),
            Effect::Destroy,
            Permission::gateway_write(),
        ),
    ]
    .into_iter()
    .map(|(name, title, description, schema, effect, permission)| {
        let tool = Tool::new(name, description, schema)
            .with_title(title)
            .annotate(effect.annotations());
        (tool, permission)
    })
    .collect()
}

/// Every gateway tool in [`TOOLS`] order, for name resolution and catalog checks.
///
/// # Panics
///
/// Panics, through [`gated`], if a gateway input schema is not a JSON object,
/// which the static literals and derived request-struct schemas never are.
#[must_use]
pub(super) fn descriptors() -> Vec<Tool> {
    gated().into_iter().map(|(tool, _)| tool).collect()
}

/// The gateway read tools every authenticated caller may be shown.
///
/// # Panics
///
/// Panics, through [`gated`], if a gateway input schema is not a JSON object,
/// which the static literals and derived request-struct schemas never are.
#[must_use]
pub(super) fn descriptors_unscoped() -> Vec<Tool> {
    gated()
        .into_iter()
        .filter(|(_, permission)| *permission == Permission::gateway_read())
        .map(|(tool, _)| tool)
        .collect()
}

/// The gateway write, revoke, reset, and delete tools shown to `caller`.
///
/// A tool is listed only when the caller's verified permissions already grant
/// what its operation enforces, so an agent is never offered a change it
/// cannot make.
///
/// # Panics
///
/// Panics, through [`gated`], if a gateway input schema is not a JSON object,
/// which the static literals and derived request-struct schemas never are.
#[must_use]
pub(super) fn write_descriptors(caller: &Caller) -> Vec<Tool> {
    gated()
        .into_iter()
        .filter(|(_, permission)| {
            *permission != Permission::gateway_read()
                && caller.principal.effective_permissions.contains(permission)
        })
        .map(|(tool, _)| tool)
        .collect()
}

impl WyrdMcpHandler {
    /// Runs one gateway administration tool through [`GatewayAdministration`].
    ///
    /// Argument-free tools reject any supplied argument as MCP invalid params.
    /// Names and replacement bodies are decoded here; a decode failure becomes
    /// the stable `GatewayInvalidConfiguration` tool error without echoing the
    /// submitted input, as the HTTP body mapper does. Credential and deployment
    /// replacements address the resource named inside the body.
    ///
    /// # Errors
    ///
    /// Returns MCP invalid params for arguments on an argument-free tool or an
    /// unknown gateway tool name. Decode, authorization, validation, conflict,
    /// audit, and storage failures become canonical structured tool errors.
    pub(super) async fn gateway_tool(
        &self,
        name: &str,
        caller: Caller,
        arguments: Option<Map<String, JsonValue>>,
    ) -> Result<CallToolResult, ErrorData> {
        let gateway = GatewayAdministration::new(&self.state);
        let caller = &caller;
        match name {
            LIST_CREDENTIALS => {
                no_arguments(name, arguments)?;
                Ok(structured(gateway.credentials(caller)).await)
            }
            LIST_DEPLOYMENTS => {
                no_arguments(name, arguments)?;
                Ok(structured(gateway.deployments(caller)).await)
            }
            GET_FALLBACK => {
                no_arguments(name, arguments)?;
                Ok(structured(gateway.fallback(caller)).await)
            }
            GET_GOVERNANCE => {
                no_arguments(name, arguments)?;
                Ok(structured(gateway.governance(caller)).await)
            }
            GET_CAPTURE => {
                no_arguments(name, arguments)?;
                Ok(structured(gateway.capture(caller)).await)
            }
            DELETE_FALLBACK => {
                no_arguments(name, arguments)?;
                Ok(structured(gateway.delete_fallback(caller)).await)
            }
            DELETE_GOVERNANCE => {
                no_arguments(name, arguments)?;
                Ok(structured(gateway.delete_governance(caller)).await)
            }
            GET_CREDENTIAL => Ok(structured(async {
                gateway
                    .credential(caller, &credential_name(arguments)?)
                    .await
            })
            .await),
            REVOKE_CREDENTIAL => Ok(structured(async {
                gateway
                    .revoke_credential(caller, &credential_name(arguments)?)
                    .await
            })
            .await),
            DELETE_CREDENTIAL => Ok(structured(async {
                gateway
                    .delete_credential(caller, &credential_name(arguments)?)
                    .await
            })
            .await),
            GET_DEPLOYMENT => Ok(structured(async {
                gateway
                    .deployment(caller, &deployment_name(arguments)?)
                    .await
            })
            .await),
            DELETE_DEPLOYMENT => Ok(structured(async {
                gateway
                    .delete_deployment(caller, &deployment_name(arguments)?)
                    .await
            })
            .await),
            PUT_CREDENTIAL => Ok(structured(async {
                let write = body::<ProviderCredentialWrite>(arguments)?;
                let name = write.name.clone();
                gateway.put_credential(caller, &name, write).await
            })
            .await),
            PUT_DEPLOYMENT => Ok(structured(async {
                let deployment = body::<ProviderDeployment>(arguments)?;
                let name = deployment.name.clone();
                gateway.put_deployment(caller, &name, deployment).await
            })
            .await),
            APPLY_DEPLOYMENTS => {
                Ok(
                    structured(async { gateway.apply_deployments(caller, body(arguments)?).await })
                        .await,
                )
            }
            PUT_FALLBACK => {
                Ok(
                    structured(async { gateway.put_fallback(caller, body(arguments)?).await })
                        .await,
                )
            }
            PUT_GOVERNANCE => {
                Ok(
                    structured(async { gateway.put_governance(caller, body(arguments)?).await })
                        .await,
                )
            }
            PUT_CAPTURE => {
                Ok(structured(async { gateway.put_capture(caller, body(arguments)?).await }).await)
            }
            unknown => Err(ErrorData::invalid_params(
                format!("unknown gateway tool: {unknown}"),
                None,
            )),
        }
    }
}

/// Refuses arguments on an argument-free tool.
///
/// # Errors
///
/// Returns MCP invalid params when `arguments` is a non-empty object.
fn no_arguments(name: &str, arguments: Option<Map<String, JsonValue>>) -> Result<(), ErrorData> {
    if arguments.is_some_and(|arguments| !arguments.is_empty()) {
        return Err(ErrorData::invalid_params(
            format!("{name} accepts no arguments; omit them or send an empty object."),
            None,
        ));
    }
    Ok(())
}

/// Decodes a gateway contract from tool arguments.
///
/// The serde message is discarded because it may quote submitted input.
///
/// # Errors
///
/// Returns `GatewayInvalidConfiguration` for field `arguments` when the
/// arguments do not match the contract shape.
fn body<T: DeserializeOwned>(arguments: Option<Map<String, JsonValue>>) -> Result<T, WyrdError> {
    serde_json::from_value(JsonValue::Object(arguments.unwrap_or_default())).map_err(|_| {
        invalid(GatewayContractError::new(
            "arguments",
            "must be a JSON object matching the resource contract",
        ))
    })
}

/// Arguments of tools addressing one named resource.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct NameArguments {
    /// Raw tenant-scoped resource name.
    name: String,
}

/// Decodes the `name` argument and validates it with `parse`.
///
/// # Errors
///
/// Returns `GatewayInvalidConfiguration` when the arguments are not exactly
/// `{name}` or the name is not a valid resource name.
fn named<N, E>(
    arguments: Option<Map<String, JsonValue>>,
    parse: impl FnOnce(&str) -> Result<N, E>,
) -> Result<N, WyrdError> {
    let NameArguments { name } = body(arguments)?;
    parse(&name).map_err(|_| {
        invalid(GatewayContractError::new(
            "name",
            "is not a valid resource name",
        ))
    })
}

/// Decodes a provider credential name argument.
///
/// # Errors
///
/// Returns `GatewayInvalidConfiguration` as [`named`] does.
fn credential_name(
    arguments: Option<Map<String, JsonValue>>,
) -> Result<ProviderCredentialName, WyrdError> {
    named(arguments, |raw| ProviderCredentialName::new(raw))
}

/// Decodes a provider deployment name argument.
///
/// # Errors
///
/// Returns `GatewayInvalidConfiguration` as [`named`] does.
fn deployment_name(
    arguments: Option<Map<String, JsonValue>>,
) -> Result<ProviderDeploymentName, WyrdError> {
    named(arguments, |raw| ProviderDeploymentName::new(raw))
}

/// Awaits one administration operation and projects it onto a tool result.
///
/// Listings are wrapped as `{"items": [...]}` and a unit outcome becomes `{}`
/// because MCP structured content must be a JSON object; single resources are
/// returned unchanged.
async fn structured<T: Serialize>(
    operation: impl Future<Output = Result<T, WyrdError>>,
) -> CallToolResult {
    let value = operation.await.and_then(|value| {
        serde_json::to_value(value).map_err(|error| WyrdError::Internal {
            message: format!("gateway tool result could not be serialized: {error}"),
            details: serde_json::json!({}),
        })
    });
    match value {
        Ok(JsonValue::Array(items)) => {
            CallToolResult::structured(serde_json::json!({ "items": items }))
        }
        Ok(JsonValue::Null) => CallToolResult::structured(serde_json::json!({})),
        Ok(value) => CallToolResult::structured(value),
        Err(error) => CallToolResult::structured_error(error.as_problem_json()),
    }
}

#[cfg(test)]
mod tests {
    use super::{TOOLS, descriptors};

    /// Proves the catalog advertises every gateway tool once, in order, with
    /// object input schemas and effect annotations matching its operation.
    ///
    /// # Panics
    /// Panics when the advertised names or order differ from [`TOOLS`], when
    /// a tool lacks annotations, has effect hints that disagree with its
    /// read/destructive operation, or a non-object input schema, when the
    /// credential write (index 7) lacks its description, a required storage
    /// claim, or still denies accepting secrets, and when the collection
    /// apply (index 9) or the name-keyed tool at index 14 has a different
    /// schema title or required fields.
    #[test]
    fn gateway_catalog_annotates_every_administration_operation() {
        let tools = descriptors();
        let names: Vec<&str> = tools.iter().map(|tool| tool.name.as_ref()).collect();
        assert_eq!(names, TOOLS);
        for tool in &tools {
            let annotations = tool
                .annotations
                .as_ref()
                .expect("gateway tools are annotated");
            let name = tool.name.as_ref();
            let read = name.contains(".get_") || name.contains(".list_");
            let destructive = name.contains(".delete_") || name.contains(".revoke_");
            assert_eq!(annotations.read_only_hint, Some(read), "{name}");
            if !read {
                assert_eq!(annotations.destructive_hint, Some(destructive), "{name}");
                assert_eq!(annotations.idempotent_hint, Some(true), "{name}");
            }
            assert_eq!(tool.input_schema["type"], "object", "{name}");
        }
        let put_credential = &tools[7];
        assert_eq!(
            put_credential.input_schema["title"],
            "ProviderCredentialWrite"
        );
        let put_credential_description = put_credential
            .description
            .as_ref()
            .expect("the credential write is described");
        for claim in [
            "environment",
            "external secret",
            "managed secret",
            "tenant-encrypted envelope",
            "redacted",
            "gateway write permission",
        ] {
            assert!(
                put_credential_description.contains(claim),
                "put-credential description omits {claim}: {put_credential_description}"
            );
        }
        assert!(
            !put_credential_description.contains("never accepts or stores secret values"),
            "put-credential description still denies accepting any secret value: \
             {put_credential_description}"
        );
        assert_eq!(tools[9].input_schema["title"], "ProviderDeploymentsApply");
        assert_eq!(
            tools[14].input_schema["required"],
            serde_json::json!(["name"])
        );
    }
}
