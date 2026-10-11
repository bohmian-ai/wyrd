//! Installed-server orchestration for local development. Durable operations
//! remain the installed server's `migrate` and `setup` commands.

use std::collections::HashMap;
use std::io::Write as _;
use std::net::SocketAddr;
use std::process::{ExitCode, Stdio};
use std::time::Duration;

use clap::Args;
use secrecy::{ExposeSecret as _, SecretString};
use tokio::process::{Child, Command};
use wyrd_client::cards::Cards;
use wyrd_client::config::ClientConfig;
use wyrd_client::environment::Environment;
use wyrd_client::saved_login::{SavedLogins, canonical_origin};
use wyrd_client::{GlobalConfig, WyrdClient};
use wyrd_spec::DataTenantId;
use wyrd_spec::ids::TenantSlug;
use wyrd_spec::registry::ListCardsRequest;

use super::install::compatible;
use super::{InstalledServer, ServerInstaller};
use crate::client::map_client_error;
use crate::error::WyrdCliError;
use crate::mcp::{self, McpHost};

/// Local inputs accepted by both installed CLI projections.
#[derive(Debug, Args)]
pub struct DevArgs {
    /// Tenant to provision on first use; an active tenant is never reissued.
    #[arg(long)]
    tenant: TenantSlug,
    /// Detected MCP hosts to connect; repeat to select several. Interactive
    /// runs offer detected hosts when omitted; scripts select none implicitly.
    #[arg(long = "host", value_enum)]
    hosts: Vec<McpHost>,
}

/// The CLI's owned foreground server, also settled after a failed startup.
struct DevServer {
    /// Verified installation selected without updating an existing release.
    installed: InstalledServer,
    /// Effective loopback listener supplied to the normal serving path.
    bind: SocketAddr,
    /// HTTP endpoint shared by setup, readiness, clients, and MCP hosts.
    endpoint: String,
    /// macOS memory detected once and supplied only to child processes.
    memory: Option<String>,
    /// Serving child retained across readiness, setup, and foreground waiting.
    child: Option<Child>,
}

impl DevArgs {
    /// Compose local setup and remain attached until interruption or exit.
    ///
    /// # Errors
    /// Refuses invalid local inputs, an incompatible release, failed server
    /// operations, unsafe storage, unusable saved credentials, or failed hosts.
    /// Committed migration/provisioning steps remain on failure; reruns never
    /// reissue an active tenant's key. Cancellation settles the owned server.
    ///
    /// # Panics
    /// Panics if the compile-time package version is not valid semver.
    pub async fn run(self) -> Result<ExitCode, WyrdCliError> {
        let bind = local_bind()?;
        self.validate_local_config(&format!("http://{bind}"))?;
        let installer = ServerInstaller::official()?;
        let interrupted = interruption();
        tokio::pin!(interrupted);
        let installed = tokio::select! {
            biased;
            signal = &mut interrupted => {
                eprintln!("Local installation stopped; rerun the same command.");
                return signal.map(|()| ExitCode::from(130)).map_err(|source| WyrdCliError::Io { source });
            }
            installed = async {
                match installer.installed() {
                    Some(installed) => Ok(installed),
                    None => installer.install().await,
                }
            } => installed?,
        };
        let mut server = DevServer::new(installed, bind)?;
        let result = tokio::select! {
            biased;
            signal = &mut interrupted => {
                eprintln!("Local startup stopped. Committed setup steps remain; rerun the same command. An active tenant's key is never reissued.");
                signal.map(|()| ExitCode::from(130)).map_err(|source| WyrdCliError::Io { source })
            }
            result = server.run(&self) => result,
        };
        let settled = server.stop().await;
        match result {
            Ok(code) => settled.map(|()| code),
            Err(error) => {
                settled?;
                Err(error)
            }
        }
    }

