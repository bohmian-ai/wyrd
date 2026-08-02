//! `wyrd` command-line entry point.

#![deny(missing_docs)]

mod auth;
mod cli;
mod dev;
mod error;
mod eval;
mod principal;
mod query;

use clap::Parser;

use crate::cli::{Cli, Command};
use crate::error::CliBoundaryError;

/// Initialize process-wide infrastructure and run one parsed CLI command.
///
/// TLS-provider conflicts and command errors are rendered to standard error and
/// returned as a failing process exit code.
#[tokio::main]
async fn main() -> std::process::ExitCode {
    if let Err(error) = wyrd_tls::install_crypto_provider() {
        eprintln!("Wyrd TLS initialization failed: {error}");
        return std::process::ExitCode::FAILURE;
    }
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    match dispatch(cli).await {
        Ok(code) => code,
        Err(error) => {
            crate::eval::output::print_cli_error(&error);
            std::process::ExitCode::from(error.exit_code())
        }
    }
}

/// Dispatches one parsed command while preserving its owning error boundary.
///
/// Query commands retain typed Vala SDK errors; every other command retains
/// the established derive-catalogued local CLI error.
///
/// # Errors
///
/// Returns the exact query or local boundary error when command execution,
/// transport, validation, persistence, or output fails.
///
/// # Cancellation
///
/// Cancelling dispatch drops the active command future. Individual command
/// owners remain responsible for their documented durable cleanup semantics.
async fn dispatch(cli: Cli) -> Result<std::process::ExitCode, CliBoundaryError> {
    match cli.command {
        Command::Auth(command) => crate::auth::dispatch(command).await.map_err(Into::into),
        Command::Dev(command) => crate::dev::dispatch(command).await.map_err(Into::into),
        Command::Eval(command) => crate::eval::run::dispatch(command)
            .await
            .map_err(Into::into),
        Command::Principal(command) => crate::principal::dispatch(command)
            .await
            .map_err(Into::into),
        Command::Query(command) => crate::query::dispatch(command).await,
    }
}
