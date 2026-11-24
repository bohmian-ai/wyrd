//! Scoped tenant access administration: roles, role bindings, local groups,
//! and one account's access summary.
//!
//! Reads need principal administration; every write needs tenant
//! administration (`*`), is decided and audited before anything is read, and
//! advances the tenant's authorization revision in the same transaction, so
//! every replica's current authority follows the commit.

use axum::Json;
use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use serde_json::json;
use uuid::Uuid;
use wyrd_runtime::builtin_roles::BUILTIN_ROLES;
use wyrd_runtime::{Permission, RoleRef};
use wyrd_spec::auth::{
    AccessGrant, AccessSummary, BindingSubject, CreateGroupRequest, CreateRoleBindingRequest,
    GrantSource, GroupList, GroupMember, GroupView, Operation, PermissionScope, PrincipalId,
    PrincipalKindTag, PutRoleRequest, RoleBindingList, RoleBindingQuery, RoleBindingView, RoleList,
    RoleSource, RoleView, ScopedRoleAssignment, compile_binding,
};
use wyrd_spec::error::{WyrdError, WyrdProblem};
use wyrd_spec::ids::{AccessGroupId, RoleBindingId};
use wyrd_sql::TenantConn;
use wyrd_sql::queries::auth::{
    MembershipRow, RoleBindingRow, RoleListRow, SubjectColumns, advance_authorization_revision,
    delete_group, delete_group_membership, delete_role, delete_role_binding, insert_group,
    insert_group_membership, insert_role, insert_role_binding, list_group_memberships, list_groups,
    list_role_bindings, list_roles, role_by_name, set_role_description,
};

use super::routes::{authorize, internal, invalid, require_assignable, summary, tenant_conn};
use crate::audit;
use crate::components::auth::Caller;
use crate::http::error::{WyrdErrorResponse, body_rejection, path_rejection};
use crate::state::AppState;

/// Decide and audit tenant administration for one access write, then open
/// the write transaction.
///
/// # Errors
/// Returns the RBAC denial without `*`, or an internal error when no tenant
/// connection is available.
async fn administer<'a>(
    state: &'a AppState,
    caller: &Caller,
    operation: &str,
    resource: &str,
) -> Result<TenantConn<'a>, WyrdErrorResponse> {
    audit::authorize(state, caller, &Permission::wildcard(), operation, resource)?;
    tenant_conn(state, caller).await
}

/// Advance the revision and commit one access write.
///
/// # Errors
/// Returns an internal error when either statement fails.
async fn commit_change(mut conn: TenantConn<'_>) -> Result<(), WyrdErrorResponse> {
    advance_authorization_revision(&mut conn)
        .await
        .map_err(internal)?;
    conn.commit().await.map_err(internal)
}

/// List every role with its operations.
///
/// # Errors
/// Returns the RBAC denial without principal administration and an internal
/// error when the read fails or a stored bundle is corrupt.
#[utoipa::path(
    get,
    path = "/access/roles",
    responses(
        (status = 200, description = "Roles ordered by name", body = RoleList),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "Tenant principal administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 500, description = "A tenant store read failed (WYRD_SPEC_500_INTERNAL)",
         body = WyrdProblem)
    ),
    tag = "Access"
)]
#[tracing::instrument(level = "info", skip(state, caller))]
pub(crate) async fn list_access_roles(
    State(state): State<AppState>,
    caller: Caller,
) -> Result<Json<RoleList>, WyrdErrorResponse> {
    let mut conn = authorize(&state, &caller, "list roles", "auth.role.list", "roles").await?;
    let rows = list_roles(&mut conn).await.map_err(internal)?;
    conn.commit().await.map_err(internal)?;
    let roles = rows
        .into_iter()
        .map(role_view)
        .collect::<Result<_, WyrdErrorResponse>>()?;
    Ok(Json(RoleList { roles }))
}

/// Longest custom role description, in characters.
const ROLE_DESCRIPTION_MAX_CHARS: usize = 200;