    /// Refuse unsafe local files or an unrelated saved identity before setup.
    ///
    /// # Errors
    /// Returns protected-file, configuration, or saved-login validation errors.
    fn validate_local_config(&self, endpoint: &str) -> Result<(), WyrdCliError> {
        // Check local files before any server operation can issue a key.
        GlobalConfig::local_api_key(&Environment::Process).map_err(map_client_error)?;
        let global = GlobalConfig::load().map_err(|_| dev_error("local config", "config.toml is invalid; correct it or use a separate WYRD_CONFIG_HOME; no server setup has run"))?;
        if let Some(dir) = Environment::Process.config_dir() {
            let origin = canonical_origin(endpoint).map_err(map_client_error)?;
            if SavedLogins::at(dir)
                .select(&origin, global.client.tenant.as_deref())
                .map_err(map_client_error)?
                .is_some_and(|login| login.tenant_key != self.tenant)
            {
                return Err(dev_error(
                    "saved login",
                    "an unrelated login for this endpoint is preserved; use a separate WYRD_CONFIG_HOME for local setup",
                ));
            }
        }
        Ok(())
    }
}

impl DevServer {
    /// Retain a compatible installed binary and its loopback listener settings.
    ///
    /// # Errors
    /// Refuses a release outside the CLI's supported version line.
    ///
    /// # Panics
    /// Panics if the compile-time package version is not valid semver.
    fn new(installed: InstalledServer, bind: SocketAddr) -> Result<Self, WyrdCliError> {
        let cli = semver::Version::parse(env!("CARGO_PKG_VERSION"))
            .expect("the CLI package version is semver");
        if !compatible(&cli, &installed.version) {
            return Err(WyrdCliError::ServerVersionIncompatible {
                cli,
                server: installed.version,
            });
        }
        Ok(Self {
            installed,
            bind,
            endpoint: format!("http://{bind}"),
            memory: None,
            child: None,
        })
    }

    /// Construct each child with the same local inputs, leaving database and
    /// storage URLs in the environment and preserving production hardening.
    fn command(&self) -> Command {
        let mut command = Command::new(self.installed.binary());
        command
            .kill_on_drop(true)
            .stdin(Stdio::null())
            .env("WYRD_SERVER_BIND", self.bind.to_string());
        if let Some(memory) = &self.memory {
            command.env("WYRD_BIFROST_MEMORY_LIMIT_BYTES", memory);
        }
        command
    }

    /// Migrate, serve, provision, persist, authenticate a new client, and wait.
    ///
    /// # Errors
    /// Returns a stage-specific recovery message without child secret output.
    /// Committed server operations survive a failed later stage. The child
    /// remains owned by this handle and is settled by `stop` on every exit.
    async fn run(&mut self, args: &DevArgs) -> Result<ExitCode, WyrdCliError> {
        self.start().await?;
        let tenant = self.provision(&args.tenant).await?;
        save_local_tenant(&tenant, &args.tenant, &self.endpoint)?;
        drop(tenant);
        verify_saved_client().await?;
        mcp::offer_local(&args.hosts, &self.endpoint).await?;
        println!(
            "Local Wyrd ready at {} for tenant {}. The saved endpoint and credential are available to fresh clients; press Ctrl-C to stop.",
            self.endpoint, args.tenant
        );
        self.wait().await
    }

    /// Reserve HTTP through migration, then start and await the ordinary server.
    ///
    /// # Errors
    /// Refuses occupied listeners, invalid memory input, migration, or readiness.
    /// Cancellation leaves committed migrations intact and the child owned here.
    async fn start(&mut self) -> Result<(), WyrdCliError> {
        let reservation = std::net::TcpListener::bind(self.bind)
            .map_err(|_| dev_error("listener", "WYRD_SERVER_BIND is occupied or unavailable; stop the other listener or choose another loopback port"))?;
        self.detect_memory().await?;
        self.migrate().await?;
        drop(reservation);
        self.child = Some(self.command().spawn().map_err(|_| {
            dev_error(
                "startup",
                "check the installed server and listener availability, then rerun",
            )
        })?);
        self.ready().await
    }

    /// Supply macOS host memory only when the caller supplied no limit.
    ///
    /// # Errors
    /// Returns redacted guidance if the operating-system probe fails.
    async fn detect_memory(&mut self) -> Result<(), WyrdCliError> {
        if cfg!(target_os = "macos")
            && Environment::Process
                .var("WYRD_BIFROST_MEMORY_LIMIT_BYTES")
                .is_none()
        {
            let output = Command::new("sysctl")
                .args(["-n", "hw.memsize"])
                .output()
                .await
                .map_err(|_| {
                    dev_error(
                        "memory",
                        "set WYRD_BIFROST_MEMORY_LIMIT_BYTES to this machine's usable memory",
                    )
                })?;
            let memory = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if !output.status.success() || !memory.parse::<u64>().is_ok_and(|bytes| bytes > 0) {
                return Err(dev_error(
                    "memory",
                    "set WYRD_BIFROST_MEMORY_LIMIT_BYTES to this machine's usable memory",
                ));
            }
            self.memory = Some(memory);
        }
        Ok(())
    }

