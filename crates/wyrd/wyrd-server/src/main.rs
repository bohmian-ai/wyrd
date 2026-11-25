use std::alloc::{GlobalAlloc, Layout};
use std::io::Write;

use clap::{Args, Parser, Subcommand};
use mimalloc::MiMalloc;
use secrecy::{ExposeSecret, SecretString};
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::HttpConfig;
use wyrd_client::{Platform, WyrdClient};
use wyrd_server::components::platform::provisioning::LocalAdmin;
use wyrd_spec::auth::{CreateTenantRequest, SecretBearer, TenantHumanAdmin};
use wyrd_sql::OperatorPool;
use wyrd_sql::queries::platform::tenants::list_tenants;
use zeroize::Zeroize as _;

use wyrd_server::ServerBootError;
use wyrd_server::app::{BootExit, run};
use wyrd_server::boot::init::{
    InitError, initialize_platform_root, issue_platform_root_credential,
};
use wyrd_server::config::ServeMode;
use wyrd_server::verification::hosted::worker;
use wyrd_sql::dsn::ResolvedDsns;
use wyrd_sql::pool::build_pool;
use wyrd_sql::{MIGRATION_LEASE_WAIT, PoolConfig, SqlError, WyrdPostgres};

/// Process-wide allocator for the server binary and its Monty workers.
///
/// mimalloc serves every allocation; a worker process also counts its heap
/// against its session's memory budget (see [`worker`]). Only this binary
/// references mimalloc: a library that did would link mimalloc, and its
/// load-time initialization, into every embedder such as the Python SDK.
#[global_allocator]
static GLOBAL: CountingMiMalloc = CountingMiMalloc;

/// mimalloc, plus the worker's live-byte count that enforces Monty's memory
/// limits and a null check that ends a worker deliberately. A `bench` build
/// also reports each allocation and free to the heap sampler, which ignores
/// them until a benchmark starts it.
///
/// Counting starts in `main`, after the runtime's few pre-`main`
/// allocations, so a free of one of those saturates the count at zero rather
/// than wrapping; the count can undercount by at most those bytes.
struct CountingMiMalloc;

// SAFETY: every method forwards its arguments unchanged to `MiMalloc` and
// returns what `MiMalloc` returned (or diverges). No pointer is fabricated,
// aliased or freed here, so this upholds exactly the invariants `MiMalloc`
// upholds.
#[expect(
    unsafe_code,
    reason = "a global allocator is unsafe; the counting it adds is safe"
)]
unsafe impl GlobalAlloc for CountingMiMalloc {
    /// Charge `layout` to a worker's budget, then allocate it from mimalloc.
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        worker::charge(layout.size());
        // SAFETY: `layout` comes from the caller and is forwarded unchanged.
        let ptr = unsafe { MiMalloc.alloc(layout) };
        if ptr.is_null() {
            worker::refused(layout.size());
        }
        #[cfg(feature = "bench")]
        wyrd_benchmark::heap::allocated(ptr, layout.size());
        ptr
    }

    /// Refund `layout` to a worker's budget, then free it through mimalloc.
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        worker::refund(layout.size());
        #[cfg(feature = "bench")]
        wyrd_benchmark::heap::freed(ptr);
        // SAFETY: `ptr` came from our `alloc`/`realloc` with this same `layout`.
        unsafe { MiMalloc.dealloc(ptr, layout) };
    }

    /// Charge `layout`, then take mimalloc's pre-zeroed allocation.
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        worker::charge(layout.size());
        // SAFETY: `layout` comes from the caller and is forwarded unchanged.
        let ptr = unsafe { MiMalloc.alloc_zeroed(layout) };
        if ptr.is_null() {
            worker::refused(layout.size());
        }
        #[cfg(feature = "bench")]
        wyrd_benchmark::heap::allocated(ptr, layout.size());
        ptr
    }

    /// Charge or refund the size change, then let mimalloc resize in place
    /// when it can.
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        match new_size.checked_sub(layout.size()) {
            Some(growth) => worker::charge(growth),
            None => worker::refund(layout.size() - new_size),
        }
        #[cfg(feature = "bench")]
        wyrd_benchmark::heap::freed(ptr);
        // SAFETY: `ptr`/`layout` describe a live block from this allocator, and
        // `new_size` is the caller's — all forwarded unchanged.
        let new_ptr = unsafe { MiMalloc.realloc(ptr, layout, new_size) };
        if new_ptr.is_null() {
            worker::refused(new_size);
        }
        #[cfg(feature = "bench")]
        wyrd_benchmark::heap::allocated(new_ptr, new_size);
        new_ptr
    }
}