/// Project one listed role row onto its wire view.
///
/// # Errors
/// Returns an internal error when the stored bundle or count is corrupt.
fn role_view(row: RoleListRow) -> Result<RoleView, WyrdErrorResponse> {
    Ok(RoleView {
        builtin: is_builtin(&row.name),
        operations: serde_json::from_value(row.permissions).map_err(internal)?,
        id: row.id,
        name: row.name,
        description: row.description,
        member_count: u64::try_from(row.member_count).map_err(internal)?,
    })
}

/// Create or replace one custom role's operations and description.
///
/// Built-in roles are read-only. Every binding of the role follows the new
/// bundle at the next authority confirmation. An omitted description clears
/// the stored one.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` for a malformed name, a built-in name,
/// an empty bundle, or a description over 200 characters, the RBAC denial
/// without `*`, and an internal error.
#[utoipa::path(
    put,
    path = "/access/roles/{role}",
    params(("role" = String, Path, description = "Custom role name")),
    request_body = PutRoleRequest,
    responses(
        (status = 200, description = "The stored role", body = RoleView),
        (status = 400, description = "Malformed or built-in name, no operations, or a \
          description over 200 characters (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 403, description = "Tenant administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 500, description = "A tenant store write failed (WYRD_SPEC_500_INTERNAL)",
         body = WyrdProblem)
    ),
    tag = "Access"
)]
#[tracing::instrument(level = "info", skip(state, caller, request))]
pub(crate) async fn put_access_role(
    State(state): State<AppState>,
    caller: Caller,
    role: Result<Path<String>, PathRejection>,
    request: Result<Json<PutRoleRequest>, JsonRejection>,
) -> Result<Json<RoleView>, WyrdErrorResponse> {
    let Path(role) = role.map_err(|rejection| path_rejection(&rejection))?;
    let Json(request) = request.map_err(|rejection| body_rejection(&rejection))?;
    let mut conn = administer(&state, &caller, "auth.role.put", &format!("role:{role}")).await?;
    let refused = |message: &str| invalid(message, json!({ "role": role }));
    RoleRef::new(&role).map_err(|_| refused("role name is not valid"))?;
    if is_builtin(&role) {
        return Err(refused("built-in roles are read-only"));
    }
    if request.operations.is_empty() {
        return Err(refused("a role needs at least one operation"));
    }
    let description = request
        .description
        .as_deref()
        .map(str::trim)
        .filter(|description| !description.is_empty());
    if description.is_some_and(|text| text.chars().count() > ROLE_DESCRIPTION_MAX_CHARS) {
        return Err(refused("a role description is at most 200 characters"));
    }
    let id = role_by_name(&mut conn, &role)
        .await
        .map_err(internal)?
        .map_or_else(Uuid::now_v7, |row| row.id);
    insert_role(
        &mut conn,
        id,
        &role,
        &serde_json::to_value(&request.operations).map_err(internal)?,
        false,
    )
    .await
    .map_err(internal)?;
    set_role_description(&mut conn, id, description)
        .await
        .map_err(internal)?;
    let stored = list_roles(&mut conn)
        .await
        .map_err(internal)?
        .into_iter()
        .find(|row| row.id == id)
        .ok_or_else(|| internal("the stored role was not listed"))?;
    commit_change(conn).await?;
    role_view(stored).map(Json)
}

/// Delete one custom role that no binding references.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` for a built-in, unknown, or still-bound
/// role, the RBAC denial without `*`, and an internal error.
#[utoipa::path(
    delete,
    path = "/access/roles/{role}",
    params(("role" = String, Path, description = "Custom role name")),
    responses(
        (status = 200, description = "The deleted role", body = RoleView),
        (status = 400, description = "Built-in, unknown, or still-bound role \
          (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 403, description = "Tenant administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 500, description = "A tenant store write failed (WYRD_SPEC_500_INTERNAL)",
         body = WyrdProblem)
    ),
    tag = "Access"
)]
#[tracing::instrument(level = "info", skip(state, caller))]
pub(crate) async fn delete_access_role(
    State(state): State<AppState>,
    caller: Caller,
    role: Result<Path<String>, PathRejection>,
) -> Result<Json<RoleView>, WyrdErrorResponse> {
    let Path(role) = role.map_err(|rejection| path_rejection(&rejection))?;
    let mut conn = administer(&state, &caller, "auth.role.delete", &format!("role:{role}")).await?;
    let refused = |message: &str| invalid(message, json!({ "role": role }));
    if is_builtin(&role) {
        return Err(refused("built-in roles are read-only"));
    }
    let row = role_by_name(&mut conn, &role)
        .await
        .map_err(internal)?
        .ok_or_else(|| refused("role does not exist in this tenant"))?;
    let bound = list_role_bindings(&mut conn)
        .await
        .map_err(internal)?
        .iter()
        .filter(|binding| binding.role_id == row.id)
        .count();
    if bound > 0 {
        return Err(invalid(
            "remove the role's bindings before deleting it",
            json!({ "role": role, "bindings": bound }),
        ));
    }
    delete_role(&mut conn, &role).await.map_err(internal)?;
    commit_change(conn).await?;
    Ok(Json(RoleView {
        id: row.id,
        name: row.name,
        operations: serde_json::from_value(row.permissions).map_err(internal)?,
        builtin: false,
        description: row.description,
        member_count: 0,
    }))
}

