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
//! Access administration (an account's access summary, roles, scoped role
//! bindings and local groups) calls the same HTTP operations, so discovery,
//! validation, tenant administration and audit are decided once.
//!
//! Credential issuance is deliberately absent. A plaintext credential returned
//! into an agent's context window would be copied into a transcript, a log, and
//! a model provider's request body in one step — the one place where a
//! once-returned secret is least defensible. Agents observe and revoke; a human
//! or a deployment pipeline issues.

use serde_json::Map as JsonMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use rmcp::model::{CallToolResult, Tool, ToolAnnotations};
use schemars::JsonSchema;
use schemars::SchemaGenerator;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value as JsonValue;
use wyrd_runtime::Permission;
use wyrd_spec::auth::{
    AccessSummary, CreateGroupRequest, CreateRoleBindingRequest, CredentialListQuery,
    CredentialListResponse, CredentialRevoked, CurrentUserArgs, CurrentUserProfile,
    DeleteGroupArgs, DeleteRoleArgs, GroupList, GroupMemberArgs, GroupView, ListAccessArgs,
    ListCredentialsArgs, PrincipalAccessArgs, PrincipalSummary, PutRoleArgs, PutRoleRequest,
    RevokeCredentialArgs, RoleBindingList, RoleBindingQuery, RoleBindingView, RoleList, RoleView,
    SetWorkloadRestrictionArgs, UnbindArgs,
};
use wyrd_spec::error::WyrdError;

use super::WyrdMcpHandler;
use crate::components::auth::Caller;
use crate::components::principals::access;
use crate::http::error::{WyrdErrorResponse, internal_failure};

/// Wire name of the credential-metadata listing.
pub(super) const LIST_CREDENTIALS: &str = "principals.list_credentials";

/// Wire name of the calling User's safe profile.
pub(super) const ME: &str = "principals.me";

/// Wire name of the credential revocation.
pub(super) const REVOKE_CREDENTIAL: &str = "principals.revoke_credential";

/// Wire name of the Service account workload-restriction replacement.
pub(super) const SET_WORKLOAD_RESTRICTION: &str = "principals.set_workload_restriction";

/// Wire name of one account's access summary.
pub(super) const ACCESS: &str = "principals.access";

/// Wire name of the role listing.
pub(super) const LIST_ROLES: &str = "principals.list_roles";

/// Wire name of the custom-role replacement.
pub(super) const PUT_ROLE: &str = "principals.put_role";

/// Wire name of the custom-role deletion.
pub(super) const DELETE_ROLE: &str = "principals.delete_role";

/// Wire name of the role-binding listing.
pub(super) const LIST_BINDINGS: &str = "principals.list_bindings";

/// Wire name of the scoped role binding.
pub(super) const BIND: &str = "principals.bind";

/// Wire name of the binding removal.
pub(super) const UNBIND: &str = "principals.unbind";

/// Wire name of the group listing.
pub(super) const LIST_GROUPS: &str = "principals.list_groups";

/// Wire name of the group creation.
pub(super) const CREATE_GROUP: &str = "principals.create_group";

/// Wire name of the group deletion.
pub(super) const DELETE_GROUP: &str = "principals.delete_group";

/// Wire name of the group membership addition.
pub(super) const ADD_GROUP_MEMBER: &str = "principals.add_group_member";

/// Wire name of the group membership removal.
pub(super) const REMOVE_GROUP_MEMBER: &str = "principals.remove_group_member";

/// Every access-administration tool [`WyrdMcpHandler::mcp_access`] dispatches.
pub(super) const ACCESS_TOOLS: [&str; 12] = [
    ACCESS,
    LIST_ROLES,
    PUT_ROLE,
    DELETE_ROLE,
    LIST_BINDINGS,
    BIND,
    UNBIND,
    LIST_GROUPS,
    CREATE_GROUP,
    DELETE_GROUP,
    ADD_GROUP_MEMBER,
    REMOVE_GROUP_MEMBER,
];