    /// Invoke the installed server's durable migration owner without its output.
    ///
    /// # Errors
    /// Returns secret-free guidance on failure; committed migrations remain.
    async fn migrate(&self) -> Result<(), WyrdCliError> {
        println!(
            "Migrating the installed Wyrd server {}.",
            self.installed.version
        );
        let migrated = self.command().arg("migrate").output().await.map_err(|_| {
            dev_error(
                "migration",
                "check the installed binary and database connectivity, then rerun",
            )
        })?;
        if !migrated.status.success() {
            return Err(dev_error(
                "migration",
                "check database connectivity and that the ordinary migration login owns the database objects; migrations may have committed and are safe to rerun",
            ));
        }
        Ok(())
    }

    /// Provision through server setup and retain only the returned tenant data.
    ///
    /// # Errors
    /// Refuses failed setup or an unrecognizable response. Committed setup may
    /// remain; cancellation never reissues or rolls it back.
    async fn provision(&self, tenant: &TenantSlug) -> Result<ProvisionedTenant, WyrdCliError> {
        let setup = self
            .command()
            .args([
                "setup",
                "--tenant",
                tenant.as_str(),
                "--server",
                &self.endpoint,
            ])
            .output()
            .await
            .map_err(|_| {
                dev_error(
                    "setup",
                    "rerun with the same tenant; committed setup remains in the database",
                )
            })?;
        let setup_succeeded = setup.status.success();
        let printed = SecretString::from(String::from_utf8(setup.stdout)
            .map_err(|_| dev_error("setup response", "the installed server returned invalid output; check its compatible release before rerunning"))?);
        disclose_root(printed.expose_secret());
        if !setup_succeeded {
            return Err(dev_error(
                "setup",
                "the platform root or tenant may already exist; rerun with WYRD_PLATFORM_CREDENTIAL if the root exists and the tenant does not; recover-root restores lost platform access",
            ));
        }
        ProvisionedTenant::parse(printed.expose_secret())
    }

    /// Observe the owned foreground server; every unsolicited exit is a failure.
    ///
    /// # Errors
    /// Returns redacted recovery guidance if waiting fails or the server exits.
    ///
    /// # Panics
    /// Panics if called before the server has started.
    async fn wait(&mut self) -> Result<ExitCode, WyrdCliError> {
        self.child
            .as_mut()
            .expect("the server is retained after readiness")
            .wait()
            .await
            .map_err(|_| {
                dev_error(
                    "server exit",
                    "rerun; saved client configuration and committed server state remain",
                )
            })?;
        Err(dev_error(
            "server exit",
            "the foreground server exited; rerun with the same tenant and configuration directory",
        ))
    }