/// List role bindings, optionally for one account, group, or role.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` for a malformed query, the RBAC denial
/// without principal administration, and an internal error.
#[utoipa::path(
    get,
    path = "/access/bindings",
    params(RoleBindingQuery),
    responses(
        (status = 200, description = "Bindings ordered by id", body = RoleBindingList),
        (status = 400, description = "Malformed filter (WYRD_SPEC_400_VALIDATION)",
         body = WyrdProblem),
        (status = 403, description = "Tenant principal administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 500, description = "A tenant store read failed (WYRD_SPEC_500_INTERNAL)",
         body = WyrdProblem)
    ),
    tag = "Access"
)]
#[tracing::instrument(level = "info", skip(state, caller))]
pub(crate) async fn list_access_bindings(
    State(state): State<AppState>,
    caller: Caller,
    query: Result<Query<RoleBindingQuery>, QueryRejection>,
) -> Result<Json<RoleBindingList>, WyrdErrorResponse> {
    let Query(query) = query.map_err(|rejection| {
        invalid(&rejection.body_text(), json!({ "reason": "query_invalid" }))
    })?;
    let mut conn = authorize(
        &state,
        &caller,
        "list role bindings",
        "auth.binding.list",
        "bindings",
    )
    .await?;
    let rows = list_role_bindings(&mut conn).await.map_err(internal)?;
    let kinds = account_kinds(&mut conn, &rows).await?;
    conn.commit().await.map_err(internal)?;
    let bindings = rows
        .into_iter()
        .filter(|row| {
            query.principal_id.is_none_or(|id| {
                row.subject.user_id.or(row.subject.service_account_id) == Some(id.as_uuid())
            }) && query
                .group_id
                .is_none_or(|id| row.subject.group_id == Some(id.as_uuid()))
                && query
                    .role
                    .as_ref()
                    .is_none_or(|role| *role == row.role_name)
        })
        .map(|row| binding_view(row, &kinds))
        .collect::<Result<_, _>>()?;
    Ok(Json(RoleBindingList { bindings }))
}

