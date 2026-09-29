use std::io::Write;

use clap::{Args, Parser, Subcommand};
use secrecy::{ExposeSecret, SecretString};
use wyrd_client::Platform;
use wyrd_spec::auth::CreateTenantRequest;
use wyrd_sql::OperatorPool;
use wyrd_sql::queries::platform::tenants::list_tenants;

use wyrd_server::ServerBootError;
use wyrd_server::app::{BootExit, run};
use wyrd_server::boot::init::{
    InitError, initialize_platform_root, issue_platform_root_credential,
};
use wyrd_server::config::ServeMode;
use wyrd_sql::dsn::{ResolvedDsns, owner_dsn_from_env};
use wyrd_sql::pool::build_pool;
use wyrd_sql::{MIGRATION_LEASE_WAIT, PoolConfig, SqlError, WyrdPostgres};

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
    /// Apply the Wyrd and Vala schema migrations, validate them, and exit.
    ///
    /// Run once per release before serving, with `WYRD_DATABASE_URL` set to
    /// the database-owner login. Serving processes never run DDL and refuse to
    /// start until this has succeeded.
    Migrate,
    /// Make a fresh deployment usable: platform root, first tenant, and that
    /// tenant's administrative credential.
    ///
    /// Idempotent. Rerunning after the tenant is active changes nothing and
    /// discloses nothing. Each credential is printed exactly once.
    Setup(SetupArgs),
}

/// Inputs to [`Command::Setup`].
#[derive(Debug, Args)]
struct SetupArgs {
    /// URL-safe slug of the first tenant.
    #[arg(long)]
    tenant: String,
    /// Display name of the first tenant. Defaults to the slug.
    #[arg(long)]
    display_name: Option<String>,
    /// Running Wyrd server that provisions the tenant through its platform API.
    #[arg(long, env = "WYRD_SERVER_URL", default_value = "http://127.0.0.1:8080")]
    server: String,
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
        Some(Command::Migrate) => migrate().await,
        Some(Command::Setup(args)) => setup(args).await,
    };

    let exit_code = match result {
        Ok(()) => 0,
        Err(BootExit::Config(err)) => {
            eprintln!("wyrd-server: config error: {err}");
            EX_CONFIG
        }
        Err(BootExit::Other(err)) => {
            match stable_code(err.as_ref()) {
                Some(code) => eprintln!("wyrd-server: fatal error [{code}]: {err}"),
                None => eprintln!("wyrd-server: fatal error: {err}"),
            }
            EX_SOFTWARE
        }
    };
    std::process::exit(exit_code);
}

