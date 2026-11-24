//! Scoped access administration from the command line.
//!
//! A role is an operation bundle; a binding assigns one role to one account
//! or group at one explicit scope. `wyrd principal access` reads an account's
//! current access, grants and removes bindings, and administers custom roles
//! and local groups. Every verb is one call through `wyrd_client::Principals`.
//!
//! A grant is written `ROLE:SCOPE`. `SCOPE` is required and is one of `all`,
//! `KIND/NAME` for a Card family (future versions included),
//! `KIND/NAME@UID` for one exact Card version, or a JSON scope object for
//! the other typed scopes.

use std::process::ExitCode;

use clap::{Args, Subcommand};
use wyrd_spec::auth::{
    AccessSummary, BindingSubject, CardPermissionScope, CreateGroupRequest,
    CreateRoleBindingRequest, GroupView, Operation, Permission, PermissionScope, PutRoleRequest,
    RoleBindingQuery, RoleBindingView, ScopedRoleAssignment,
};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{AccessGroupId, CardName, CardUid, RoleBindingId};

use crate::error::WyrdCliError;
use crate::principal::assignment::AssignableKind;
use crate::principal::credential::{TenantEndpoint, principal_id};

/// Access verbs.
#[derive(Debug, Subcommand)]
pub enum AccessCommand {
    /// Show one account's current access: every binding, its source, and the
    /// combined permissions.
    Show(ShowArgs),
    /// Bind a role at an explicit scope to an account or group; requires
    /// tenant administration.
    Grant(GrantArgs),
    /// Remove one directly administered binding; requires tenant
    /// administration.
    Revoke(BindingArgs),
    /// List bindings, optionally for one account, group, or role.
    Bindings(BindingsArgs),
    /// List every role and its operations.
    Roles(EndpointArgs),
    /// Create or replace a custom role; requires tenant administration.
    PutRole(PutRoleArgs),
    /// Delete an unbound custom role; requires tenant administration.
    DeleteRole(RoleNameArgs),
    /// List local groups and their members.
    Groups(EndpointArgs),
    /// Create a local group; requires tenant administration.
    CreateGroup(CreateGroupArgs),
    /// Delete a local group with its memberships and bindings.
    DeleteGroup(GroupArgs),
    /// Add an account to a group.
    AddMember(MemberArgs),
    /// Remove an account from a group.
    RemoveMember(MemberArgs),
}

/// Only the deployment.
#[derive(Debug, Args)]
pub struct EndpointArgs {
    /// Deployment; the credential comes from the ambient chain.
    #[command(flatten)]
    pub endpoint: TenantEndpoint,
}

/// One account.
#[derive(Debug, Args)]
pub struct ShowArgs {
    /// Account to summarize.
    #[arg(long, value_name = "UUID")]
    pub principal: String,
    /// Deployment; the credential comes from the ambient chain.
    #[command(flatten)]
    pub endpoint: TenantEndpoint,
}

/// One grant to one account or group.
#[derive(Debug, Args)]
pub struct GrantArgs {
    /// Account receiving the grant; give `--kind` with it.
    #[arg(
        long,
        value_name = "UUID",
        required_unless_present = "group",
        conflicts_with = "group"
    )]
    pub principal: Option<String>,
    /// The account's kind.
    #[arg(long, value_enum, requires = "principal")]
    pub kind: Option<AssignableKind>,
    /// Group receiving the grant.
    #[arg(long, value_name = "UUID")]
    pub group: Option<String>,
    /// `ROLE:SCOPE`, such as `viewer:Repository/payments`.
    #[arg(long, value_name = "ROLE:SCOPE", value_parser = parse_assignment)]
    pub grant: ScopedRoleAssignment,
    /// Deployment; the credential comes from the ambient chain.
    #[command(flatten)]
    pub endpoint: TenantEndpoint,
}

/// One binding.
#[derive(Debug, Args)]
pub struct BindingArgs {
    /// Binding to remove.
    #[arg(long, value_name = "UUID")]
    pub binding: RoleBindingId,
    /// Deployment; the credential comes from the ambient chain.
    #[command(flatten)]
    pub endpoint: TenantEndpoint,
}

