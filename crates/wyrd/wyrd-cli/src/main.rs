//! `wyrd` command-line entry point.

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

    wyrd_cli::run_cli(std::env::args_os()).await
}