/// Returns the stable catalog code a boxed boot failure carries, if any.
///
/// Boot failures reach `main` type-erased, and `ServerBootError::Sql` is
/// transparent, so its `source()` skips the `SqlError` itself. Recovering the
/// code here is what lets an operator, or a startup script, match a schema
/// refusal on `WYRD_SQL_503_SCHEMA_NOT_READY` rather than on prose.
fn stable_code(err: &(dyn std::error::Error + Send + Sync + 'static)) -> Option<&'static str> {
    if let Some(sql) = err.downcast_ref::<SqlError>() {
        return Some(sql.code());
    }
    match err.downcast_ref::<ServerBootError>()? {
        ServerBootError::Sql(sql) => Some(sql.code()),
        _ => None,
    }
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

/// Apply every embedded Wyrd then Vala migration with the owner login.
///
/// Builds one short-lived [`OperatorPool`] on the owner DSN — the only place
/// the owner credential is used — and holds its database-wide
/// [`wyrd_sql::MigrationLease`] across the whole sequence: `wyrd_sql::migrate` (which
/// creates the `platform` and `wyrd` schemas Vala depends on), then
/// `vala_sql::migrate`, then both owners' schema contracts — the same checks
/// serving boot applies. A competing migrator waits at most
/// [`MIGRATION_LEASE_WAIT`] and then fails, so two release slots never
/// interleave. Migrations are idempotent, so rerunning after a partial failure
/// resumes safely.
///
/// # Errors
/// Returns [`BootExit::Config`] when the owner DSN is missing or invalid, and
/// [`BootExit::Other`] when the pool cannot connect, the lease stays held past
/// its bound, a migration fails, or the post-migration validation does not
/// pass.
async fn migrate() -> Result<(), BootExit> {
    let owner_dsn = owner_dsn_from_env().map_err(|e| BootExit::Config(Box::new(e)))?;
    let owner = build_pool(owner_dsn.expose_secret(), PoolConfig::migrator_from_env())
        .await
        .map(OperatorPool::from)
        .map_err(|e| BootExit::Other(Box::new(SqlError::Connect(e))))?;
    let result = async {
        let mut lease = owner.migration_lease(MIGRATION_LEASE_WAIT).await?;
        let migrated = async {
            wyrd_sql::migrate(&mut lease).await?;
            vala_sql::migrate(&mut lease).await?;
            wyrd_sql::verify_schema(&owner).await?;
            vala_sql::verify_schema(&owner).await
        }
        .await;
        let released = lease.release().await;
        migrated.and(released)
    }
    .await;
    owner.pool().close().await;
    result.map_err(|e| BootExit::Other(Box::new(e)))?;
    println!("Wyrd and Vala schemas are migrated and valid.");
    Ok(())
}

/// Environment variable carrying an existing platform credential.
const PLATFORM_CREDENTIAL_ENV: &str = "WYRD_PLATFORM_CREDENTIAL";

/// Establish the platform root if absent, then provision the first tenant.
///
/// Composes the existing owners rather than duplicating them: the root comes
/// from [`initialize_platform_root`], whose disclosure is teed to this terminal
/// inside its transaction, and the tenant is provisioned through the running
/// server's authenticated platform API, so authorization and audit are the
/// same as for any platform client. An already-active tenant with the slug
/// ends the command successfully without disclosing anything. When the root
/// already exists, the credential to act with comes from
/// `WYRD_PLATFORM_CREDENTIAL`.
///
/// # Errors
/// Returns [`BootExit::Config`] when the DSNs are missing, the slug is not a
/// valid tenant slug, or the root exists and `WYRD_PLATFORM_CREDENTIAL` is
/// unset. Returns [`BootExit::Other`] when the directory read, root
/// initialization, the platform session exchange, or tenant provisioning
/// fails. A failure leaves every committed step in place for a rerun.
async fn setup(args: SetupArgs) -> Result<(), BootExit> {
    let slug = args
        .tenant
        .parse()
        .map_err(|e| BootExit::Config(Box::new(e)))?;
    let pool = operator_pool().await?;
    let existing = list_tenants(&pool)
        .await
        .map_err(|e| BootExit::Other(Box::new(e)))?
        .into_iter()
        .find(|row| row.slug == args.tenant && row.status == "active");
    if let Some(row) = existing {
        println!(
            "Tenant `{}` ({}) is already set up; nothing to do.",
            row.slug, row.data_tenant_id
        );
        return Ok(());
    }

    let credential = root_credential(&pool).await?;
    let created = Platform::connect(&args.server, &credential)
        .await
        .map_err(|e| BootExit::Other(Box::new(e)))?
        .create_tenant(&CreateTenantRequest {
            slug,
            display_name: args.display_name.unwrap_or_else(|| args.tenant.clone()),
        })
        .await
        .map_err(|e| BootExit::Other(Box::new(e)))?;
    println!("tenant_id: {}", created.tenant.id);
    println!("slug: {}", created.tenant.slug);
    println!("admin_principal_id: {}", created.admin.principal_id);
    println!("admin_credential: {}", created.admin.credential.expose());
    println!("This is the only time the tenant credential is shown. Store it now.");
    Ok(())
}

/// Initialize the platform root, or read the operator's existing credential.
///
/// # Errors
/// Returns [`BootExit::Config`] when the root already exists and
/// `WYRD_PLATFORM_CREDENTIAL` is unset, and [`BootExit::Other`] when
/// initialization fails or disclosed no credential.
async fn root_credential(pool: &OperatorPool) -> Result<SecretString, BootExit> {
    let mut tee = Tee {
        terminal: std::io::stdout().lock(),
        captured: Vec::new(),
    };
    match initialize_platform_root(pool, &mut tee).await {
        Ok(()) => String::from_utf8_lossy(&tee.captured)
            .lines()
            .find(|line| line.starts_with("wyrd_global_"))
            .map(|line| SecretString::from(line.to_owned()))
            .ok_or_else(|| {
                BootExit::Other(Box::new(std::io::Error::other(
                    "initialization disclosed no platform credential",
                )))
            }),
        Err(InitError::AlreadyInitialized) => std::env::var(PLATFORM_CREDENTIAL_ENV)
            .ok()
            .filter(|value| !value.is_empty())
            .map(SecretString::from)
            .ok_or_else(|| {
                BootExit::Config(Box::new(std::io::Error::other(
                    "the platform root already exists; set WYRD_PLATFORM_CREDENTIAL \
                     (or run `wyrd-server recover-root`) to continue setup",
                )))
            }),
        Err(error) => Err(BootExit::Other(Box::new(error))),
    }
}

/// Writer that shows the root disclosure on the terminal and keeps a copy.
///
/// The terminal write and flush still happen inside initialization's
/// transaction, so the one-time disclosure guarantee is unchanged; the copy
/// lets setup continue with the credential it just minted.
struct Tee<W> {
    /// Operator terminal receiving the disclosure.
    terminal: W,
    /// In-memory copy read back by [`root_credential`].
    captured: Vec<u8>,
}

impl<W: Write> Write for Tee<W> {
    /// Write to the terminal first, then capture exactly the bytes it accepted.
    ///
    /// A partial terminal write captures only the accepted prefix, so the copy
    /// never holds bytes the operator did not see; `write_all` retries the rest.
    ///
    /// # Errors
    /// Returns the terminal's IO error unchanged; nothing is captured then.
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let written = self.terminal.write(buf)?;
        self.captured.extend_from_slice(&buf[..written]);
        Ok(written)
    }

    /// Flush the terminal; the in-memory copy needs no flushing.
    ///
    /// # Errors
    /// Returns the terminal's flush IO error unchanged.
    fn flush(&mut self) -> std::io::Result<()> {
        self.terminal.flush()
    }
}

/// Open the deployment's cross-tenant platform boundary from the environment.
///
/// The one thing both operator subcommands need and neither may take from a
/// serving surface: the DSNs come from the environment, so an operator with this
/// machine's database access can run either without the server running.
///
/// # Errors
/// Returns [`BootExit::Config`] when either serving DSN is missing or invalid,
/// and [`BootExit::Other`] when the connection cannot be established or the
/// schema has not been migrated by `wyrd-server migrate`.
async fn operator_pool() -> Result<OperatorPool, BootExit> {
    let dsns = ResolvedDsns::from_env().map_err(|e| BootExit::Config(Box::new(e)))?;
    let postgres = WyrdPostgres::connect_from_dsns(&dsns)
        .await
        .map_err(|e| BootExit::Other(Box::new(e)))?;
    postgres
        .validate_schema()
        .await
        .map_err(|e| BootExit::Other(Box::new(e)))?;
    postgres.operator_pool().ok_or_else(|| {
        BootExit::Config(Box::new(std::io::Error::other(
            "platform control plane is not configured; set the platform-admin DSN",
        )))
    })
}