/// Bind one role to one account or group at one explicit scope.
///
/// Idempotent: an identical binding is returned unchanged. The grant reaches
/// the subject's existing credentials at the next authority confirmation,
/// still bounded by every credential ceiling.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` for an unknown role, an invalid scope,
/// or a role with no operation applicable to it,
/// `WYRD_AUTH_404_PRINCIPAL_NOT_FOUND` for an unassignable account or a kind
/// that does not match it, the RBAC denial without `*`, and an internal error.
#[utoipa::path(
    post,
    path = "/access/bindings",
    request_body = CreateRoleBindingRequest,
    responses(
        (status = 200, description = "The binding", body = RoleBindingView),
        (status = 400, description = "Unknown role, invalid or inapplicable scope, or \
          unknown group (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 403, description = "Tenant administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No assignable account with this id and kind \
          (WYRD_AUTH_404_PRINCIPAL_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "A tenant store write failed (WYRD_SPEC_500_INTERNAL)",
         body = WyrdProblem)
    ),
    tag = "Access"
)]
#[tracing::instrument(level = "info", skip(state, caller, request))]
pub(crate) async fn create_access_binding(
    State(state): State<AppState>,
    caller: Caller,
    request: Result<Json<CreateRoleBindingRequest>, JsonRejection>,
) -> Result<Json<RoleBindingView>, WyrdErrorResponse> {
    let Json(request) = request.map_err(|rejection| body_rejection(&rejection))?;
    let mut conn = administer(
        &state,
        &caller,
        "auth.binding.create",
        &format!("role:{}", request.role),
    )
    .await?;
    let subject = subject_columns(&mut conn, &request.subject).await?;
    let assignment = ScopedRoleAssignment {
        role: request.role,
        scope: request.scope,
    };
    let role_id =
        wyrd_auth::roles::validate_assignments(&mut conn, std::slice::from_ref(&assignment))
            .await?
            .remove(0);
    insert_role_binding(
        &mut conn,
        Uuid::now_v7(),
        role_id,
        subject,
        &assignment.scope,
        RoleSource::Direct,
    )
    .await
    .map_err(internal)?;
    let rows = list_role_bindings(&mut conn).await.map_err(internal)?;
    let row = rows
        .into_iter()
        .find(|row| {
            row.role_id == role_id
                && row.subject == subject
                && row.source == RoleSource::Direct
                && serde_json::from_value::<PermissionScope>(row.scope.clone())
                    .ok()
                    .as_ref()
                    == Some(&assignment.scope)
        })
        .ok_or_else(|| internal("the inserted binding is not visible"))?;
    let kinds = account_kinds(&mut conn, std::slice::from_ref(&row)).await?;
    commit_change(conn).await?;
    Ok(Json(binding_view(row, &kinds)?))
}

/// Remove one directly administered binding.
///
/// Authority another binding still grants remains. A provider binding is
/// replaced only by its provider's login.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` for an unknown or provider binding, the
/// RBAC denial without `*`, and an internal error.
#[utoipa::path(
    delete,
    path = "/access/bindings/{binding_id}",
    params(("binding_id" = Uuid, Path, description = "Binding to remove")),
    responses(
        (status = 200, description = "The removed binding", body = RoleBindingView),
        (status = 400, description = "Unknown or provider-administered binding \
          (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 403, description = "Tenant administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 500, description = "A tenant store write failed (WYRD_SPEC_500_INTERNAL)",
         body = WyrdProblem)
    ),
    tag = "Access"
)]
#[tracing::instrument(level = "info", skip(state, caller))]
pub(crate) async fn delete_access_binding(
    State(state): State<AppState>,
    caller: Caller,
    binding_id: Result<Path<RoleBindingId>, PathRejection>,
) -> Result<Json<RoleBindingView>, WyrdErrorResponse> {
    let Path(binding_id) = binding_id.map_err(|rejection| path_rejection(&rejection))?;
    let mut conn = administer(
        &state,
        &caller,
        "auth.binding.delete",
        &format!("binding:{binding_id}"),
    )
    .await?;
    let refused = |message: &str| invalid(message, json!({ "binding_id": binding_id }));
    let row = list_role_bindings(&mut conn)
        .await
        .map_err(internal)?
        .into_iter()
        .find(|row| row.id == binding_id.as_uuid())
        .ok_or_else(|| refused("binding does not exist in this tenant"))?;
    if row.source != RoleSource::Direct {
        return Err(refused(
            "a provider binding is replaced only by its provider's login",
        ));
    }
    if let Some(account) = row.subject.service_account_id {
        require_assignable(&mut conn, account).await.map_err(|_| {
            refused("the tenant administrative account's bindings are not administered here")
        })?;
    }
    let kinds = account_kinds(&mut conn, std::slice::from_ref(&row)).await?;
    delete_role_binding(&mut conn, row.id)
        .await
        .map_err(internal)?;
    commit_change(conn).await?;
    Ok(Json(binding_view(row, &kinds)?))
}

/// List every local group with its members.
///
/// # Errors
/// Returns the RBAC denial without principal administration and an internal
/// error.
#[utoipa::path(
    get,
    path = "/access/groups",
    responses(
        (status = 200, description = "Groups ordered by name", body = GroupList),
        (status = 403, description = "Tenant principal administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 500, description = "A tenant store read failed (WYRD_SPEC_500_INTERNAL)",
         body = WyrdProblem)
    ),
    tag = "Access"
)]
#[tracing::instrument(level = "info", skip(state, caller))]
pub(crate) async fn list_access_groups(
    State(state): State<AppState>,
    caller: Caller,
) -> Result<Json<GroupList>, WyrdErrorResponse> {
    let mut conn = authorize(&state, &caller, "list groups", "auth.group.list", "groups").await?;
    let groups = groups(&mut conn).await?;
    conn.commit().await.map_err(internal)?;
    Ok(Json(GroupList { groups }))
}

