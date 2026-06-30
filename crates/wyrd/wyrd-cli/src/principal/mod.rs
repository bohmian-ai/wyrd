pub mod revoke;

use std::process::ExitCode;

use clap::Subcommand;

use crate::error::WyrdCliError;

#[derive(Debug, Subcommand)]
pub enum PrincipalCommand {
    /// Revoke a principal's credentials immediately.
    Revoke(revoke::RevokeArgs),
}

pub async fn dispatch(command: PrincipalCommand) -> Result<ExitCode, WyrdCliError> {
    match command {
        PrincipalCommand::Revoke(args) => revoke::dispatch(args).await,
    }
}