/// Bytes mimalloc has committed for this process: its aggregate commitment
/// statistic, which the release allocator keeps without detailed statistics.
///
/// The server samples it for process pressure and a worker reports it in its
/// telemetry; it overlaps the server's ledger and is never added to it.
fn committed_bytes() -> u64 {
    let mut fields = [0_usize; 8];
    let [
        elapsed,
        user,
        system,
        rss,
        peak_rss,
        commit,
        peak_commit,
        faults,
    ] = &mut fields;
    // SAFETY: every argument points at a distinct live `usize`, which is all
    // `mi_process_info` writes through.
    #[expect(unsafe_code, reason = "mimalloc's statistics are a C API")]
    unsafe {
        libmimalloc_sys::mi_process_info(
            elapsed,
            user,
            system,
            rss,
            peak_rss,
            commit,
            peak_commit,
            faults,
        );
    }
    u64::try_from(fields[5]).unwrap_or(u64::MAX)
}

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
    /// Run once per release before serving, as the platform login
    /// (`WYRD_PLATFORM_DATABASE_URL`, else `WYRD_DATABASE_URL`), which then
    /// owns Wyrd's objects. Serving processes never run DDL and refuse to
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
    /// Username of the tenant's first human administrator, who signs in with
    /// a password and needs neither SMTP nor an identity provider.
    #[arg(long)]
    admin_username: String,
    /// Read the administrator password from the first line of stdin instead
    /// of a hidden terminal prompt.
    #[arg(long)]
    admin_password_stdin: bool,
    /// Optional profile email for the administrator. It stays unverified.
    #[arg(long)]
    admin_email: Option<String>,
}