/// Create one empty local group.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` for an empty or duplicate name, the RBAC
/// denial without `*`, and an internal error.
#[utoipa::path(
    post,
    path = "/access/groups",
    request_body = CreateGroupRequest,
    responses(
        (status = 200, description = "The group", body = GroupView),
        (status = 400, description = "Empty or duplicate name (WYRD_SPEC_400_VALIDATION)",
         body = WyrdProblem),
        (status = 403, description = "Tenant administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 500, description = "A tenant store write failed (WYRD_SPEC_500_INTERNAL)",
         body = WyrdProblem)
    ),
    tag = "Access"
)]
#[tracing::instrument(level = "info", skip(state, caller, request))]
pub(crate) async fn create_access_group(
    State(state): State<AppState>,
    caller: Caller,
    request: Result<Json<CreateGroupRequest>, JsonRejection>,
) -> Result<Json<GroupView>, WyrdErrorResponse> {
    let Json(request) = request.map_err(|rejection| body_rejection(&rejection))?;
    let mut conn = administer(
        &state,
        &caller,
        "auth.group.create",
        &format!("group:{}", request.name),
    )
    .await?;
    let name = request.name.trim();
    let refused = |message: &str| invalid(message, json!({ "name": request.name }));
    if name.is_empty() {
        return Err(refused("group name must not be empty"));
    }
    if list_groups(&mut conn)
        .await
        .map_err(internal)?
        .iter()
        .any(|(_, existing)| existing == name)
    {
        return Err(refused("a group with this name already exists"));
    }
    let id = AccessGroupId::new_v7();
    insert_group(&mut conn, id.as_uuid(), name)
        .await
        .map_err(internal)?;
    commit_change(conn).await?;
    Ok(Json(GroupView {
        id,
        name: name.to_owned(),
        members: Vec::new(),
    }))
}

/// Delete one group with its memberships and bindings.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` for an unknown group, the RBAC denial
/// without `*`, and an internal error.
#[utoipa::path(
    delete,
    path = "/access/groups/{group_id}",
    params(("group_id" = Uuid, Path, description = "Group to delete")),
    responses(
        (status = 200, description = "The deleted group", body = GroupView),
        (status = 400, description = "Unknown group (WYRD_SPEC_400_VALIDATION)",
         body = WyrdProblem),
        (status = 403, description = "Tenant administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 500, description = "A tenant store write failed (WYRD_SPEC_500_INTERNAL)",
         body = WyrdProblem)
    ),
    tag = "Access"
)]
#[tracing::instrument(level = "info", skip(state, caller))]
pub(crate) async fn delete_access_group(
    State(state): State<AppState>,
    caller: Caller,
    group_id: Result<Path<AccessGroupId>, PathRejection>,
) -> Result<Json<GroupView>, WyrdErrorResponse> {
    let Path(group_id) = group_id.map_err(|rejection| path_rejection(&rejection))?;
    let mut conn = administer(
        &state,
        &caller,
        "auth.group.delete",
        &format!("group:{group_id}"),
    )
    .await?;
    let group = group(&mut conn, group_id).await?;
    delete_group(&mut conn, group_id.as_uuid())
        .await
        .map_err(internal)?;
    commit_change(conn).await?;
    Ok(Json(group))
}