/// The tools every authenticated caller may be shown.
///
/// Read tools are unconditional, which is the MCP contract: an agent can always
/// discover and observe. Nothing here returns secret material.
#[must_use]
pub(super) fn descriptors_unscoped() -> Vec<Tool> {
    vec![
        list_credentials_tool(),
        me_tool(),
        tool::<PrincipalAccessArgs, AccessSummary>(
            ACCESS,
            "Read an account's access",
            "Read one User or Service account's current access in this caller's tenant: every \
             contributing direct, group and identity-provider binding with its role, scope and \
             source, and the combined scoped permissions. Requires principal administration.",
            true,
        ),
        tool::<ListAccessArgs, RoleList>(
            LIST_ROLES,
            "List roles",
            "List every built-in and custom role of this caller's tenant with its operations. \
             Takes an empty object. Requires principal administration.",
            true,
        ),
        tool::<RoleBindingQuery, RoleBindingList>(
            LIST_BINDINGS,
            "List role bindings",
            "List this caller's tenant's role bindings, optionally only those of one account \
             (principal_id), group (group_id) or role. Each names its subject, role, explicit \
             scope, source and compiled permissions. Requires principal administration.",
            true,
        ),
        tool::<ListAccessArgs, GroupList>(
            LIST_GROUPS,
            "List groups",
            "List every local access group of this caller's tenant with its members. Takes an \
             empty object. Requires principal administration.",
            true,
        ),
    ]
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
    vec![
        set_workload_restriction_tool(),
        tool::<PutRoleArgs, RoleView>(
            PUT_ROLE,
            "Create or replace a custom role",
            "Create or replace one custom role's operation bundle in this caller's tenant. \
             Built-in roles are read-only and a bundle is never empty. Every binding of the role \
             follows the new bundle within about five seconds. Requires tenant administration \
             (*).",
            false,
        ),
        tool::<DeleteRoleArgs, RoleView>(
            DELETE_ROLE,
            "Delete a custom role",
            "Delete one custom role that no binding references, and return it. Requires tenant \
             administration (*).",
            false,
        ),
        tool::<CreateRoleBindingRequest, RoleBindingView>(
            BIND,
            "Bind a role at a scope",
            "Bind one role to one account or group at one explicit scope: \"all\", a Card family \
             {\"card\":{\"family\":{\"kind\":...,\"name\":...}}} covering its future \
             versions, or one exact version {\"card\":{\"version\":{\"kind\":...,\"name\":...,\
             \"uid\":...}}}. Idempotent; the grant reaches the subject's existing \
             credentials within about five seconds, still bounded by every credential ceiling. \
             Requires tenant administration (*).",
            false,
        ),
        tool::<UnbindArgs, RoleBindingView>(
            UNBIND,
            "Remove a role binding",
            "Remove one directly administered binding and return it. Access another binding \
             still grants remains; an identity-provider binding changes only through its \
             provider. Requires tenant administration (*).",
            false,
        ),
        tool::<CreateGroupRequest, GroupView>(
            CREATE_GROUP,
            "Create a group",
            "Create one empty local access group with a tenant-unique name. Requires tenant \
             administration (*).",
            false,
        ),
        tool::<DeleteGroupArgs, GroupView>(
            DELETE_GROUP,
            "Delete a group",
            "Delete one local group with its memberships and bindings, and return it as it was. \
             Requires tenant administration (*).",
            false,
        ),
        tool::<GroupMemberArgs, GroupView>(
            ADD_GROUP_MEMBER,
            "Add a group member",
            "Add one User or Service account to a local group; idempotent. Returns the group. \
             Requires tenant administration (*).",
            false,
        ),
        tool::<GroupMemberArgs, GroupView>(
            REMOVE_GROUP_MEMBER,
            "Remove a group member",
            "Remove one account from a local group; idempotent. Returns the group. Requires \
             tenant administration (*).",
            false,
        ),
    ]
}

/// Whether this caller holds tenant administration.
pub(super) fn may_administer_tenant(caller: &Caller) -> bool {
    caller
        .principal
        .effective_permissions
        .contains(&Permission::wildcard())
}

/// Whether this caller may revoke a credential: tenant principal
/// administration, or a User's own-key `personal_credentials:write`.
///
/// Reads the same permissions the HTTP surface requires, from the verified
/// token, so the two cannot disagree about who may write.
pub(super) fn may_administer(caller: &Caller) -> bool {
    let held = &caller.principal.effective_permissions;
    held.contains(&Permission::service_accounts_write())
        || held.contains(&Permission::personal_credentials_write())
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
         confirmed complete. Without limit or cursor an administrator reads the complete \
         listing; either selects one page (limit 1-100, default 50) whose next_cursor, when \
         present, continues it. A User listing its own keys without principal administration \
         must page.",
        true,
    )
}

