//! Platform control-plane commands.
//!
//! The platform plane is reached with a platform credential rather than a
//! tenant token, so these commands take `--credential` and exchange it for a
//! short-lived session through [`wyrd_client::Platform`] — the same exchange any
//! other platform client performs. The CLI owns no platform transport of its
//! own.

pub mod credential;
pub mod tenant;

use std::process::ExitCode;

use clap::Subcommand;

use crate::error::WyrdCliError;

/// Platform control-plane verbs.
#[derive(Debug, Subcommand)]
pub enum PlatformCommand {
    /// Issue, list, and retire platform credentials.
    #[command(subcommand)]
    Credential(credential::CredentialCommand),
    /// Create, inspect, suspend, resume, and recover tenants.
    #[command(subcommand)]
    Tenant(tenant::TenantCommand),
}

/// Dispatch one platform command through its owning module.
///
/// # Errors
/// Returns the CLI's local error for a rejected endpoint or argument, and the
/// server's stable Wyrd error for an authorization or store failure.
pub async fn dispatch(command: PlatformCommand) -> Result<ExitCode, WyrdCliError> {
    match command {
        PlatformCommand::Credential(command) => credential::dispatch(command).await,
        PlatformCommand::Tenant(command) => tenant::dispatch(command).await,
    }
}