/// Binding filters.
#[derive(Debug, Args)]
pub struct BindingsArgs {
    /// Only this account's bindings.
    #[arg(long, value_name = "UUID")]
    pub principal: Option<String>,
    /// Only this group's bindings.
    #[arg(long, value_name = "UUID")]
    pub group: Option<AccessGroupId>,
    /// Only this role's bindings.
    #[arg(long, value_name = "ROLE")]
    pub role: Option<String>,
    /// Deployment; the credential comes from the ambient chain.
    #[command(flatten)]
    pub endpoint: TenantEndpoint,
}

/// One custom role and its operations.
#[derive(Debug, Args)]
pub struct PutRoleArgs {
    /// Custom role name.
    #[arg(long, value_name = "ROLE")]
    pub role: String,
    /// Operation such as `cards:read`; repeat for several.
    #[arg(long = "operation", value_name = "RESOURCE:ACTION", required = true)]
    pub operations: Vec<Permission>,
    /// Deployment; the credential comes from the ambient chain.
    #[command(flatten)]
    pub endpoint: TenantEndpoint,
}

/// One role name.
#[derive(Debug, Args)]
pub struct RoleNameArgs {
    /// Custom role name.
    #[arg(long, value_name = "ROLE")]
    pub role: String,
    /// Deployment; the credential comes from the ambient chain.
    #[command(flatten)]
    pub endpoint: TenantEndpoint,
}

/// A new group.
#[derive(Debug, Args)]
pub struct CreateGroupArgs {
    /// Tenant-unique group name.
    #[arg(long, value_name = "NAME")]
    pub name: String,
    /// Deployment; the credential comes from the ambient chain.
    #[command(flatten)]
    pub endpoint: TenantEndpoint,
}

/// One group.
#[derive(Debug, Args)]
pub struct GroupArgs {
    /// Group.
    #[arg(long, value_name = "UUID")]
    pub group: AccessGroupId,
    /// Deployment; the credential comes from the ambient chain.
    #[command(flatten)]
    pub endpoint: TenantEndpoint,
}

/// One group and one account.
#[derive(Debug, Args)]
pub struct MemberArgs {
    /// Group.
    #[arg(long, value_name = "UUID")]
    pub group: AccessGroupId,
    /// Account.
    #[arg(long, value_name = "UUID")]
    pub principal: String,
    /// Deployment; the credential comes from the ambient chain.
    #[command(flatten)]
    pub endpoint: TenantEndpoint,
}

/// Parse `ROLE:SCOPE`; the scope is required and never defaults to `all`.
///
/// # Errors
/// Returns a message naming the accepted forms when either half is missing
/// or the scope is malformed.
pub fn parse_assignment(value: &str) -> Result<ScopedRoleAssignment, String> {
    let (role, scope) = value
        .split_once(':')
        .filter(|(role, scope)| !role.is_empty() && !scope.is_empty())
        .ok_or_else(|| "expected ROLE:SCOPE, such as viewer:Repository/payments".to_owned())?;
    Ok(ScopedRoleAssignment {
        role: role.to_owned(),
        scope: parse_scope(scope)?,
    })
}

/// Parse one scope: `all`, `KIND/NAME`, `KIND/NAME@UID`, or JSON.
fn parse_scope(value: &str) -> Result<PermissionScope, String> {
    if value == "all" {
        return Ok(PermissionScope::All);
    }
    if value.starts_with('{') {
        return serde_json::from_str(value).map_err(|error| format!("invalid scope JSON: {error}"));
    }
    let (family, uid) = match value.split_once('@') {
        Some((family, uid)) => (family, Some(uid)),
        None => (value, None),
    };
    let (kind, name) = family
        .split_once('/')
        .ok_or_else(|| "expected all, KIND/NAME, KIND/NAME@UID, or a JSON scope".to_owned())?;
    let kind: CardKind = serde_json::from_value(serde_json::Value::String(kind.to_owned()))
        .map_err(|_| format!("unknown Card kind {kind:?}"))?;
    let name = CardName::new(name).map_err(|error| error.to_string())?;
    let scope = match uid {
        None => CardPermissionScope::family(kind, name),
        Some(uid) => CardPermissionScope::version(
            kind,
            name,
            uid.parse::<CardUid>().map_err(|error| error.to_string())?,
        ),
    };
    Ok(PermissionScope::Card(scope))
}

