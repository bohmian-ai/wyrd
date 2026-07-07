use clap::{Parser, Subcommand};

use wyrd_server::WyrdServerConfig;
use wyrd_server::app::{BootExit, run};
use wyrd_server::boot::bootstrap::bootstrap_admin_key;
use wyrd_server::config::ServeMode;
use wyrd_spec::TenantSlug;
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
    /// Mint the first admin API key for a fresh deployment.
    BootstrapKey {
        /// Tenant slug to bootstrap.
        #[arg(long)]
        tenant: String,
    },
}

#[tokio::main]
async fn main() {
    // Parse the CLI before any telemetry/serve init so the serve path on the
    // `None` arm stays byte-for-byte today's `run()`.
    let cli = Cli::parse();
    let result = match cli.command {
        None => run(cli.mode).await,
        Some(Command::BootstrapKey { tenant }) => bootstrap_key(&tenant).await,
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

/// Mint and print the first admin API key for `tenant`.
///
/// Builds only the runtime `wyrd_app` pool — never telemetry, storage,
/// listeners, or `AppState` — runs the issuance chain in one transaction, then
/// prints the plaintext key once to stdout.
async fn bootstrap_key(tenant: &str) -> Result<(), BootExit> {
    let _config = WyrdServerConfig::load().map_err(|e| BootExit::Config(Box::new(e)))?;
    let slug = TenantSlug::new(tenant).map_err(|e| BootExit::Config(Box::new(e)))?;

    let boot = PostgresBoot::from_env()
        .await
        .map_err(|e| BootExit::Other(Box::new(e)))?;
    let dsns = boot.dsns().map_err(|e| BootExit::Other(Box::new(e)))?;
    let postgres = WyrdPostgres::connect_from_dsns(&dsns)
        .await
        .map_err(|e| BootExit::Other(Box::new(e)))?;

    let key = bootstrap_admin_key(postgres.app_pool(), &slug)
        .await
        .map_err(|e| BootExit::Other(Box::new(e)))?;

    println!("{}", key.expose());
    Ok(())
}
