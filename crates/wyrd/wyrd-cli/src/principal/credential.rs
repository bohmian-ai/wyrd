//! Tenant principal and credential administration from the command line.
//!
//! The tenant-side counterpart to `wyrd platform tenant`: create a restricted
//! principal, rotate its credentials with an overlap, and retire the one it
//! replaced. Every verb is one call through `wyrd_client::Principals`; nothing
//! here reaches the database.

use std::process::ExitCode;

use clap::{Args, Subcommand};
use url::Url;
use uuid::Uuid;
use wyrd_spec::auth::{
    CreateServicePrincipalRequest, PrincipalId, WorkloadFamily, WorkloadKind, WorkloadRestriction,
};
use wyrd_spec::ids::CardName;

use crate::error::WyrdCliError;

/// Principal and credential verbs inside one tenant.
#[derive(Debug, Subcommand)]
pub enum CredentialCommand {
    /// Create a machine principal holding only the roles named.
    CreatePrincipal(CreateArgs),
    /// Issue an additional credential, leaving the current one live.
    Issue(PrincipalArgs),
    /// List a principal's credential metadata, revoked ones included.
    List(PrincipalArgs),
    /// Retire one credential, leaving the principal and its roles untouched.
    Revoke(RevokeCredentialArgs),
    /// Replace a Service account's workload restriction; omitting the family
    /// makes the account unrestricted.
    SetRestriction(SetRestrictionArgs),
}

/// An optional Service workload family, named by its Card name.
#[derive(Debug, Args)]
pub struct WorkloadFamilyArgs {
    /// Card name of the Service family the account is restricted to.
    #[arg(long, value_name = "NAME")]
    pub workload_name: Option<CardName>,
}

impl WorkloadFamilyArgs {
    /// The restriction these flags name: the Service family when a name is
    /// given, otherwise unrestricted.
    fn restriction(self) -> WorkloadRestriction {
        self.workload_name
            .map(|name| WorkloadFamily {
                kind: WorkloadKind::Service,
                name,
            })
            .into()
    }
}

/// Replace one Service account's workload restriction.
#[derive(Debug, Args)]
pub struct SetRestrictionArgs {
    /// Service account to restrict.
    #[arg(long, value_name = "UUID")]
    pub principal: String,
    /// Family to restrict to; omitted means unrestricted.
    #[command(flatten)]
    pub family: WorkloadFamilyArgs,
    /// Deployment; the credential comes from the ambient chain.
    #[command(flatten)]
    pub endpoint: TenantEndpoint,
}

/// How every tenant command reaches a deployment.
///
/// Only the endpoint is an argument. The credential comes from the same
/// ambient chain the SDKs read — `WYRD_ACCESS_TOKEN`, workload identity,
/// `WYRD_API_KEY`, or `credentials.toml` — so it never enters argv, shell
/// history, or the derived `Debug` of parsed arguments.
#[derive(Debug, Args)]
pub struct TenantEndpoint {
    /// Wyrd server base URL. The credential is read from the ambient chain
    /// (`WYRD_ACCESS_TOKEN`, workload identity, `WYRD_API_KEY`, or
    /// `credentials.toml`).
    #[arg(long, value_name = "URL", env = "WYRD_SERVER_URL")]
    pub server: Url,
}

/// Create a restricted machine principal.
#[derive(Debug, Args)]
pub struct CreateArgs {
    /// Operator-facing name for the principal.
    #[arg(long, value_name = "NAME")]
    pub name: String,
    /// Roles to grant, repeated. A principal with none can authenticate and
    /// do nothing, which is a deliberate and useful starting point.
    #[arg(long = "role", value_name = "ROLE")]
    pub roles: Vec<String>,
    /// Optional description recorded with the principal.
    #[arg(long, value_name = "TEXT")]
    pub description: Option<String>,
    /// Optional family restriction; the account is that family's unique
    /// scheduled runtime account and creating it needs tenant administration.
    #[command(flatten)]
    pub family: WorkloadFamilyArgs,
    /// Deployment; the credential comes from the ambient chain.
    #[command(flatten)]
    pub endpoint: TenantEndpoint,
}