/// Dispatch one access verb.
///
/// # Errors
/// Returns [`WyrdCliError::InvalidArgument`] for a malformed id, and the
/// server's stable Wyrd error when the caller is unauthorized or the request
/// is refused.
pub async fn dispatch(command: AccessCommand) -> Result<ExitCode, WyrdCliError> {
    let server = |source| WyrdCliError::Server { source };
    let endpoint = match &command {
        AccessCommand::Show(args) => &args.endpoint,
        AccessCommand::Grant(args) => &args.endpoint,
        AccessCommand::Revoke(args) => &args.endpoint,
        AccessCommand::Bindings(args) => &args.endpoint,
        AccessCommand::Roles(args) | AccessCommand::Groups(args) => &args.endpoint,
        AccessCommand::PutRole(args) => &args.endpoint,
        AccessCommand::DeleteRole(args) => &args.endpoint,
        AccessCommand::CreateGroup(args) => &args.endpoint,
        AccessCommand::DeleteGroup(args) => &args.endpoint,
        AccessCommand::AddMember(args) | AccessCommand::RemoveMember(args) => &args.endpoint,
    };
    let principals = crate::client::principals(endpoint.server.as_str()).await?;
    match command {
        AccessCommand::Show(args) => {
            let summary = principals
                .access(&principal_id(&args.principal)?)
                .await
                .map_err(server)?;
            print_summary(&summary);
        }
        AccessCommand::Grant(args) => {
            let subject = match (args.principal, args.group) {
                (Some(principal), _) => BindingSubject::Principal {
                    id: principal_id(&principal)?,
                    kind: args
                        .kind
                        .ok_or_else(|| invalid("kind", "", "user or service with --principal"))?
                        .into(),
                },
                (None, Some(group)) => BindingSubject::Group {
                    id: group
                        .parse()
                        .map_err(|_| invalid("group", &group, "a UUIDv7 group id"))?,
                },
                (None, None) => {
                    return Err(invalid("principal", "", "--principal or --group"));
                }
            };
            let binding = principals
                .bind(&CreateRoleBindingRequest {
                    subject,
                    role: args.grant.role,
                    scope: args.grant.scope,
                })
                .await
                .map_err(server)?;
            print_binding(&binding);
        }
        AccessCommand::Revoke(args) => {
            print_binding(&principals.unbind(&args.binding).await.map_err(server)?);
        }
        AccessCommand::Bindings(args) => {
            let query = RoleBindingQuery {
                principal_id: args.principal.as_deref().map(principal_id).transpose()?,
                group_id: args.group,
                role: args.role,
            };
            for binding in principals.bindings(&query).await.map_err(server)?.bindings {
                print_binding(&binding);
            }
        }
        AccessCommand::Roles(_) => {
            for role in principals.roles().await.map_err(server)?.roles {
                let operations: Vec<String> = role
                    .operations
                    .iter()
                    .map(|operation| {
                        Permission {
                            resource: operation.resource.clone(),
                            action: operation.action.clone(),
                            scope: PermissionScope::All,
                        }
                        .to_string()
                    })
                    .collect();
                let builtin = if role.builtin { "  builtin" } else { "" };
                println!("{}  {}{builtin}", role.name, operations.join(","));
            }
        }
        AccessCommand::PutRole(args) => {
            let request = PutRoleRequest {
                operations: args.operations.iter().map(Operation::of).collect(),
                description: None,
            };
            let role = principals
                .put_role(&args.role, &request)
                .await
                .map_err(server)?;
            println!("{}", role.name);
        }
        AccessCommand::DeleteRole(args) => {
            let role = principals.delete_role(&args.role).await.map_err(server)?;
            println!("{}", role.name);
        }
        AccessCommand::Groups(_) => {
            for group in principals.groups().await.map_err(server)?.groups {
                print_group(&group);
            }
        }
        AccessCommand::CreateGroup(args) => {
            print_group(
                &principals
                    .create_group(&CreateGroupRequest { name: args.name })
                    .await
                    .map_err(server)?,
            );
        }
        AccessCommand::DeleteGroup(args) => {
            print_group(&principals.delete_group(&args.group).await.map_err(server)?);
        }
        AccessCommand::AddMember(args) => {
            let principal = principal_id(&args.principal)?;
            print_group(
                &principals
                    .add_group_member(&args.group, &principal)
                    .await
                    .map_err(server)?,
            );
        }
        AccessCommand::RemoveMember(args) => {
            let principal = principal_id(&args.principal)?;
            print_group(
                &principals
                    .remove_group_member(&args.group, &principal)
                    .await
                    .map_err(server)?,
            );
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// The refusal of a malformed `field`.
fn invalid(field: &str, value: &str, expected: &str) -> WyrdCliError {
    WyrdCliError::InvalidArgument {
        field: field.to_owned(),
        value: value.to_owned(),
        expected: expected.to_owned(),
    }
}

/// The scope as `parse_scope` reads it back.
fn scope_label(scope: &PermissionScope) -> String {
    match scope {
        PermissionScope::All => "all".to_owned(),
        PermissionScope::Card(card) => {
            let kind = serde_json::to_value(card.kind())
                .ok()
                .and_then(|value| value.as_str().map(str::to_owned))
                .unwrap_or_default();
            match card {
                CardPermissionScope::Version { name, uid, .. } => format!("{kind}/{name}@{uid}"),
                CardPermissionScope::Family { name, .. } => format!("{kind}/{name}"),
            }
        }
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

/// Print `binding  role:scope  subject  source`.
fn print_binding(binding: &RoleBindingView) {
    let subject = match &binding.subject {
        BindingSubject::Principal { id, kind } => format!("{}:{id}", kind.as_str()),
        BindingSubject::Group { id } => format!("group:{id}"),
    };
    println!(
        "{}  {}:{}  {subject}  {}",
        binding.id,
        binding.role,
        scope_label(&binding.scope),
        binding.source.as_str()
    );
}

/// Print one group and its members.
fn print_group(group: &GroupView) {
    let members: Vec<String> = group
        .members
        .iter()
        .map(|member| member.principal_id.to_string())
        .collect();
    println!("{}  {}  {}", group.id, group.name, members.join(","));
}

/// Print the account's grants, then its combined permissions.
fn print_summary(summary: &AccessSummary) {
    println!("{}  {}", summary.principal_id, summary.kind.as_str());
    for grant in &summary.grants {
        let source = match &grant.source {
            wyrd_spec::auth::GrantSource::Direct => "direct".to_owned(),
            wyrd_spec::auth::GrantSource::Provider => "provider".to_owned(),
            wyrd_spec::auth::GrantSource::Group { name, .. } => format!("group:{name}"),
        };
        println!(
            "grant  {}  {}:{}  {source}",
            grant.binding_id,
            grant.role,
            scope_label(&grant.scope)
        );
    }
    for permission in &summary.permissions {
        println!(
            "permission  {permission}  {}",
            scope_label(&permission.scope)
        );
    }
}

/// Grant parsing.
#[cfg(test)]
mod tests {
    use wyrd_spec::auth::{CardPermissionScope, PermissionScope};
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::CardName;

    use super::parse_assignment;

    /// A grant names an explicit scope; family, tenant and malformed forms
    /// parse or refuse as documented, and a bare role is never tenant-wide.
    ///
    /// # Panics
    /// Panics when a form parses differently than documented.
    #[test]
    fn grants_require_an_explicit_scope() {
        let family = parse_assignment("viewer:Repository/payments").expect("family parses");
        assert_eq!(family.role, "viewer");
        assert_eq!(
            family.scope,
            PermissionScope::Card(CardPermissionScope::family(
                CardKind::Repository,
                CardName::new("payments").expect("name"),
            ))
        );
        assert_eq!(
            parse_assignment("admin:all").expect("tenant parses").scope,
            PermissionScope::All
        );
        for refused in [
            "viewer",
            "viewer:",
            ":all",
            "viewer:payments",
            "viewer:Nope/x",
        ] {
            assert!(parse_assignment(refused).is_err(), "{refused} is refused");
        }
    }
}