/// Add one account to a group; idempotent.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` for an unknown group,
/// `WYRD_AUTH_404_PRINCIPAL_NOT_FOUND` for an unassignable account, the RBAC
/// denial without `*`, and an internal error.
#[utoipa::path(
    put,
    path = "/access/groups/{group_id}/members/{principal_id}",
    params(
        ("group_id" = Uuid, Path, description = "Group"),
        ("principal_id" = Uuid, Path, description = "Account to add")
    ),
    responses(
        (status = 200, description = "The group after the change", body = GroupView),
        (status = 400, description = "Unknown group (WYRD_SPEC_400_VALIDATION)",
         body = WyrdProblem),
        (status = 403, description = "Tenant administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No assignable account with this id \
          (WYRD_AUTH_404_PRINCIPAL_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "A tenant store write failed (WYRD_SPEC_500_INTERNAL)",
         body = WyrdProblem)
    ),
    tag = "Access"
)]
#[tracing::instrument(level = "info", skip(state, caller))]
pub(crate) async fn add_access_group_member(
    State(state): State<AppState>,
    caller: Caller,
    target: Result<Path<(AccessGroupId, Uuid)>, PathRejection>,
) -> Result<Json<GroupView>, WyrdErrorResponse> {
    change_membership(&state, &caller, target, true).await
}

/// Remove one account from a group; idempotent.
///
/// # Errors
/// Returns the errors documented on [`add_access_group_member`].
#[utoipa::path(
    delete,
    path = "/access/groups/{group_id}/members/{principal_id}",
    params(
        ("group_id" = Uuid, Path, description = "Group"),
        ("principal_id" = Uuid, Path, description = "Account to remove")
    ),
    responses(
        (status = 200, description = "The group after the change", body = GroupView),
        (status = 400, description = "Unknown group (WYRD_SPEC_400_VALIDATION)",
         body = WyrdProblem),
        (status = 403, description = "Tenant administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No assignable account with this id \
          (WYRD_AUTH_404_PRINCIPAL_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "A tenant store write failed (WYRD_SPEC_500_INTERNAL)",
         body = WyrdProblem)
    ),
    tag = "Access"
)]
#[tracing::instrument(level = "info", skip(state, caller))]
pub(crate) async fn remove_access_group_member(
    State(state): State<AppState>,
    caller: Caller,
    target: Result<Path<(AccessGroupId, Uuid)>, PathRejection>,
) -> Result<Json<GroupView>, WyrdErrorResponse> {
    change_membership(&state, &caller, target, false).await
}

/// Add (`add`) or remove one membership and return the group afterwards.
///
/// # Errors
/// Returns the errors documented on [`add_access_group_member`].
async fn change_membership(
    state: &AppState,
    caller: &Caller,
    target: Result<Path<(AccessGroupId, Uuid)>, PathRejection>,
    add: bool,
) -> Result<Json<GroupView>, WyrdErrorResponse> {
    let Path((group_id, principal_id)) = target.map_err(|rejection| path_rejection(&rejection))?;
    let operation = if add {
        "auth.group.member.add"
    } else {
        "auth.group.member.remove"
    };
    let mut conn = administer(
        state,
        caller,
        operation,
        &format!("group:{group_id}/principal:{principal_id}"),
    )
    .await?;
    group(&mut conn, group_id).await?;
    let (_, kind) = require_assignable(&mut conn, principal_id).await?;
    let member = account_columns(principal_id, kind);
    if add {
        insert_group_membership(&mut conn, group_id.as_uuid(), member).await
    } else {
        delete_group_membership(&mut conn, group_id.as_uuid(), member).await
    }
    .map_err(internal)?;
    let group = group(&mut conn, group_id).await?;
    commit_change(conn).await?;
    Ok(Json(group))
}