    /// Wait for this child to become ready, refusing an occupied listener.
    ///
    /// # Errors
    /// Returns a redacted readiness failure if the child exits or is not ready
    /// within the existing two-minute local-server readiness budget.
    async fn ready(&mut self) -> Result<(), WyrdCliError> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .map_err(|_| dev_error("readiness", "check local HTTP connectivity and rerun"))?;
        tokio::time::timeout(Duration::from_mins(2), async {
            loop {
                if self.child.as_mut().expect("readiness follows spawn").try_wait()
                    .map_err(|_| dev_error("readiness", "the server could not be observed; rerun"))?.is_some() {
                    return Err(dev_error("readiness", "the server exited before readiness; inspect its redacted diagnostics and rerun"));
                }
                if http.get(format!("{}/readyz", self.endpoint)).send().await
                    .is_ok_and(|response| response.status().is_success()) {
                    return Ok(());
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }).await.map_err(|_| dev_error("readiness", "the server did not become ready within two minutes; inspect its diagnostics and rerun"))?
    }

    /// Forward termination and reap the child before returning to the caller.
    ///
    /// # Errors
    /// Returns a shutdown failure if signalling or reaping fails. A child
    /// exceeding the server's 35-second drain budget plus ten-second grace is
    /// killed and reaped; it cannot remain detached from the CLI.
    async fn stop(&mut self) -> Result<(), WyrdCliError> {
        let Some(mut child) = self.child.take() else {
            return Ok(());
        };
        if let Some(pid) = child.id() {
            let signalled = Command::new("kill")
                .args(["-TERM", &pid.to_string()])
                .status()
                .await
                .map_err(|_| {
                    dev_error(
                        "shutdown",
                        "the server could not be signalled; stop it before rerunning",
                    )
                })?;
            if !signalled.success()
                && child
                    .try_wait()
                    .map_err(|_| dev_error("shutdown", "the server could not be observed"))?
                    .is_none()
            {
                child.kill().await.map_err(|_| {
                    dev_error("shutdown", "stop the remaining server before rerunning")
                })?;
            }
        }
        if let Ok(result) = tokio::time::timeout(Duration::from_secs(45), child.wait()).await {
            result.map_err(|_| {
                dev_error(
                    "shutdown",
                    "the server could not be reaped; stop it before rerunning",
                )
            })?;
        } else {
            child
                .kill()
                .await
                .map_err(|_| dev_error("shutdown", "stop the remaining server before rerunning"))?;
            return Err(dev_error(
                "shutdown",
                "the server exceeded its drain deadline and was killed; rerun with the same configuration",
            ));
        }
        Ok(())
    }
}

/// Server-derived tenant identity and its optional one-time credential.
struct ProvisionedTenant {
    /// Exact identity used to reject another tenant's saved key.
    id: DataTenantId,
    /// Newly issued key; absent when the tenant was already active.
    key: Option<SecretString>,
}

impl ProvisionedTenant {
    /// Parse the installed server's existing first-use or active-tenant output.
    ///
    /// # Errors
    /// Refuses responses without a recognizable typed tenant identity.
    fn parse(printed: &str) -> Result<Self, WyrdCliError> {
        let key = printed
            .lines()
            .find_map(|line| line.strip_prefix("admin_credential: "))
            .map(|key| SecretString::from(key.to_owned()));
        let tenant_id: DataTenantId = printed.lines().find_map(|line| {
            line.strip_prefix("tenant_id: ").or_else(|| line.strip_prefix("Tenant `")
                .and_then(|line| line.split_once(" (").and_then(|(_, tail)| tail.split_once(')').map(|(id, _)| id))))
        }).and_then(|id| id.parse().ok()).ok_or_else(|| dev_error("setup response", "the installed server returned no recognizable tenant identity; check its compatible release before rerunning"))?;
        Ok(Self { id: tenant_id, key })
    }

    /// Select the issued or saved key and verify it names this tenant.
    ///
    /// # Errors
    /// Refuses another tenant's default key without changing it.
    fn credential<'a>(
        &'a self,
        saved: Option<&'a SecretString>,
    ) -> Result<Option<&'a SecretString>, WyrdCliError> {
        let candidate = self.key.as_ref().or(saved);
        if let Some(candidate) = candidate
            && !candidate
                .expose_secret()
                .starts_with(&format!("wyrd_sk_{}_", self.id.as_uuid().simple()))
        {
            return Err(dev_error(
                "saved credential",
                "the existing default key belongs to another tenant and is preserved; use a separate WYRD_CONFIG_HOME or restore the requested tenant's key",
            ));
        }
        Ok(candidate)
    }
}

/// Disclose a newly initialized root only to the invoking terminal.
/// An unavailable terminal prints recovery guidance; tenant persistence continues.
fn disclose_root(printed: &str) {
    if let Some(root) = printed
        .lines()
        .find(|line| line.starts_with("wyrd_global_"))
    {
        if let Ok(mut terminal) = std::fs::OpenOptions::new().write(true).open("/dev/tty") {
            if writeln!(
                terminal,
                "Platform administrative credential (store securely):\n{root}"
            )
            .and_then(|()| terminal.flush())
            .is_err()
            {
                eprintln!(
                    "Platform root initialized, but its credential could not be delivered to the terminal; use wyrd-server recover-root when platform administration is needed."
                );
            }
        } else {
            eprintln!(
                "Platform root initialized. No invoking terminal was available for disclosure; use wyrd-server recover-root when platform administration is needed."
            );
        }
    }
}

