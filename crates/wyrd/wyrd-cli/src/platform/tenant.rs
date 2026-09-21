//! Tenant lifecycle from the command line.
//!
//! Everything an operator does to a tenant from outside it: create one, read
//! the directory, freeze and restore admission, and restore administrative
//! access when a tenant has lost every credential. Each verb is one call to
//! [`wyrd_client::Platform`]; the CLI holds no tenant state and no SQL.

use std::process::ExitCode;

use clap::{Args, Subcommand};
use wyrd_spec::DataTenantId;

use super::credential::PlatformEndpoint;
use crate::error::WyrdCliError;

/// Tenant verbs on the platform plane.
#[derive(Debug, Subcommand)]
pub enum TenantCommand {
    /// Provision a tenant and print its one-time administrative credential.
    Create(CreateArgs),
    /// List every tenant, in every lifecycle state.
    List(ListArgs),
    /// Read one tenant's directory row.
    Inspect(TenantArgs),
    /// Freeze a tenant's admission without destroying anything.
    Suspend(TenantArgs),
    /// Restore a suspended tenant to exactly what it was.
    Resume(TenantArgs),
    /// Issue a replacement credential for a tenant's existing administrator.
    RecoverAdmin(TenantArgs),
}

/// Provision a tenant.
#[derive(Debug, Args)]
pub struct CreateArgs {
    /// URL-safe tenant identifier, unique across the deployment.
    #[arg(long, value_name = "SLUG")]
    pub slug: String,
    /// Human-readable tenant name.
    #[arg(long, value_name = "NAME")]
    pub display_name: String,
    /// Deployment and platform credential.
    #[command(flatten)]
    pub endpoint: PlatformEndpoint,
}

/// Read the whole directory.
#[derive(Debug, Args)]
pub struct ListArgs {
    /// Deployment and platform credential.
    #[command(flatten)]
    pub endpoint: PlatformEndpoint,
}

/// Act on one named tenant.
#[derive(Debug, Args)]
pub struct TenantArgs {
    /// Tenant to act on.
    #[arg(long, value_name = "UUID")]
    pub tenant: String,
    /// Deployment and platform credential.
    #[command(flatten)]
    pub endpoint: PlatformEndpoint,
}

/// Dispatch one tenant verb.
///
/// # Errors
/// Returns [`WyrdCliError::InvalidArgument`] when the slug or tenant id is
/// malformed, and the server's stable Wyrd error when the caller is
/// unauthorized, the slug is taken, or the tenant is not in the state the
/// requested transition needs.
pub async fn dispatch(command: TenantCommand) -> Result<ExitCode, WyrdCliError> {
    match command {
        TenantCommand::Create(args) => create(args).await,
        TenantCommand::List(args) => list(args).await,
        TenantCommand::Inspect(args) => inspect(args).await,
        TenantCommand::Suspend(args) => set_status(args, "suspended").await,
        TenantCommand::Resume(args) => set_status(args, "active").await,
        TenantCommand::RecoverAdmin(args) => recover_admin(args).await,
    }
}

/// Parse an operator-supplied tenant id.
///
/// # Errors
/// Returns [`WyrdCliError::InvalidArgument`] when the value is not a Wyrd
/// tenant id.
fn tenant_id(value: &str) -> Result<DataTenantId, WyrdCliError> {
    value.parse().map_err(|_| WyrdCliError::InvalidArgument {
        field: "tenant".to_owned(),
        value: value.to_owned(),
        expected: "a tenant UUIDv7".to_owned(),
    })
}

/// Provision a tenant and print its credential, once.
///
/// # Errors
/// Returns the errors documented on [`dispatch`].
async fn create(args: CreateArgs) -> Result<ExitCode, WyrdCliError> {
    let slug = args
        .slug
        .parse()
        .map_err(|_| WyrdCliError::InvalidArgument {
            field: "slug".to_owned(),
            value: args.slug.clone(),
            expected: "a URL-safe tenant slug".to_owned(),
        })?;
    let created = args
        .endpoint
        .connect()
        .await?
        .create_tenant(&wyrd_spec::auth::CreateTenantRequest {
            slug,
            display_name: args.display_name,
        })
        .await
        .map_err(|source| WyrdCliError::Server { source })?;

    println!("tenant_id: {}", created.tenant.id);
    println!("slug: {}", created.tenant.slug);
    println!("status: {}", created.tenant.status);
    println!("admin_principal_id: {}", created.admin.principal_id);
    println!("admin_credential: {}", created.admin.credential.expose());
    println!("This is the only time the credential is shown. Store it now.");
    Ok(ExitCode::SUCCESS)
}