/// One account's current access: every contributing direct, group, and
/// provider binding with its source, and the combined scoped permissions.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` for a malformed id, the RBAC denial
/// without principal administration, the non-enumerating not-found error, and
/// an internal error.
#[utoipa::path(
    get,
    path = "/principals/{principal_id}/access",
    params(("principal_id" = Uuid, Path, description = "Account whose access to summarize")),
    responses(
        (status = 200, description = "The account's current access", body = AccessSummary),
        (status = 400, description = "A path identifier is not a valid UUID \
          (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 403, description = "Tenant principal administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No assignable principal with this id in the caller's \
          tenant (WYRD_AUTH_404_PRINCIPAL_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "A tenant store read failed (WYRD_SPEC_500_INTERNAL)",
         body = WyrdProblem)
    ),
    tag = "Principals"
)]
#[tracing::instrument(level = "info", skip(state, caller))]
pub(crate) async fn principal_access(
    State(state): State<AppState>,
    caller: Caller,
    principal_id: Result<Path<Uuid>, PathRejection>,
) -> Result<Json<AccessSummary>, WyrdErrorResponse> {
    let Path(principal_id) = principal_id.map_err(|rejection| path_rejection(&rejection))?;
    let mut conn = authorize(
        &state,
        &caller,
        "read principal access",
        "auth.principal.access.read",
        &format!("principal:{principal_id}"),
    )
    .await?;
    let (row, kind) = require_assignable(&mut conn, principal_id).await?;
    let bindings = list_role_bindings(&mut conn).await.map_err(internal)?;
    let memberships = list_group_memberships(&mut conn).await.map_err(internal)?;
    let names = list_groups(&mut conn).await.map_err(internal)?;
    conn.commit().await.map_err(internal)?;
    let account = account_columns(principal_id, kind);
    let groups: Vec<Uuid> = memberships
        .iter()
        .filter(|membership| {
            membership.user_id == account.user_id
                && membership.service_account_id == account.service_account_id
        })
        .map(|membership| membership.group_id)
        .collect();
    let mut grants = Vec::new();
    let mut permissions: Vec<Permission> = Vec::new();
    for binding in bindings {
        let source = if binding.subject == account {
            match binding.source {
                RoleSource::Direct => GrantSource::Direct,
                RoleSource::Idp => GrantSource::Provider,
            }
        } else if let Some(group) = binding.subject.group_id.filter(|id| groups.contains(id)) {
            GrantSource::Group {
                id: AccessGroupId::new(group).map_err(internal)?,
                name: names
                    .iter()
                    .find(|(id, _)| *id == group)
                    .map(|(_, name)| name.clone())
                    .unwrap_or_default(),
            }
        } else {
            continue;
        };
        let (scope, compiled) = compiled(&binding)?;
        for permission in &compiled {
            if !permissions.contains(permission) {
                permissions.push(permission.clone());
            }
        }
        grants.push(AccessGrant {
            binding_id: RoleBindingId::new(binding.id).map_err(internal)?,
            role: binding.role_name,
            scope,
            source,
            permissions: compiled,
        });
    }
    let summary = summary(&state, row)?;
    Ok(Json(AccessSummary {
        principal_id: PrincipalId::new(principal_id),
        kind,
        status: summary.status,
        workload_restriction: summary.workload_restriction,
        grants,
        permissions,
    }))
}

/// True for a role name the server seeds and keeps read-only.
fn is_builtin(name: &str) -> bool {
    BUILTIN_ROLES.iter().any(|role| role.name == name)
}

/// The binding columns of an assignable account of `kind`.
fn account_columns(id: Uuid, kind: PrincipalKindTag) -> SubjectColumns {
    match kind {
        PrincipalKindTag::User => SubjectColumns {
            user_id: Some(id),
            ..SubjectColumns::default()
        },
        _ => SubjectColumns {
            service_account_id: Some(id),
            ..SubjectColumns::default()
        },
    }
}

/// Resolve a binding subject in the caller's tenant into its columns.
///
/// # Errors
/// Returns the non-enumerating not-found error for an unassignable account
/// or a kind that does not match it, and a validation error for an unknown
/// group.
async fn subject_columns(
    conn: &mut TenantConn<'_>,
    subject: &BindingSubject,
) -> Result<SubjectColumns, WyrdErrorResponse> {
    match subject {
        BindingSubject::Principal { id, kind } => {
            let (_, found) = require_assignable(conn, id.as_uuid()).await?;
            if found != *kind {
                return Err(WyrdErrorResponse::from(WyrdError::PrincipalNotFound {
                    message: "principal not found in this tenant".to_owned(),
                    details: json!({ "id": id.to_string() }),
                }));
            }
            Ok(account_columns(id.as_uuid(), found))
        }
        BindingSubject::Group { id } => {
            group(conn, *id).await?;
            Ok(SubjectColumns {
                group_id: Some(id.as_uuid()),
                ..SubjectColumns::default()
            })
        }
    }
}