/// Act on one principal's credentials.
#[derive(Debug, Args)]
pub struct PrincipalArgs {
    /// Principal to act on.
    #[arg(long, value_name = "UUID")]
    pub principal: String,
    /// Deployment; the credential comes from the ambient chain.
    #[command(flatten)]
    pub endpoint: TenantEndpoint,
}

/// Retire one credential of one principal.
#[derive(Debug, Args)]
pub struct RevokeCredentialArgs {
    /// Principal that owns the credential.
    #[arg(long, value_name = "UUID")]
    pub principal: String,
    /// Credential to retire.
    #[arg(long, value_name = "UUID")]
    pub credential_id: Uuid,
    /// Deployment; the credential comes from the ambient chain.
    #[command(flatten)]
    pub endpoint: TenantEndpoint,
}

/// Dispatch one principal-credential verb.
///
/// # Errors
/// Returns [`WyrdCliError::InvalidArgument`] when the principal is not a UUID,
/// and the server's stable Wyrd error when the caller is unauthorized, a role
/// does not exist in the tenant, the principal or credential is unknown, or
/// another live account already holds the requested workload family.
pub async fn dispatch(command: CredentialCommand) -> Result<ExitCode, WyrdCliError> {
    match command {
        CredentialCommand::CreatePrincipal(args) => create(args).await,
        CredentialCommand::Issue(args) => issue(args).await,
        CredentialCommand::List(args) => list(args).await,
        CredentialCommand::Revoke(args) => revoke(args).await,
        CredentialCommand::SetRestriction(args) => set_restriction(args).await,
    }
}

/// Parse an operator-supplied principal id.
///
/// # Errors
/// Returns [`WyrdCliError::InvalidArgument`] when the value is not a UUID.
pub(crate) fn principal_id(value: &str) -> Result<PrincipalId, WyrdCliError> {
    value.parse().map_err(|_| WyrdCliError::InvalidArgument {
        field: "principal".to_owned(),
        value: value.to_owned(),
        expected: "a principal UUID".to_owned(),
    })
}

/// Create the principal and print its first credential, once.
///
/// # Errors
/// Returns the errors documented on [`dispatch`].
async fn create(args: CreateArgs) -> Result<ExitCode, WyrdCliError> {
    let created = crate::client::principals(args.endpoint.server.as_str())
        .await?
        .create_service_principal(&CreateServicePrincipalRequest {
            name: args.name,
            roles: args.roles,
            description: args.description,
            workload_restriction: Some(args.family.restriction()),
        })
        .await
        .map_err(|source| WyrdCliError::Server { source })?;

    println!("principal_id: {}", created.principal_id);
    println!("credential: {}", created.credential.expose());
    println!("This is the only time the credential is shown. Store it now.");
    Ok(ExitCode::SUCCESS)
}

/// Issue a second live credential, which is the first half of a rotation.
///
/// # Errors
/// Returns the errors documented on [`dispatch`].
async fn issue(args: PrincipalArgs) -> Result<ExitCode, WyrdCliError> {
    let principal = principal_id(&args.principal)?;
    let issued = crate::client::principals(args.endpoint.server.as_str())
        .await?
        .issue_credential(&principal)
        .await
        .map_err(|source| WyrdCliError::Server { source })?;

    println!("credential_id: {}", issued.id);
    println!("credential: {}", issued.credential.expose());
    println!("This is the only time the credential is shown. Store it now.");
    Ok(ExitCode::SUCCESS)
}

/// Print one principal's credential metadata, newest first.
///
/// # Errors
/// Returns the errors documented on [`dispatch`].
async fn list(args: PrincipalArgs) -> Result<ExitCode, WyrdCliError> {
    let principal = principal_id(&args.principal)?;
    let listing = crate::client::principals(args.endpoint.server.as_str())
        .await?
        .list_credentials(&principal)
        .await
        .map_err(|source| WyrdCliError::Server { source })?;

    for credential in listing.credentials {
        let state = credential.revoked_at.map_or_else(
            || {
                credential
                    .expires_at
                    .map_or_else(|| "live".to_owned(), |at| format!("expires {at}"))
            },
            |at| format!("revoked {at}"),
        );
        println!(
            "{}  {}  created {}  {state}",
            credential.id, credential.prefix, credential.created_at
        );
    }
    Ok(ExitCode::SUCCESS)
}

