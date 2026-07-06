pub mod bootstrap;

use std::process::ExitCode;

use clap::Subcommand;

use crate::error::WyrdCliError;

#[derive(Debug, Subcommand)]
pub enum DevCommand {
    /// Seed a local-dev principal and write credentials.toml for zero-config SDK use.
    Bootstrap(bootstrap::BootstrapArgs),
}

pub async fn dispatch(command: DevCommand) -> Result<ExitCode, WyrdCliError> {
    match command {
        DevCommand::Bootstrap(args) => bootstrap::dispatch(args).await,
    }
}