/// Print the directory, newest first.
///
/// # Errors
/// Returns the errors documented on [`dispatch`].
async fn list(args: ListArgs) -> Result<ExitCode, WyrdCliError> {
    let listing = args
        .endpoint
        .connect()
        .await?
        .list_tenants()
        .await
        .map_err(|source| WyrdCliError::Server { source })?;

    for tenant in listing.tenants {
        println!(
            "{}  {}  {}  {}",
            tenant.id, tenant.slug, tenant.status, tenant.display_name
        );
    }
    Ok(ExitCode::SUCCESS)
}

/// Print one tenant's directory row.
///
/// # Errors
/// Returns the errors documented on [`dispatch`].
async fn inspect(args: TenantArgs) -> Result<ExitCode, WyrdCliError> {
    let tenant = args
        .endpoint
        .connect()
        .await?
        .tenant(tenant_id(&args.tenant)?)
        .await
        .map_err(|source| WyrdCliError::Server { source })?;

    println!("tenant_id: {}", tenant.id);
    println!("slug: {}", tenant.slug);
    println!("display_name: {}", tenant.display_name);
    println!("status: {}", tenant.status);
    Ok(ExitCode::SUCCESS)
}

/// Move a tenant to `status`, which is refused unless it is in the opposite one.
///
/// # Errors
/// Returns the errors documented on [`dispatch`].
async fn set_status(args: TenantArgs, status: &str) -> Result<ExitCode, WyrdCliError> {
    let id = tenant_id(&args.tenant)?;
    args.endpoint
        .connect()
        .await?
        .set_tenant_status(id, status)
        .await
        .map_err(|source| WyrdCliError::Server { source })?;

    println!("{id}: {status}");
    Ok(ExitCode::SUCCESS)
}

/// Issue a replacement credential for the tenant's existing administrator.
///
/// # Errors
/// Returns the errors documented on [`dispatch`].
async fn recover_admin(args: TenantArgs) -> Result<ExitCode, WyrdCliError> {
    let recovered = args
        .endpoint
        .connect()
        .await?
        .recover_tenant_admin(tenant_id(&args.tenant)?)
        .await
        .map_err(|source| WyrdCliError::Server { source })?;

    println!("principal_id: {}", recovered.principal_id);
    println!("credential: {}", recovered.credential.expose());
    println!("This is the only time the credential is shown. Store it now.");
    Ok(ExitCode::SUCCESS)
}

/// Argument parsing for the platform tenant verbs.
#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::TenantCommand;

    /// Bare wrapper so the group parses without the binary's propagated flags.
    #[derive(Debug, Parser)]
    struct Cli {
        /// The tenant verb under test.
        #[command(subcommand)]
        command: TenantCommand,
    }

    /// Creating a tenant names both the slug and the display name.
    #[test]
    fn create_requires_a_slug_and_a_display_name() {
        let parsed = Cli::try_parse_from([
            "wyrd",
            "create",
            "--slug",
            "acme",
            "--display-name",
            "Acme",
            "--server",
            "https://wyrd.example",
        ])
        .expect("the create verb parses");
        let TenantCommand::Create(args) = parsed.command else {
            panic!("create parsed as another verb");
        };
        assert_eq!(args.slug, "acme");
    }

    /// Suspend and resume are separate verbs over one tenant, so neither can be
    /// invoked without naming which tenant it freezes or restores.
    #[test]
    fn suspending_requires_the_tenant() {
        let parsed = Cli::try_parse_from([
            "wyrd",
            "suspend",
            "--server",
            "https://wyrd.example",
        ]);
        assert!(parsed.is_err(), "suspend must require --tenant");
    }
}