/// Retire the superseded credential, which is the second half of a rotation.
///
/// # Errors
/// Returns the errors documented on [`dispatch`].
async fn revoke(args: RevokeCredentialArgs) -> Result<ExitCode, WyrdCliError> {
    let principal = principal_id(&args.principal)?;
    crate::client::principals(args.endpoint.server.as_str())
        .await?
        .revoke_credential(&principal, args.credential_id)
        .await
        .map_err(|source| WyrdCliError::Server { source })?;

    println!("revoked: {}", args.credential_id);
    Ok(ExitCode::SUCCESS)
}

/// Replace the account's restriction and print the one it now carries.
///
/// # Errors
/// Returns the errors documented on [`dispatch`].
async fn set_restriction(args: SetRestrictionArgs) -> Result<ExitCode, WyrdCliError> {
    let principal = principal_id(&args.principal)?;
    let summary = crate::client::principals(args.endpoint.server.as_str())
        .await?
        .set_workload_restriction(&principal, &args.family.restriction())
        .await
        .map_err(|source| WyrdCliError::Server { source })?;

    match summary.workload_restriction {
        Some(WorkloadRestriction::Family(family)) => println!(
            "{principal}: restricted to {:?}/{}",
            family.kind.card_kind(),
            family.name
        ),
        _ => println!("{principal}: unrestricted"),
    }
    Ok(ExitCode::SUCCESS)
}

/// Argument parsing for the tenant principal credential verbs.
#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::CredentialCommand;

    /// Bare wrapper so the group parses without the binary's propagated flags.
    #[derive(Debug, Parser)]
    struct Cli {
        /// The verb under test.
        #[command(subcommand)]
        command: CredentialCommand,
    }

    /// A principal may be created with no role at all, which is the narrowest
    /// thing a tenant can hand out and the right default to start from.
    #[test]
    fn creating_a_principal_does_not_require_a_role() {
        let parsed = Cli::try_parse_from([
            "wyrd",
            "create-principal",
            "--name",
            "ci-runner",
            "--server",
            "https://wyrd.example",
        ])
        .expect("the create verb parses");
        let CredentialCommand::CreatePrincipal(args) = parsed.command else {
            panic!("create parsed as another verb");
        };
        assert!(args.roles.is_empty());
    }

    /// Retiring a credential names the owning principal, so a credential id
    /// alone cannot retire someone else's.
    #[test]
    fn revoking_a_credential_requires_the_owning_principal() {
        let parsed = Cli::try_parse_from([
            "wyrd",
            "revoke",
            "--credential-id",
            "00000000-0000-7000-8000-000000000002",
            "--server",
            "https://wyrd.example",
        ]);
        assert!(parsed.is_err(), "revoke must require --principal");
    }

    /// A credential id is parsed as a UUID at the argument edge, so malformed
    /// or path-shaping text is refused before any request is constructed.
    ///
    /// # Panics
    /// Panics when clap accepts the non-UUID credential id.
    #[test]
    fn a_malformed_credential_id_is_refused_before_dispatch() {
        let parsed = Cli::try_parse_from([
            "wyrd",
            "revoke",
            "--principal",
            "00000000-0000-7000-8000-000000000001",
            "--credential-id",
            "../other/credentials/x",
            "--server",
            "https://wyrd.example",
        ]);
        assert!(
            parsed.is_err(),
            "revoke must refuse a non-UUID credential id"
        );
    }

    /// The tenant credential is never an argument: passing one is refused
    /// without echoing it, and parsed arguments cannot render one in `Debug`.
    #[test]
    fn the_tenant_credential_is_not_an_argument() {
        let secret = "wyrd_sk_secret_value";
        let refused = Cli::try_parse_from([
            "wyrd",
            "list",
            "--principal",
            "00000000-0000-7000-8000-000000000001",
            "--server",
            "https://wyrd.example",
            "--token",
            secret,
        ])
        .expect_err("--token is not accepted");
        assert!(!refused.to_string().contains(secret));

        let parsed = Cli::try_parse_from([
            "wyrd",
            "list",
            "--principal",
            "00000000-0000-7000-8000-000000000001",
            "--server",
            "https://wyrd.example",
        ])
        .expect("the list verb parses from --server alone");
        assert!(!format!("{parsed:?}").contains(secret));
    }
}
