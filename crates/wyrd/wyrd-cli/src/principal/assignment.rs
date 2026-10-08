//! Principal discovery and Role assignment from the command line.
//!
//! An administrator finds a principal with `wyrd principal list`, reads its
//! Roles and their sources with `wyrd principal role list`, and grants or
//! revokes a direct assignment with `wyrd principal role grant|revoke`. Every
//! verb is one call through `wyrd_client::Principals`; nothing here reaches
//! the database.

use std::process::ExitCode;

use clap::{Args, Subcommand, ValueEnum};
use wyrd_spec::auth::{PrincipalKindTag, PrincipalQuery, PrincipalStatus, RoleAssignment};

use crate::error::WyrdCliError;
use crate::principal::credential::{PrincipalArgs, TenantEndpoint, principal_id};

/// Kinds of principal a Role can be assigned to.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum AssignableKind {
    /// Human users.
    User,
    /// Service principals.
    Service,
    /// Agent principals.
    Agent,
}

impl From<AssignableKind> for PrincipalKindTag {
    /// The wire discriminator for the selected kind.
    fn from(kind: AssignableKind) -> Self {
        match kind {
            AssignableKind::User => Self::User,
            AssignableKind::Service => Self::Service,
            AssignableKind::Agent => Self::Agent,
        }
    }
}

/// Filters and paging for `wyrd principal list`.
#[derive(Debug, Args)]
pub struct ListArgs {
    /// Only principals of this kind.
    #[arg(long, value_enum)]
    pub kind: Option<AssignableKind>,
    /// Only the user with this exact email.
    #[arg(long, value_name = "EMAIL")]
    pub email: Option<String>,
    /// Only the service or agent with this exact name.
    #[arg(long, value_name = "NAME")]
    pub name: Option<String>,
    /// Page size, 1 to 200; the server defaults to 100.
    #[arg(long, value_name = "N")]
    pub limit: Option<u32>,
    /// Continue after this principal id, the `next` of the previous page.
    #[arg(long, value_name = "UUID")]
    pub after: Option<String>,
    /// Deployment; the credential comes from the ambient chain.
    #[command(flatten)]
    pub endpoint: TenantEndpoint,
}

/// Role verbs on one principal.
#[derive(Debug, Subcommand)]
pub enum RoleCommand {
    /// List the principal's Roles and where each came from.
    List(PrincipalArgs),
    /// Grant the principal a direct Role; requires tenant administration.
    Grant(RoleArgs),
    /// Revoke the principal's direct Role; requires tenant administration.
    Revoke(RoleArgs),
}

/// One principal and one Role.
#[derive(Debug, Args)]
pub struct RoleArgs {
    /// Principal to act on.
    #[arg(long, value_name = "UUID")]
    pub principal: String,
    /// Role name, such as `editor`.
    #[arg(long, value_name = "ROLE")]
    pub role: String,
    /// Deployment; the credential comes from the ambient chain.
    #[command(flatten)]
    pub endpoint: TenantEndpoint,
}

/// Print one page of assignable principals, then the next-page cursor.
///
/// # Errors
/// Returns [`WyrdCliError::InvalidArgument`] when `--after` is not a UUID, and
/// the server's stable Wyrd error when a filter or page size is invalid or
/// the caller lacks principal administration.
pub async fn list(args: ListArgs) -> Result<ExitCode, WyrdCliError> {
    let query = PrincipalQuery {
        kind: args.kind.map(PrincipalKindTag::from),
        email: args.email,
        name: args.name,
        limit: args.limit,
        after: args.after.as_deref().map(principal_id).transpose()?,
    };
    let page = crate::client::principals(args.endpoint.server.as_str())?
        .list(&query)
        .await
        .map_err(|source| WyrdCliError::Server { source })?;

    for principal in page.principals {
        let label = principal
            .email
            .or(principal.name)
            .unwrap_or_else(|| "-".to_owned());
        let card = principal
            .card_ref
            .map_or_else(String::new, |card_ref| format!("  {card_ref}"));
        println!(
            "{}  {}  {}  {label}{card}",
            principal.principal_id,
            principal.kind.as_str(),
            match principal.status {
                PrincipalStatus::Active => "active",
                PrincipalStatus::Suspended => "suspended",
            },
        );
    }
    if let Some(next) = page.next {
        println!("next: {next}");
    }
    Ok(ExitCode::SUCCESS)
}

