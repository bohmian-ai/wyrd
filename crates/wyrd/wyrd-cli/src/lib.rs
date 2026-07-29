//! Library surface of the `wyrd` CLI for in-process test drivers.
//!
//! The CLI ships as a binary (`src/main.rs`); this thin facade re-exports the
//! command modules the binary already owns so integration tests can drive the
//! real argument-parsing, request-building, and response-handling code paths
//! in process — exactly what an operator runs — instead of re-implementing the
//! admin wire calls.

pub mod audit;
pub mod auth;
mod card;
mod cli;
pub mod error;
mod eval;
pub mod load;
mod principal;
#[cfg(feature = "python")]
pub mod python;
pub mod registration;

use clap::Parser;

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
            let _ = error.print();
            return 64;
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

fn code_to_u8(code: std::process::ExitCode) -> u8 {
    if code == std::process::ExitCode::SUCCESS {
        0
    } else {
        1
    }
}
