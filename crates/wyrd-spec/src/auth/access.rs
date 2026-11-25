//! Wire contract for scoped tenant access administration.
//!
//! A role says *what* an account can do: a bundle of [`Operation`]s from the
//! closed resource/action vocabulary, with no object selection. A role
//! binding says *where*: it assigns one role to one account or optional local
//! group at one explicit [`PermissionScope`]. The server compiles bindings
//! into scoped [`Permission`]s; credentials and delegation can only restrict
//! that authority.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::{
    Action, Permission, PermissionScope, PrincipalId, PrincipalKindTag, PrincipalStatus, Resource,
    RoleSource, WorkloadRestriction,
};
use crate::ids::{AccessGroupId, RoleBindingId};

/// One operation a reusable role grants, without an object selection.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct Operation {
    /// Resource the operation acts on.
    pub resource: Resource,
    /// Action the operation performs.
    pub action: Action,
}

impl Operation {
    /// The operation of `permission`, dropping its scope.
    #[must_use]
    pub fn of(permission: &Permission) -> Self {
        Self {
            resource: permission.resource.clone(),
            action: permission.action.clone(),
        }
    }

    /// This operation at `scope`, or `None` when the operation catalog does
    /// not apply it to that kind of object.
    #[must_use]
    pub fn at(&self, scope: &PermissionScope) -> Option<Permission> {
        let permission = Permission {
            resource: self.resource.clone(),
            action: self.action.clone(),
            scope: scope.clone(),
        };
        permission.validate().ok().map(|()| permission)
    }
}

/// The account or optional local group a role binding assigns to.
///
/// Users and Services are the managed principals; a group is a roster, never
/// a principal, and never logs in.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum BindingSubject {
    /// One tenant account, with the kind its id resolves under.
    Principal {
        /// Account id.
        id: PrincipalId,
        /// Account kind: `user` or `service`.
        kind: PrincipalKindTag,
    },
    /// One local access group.
    Group {
        /// Group id.
        id: AccessGroupId,
    },
}

/// One role assigned at one explicit scope, before it has a subject.
///
/// Used by invitations, account creation, and binding creation. The scope is
/// required: a missing scope is refused, never read as tenant-wide.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct ScopedRoleAssignment {
    /// Role name.
    pub role: String,
    /// Objects the role's operations reach.
    pub scope: PermissionScope,
}

impl ScopedRoleAssignment {
    /// `role` bound explicitly at tenant scope.
    #[must_use]
    pub fn tenant(role: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            scope: PermissionScope::All,
        }
    }
}

/// Request to bind one role to one account or group at one scope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct CreateRoleBindingRequest {
    /// Account or group receiving the grant.
    pub subject: BindingSubject,
    /// Role name.
    pub role: String,
    /// Objects the role's operations reach.
    pub scope: PermissionScope,
}

/// One stored role binding.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct RoleBindingView {
    /// Binding id.
    pub id: RoleBindingId,
    /// Account or group the binding assigns to.
    pub subject: BindingSubject,
    /// Role name.
    pub role: String,
    /// Objects the role's operations reach.
    pub scope: PermissionScope,
    /// Who administers the binding: a tenant administrator or an identity
    /// provider's login projection.
    pub source: RoleSource,
    /// The scoped permissions this binding compiles to.
    pub permissions: Vec<Permission>,
}

/// One page of role bindings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct RoleBindingList {
    /// Bindings, ordered by id.
    pub bindings: Vec<RoleBindingView>,
}

/// Filter for listing role bindings.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::IntoParams))]
#[cfg_attr(feature = "server", into_params(parameter_in = Query))]
#[serde(deny_unknown_fields)]
pub struct RoleBindingQuery {
    /// Only bindings of this account.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal_id: Option<PrincipalId>,
    /// Only bindings of this group.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<AccessGroupId>,
    /// Only bindings of this role.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// One role: a reusable operation bundle.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct RoleView {
    /// Role id.
    pub id: Uuid,
    /// Role name.
    pub name: String,
    /// Operations the role grants at a binding's scope.
    pub operations: Vec<Operation>,
    /// Built-in roles are read-only.
    pub builtin: bool,
    /// One-line summary: server-set for a built-in role, administrator-set
    /// for a custom role; `null` when it has none.
    #[serde(default)]
    pub description: Option<String>,
    /// Distinct Users and Service accounts holding the role at any scope,
    /// directly or through a Group, each counted once.
    pub member_count: u64,
}

