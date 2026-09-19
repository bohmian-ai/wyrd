pub mod credential;
pub mod revoke;

use std::process::ExitCode;

use clap::Subcommand;

use crate::error::WyrdCliError;

#[derive(Debug, Subcommand)]
pub enum PrincipalCommand {
    /// Revoke a principal's credentials immediately.
    Revoke(revoke::RevokeArgs),
    /// Create principals and administer their credentials.
    #[command(subcommand)]
    Credential(credential::CredentialCommand),
}

pub async fn dispatch(command: PrincipalCommand) -> Result<ExitCode, WyrdCliError> {
    match command {
        PrincipalCommand::Revoke(args) => revoke::dispatch(args).await,
        PrincipalCommand::Credential(command) => credential::dispatch(command).await,
    }
}