/// Run the binary as a Monty pool worker when the pool spawned it with
/// [`worker::SUBPROCESS_ARG`], and as the Wyrd server otherwise.
///
/// The worker check runs before any runtime, CLI, or telemetry exists, so a
/// worker process holds only the interpreter thread and its own heap count.
fn main() -> std::process::ExitCode {
    if let Some(code) = worker::dispatch(committed_bytes) {
        return code;
    }
    server_main();
    std::process::ExitCode::SUCCESS
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
async fn server_main() {
    // Parse the CLI before any telemetry/serve init so the serve path on the
    // `None` arm stays byte-for-byte today's `run()`.
    let cli = Cli::parse();
    let result = match cli.command {
        None => Box::pin(run(cli.mode, committed_bytes)).await,
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

/// Apply every embedded Wyrd then Vala migration as the platform login.
///
/// Builds one short-lived [`OperatorPool`] on the platform operator DSN
/// ([`ResolvedDsns::platform`]), so the platform login owns every object the
/// migrations create, and holds its database-wide
/// [`wyrd_sql::MigrationLease`] across the whole sequence: `wyrd_sql::migrate` (which
/// creates the `platform` and `wyrd` schemas Vala depends on), then
/// `vala_sql::migrate`, then both owners' schema contracts — the same checks
/// serving boot applies. A competing migrator waits at most
/// [`MIGRATION_LEASE_WAIT`] and then fails, so two release slots never
/// interleave. Migrations are idempotent, so rerunning after a partial failure
/// resumes safely.
///
/// # Errors
/// Returns [`BootExit::Config`] when the database URLs are missing or invalid, and
/// [`BootExit::Other`] when the pool cannot connect, the lease stays held past
/// its bound, a migration fails, or the post-migration validation does not
/// pass.
async fn migrate() -> Result<(), BootExit> {
    let dsns = ResolvedDsns::from_env().map_err(|e| BootExit::Config(Box::new(e)))?;
    let owner = build_pool(
        dsns.platform().expose_secret(),
        PoolConfig::migrator_from_env(),
    )
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
/// The tenant's named human administrator is read and validated before the
/// root or the tenant is touched, so refused input claims nothing. Its
/// password comes from stdin or a hidden prompt and is never printed; a
/// resumed setup keeps the administrator an interrupted one created.
///
/// # Errors
/// Returns [`BootExit::Config`] when the DSNs are missing, the slug is not a
/// valid tenant slug, the administrator password cannot be read or the
/// administrator input is refused, or the root exists and
/// `WYRD_PLATFORM_CREDENTIAL` is unset. Returns [`BootExit::Other`] when the directory read, root
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

    let human_admin = TenantHumanAdmin {
        username: args.admin_username,
        password: SecretBearer::new(
            admin_password(args.admin_password_stdin)
                .map_err(|e| BootExit::Config(Box::new(e)))?
                .expose_secret()
                .to_owned(),
        ),
        email: args.admin_email,
    };
    LocalAdmin::check(&human_admin).map_err(|e| BootExit::Config(Box::new(e)))?;

    let credential = root_credential(&pool).await?;
    let client = WyrdClient::with_config(ClientConfig {
        http: HttpConfig {
            base_url: args.server.trim_end_matches('/').to_owned(),
            ..HttpConfig::default()
        },
        credential: Some(credential),
        ..ClientConfig::default()
    })
    .map_err(|e| BootExit::Other(Box::new(e)))?;
    let created = Platform::connect_with(&client)
        .await
        .map_err(|e| BootExit::Other(Box::new(e)))?
        .create_tenant(&CreateTenantRequest {
            slug,
            display_name: args.display_name.unwrap_or_else(|| args.tenant.clone()),
            human_admin: Some(human_admin),
        })
        .await
        .map_err(|e| BootExit::Other(Box::new(e)))?;
    println!("tenant_id: {}", created.tenant.id);
    println!("slug: {}", created.tenant.slug);
    println!("admin_principal_id: {}", created.admin.principal_id);
    println!("admin_credential: {}", created.admin.credential.expose());
    println!("This is the only time the tenant credential is shown. Store it now.");
    if let Some(human) = created.human_admin {
        println!("admin_username: {}", human.username);
        println!("login_url: {}", human.login_url);
    }
    Ok(())
}

/// The administrator password: the first line of stdin when `from_stdin`,
/// else typed at a terminal prompt with echo off.
///
/// # Errors
/// Returns an IO error when stdin cannot be read, when it is not a terminal
/// and `from_stdin` is unset, or when the terminal's echo cannot be changed.
fn admin_password(from_stdin: bool) -> std::io::Result<SecretString> {
    use rustix::termios::{LocalModes, OptionalActions, tcgetattr, tcsetattr};
    use std::io::{BufRead as _, IsTerminal as _};

    let stdin = std::io::stdin();
    let mut line = String::new();
    if from_stdin {
        stdin.lock().read_line(&mut line)?;
    } else {
        if !stdin.is_terminal() {
            return Err(std::io::Error::other(
                "stdin is not a terminal to prompt for the administrator password; \
                 pass --admin-password-stdin and write the password to stdin",
            ));
        }
        eprint!("Administrator password: ");
        let shown = tcgetattr(&stdin)?;
        let mut hidden = shown.clone();
        hidden.local_modes.remove(LocalModes::ECHO);
        tcsetattr(&stdin, OptionalActions::Now, &hidden)?;
        let read = stdin.lock().read_line(&mut line);
        let restored = tcsetattr(&stdin, OptionalActions::Now, &shown);
        eprintln!();
        read?;
        restored?;
    }
    let password = SecretString::from(line.trim_end_matches(['\r', '\n']).to_owned());
    line.zeroize();
    Ok(password)
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
/// Returns [`BootExit::Config`] when `WYRD_DATABASE_URL` is missing or a
/// serving DSN is invalid,
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

#[cfg(test)]
mod tests {
    use super::committed_bytes;

    /// The pressure sampler reads the allocator's live commitment, which
    /// always covers a touched block the process still holds.
    ///
    /// # Panics
    /// Panics when the reading is below the held 64 MiB block.
    #[test]
    fn committed_bytes_follow_touched_allocations() {
        const BLOCK: usize = 64 << 20;
        let block = std::hint::black_box(vec![1_u8; BLOCK]);
        let committed = committed_bytes();
        assert!(
            committed >= BLOCK as u64,
            "a held, touched {BLOCK}-byte block is committed: {committed}"
        );
        drop(block);
    }
}
