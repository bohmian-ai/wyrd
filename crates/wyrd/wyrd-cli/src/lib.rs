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
pub mod declaration;
pub mod error;
pub mod gateway;
/// `wyrd init`: local server bootstrap and its guided setup handoff.
pub mod init;
pub mod load;
/// `wyrd mcp`: MCP host installation and the host-facing stdio proxy.
pub mod mcp;
mod operator_connection;
mod platform;
mod principal;
pub mod query;
/// `wyrd server`: verified installation of official server releases.
pub mod server;
#[cfg(unix)]
mod terminal;

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
    pub use crate::card::{
        CheckCard, CheckReport, SelectorArgs, declare, declare_check, get, pull,
    };
    pub use crate::gateway::{
        delete_provider_credential, put_provider_credential, revoke_provider_credential,
    };
    pub use crate::mcp::install as mcp_install;
    pub use crate::mcp::{McpHost, McpHostReport, McpHostStatus, McpInstallReport};
}

// Re-export the public loader API
pub use load::{
    AuthoredCard, CardSubmission, DeclarationInput, Diagnostic, LoadError, LoadedCard, LoadedTree,
    Severity, SourceSpan, build_declaration_input, build_submissions, emit_json, load,
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

/// Whether `args` request machine output with `--format json`.
///
/// Read from the raw arguments, before `--`, so a usage failure that never
/// parses still learns the requested mode. In JSON mode stdout carries only
/// the command's one result and stderr only its one error document.
pub fn json_requested<I, T>(args: I) -> bool
where
    I: IntoIterator<Item = T>,
    T: AsRef<std::ffi::OsStr>,
{
    let mut previous_format = false;
    for arg in args {
        let arg = arg.as_ref();
        if arg == "--" {
            return false;
        }
        if (previous_format && arg == "json") || arg == "--format=json" {
            return true;
        }
        previous_format = arg == "--format";
    }
    false
}

/// Install the `wyrd` executable's tracing subscriber.
///
/// Human mode logs to stderr at `warn` unless `RUST_LOG` selects otherwise.
/// JSON mode keeps stdout and stderr for its one result or error document:
/// without a valid `RUST_LOG` it logs nothing, and a `RUST_LOG` selection is
/// appended without color to `<wyrd config dir>/logs/cli.log`, so requested
/// diagnostics are retained outside both machine streams. Only the binary
/// calls this; in-process embedders own their own subscriber.
///
/// # Errors
/// Returns [`error::WyrdCliError::Io`] when JSON mode's requested log cannot
/// be resolved or opened. No command has run.
pub fn install_tracing(json: bool) -> Result<(), error::WyrdCliError> {
    use tracing_subscriber::EnvFilter;

    let requested = EnvFilter::try_from_default_env();
    if !json {
        tracing_subscriber::fmt()
            .with_env_filter(requested.unwrap_or_else(|_| EnvFilter::new("warn")))
            .with_writer(std::io::stderr)
            .init();
        return Ok(());
    }
    let Ok(filter) = requested else {
        return Ok(());
    };
    let path = wyrd_client::environment::Environment::Process
        .config_dir()
        .ok_or_else(|| {
            std::io::Error::other(
                "cannot resolve the Wyrd config directory for the RUST_LOG diagnostic log: set WYRD_CONFIG_HOME, XDG_CONFIG_HOME, or HOME, or unset RUST_LOG",
            )
        })
        .map(|dir| dir.join("logs").join("cli.log"));
    let file = path
        .and_then(|path| open_diagnostic_log(&path))
        .map_err(|source| error::WyrdCliError::Io { source })?;
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(false)
        .with_writer(std::sync::Mutex::new(file))
        .init();
    Ok(())
}

/// Open `path` for appending diagnostics, creating it and its directory.
///
/// Shared by JSON mode's CLI tracing log and `wyrd init`'s diagnostic log, so
/// both keep machine streams clean the same way.
///
/// # Errors
/// Propagates directory creation and open failures.
pub(crate) fn open_diagnostic_log(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
}

/// Parse and run the CLI, returning its numeric process exit code for language
/// bindings and other embedding surfaces.
///
/// With `--format json`, a parse failure is the one
/// [`error::WyrdCliError::Usage`] document on stderr, like every other
/// failure; help and version output are unchanged.
pub async fn run_cli_code<I, T>(args: I) -> u8
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let args: Vec<std::ffi::OsString> = args.into_iter().map(Into::into).collect();
    let json = json_requested(&args);
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(error) => {
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) {
                let _ = error.print();
                return 0;
            }
            if json {
                error::CliBoundaryError::from(error::WyrdCliError::usage(&error)).print(true);
            } else {
                let _ = error.print();
            }
            return 64;
        }
    };
    match cli.dispatch().await {
        Ok(code) => code_to_u8(code),
        Err(error) => {
            error.print(json);
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
    } else if code == std::process::ExitCode::from(130) {
        130
    } else {
        1
    }
}

/// Exit-code contract tests for the shared native and Python CLI entrypoint.
#[cfg(test)]
mod tests {
    use super::{code_to_u8, json_requested, run_cli_code};

    /// Only an exact `--format json` before `--` selects machine output.
    ///
    /// Covers both spellings, a different format value, a lookalike value,
    /// and a `--format json` after `--`, which belongs to a child command.
    ///
    /// # Panics
    /// Panics when any of those argument lists is classified the other way.
    #[test]
    fn json_mode_is_read_from_raw_arguments() {
        assert!(json_requested(["wyrd", "get", "--format", "json"]));
        assert!(json_requested(["wyrd", "get", "--format=json"]));
        assert!(!json_requested(["wyrd", "query", "--format", "jsonl"]));
        assert!(!json_requested(["wyrd", "get", "--format", "text"]));
        assert!(!json_requested(["wyrd", "x", "--", "--format", "json"]));
    }

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