/// Dispatch one principal Role verb.
///
/// # Errors
/// Returns [`WyrdCliError::InvalidArgument`] when the principal is not a UUID,
/// and the server's stable Wyrd error when the caller is unauthorized, the
/// Role does not exist, or the principal is not assignable in the tenant.
pub async fn dispatch(command: RoleCommand) -> Result<ExitCode, WyrdCliError> {
    let (endpoint, principal) = match &command {
        RoleCommand::List(args) => (&args.endpoint, principal_id(&args.principal)?),
        RoleCommand::Grant(args) | RoleCommand::Revoke(args) => {
            (&args.endpoint, principal_id(&args.principal)?)
        }
    };
    let principals = crate::client::principals(endpoint.server.as_str())?;
    let server = |source| WyrdCliError::Server { source };
    let roles = match &command {
        RoleCommand::List(_) => principals.roles(&principal).await.map_err(server)?.roles,
        RoleCommand::Grant(args) => {
            let change = principals
                .grant_role(&principal, &args.role)
                .await
                .map_err(server)?;
            println!("changed: {}", change.changed);
            change.roles
        }
        RoleCommand::Revoke(args) => {
            let change = principals
                .revoke_role(&principal, &args.role)
                .await
                .map_err(server)?;
            println!("changed: {}", change.changed);
            change.roles
        }
    };
    print_assignments(&roles);
    Ok(ExitCode::SUCCESS)
}

/// Print one `role  source` line per assignment.
fn print_assignments(roles: &[RoleAssignment]) {
    for assignment in roles {
        println!("{}  {}", assignment.role, assignment.source.as_str());
    }
}

/// Argument parsing for the principal discovery and Role verbs.
#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{ListArgs, RoleCommand};

    /// Bare wrapper so the Role group parses without the binary's flags.
    #[derive(Debug, Parser)]
    struct RoleCli {
        /// The verb under test.
        #[command(subcommand)]
        command: RoleCommand,
    }

    /// Bare wrapper for the list verb.
    #[derive(Debug, Parser)]
    struct ListCli {
        /// The verb under test.
        #[command(flatten)]
        args: ListArgs,
    }

    /// A Role write names both the principal and the Role.
    #[test]
    fn role_writes_require_principal_and_role() {
        for verb in ["grant", "revoke"] {
            assert!(
                RoleCli::try_parse_from([
                    "wyrd",
                    verb,
                    "--principal",
                    "00000000-0000-7000-8000-000000000001",
                    "--server",
                    "https://wyrd.example",
                ])
                .is_err(),
                "{verb} requires --role"
            );
            assert!(
                RoleCli::try_parse_from([
                    "wyrd",
                    verb,
                    "--principal",
                    "00000000-0000-7000-8000-000000000001",
                    "--role",
                    "editor",
                    "--server",
                    "https://wyrd.example",
                ])
                .is_ok(),
                "{verb} parses"
            );
        }
    }

    /// Discovery accepts only assignable kinds.
    #[test]
    fn list_accepts_only_assignable_kinds() {
        for kind in ["user", "service", "agent"] {
            assert!(
                ListCli::try_parse_from([
                    "wyrd",
                    "--kind",
                    kind,
                    "--server",
                    "https://wyrd.example"
                ])
                .is_ok(),
                "{kind} parses"
            );
        }
        assert!(
            ListCli::try_parse_from([
                "wyrd",
                "--kind",
                "tenant_admin",
                "--server",
                "https://wyrd.example"
            ])
            .is_err()
        );
    }
}
