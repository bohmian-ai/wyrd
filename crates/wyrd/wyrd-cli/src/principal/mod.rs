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
    /// Revoke a principal's credentials immediately.
    Revoke(revoke::RevokeArgs),
    /// Create principals and administer their credentials.
    #[command(subcommand)]
    Credential(credential::CredentialCommand),
}

/// Route a `wyrd principal` invocation to the subcommand that owns it.
///
/// # Errors
/// Returns whatever [`WyrdCliError`] the selected subcommand produced.
pub async fn dispatch(command: PrincipalCommand) -> Result<ExitCode, WyrdCliError> {
    match command {
        PrincipalCommand::Revoke(args) => revoke::dispatch(args).await,
        PrincipalCommand::Credential(command) => credential::dispatch(command).await,
    }
}
