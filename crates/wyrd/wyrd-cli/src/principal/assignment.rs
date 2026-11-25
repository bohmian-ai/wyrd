//! Principal discovery from the command line.
//!
//! An administrator finds a principal with `wyrd principal list`, then reads
//! and changes its scoped access with `wyrd principal access`. Every verb is
//! one call through `wyrd_client::Principals`; nothing here reaches the
//! database.

use std::process::ExitCode;

use clap::{Args, ValueEnum};
use wyrd_spec::auth::{PrincipalKindTag, PrincipalQuery, PrincipalStatus, WorkloadRestriction};

use crate::error::WyrdCliError;
use crate::principal::credential::{TenantEndpoint, principal_id};

/// Kinds of principal a Role can be assigned to.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum AssignableKind {
    /// Human users.
    User,
    /// Service principals.
    Service,
}

impl From<AssignableKind> for PrincipalKindTag {
    /// The wire discriminator for the selected kind.
    fn from(kind: AssignableKind) -> Self {
        match kind {
            AssignableKind::User => Self::User,
            AssignableKind::Service => Self::Service,
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
    /// Only the service account with this exact name.
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
        role: None,
        limit: args.limit,
        after: args.after.as_deref().map(principal_id).transpose()?,
    };
    let page = crate::client::principals(args.endpoint.server.as_str())
        .await?
        .list(&query)
        .await
        .map_err(|source| WyrdCliError::Server { source })?;

    for principal in page.principals {
        let label = principal
            .email
            .or(principal.name)
            .unwrap_or_else(|| "-".to_owned());
        let card = match principal.workload_restriction {
            Some(WorkloadRestriction::Family(family)) => {
                format!("  {:?}/{}", family.kind.card_kind(), family.name)
            }
            _ => String::new(),
        };
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

/// Argument parsing for the principal discovery and Role verbs.
#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::ListArgs;

    /// Bare wrapper for the list verb.
    #[derive(Debug, Parser)]
    struct ListCli {
        /// The verb under test.
        #[command(flatten)]
        args: ListArgs,
    }

    /// Discovery accepts only assignable kinds.
    ///
    /// # Panics
    /// Panics when `user` or `service` is refused, or `agent` or
    /// `tenant_admin` parses.
    #[test]
    fn list_accepts_only_assignable_kinds() {
        for kind in ["user", "service"] {
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
        for kind in ["agent", "tenant_admin"] {
            assert!(
                ListCli::try_parse_from([
                    "wyrd",
                    "--kind",
                    kind,
                    "--server",
                    "https://wyrd.example"
                ])
                .is_err(),
                "{kind} is refused"
            );
        }
    }
}