/// Every role of the tenant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct RoleList {
    /// Roles, ordered by name.
    pub roles: Vec<RoleView>,
}

/// Request to create or replace a custom role's operations.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct PutRoleRequest {
    /// Operations from the closed catalog; never empty.
    pub operations: Vec<Operation>,
    /// One-line summary; omitted or `null` clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// One optional local access group.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct GroupView {
    /// Group id.
    pub id: AccessGroupId,
    /// Tenant-unique group name.
    pub name: String,
    /// Member accounts.
    pub members: Vec<GroupMember>,
}

/// One account in a group's roster.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct GroupMember {
    /// Account id.
    pub principal_id: PrincipalId,
    /// Account kind: `user` or `service`.
    pub kind: PrincipalKindTag,
}

/// Every group of the tenant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct GroupList {
    /// Groups, ordered by name.
    pub groups: Vec<GroupView>,
}

/// Request to create a group.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct CreateGroupRequest {
    /// Tenant-unique group name.
    pub name: String,
}

/// Where one of an account's grants comes from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum GrantSource {
    /// A binding on the account itself, administered in Wyrd.
    Direct,
    /// A binding on a group the account belongs to.
    Group {
        /// Group id.
        id: AccessGroupId,
        /// Group name.
        name: String,
    },
    /// A binding an identity provider's login projected onto the account.
    Provider,
}

/// One binding contributing to an account's access.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct AccessGrant {
    /// Binding id; remove it at its source to remove this grant.
    pub binding_id: RoleBindingId,
    /// Role name.
    pub role: String,
    /// Objects the role's operations reach.
    pub scope: PermissionScope,
    /// Where the binding comes from.
    pub source: GrantSource,
    /// The scoped permissions it contributes.
    pub permissions: Vec<Permission>,
}

/// One account's current access, as the server enforces it.
///
/// Grants are additive: removing one binding does not remove authority that
/// another binding still grants.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct AccessSummary {
    /// Account id.
    pub principal_id: PrincipalId,
    /// Account kind.
    pub kind: PrincipalKindTag,
    /// Account status.
    pub status: PrincipalStatus,
    /// The Service family a runtime account may run as, separate from its
    /// operation grants; absent for Users and unrestricted automation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "crate::schema::skipped_option::<WorkloadRestriction>")]
    pub workload_restriction: Option<WorkloadRestriction>,
    /// Every contributing binding with its source.
    pub grants: Vec<AccessGrant>,
    /// The combined scoped permissions.
    pub permissions: Vec<Permission>,
}

/// Compiles one role's operations at one binding scope into the scoped
/// permissions the checker evaluates.
///
/// Each operation applies only where the fixed operation catalog accepts the
/// scope; inapplicable operations are skipped, never promoted to
/// [`PermissionScope::All`]. Reading a Card includes reading the executions
/// and retained evidence whose trusted parent is that Card, so `cards:read`
/// at a Card scope also yields `executions:read` at the same scope — fixed
/// parent inheritance, independent of the role's name. The same holds at
/// tenant scope.
///
/// # Errors
///
/// Returns [`NoApplicableOperation`] when no operation applies to `scope`,
/// which is how an administrative role at a resource scope is refused.
pub fn compile_binding(
    operations: &[Operation],
    scope: &PermissionScope,
) -> Result<Vec<Permission>, NoApplicableOperation> {
    let mut permissions: Vec<Permission> = Vec::new();
    let mut push = |permission: Permission| {
        if !permissions.contains(&permission) {
            permissions.push(permission);
        }
    };
    for operation in operations {
        if let Some(permission) = operation.at(scope) {
            let inherits_executions = (scope.card().is_some() || *scope == PermissionScope::All)
                && permission.resource == Resource::Cards
                && permission.action == Action::Read;
            push(permission);
            if inherits_executions {
                push(Permission::executions_read_any().at(scope.clone()));
            }
        }
    }
    if permissions.is_empty() {
        return Err(NoApplicableOperation);
    }
    Ok(permissions)
}

