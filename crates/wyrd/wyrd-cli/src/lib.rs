//! Library surface of the `wyrd` CLI.
//!
//! The CLI ships as a binary (`src/main.rs`) that renders the command
//! functions this library owns. [`commands`] exposes those functions in
//! process, returning the typed value the binary prints with `--format json`
//! and a shared-catalog error instead of an exit code; every SDK projects
//! them. The remaining modules let integration tests drive the real
//! argument-parsing, request-building, and response-handling code paths.

/// `wyrd auth` tenant identity commands: API keys, OIDC login, refresh, trusted issuers, and workload bindings.
pub mod auth;
mod card;
mod cli;
mod client;
pub mod error;
mod eval;
pub mod gateway;
pub mod load;
mod operator_connection;
mod platform;
mod principal;
pub mod query;
pub mod registration;
mod workflow;

use clap::{Parser, error::ErrorKind};

pub use cli::{Cli, Command};

/// In-process CLI commands: the same implementation the `wyrd` executable
/// renders, returning the typed value it prints with `--format json` and a
/// [`wyrd_spec::error::WyrdError`] instead of an exit code.
///
/// Networked commands take an optional [`wyrd_client::WyrdClient`] and run
/// as its principal; omitted, they resolve the server and credential from the
/// ambient chain (`WYRD_SERVER_URL`, then `WYRD_ACCESS_TOKEN`, workload
/// identity, `WYRD_API_KEY`, or `credentials.toml`) exactly as the executable
/// does. No credential is ever a string argument.
pub mod commands {
    pub use crate::auth::issue_key::issue_key;
    pub use crate::card::{LoadOutput, PlanCard, PlanReport, SelectorArgs, apply, get, load, plan};
    pub use crate::gateway::{
        delete_provider_credential, put_provider_credential, revoke_provider_credential,
    };
}

// Re-export the public loader API
pub use load::{
    AuthoredCard, CardSubmission, Diagnostic, LoadError, LoadedCard, LoadedTree, RegistrationInput,
    Severity, SourceSpan, build_registration_input, build_submissions, emit_json, load,
};

/// Parse and run the CLI using the same dispatcher and error formatter as the
/// standalone `wyrd` binary.
pub async fn run_cli<I, T>(args: I) -> std::process::ExitCode
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    std::process::ExitCode::from(run_cli_code(args).await)
}

/// Parse and run the CLI, returning its numeric process exit code for language
/// bindings and other embedding surfaces.
pub async fn run_cli_code<I, T>(args: I) -> u8
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(error) => {
            let success = matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            );
            let _ = error.print();
            return if success { 0 } else { 64 };
        }
    };
    match cli.dispatch().await {
        Ok(code) => code_to_u8(code),
        Err(error) => {
            eval::output::print_cli_error(&error);
            error.exit_code()
        }
    }
}

/// Convert the CLI's supported process codes into the numeric embedding form.
fn code_to_u8(code: std::process::ExitCode) -> u8 {
    if code == std::process::ExitCode::SUCCESS {
        0
    } else if code == std::process::ExitCode::from(2) {
        2
    } else if code == std::process::ExitCode::from(64) {
        64
    } else if code == std::process::ExitCode::from(workflow::INTERRUPTED) {
        workflow::INTERRUPTED
    } else {
        1
    }
}

/// Exit-code contract tests for the shared native and Python CLI entrypoint.
#[cfg(test)]
mod tests {
    use super::{code_to_u8, run_cli_code};

    /// Preserve the process codes explicitly used by shared CLI dispatch.
    #[test]
    fn numeric_exit_conversion_preserves_supported_codes() {
        assert_eq!(code_to_u8(std::process::ExitCode::SUCCESS), 0);
        assert_eq!(code_to_u8(std::process::ExitCode::FAILURE), 1);
        assert_eq!(code_to_u8(std::process::ExitCode::from(2)), 2);
        assert_eq!(code_to_u8(std::process::ExitCode::from(64)), 64);
        assert_eq!(code_to_u8(std::process::ExitCode::from(130)), 130);
    }

    /// Treat Clap's non-executing help path as success.
    #[tokio::test]
    async fn top_level_help_returns_success() {
        assert_eq!(run_cli_code(["wyrd", "--help"]).await, 0);
    }

    /// Keep removed development commands on the standard usage-error path.
    #[tokio::test]
    async fn removed_dev_bootstrap_returns_usage_error() {
        assert_eq!(run_cli_code(["wyrd", "dev", "bootstrap"]).await, 64);
    }
}