/// Descriptor for the calling User's safe profile.
fn me_tool() -> Tool {
    tool::<CurrentUserArgs, CurrentUserProfile>(
        ME,
        "Read my profile",
        "Read the calling User's safe profile in this tenant: route key, ids, canonical local \
         username (null for an SSO-only account), whether the account signs in by password or \
         SSO, and the caller's effective permission names. Takes an empty object. Refused for \
         a caller that is not a User.",
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
         service_accounts:write, or personal_credentials:write for a User's own key.",
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
            &CredentialListQuery {
                limit: args.limit,
                cursor: args.cursor,
            },
        )
        .await?;

        Ok(CallToolResult::structured(
            serde_json::to_value(listing).map_err(|error| {
                internal_failure("credential listing could not be projected", &error)
            })?,
        ))
    }

    /// Read the calling User's safe profile.
    ///
    /// # Errors
    /// Returns a Wyrd error when the arguments are not an empty object, the
    /// caller is not a User, or the read fails.
    pub(super) async fn mcp_me(
        &self,
        caller: Caller,
        arguments: Option<JsonMap<String, JsonValue>>,
    ) -> Result<CallToolResult, WyrdError> {
        let CurrentUserArgs {} = parse_args(Some(arguments.unwrap_or_default()), ME)?;
        let profile =
            crate::components::principals::accounts::current_user_profile(&self.state, &caller)
                .await?;
        Ok(CallToolResult::structured(
            serde_json::to_value(profile)
                .map_err(|error| internal_failure("profile could not be projected", &error))?,
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

    /// Dispatch one access-administration tool to its HTTP operation, which
    /// decides, validates and audits it exactly as it does for HTTP.
    ///
    /// # Errors
    /// Returns a Wyrd error when the arguments are invalid, or the
    /// operation's own refusal or failure.
    pub(super) async fn mcp_access(
        &self,
        name: &str,
        caller: Caller,
        arguments: Option<JsonMap<String, JsonValue>>,
    ) -> Result<CallToolResult, WyrdError> {
        let state = || State(self.state.clone());
        match name {
            ACCESS => {
                let args: PrincipalAccessArgs = parse_args(arguments, name)?;
                project(
                    access::principal_access(
                        state(),
                        caller,
                        Ok(Path(args.principal_id.as_uuid())),
                    )
                    .await,
                )
            }
            LIST_ROLES => {
                let ListAccessArgs {} = parse_args(Some(arguments.unwrap_or_default()), name)?;
                project(access::list_access_roles(state(), caller).await)
            }
            PUT_ROLE => {
                let args: PutRoleArgs = parse_args(arguments, name)?;
                let body = PutRoleRequest {
                    operations: args.operations,
                    description: args.description,
                };
                project(
                    access::put_access_role(state(), caller, Ok(Path(args.role)), Ok(Json(body)))
                        .await,
                )
            }
            DELETE_ROLE => {
                let args: DeleteRoleArgs = parse_args(arguments, name)?;
                project(access::delete_access_role(state(), caller, Ok(Path(args.role))).await)
            }
            LIST_BINDINGS => {
                let query: RoleBindingQuery =
                    parse_args(Some(arguments.unwrap_or_default()), name)?;
                project(access::list_access_bindings(state(), caller, Ok(Query(query))).await)
            }
            BIND => {
                let request: CreateRoleBindingRequest = parse_args(arguments, name)?;
                project(access::create_access_binding(state(), caller, Ok(Json(request))).await)
            }
            UNBIND => {
                let args: UnbindArgs = parse_args(arguments, name)?;
                project(
                    access::delete_access_binding(state(), caller, Ok(Path(args.binding_id))).await,
                )
            }
            LIST_GROUPS => {
                let ListAccessArgs {} = parse_args(Some(arguments.unwrap_or_default()), name)?;
                project(access::list_access_groups(state(), caller).await)
            }
            CREATE_GROUP => {
                let request: CreateGroupRequest = parse_args(arguments, name)?;
                project(access::create_access_group(state(), caller, Ok(Json(request))).await)
            }
            DELETE_GROUP => {
                let args: DeleteGroupArgs = parse_args(arguments, name)?;
                project(access::delete_access_group(state(), caller, Ok(Path(args.group_id))).await)
            }
            ADD_GROUP_MEMBER | REMOVE_GROUP_MEMBER => {
                let args: GroupMemberArgs = parse_args(arguments, name)?;
                let target = Ok(Path((args.group_id, args.principal_id.as_uuid())));
                project(if name == ADD_GROUP_MEMBER {
                    access::add_access_group_member(state(), caller, target).await
                } else {
                    access::remove_access_group_member(state(), caller, target).await
                })
            }
            _ => Err(WyrdError::internal(format!("{name} is not an access tool"))),
        }
    }
}

/// Project one access operation's answer as structured tool output.
///
/// # Errors
/// Returns the operation's own error, or an internal error when the answer
/// cannot be serialized.
fn project<T: Serialize>(
    answer: Result<Json<T>, WyrdErrorResponse>,
) -> Result<CallToolResult, WyrdError> {
    let Json(answer) = answer.map_err(|WyrdErrorResponse(error)| error)?;
    Ok(CallToolResult::structured(
        serde_json::to_value(answer)
            .map_err(|error| internal_failure("access answer could not be projected", &error))?,
    ))
}