/// The scope and compiled permissions of one stored binding.
///
/// # Errors
/// Returns an internal error for a corrupt stored scope or bundle.
fn compiled(row: &RoleBindingRow) -> Result<(PermissionScope, Vec<Permission>), WyrdErrorResponse> {
    let scope: PermissionScope = serde_json::from_value(row.scope.clone()).map_err(internal)?;
    let operations: Vec<Operation> =
        serde_json::from_value(row.role_operations.clone()).map_err(internal)?;
    let permissions = compile_binding(&operations, &scope).unwrap_or_default();
    Ok((scope, permissions))
}

/// The account kind of every account a binding row names.
///
/// # Errors
/// Returns an internal error when a lookup fails.
async fn account_kinds(
    conn: &mut TenantConn<'_>,
    rows: &[RoleBindingRow],
) -> Result<Vec<(Uuid, PrincipalKindTag)>, WyrdErrorResponse> {
    let mut kinds = Vec::new();
    for row in rows {
        if let Some(id) = row.subject.user_id {
            kinds.push((id, PrincipalKindTag::User));
        } else if let Some(id) = row.subject.service_account_id
            && !kinds.iter().any(|(known, _)| *known == id)
        {
            let kind = wyrd_sql::queries::auth::service_account_by_id(conn, id)
                .await
                .map_err(internal)?
                .map_or(PrincipalKindTag::Service, |account| {
                    if account.principal_kind == "tenant_admin" {
                        PrincipalKindTag::TenantAdmin
                    } else {
                        PrincipalKindTag::Service
                    }
                });
            kinds.push((id, kind));
        }
    }
    Ok(kinds)
}

/// Project one stored binding onto its wire view.
///
/// # Errors
/// Returns an internal error for a corrupt row.
fn binding_view(
    row: RoleBindingRow,
    kinds: &[(Uuid, PrincipalKindTag)],
) -> Result<RoleBindingView, WyrdErrorResponse> {
    let (scope, permissions) = compiled(&row)?;
    let subject = if let Some(group) = row.subject.group_id {
        BindingSubject::Group {
            id: AccessGroupId::new(group).map_err(internal)?,
        }
    } else {
        let id = row
            .subject
            .user_id
            .or(row.subject.service_account_id)
            .ok_or_else(|| internal("a binding names no subject"))?;
        let kind = kinds
            .iter()
            .find(|(known, _)| *known == id)
            .map_or(PrincipalKindTag::Service, |(_, kind)| *kind);
        BindingSubject::Principal {
            id: PrincipalId::new(id),
            kind,
        }
    };
    Ok(RoleBindingView {
        id: RoleBindingId::new(row.id).map_err(internal)?,
        subject,
        role: row.role_name,
        scope,
        source: row.source,
        permissions,
    })
}

/// Every group with its members, ordered by name.
///
/// # Errors
/// Returns an internal error when a read fails.
async fn groups(conn: &mut TenantConn<'_>) -> Result<Vec<GroupView>, WyrdErrorResponse> {
    let names = list_groups(conn).await.map_err(internal)?;
    let memberships = list_group_memberships(conn).await.map_err(internal)?;
    names
        .into_iter()
        .map(|(id, name)| {
            Ok(GroupView {
                id: AccessGroupId::new(id).map_err(internal)?,
                name,
                members: members(&memberships, id),
            })
        })
        .collect()
}

/// One group with its members.
///
/// # Errors
/// Returns a validation error for an unknown group and an internal error.
async fn group(
    conn: &mut TenantConn<'_>,
    id: AccessGroupId,
) -> Result<GroupView, WyrdErrorResponse> {
    groups(conn)
        .await?
        .into_iter()
        .find(|group| group.id == id)
        .ok_or_else(|| {
            invalid(
                "group does not exist in this tenant",
                json!({ "group_id": id }),
            )
        })
}

/// The members of group `id`.
fn members(memberships: &[MembershipRow], id: Uuid) -> Vec<GroupMember> {
    memberships
        .iter()
        .filter(|membership| membership.group_id == id)
        .filter_map(
            |membership| match (membership.user_id, membership.service_account_id) {
                (Some(user), None) => Some(GroupMember {
                    principal_id: PrincipalId::new(user),
                    kind: PrincipalKindTag::User,
                }),
                (None, Some(service)) => Some(GroupMember {
                    principal_id: PrincipalId::new(service),
                    kind: PrincipalKindTag::Service,
                }),
                _ => None,
            },
        )
        .collect()
}
