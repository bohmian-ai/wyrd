use clap::{Parser, Subcommand};
use secrecy::ExposeSecret;

use wyrd_server::WyrdServerConfig;
use wyrd_server::app::{BootExit, run};
use wyrd_server::boot::init::initialize_platform_root;
use wyrd_server::config::ServeMode;
use wyrd_sql::WyrdPostgres;
use wyrd_sql::postgres_boot::PostgresBoot;

const EX_CONFIG: i32 = 78;
const EX_SOFTWARE: i32 = 70;

/// Wyrd control-plane server.
#[derive(Debug, Parser)]
#[command(name = "wyrd-server", version, about = "Wyrd control-plane server")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    /// Transport to serve (overrides config `serve.mode` when set).
    #[arg(long, value_enum, global = true)]
    mode: Option<ServeMode>,
}

/// Operator subcommands. The absent arm runs the server.
#[derive(Debug, Subcommand)]
enum Command {
    /// Establish this deployment's platform administrative root.
    ///
    /// Run once per deployment. Prints the root credential to this terminal
    /// and nowhere else; it cannot be retrieved afterwards.
    Init,
}

#[tokio::main]
async fn main() {
    // Parse the CLI before any telemetry/serve init so the serve path on the
    // `None` arm stays byte-for-byte today's `run()`.
    let cli = Cli::parse();
    let result = match cli.command {
        None => run(cli.mode).await,
        Some(Command::Init) => init().await,
    };

    let exit_code = match result {
        Ok(()) => 0,
        Err(BootExit::Config(err)) => {
            eprintln!("wyrd-server: config error: {err}");
            EX_CONFIG
        }
        Err(BootExit::Other(err)) => {
            eprintln!("wyrd-server: fatal error: {err}");
            EX_SOFTWARE
        }
    };
    std::process::exit(exit_code);
}

/// Establish the deployment's administrative root and print its credential.
///
/// Builds only the Postgres handles — never telemetry, storage, listeners, or
/// `AppState` — so initialization neither starts nor depends on a serving
/// surface. The plaintext is printed once to this process's stdout, which is
/// the operator's terminal rather than the server's log pipeline.
async fn init() -> Result<(), BootExit> {
    let _config = WyrdServerConfig::load().map_err(|e| BootExit::Config(Box::new(e)))?;

    let boot = PostgresBoot::from_env()
        .await
        .map_err(|e| BootExit::Other(Box::new(e)))?;
    let dsns = boot.dsns().map_err(|e| BootExit::Other(Box::new(e)))?;
    let postgres = WyrdPostgres::connect_from_dsns(&dsns)
        .await
        .map_err(|e| BootExit::Other(Box::new(e)))?;
    let Some(pool) = postgres.operator_pool() else {
        return Err(BootExit::Config(Box::new(std::io::Error::other(
            "platform control plane is not configured; set the platform-admin DSN",
        ))));
    };

    let credential = initialize_platform_root(&pool)
        .await
        .map_err(|e| BootExit::Other(Box::new(e)))?;

    println!("Wyrd initialization complete.");
    println!("Platform administrative credential:");
    println!("{}", credential.expose_secret());
    println!("Store this credential securely. It cannot be retrieved again.");
    Ok(())
}
