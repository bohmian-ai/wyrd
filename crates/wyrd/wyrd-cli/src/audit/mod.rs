//! Audit CLI commands.

pub mod verify;

use std::process::ExitCode;

use clap::Subcommand;

use crate::error::WyrdCliError;

/// Audit subcommands.
#[derive(Debug, Subcommand)]
pub enum AuditCommand {
    /// Verify all audit seal checkpoints for the token's own tenant (POST /v1/admin/audit/verify).
    Verify(verify::AuditVerifyArgs),
}

/// Dispatch an audit subcommand.
pub async fn dispatch(command: AuditCommand) -> Result<ExitCode, WyrdCliError> {
    match command {
        AuditCommand::Verify(args) => verify::dispatch(args).await,
    }
}
