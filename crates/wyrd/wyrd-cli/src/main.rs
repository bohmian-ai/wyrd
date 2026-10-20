//! `wyrd` command-line entry point.

/// Run the `wyrd` executable.
///
/// Installs the process TLS crypto provider, then the tracing subscriber
/// through [`wyrd_cli::install_tracing`]: human mode logs to stderr, while
/// `--format json` keeps stdout and stderr for its one result or error
/// document and appends any `RUST_LOG` selection to the CLI diagnostic log.
/// Every command then runs through the shared [`wyrd_cli::run_cli`]
/// dispatcher, whose code becomes the process exit status.
///
/// A TLS failure prints one line on stderr and exits 1 before any command
/// runs. A JSON-mode diagnostic log that cannot be opened prints its error
/// document and exits with that error's code; no command runs either.
#[tokio::main]
async fn main() -> std::process::ExitCode {
    if let Err(error) = wyrd_tls::install_crypto_provider() {
        eprintln!("Wyrd TLS initialization failed: {error}");
        return std::process::ExitCode::FAILURE;
    }
    let json = wyrd_cli::json_requested(std::env::args_os());
    if let Err(error) = wyrd_cli::install_tracing(json) {
        let error = wyrd_cli::error::CliBoundaryError::from(error);
        error.print(json);
        return std::process::ExitCode::from(error.exit_code());
    }

    wyrd_cli::run_cli(std::env::args_os()).await
}
