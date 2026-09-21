use clap::{Parser, Subcommand};
use secrecy::ExposeSecret;
use wyrd_sql::OperatorPool;

use wyrd_server::app::{BootExit, run};
use wyrd_server::boot::init::{initialize_platform_root, issue_platform_root_credential};
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
    /// Issue a replacement credential for this deployment's existing root.
    ///
    /// The recovery of last resort, for a deployment that has lost every
    /// platform credential. Requires this machine's database access, creates no
    /// identity, and leaves the root's other credentials alone. Prints the new
    /// credential to this terminal and nowhere else.
    RecoverRoot,
}

/// Run the Wyrd server binary.
///
/// The CLI is parsed before any telemetry or serve initialization so the
/// default serve path stays exactly what it was before the administrative
/// subcommands existed; each subcommand owns its own exit code.
///
/// # Panics
/// Panics when argument parsing fails, which `clap` reports to the terminal
/// before exiting.
#[tokio::main]
async fn main() {
    // Parse the CLI before any telemetry/serve init so the serve path on the
    // `None` arm stays byte-for-byte today's `run()`.
    let cli = Cli::parse();
    let result = match cli.command {
        None => run(cli.mode).await,
        Some(Command::Init) => init().await,
        Some(Command::RecoverRoot) => recover_root().await,
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
/// surface. It deliberately does not load the serving configuration either: the
/// DSNs come from the environment, and validating query-engine settings here
/// would mean an operator could not establish the administrative root until the
/// whole serving surface was already configured.
///
/// The plaintext is written once to this process's stdout, which is the
/// operator's terminal rather than the server's log pipeline. Initialization
/// owns that write: it happens inside the transaction, so a terminal that
/// cannot take the secret leaves the deployment uninitialized rather than
/// durable and unadministrable.
///
/// # Errors
/// Returns [`BootExit::Config`] when the platform-admin DSN is missing or the
/// operator pool cannot be opened, and [`BootExit::Other`] when establishing
/// the root fails — including a deployment that is already initialized,
/// credential hashing, a stdout write or flush failure, and any Postgres write
/// or commit failure.
async fn init() -> Result<(), BootExit> {
    let pool = operator_pool().await?;
    initialize_platform_root(&pool, &mut std::io::stdout().lock())
        .await
        .map_err(|e| BootExit::Other(Box::new(e)))?;
    Ok(())
}

/// Issue a replacement credential for the existing administrative root.
///
/// Shares initialization's shape for the same reasons: Postgres handles only, no
/// serving configuration, and the plaintext printed once to this terminal. It
/// differs in creating nothing — the root must already exist.
///
/// # Errors
/// Returns [`BootExit::Config`] when the platform-admin DSN is missing or the
/// operator pool cannot be opened, and [`BootExit::Other`] when issuing the
/// replacement fails — including a deployment whose root has never been
/// initialized, credential hashing, and any Postgres write or commit failure.
async fn recover_root() -> Result<(), BootExit> {
    let pool = operator_pool().await?;
    let credential = issue_platform_root_credential(&pool)
        .await
        .map_err(|e| BootExit::Other(Box::new(e)))?;

    println!("Replacement platform administrative credential:");
    println!("{}", credential.expose_secret());
    println!("Store this credential securely. It cannot be retrieved again.");
    println!("The root's other credentials are untouched; retire them if they are lost.");
    Ok(())
}

/// Open the deployment's cross-tenant platform boundary from the environment.
///
/// The one thing both operator subcommands need and neither may take from a
/// serving surface: the DSNs come from the environment, so an operator with this
/// machine's database access can run either without the server running.
///
/// # Errors
/// Returns [`BootExit::Config`] when the platform-admin DSN is not configured
/// and [`BootExit::Other`] when the connection cannot be established.
async fn operator_pool() -> Result<OperatorPool, BootExit> {
    let boot = PostgresBoot::from_env()
        .await
        .map_err(|e| BootExit::Other(Box::new(e)))?;
    let dsns = boot.dsns().map_err(|e| BootExit::Other(Box::new(e)))?;
    let postgres = WyrdPostgres::connect_from_dsns(&dsns)
        .await
        .map_err(|e| BootExit::Other(Box::new(e)))?;
    postgres.operator_pool().ok_or_else(|| {
        BootExit::Config(Box::new(std::io::Error::other(
            "platform control plane is not configured; set the platform-admin DSN",
        )))
    })
}
