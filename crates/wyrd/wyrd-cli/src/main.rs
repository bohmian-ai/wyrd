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
use crate::error::WyrdCliError;

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

async fn dispatch(cli: Cli) -> Result<std::process::ExitCode, WyrdCliError> {
    match cli.command {
        Command::Auth(command) => crate::auth::dispatch(command).await,
        Command::Dev(command) => crate::dev::dispatch(command).await,
        Command::Eval(command) => crate::eval::run::dispatch(command).await,
        Command::Principal(command) => crate::principal::dispatch(command).await,
        Command::Query(command) => crate::query::dispatch(command).await,
    }
}