/// Validate external PostgreSQL and loopback HTTP inputs before installation.
///
/// # Errors
/// Refuses absent/invalid database URLs and non-loopback or zero-port binds.
fn local_bind() -> Result<SocketAddr, WyrdCliError> {
    let database = SecretString::from(std::env::var("WYRD_DATABASE_URL").unwrap_or_default());
    if database.expose_secret().is_empty()
        || !url::Url::parse(database.expose_secret())
            .is_ok_and(|url| matches!(url.scheme(), "postgres" | "postgresql"))
    {
        return Err(dev_error(
            "input",
            "set WYRD_DATABASE_URL to an externally managed PostgreSQL database; the migration login must own its objects",
        ));
    }
    let bind: SocketAddr = std::env::var("WYRD_SERVER_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8080".to_owned())
        .parse()
        .map_err(|_| {
            dev_error(
                "input",
                "WYRD_SERVER_BIND must be a loopback socket address with a nonzero port",
            )
        })?;
    if !bind.ip().is_loopback() || bind.port() == 0 {
        return Err(dev_error(
            "input",
            "WYRD_SERVER_BIND must be a loopback socket address with a nonzero port",
        ));
    }
    Ok(bind)
}

/// Commit the validated key before its endpoint through existing file owners.
///
/// # Errors
/// Refuses conflicting keys or unsafe persistence; committed setup remains.
fn save_local_tenant(
    tenant: &ProvisionedTenant,
    name: &TenantSlug,
    endpoint: &str,
) -> Result<(), WyrdCliError> {
    let saved = GlobalConfig::local_api_key(&Environment::Process).map_err(map_client_error)?;
    let candidate = tenant.credential(saved.as_ref())?;
    println!(
        "Tenant {} ({}) is active; saving local client configuration.",
        name, tenant.id
    );
    GlobalConfig::save_local(
            &Environment::Process,
            endpoint,
            candidate,
        )
            .map_err(|_| dev_error("local persistence", "the tenant may already be active; restore safe local file permissions and rerun. If its one-time key was not saved, use explicit platform tenant recover-admin; rerunning never replaces it"))?;
    Ok(())
}

/// Authenticate and perform a bounded read using only fresh saved-file inputs.
///
/// # Errors
/// Returns redacted guidance for unusable configuration or credentials.
async fn verify_saved_client() -> Result<(), WyrdCliError> {
    let directory = Environment::Process.config_dir().ok_or_else(|| {
        dev_error(
            "local persistence",
            "set WYRD_CONFIG_HOME, XDG_CONFIG_HOME, or HOME",
        )
    })?;
    let environment = Environment::Fixed(HashMap::from([(
        "WYRD_CONFIG_HOME".to_owned(),
        directory.to_string_lossy().into_owned(),
    )]));
    let global = GlobalConfig::load_in(&environment).map_err(map_client_error)?;
    let client = WyrdClient::with_config(ClientConfig::from_environment(
            environment,
            &global,
            None,
            None,
        ))
        .map_err(|_| dev_error("saved credential", "the active tenant has no usable saved credential or its configuration selects another identity; restore its key, correct the selector, or explicitly recover tenant administration through the platform"))?;
    client.access_token().await.map_err(|_| dev_error("saved credential", "the tenant is active but its saved key is missing, invalid, or expired; restore that key or explicitly recover tenant administration through the platform"))?;
    Cards::with_client(client).list(ListCardsRequest {
            kind: None, space: None, name: None, version_range: None,
            status: None, filter: None, include_prerelease: false,
            limit: Some(1), cursor: None,
        }).await
            .map_err(|_| dev_error("saved credential", "the ready server refused a fresh client's saved credential; restore the requested tenant's key or explicitly recover its administration"))?;
    Ok(())
}

/// Construct a stable local-stage error with static, secret-free recovery.
fn dev_error(stage: &'static str, recovery: &'static str) -> WyrdCliError {
    WyrdCliError::ServerDev { stage, recovery }
}

/// Wait for an operating-system interruption without detaching any child.
///
/// # Errors
/// Returns the IO error if a signal listener cannot be registered.
async fn interruption() -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let mut terminated =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {
            result = tokio::signal::ctrl_c() => result,
            _ = terminated.recv() => Ok(()),
        }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c().await
}