/// A binding whose role grants nothing at its scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the role grants no operation applicable to this scope")]
pub struct NoApplicableOperation;

#[cfg(test)]
mod tests {
    use super::{NoApplicableOperation, Operation, compile_binding};
    use crate::auth::{
        Action, CardPermissionScope, GatewayAccess, Permission, PermissionScope, Resource,
    };
    use crate::envelope::CardKind;
    use crate::ids::CardName;

    fn op(resource: Resource, action: Action) -> Operation {
        Operation { resource, action }
    }

    fn payments() -> PermissionScope {
        PermissionScope::Card(CardPermissionScope::family(
            CardKind::Repository,
            CardName::new("payments").expect("name"),
        ))
    }

    /// Proves a binding applies only its applicable operations at its scope,
    /// that Card read inherits execution read at the same parent, and that a
    /// role with nothing applicable is refused rather than widened.
    ///
    /// # Panics
    /// Panics when compilation yields the wrong scoped permissions.
    #[test]
    fn compile_applies_operations_at_the_binding_scope() {
        let viewer = [
            op(Resource::Cards, Action::Read),
            op(Resource::Operators, Action::Read),
            op(Resource::Gateway, Action::Read),
            op(Resource::BifrostQuery, Action::Read),
        ];
        assert_eq!(
            compile_binding(&viewer, &payments()).expect("applies"),
            vec![
                Permission::card_read().at(payments()),
                Permission::executions_read_any().at(payments()),
            ]
        );
        assert_eq!(
            compile_binding(&viewer, &PermissionScope::All).expect("tenant scope"),
            vec![
                Permission::card_read(),
                Permission::executions_read_any(),
                Permission::operators_read(),
                Permission::gateway_read(),
                Permission::bifrost_query_read(),
            ]
        );
        assert_eq!(
            compile_binding(&[op(Resource::Wildcard, Action::Wildcard)], &payments()),
            Err(NoApplicableOperation),
            "admin is tenant-scope only"
        );
        let gateway = PermissionScope::Gateway(GatewayAccess::Deployment {
            deployment: "chat".parse().expect("deployment"),
        });
        assert_eq!(
            compile_binding(&viewer, &gateway),
            Err(NoApplicableOperation),
            "no viewer operation applies to a deployment"
        );
        assert_eq!(
            compile_binding(&[op(Resource::Gateway, Action::Invoke)], &gateway)
                .expect("invoke applies"),
            vec![Permission::gateway_invoke_any().at(gateway)]
        );
    }
}

/// Empty input of the read-only MCP role and group listings.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListAccessArgs {}

/// MCP input naming the account whose access summary to read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PrincipalAccessArgs {
    /// Account whose access to summarize.
    pub principal_id: PrincipalId,
}

/// MCP input creating or replacing one custom role: the HTTP path's role
/// name beside its [`PutRoleRequest`] body.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PutRoleArgs {
    /// Custom role name; built-in roles are read-only.
    pub role: String,
    /// Operations from the closed catalog; never empty.
    pub operations: Vec<Operation>,
    /// One-line summary; omitted or `null` clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// MCP input naming one custom role to delete.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeleteRoleArgs {
    /// Custom role name no binding references.
    pub role: String,
}

/// MCP input naming one directly administered binding to remove.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UnbindArgs {
    /// Binding to remove.
    pub binding_id: RoleBindingId,
}

/// MCP input naming one group to delete.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeleteGroupArgs {
    /// Group to delete with its memberships and bindings.
    pub group_id: AccessGroupId,
}

/// MCP input naming one group membership to add or remove.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GroupMemberArgs {
    /// Group whose roster changes.
    pub group_id: AccessGroupId,
    /// Account added or removed.
    pub principal_id: PrincipalId,
}
