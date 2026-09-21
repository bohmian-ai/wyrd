//! Library surface of the `wyrd` CLI for in-process test drivers.
//!
//! The CLI ships as a binary (`src/main.rs`); this thin facade re-exports the
//! command modules the binary already owns so integration tests can drive the
//! real argument-parsing, request-building, and response-handling code paths
//! in process — exactly what an operator runs — instead of re-implementing the
//! admin wire calls.

/// `wyrd auth` tenant identity commands: API keys, OIDC login, refresh, trusted issuers, and workload bindings.
pub mod auth;
mod card;
mod cli;
mod client;
pub mod error;
mod eval;
pub mod load;
mod platform;
mod principal;
#[cfg(feature = "python")]
/// Optional PyO3 adapter for the shared numeric CLI entrypoint.
pub mod python;
pub mod query;
pub mod registration;

use clap::{Parser, error::ErrorKind};

pub use cli::{Cli, Command};

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
