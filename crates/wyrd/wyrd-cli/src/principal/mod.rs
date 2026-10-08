pub mod assignment;
pub mod credential;
pub mod revoke;

use std::process::ExitCode;

use clap::Subcommand;

use crate::error::WyrdCliError;

#[derive(Debug, Subcommand)]
/// The `wyrd principal` subcommands: acting on a principal's identity and
/// acting on the credentials that authenticate it are kept apart so revoking
/// access never reads as a credential edit.
pub enum PrincipalCommand {
    /// Suspend a principal so it can mint no new token; tokens it already
    /// holds lapse within five minutes.
    Revoke(revoke::RevokeArgs),
    /// Create principals and administer their credentials.
    #[command(subcommand)]
    Credential(credential::CredentialCommand),
    /// List the tenant's assignable users, services, and agents.
    List(assignment::ListArgs),
    /// List, grant, and revoke a principal's Roles.
    #[command(subcommand)]
    Role(assignment::RoleCommand),
}

/// Route a `wyrd principal` invocation to the subcommand that owns it.
///
/// # Errors
/// Returns whatever [`WyrdCliError`] the selected subcommand produced.
pub async fn dispatch(command: PrincipalCommand) -> Result<ExitCode, WyrdCliError> {
    match command {
        PrincipalCommand::Revoke(args) => revoke::dispatch(args).await,
        PrincipalCommand::Credential(command) => credential::dispatch(command).await,
        PrincipalCommand::List(args) => assignment::list(args).await,
        PrincipalCommand::Role(command) => assignment::dispatch(command).await,
    }
}
