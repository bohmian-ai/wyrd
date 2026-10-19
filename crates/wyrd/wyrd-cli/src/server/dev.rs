//! Installed-server orchestration for local development. Durable operations
//! remain the installed server's `migrate` and `setup` commands.

use std::collections::HashMap;
use std::io::Write as _;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::{ExitCode, Stdio};
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
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

/// The production UI's adapter-node entry inside an installed release.
const UI_ENTRY: &str = "ui/build/index.js";

/// Oldest system Node.js major version the bundled UI is built and tested on.
const NODE_MAJOR: u32 = 26;

/// Recovery for an absent, failing, or older system `node`.
const NODE_REQUIRED: &str =
    "install Node.js 26 or newer as `node` on PATH and rerun; no server step has run";

/// The CLI's owned foreground server and production UI, also settled after a
/// failed startup.
struct DevServer {
    /// Verified installation selected without updating an existing release.
    installed: InstalledServer,
    /// Effective loopback listener supplied to the normal serving path.
    bind: SocketAddr,
    /// Effective loopback public gRPC listener, supplied to every child and
    /// saved as the fresh-client gRPC destination.
    grpc: SocketAddr,
    /// HTTP endpoint shared by setup, readiness, clients, MCP hosts, and the UI.
    endpoint: String,
    /// Effective loopback listener of the UI's Node BFF; `http://{ui_bind}`
    /// is the browser's public origin.
    ui_bind: SocketAddr,
    /// Local deployment sealing key supplied as `WYRD_SEALING_KEY_FILE`;
    /// `None` when the caller already supplies the server's sealing key.
    sealing_key: Option<PathBuf>,
    /// Serving child retained across readiness, setup, and foreground waiting.
    child: Option<Child>,
    /// UI child started after the server is ready and settled with it.
    ui: Option<Child>,
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
        let (bind, grpc, ui_bind) = local_binds()?;
        let configured = self.validate_local_config(&format!("http://{bind}"))?;
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
        let sealing_key = local_sealing_key(installer.root())?;
        let mut server = DevServer::new(installed, bind, grpc, ui_bind, sealing_key)?;
        let result = tokio::select! {
            biased;
            signal = &mut interrupted => {
                eprintln!("Local startup stopped. Committed setup steps remain; rerun the same command. An active tenant's key is never reissued.");
                signal.map(|()| ExitCode::from(130)).map_err(|source| WyrdCliError::Io { source })
            }
            result = server.run(&self, configured) => result,
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

    /// Refuse unsafe local files or an unrelated saved identity before setup,
    /// and report whether an earlier run already completed local setup.
    ///
    /// Completed setup means this endpoint and a protected key were saved
    /// before this invocation wrote anything. It only suppresses the automatic
    /// host offer; the saved key is still verified after provisioning.
    ///
    /// # Errors
    /// Returns protected-file, configuration, or saved-login validation errors.
    fn validate_local_config(&self, endpoint: &str) -> Result<bool, WyrdCliError> {
        // Check local files before any server operation can issue a key.
        let key = GlobalConfig::local_api_key(&Environment::Process).map_err(map_client_error)?;
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
        Ok(key.is_some() && global.client.http_url.as_deref() == Some(endpoint))
    }
}

impl DevServer {
    /// Retain a compatible installed release, its loopback listener settings,
    /// and the local sealing key every server child receives.
    ///
    /// # Errors
    /// Refuses a release outside the CLI's supported version line.
    ///
    /// # Panics
    /// Panics if the compile-time package version is not valid semver.
    fn new(
        installed: InstalledServer,
        bind: SocketAddr,
        grpc: SocketAddr,
        ui_bind: SocketAddr,
        sealing_key: Option<PathBuf>,
    ) -> Result<Self, WyrdCliError> {
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
            grpc,
            endpoint: format!("http://{bind}"),
            ui_bind,
            sealing_key,
            child: None,
            ui: None,
        })
    }

    /// Construct each child with the same local inputs, leaving database and
    /// storage URLs in the environment and preserving production hardening.
    /// The selected listeners override any server-file setting, and the local
    /// sealing key, when the caller supplied none, is the server's ordinary
    /// `WYRD_SEALING_KEY_FILE`.
    fn command(&self) -> Command {
        let mut command = Command::new(self.installed.binary());
        command
            .kill_on_drop(true)
            .stdin(Stdio::null())
            .env("WYRD_SERVER_BIND", self.bind.to_string())
            .env("WYRD_GRPC_BIND", self.grpc.to_string());
        if let Some(sealing_key) = &self.sealing_key {
            command.env("WYRD_SEALING_KEY_FILE", sealing_key);
        }
        command
    }

    /// Construct the UI child: the installed adapter-node build on the system
    /// Node, listening on the UI bind with that address as its trusted public
    /// origin, so forwarded headers are never consulted, and calling the
    /// server at its loopback endpoint.
    fn ui_command(&self) -> Command {
        let mut command = Command::new("node");
        command
            .arg(self.installed.dir.join(UI_ENTRY))
            .kill_on_drop(true)
            .stdin(Stdio::null())
            .env("HOST", self.ui_bind.ip().to_string())
            .env("PORT", self.ui_bind.port().to_string())
            .env("ORIGIN", format!("http://{}", self.ui_bind))
            .env("WYRD_SERVER_URL", &self.endpoint);
        command
    }

    /// Refuse an installed release without the production UI, or an absent,
    /// failing, or pre-[`NODE_MAJOR`] system `node`, before any server step.
    ///
    /// # Errors
    /// Returns the `ui` stage with install or Node.js guidance.
    async fn check_ui(&self) -> Result<(), WyrdCliError> {
        if !self.installed.dir.join(UI_ENTRY).is_file() {
            return Err(dev_error(
                "ui",
                "the installed release has no production UI; run `wyrd server install` and rerun; no server step has run",
            ));
        }
        let version = Command::new("node")
            .arg("--version")
            .output()
            .await
            .map_err(|_| dev_error("ui", NODE_REQUIRED))?;
        let major = String::from_utf8_lossy(&version.stdout)
            .trim()
            .strip_prefix('v')
            .and_then(|version| version.split('.').next()?.parse::<u32>().ok());
        if !version.status.success() || major.is_none_or(|major| major < NODE_MAJOR) {
            return Err(dev_error("ui", NODE_REQUIRED));
        }
        Ok(())
    }

    /// Migrate, serve, provision, persist, authenticate a new client, and wait.
    ///
    /// Only first setup offers detected hosts; `configured` restarts skip the
    /// offer unless hosts were selected explicitly. Both children stay
    /// observed during the offer: either exit cancels any unanswered question.
    ///
    /// # Errors
    /// Returns a stage-specific recovery message without child secret output.
    /// Committed server operations survive a failed later stage. The children
    /// remain owned by this handle and are settled by `stop` on every exit.
    ///
    /// # Panics
    /// Panics if either child is not retained after readiness.
    async fn run(&mut self, args: &DevArgs, configured: bool) -> Result<ExitCode, WyrdCliError> {
        self.start().await?;
        let tenant = self.provision(&args.tenant).await?;
        save_local_tenant(&tenant, &args.tenant, &self.endpoint, self.grpc)?;
        drop(tenant);
        verify_saved_client().await?;
        let Self {
            child,
            ui,
            endpoint,
            ui_bind,
            ..
        } = self;
        let child = child
            .as_mut()
            .expect("the server is retained after readiness");
        let ui = ui.as_mut().expect("the UI is retained after readiness");
        if !configured || !args.hosts.is_empty() {
            tokio::select! {
                offered = mcp::offer_local(&args.hosts, endpoint) => offered?,
                exited = Self::exited(child, ui) => return exited,
            }
        }
        println!(
            "Local Wyrd ready at {endpoint} for tenant {}, with its UI at http://{ui_bind}. The saved endpoint and credential are available to fresh clients; press Ctrl-C to stop.",
            args.tenant
        );
        Self::exited(child, ui).await
    }

    /// Check the UI prerequisites, reserve both HTTP listeners through
    /// migration, then start and await the ordinary server and, once it is
    /// ready, the UI.
    ///
    /// # Errors
    /// Refuses a missing UI or Node.js, occupied listeners, migration, or
    /// readiness. Cancellation leaves committed migrations intact and the
    /// children owned here.
    async fn start(&mut self) -> Result<(), WyrdCliError> {
        self.check_ui().await?;
        let reservation = std::net::TcpListener::bind(self.bind)
            .map_err(|_| dev_error("listener", "WYRD_SERVER_BIND is occupied or unavailable; stop the other listener or choose another loopback port"))?;
        let ui_reservation = std::net::TcpListener::bind(self.ui_bind)
            .map_err(|_| dev_error("listener", "WYRD_UI_BIND is occupied or unavailable; stop the other listener or choose another loopback port"))?;
        self.migrate().await?;
        drop(reservation);
        let child = self.child.insert(self.command().spawn().map_err(|_| {
            dev_error(
                "startup",
                "check the installed server and listener availability, then rerun",
            )
        })?);
        Self::ready(child, &format!("{}/readyz", self.endpoint)).await?;
        drop(ui_reservation);
        let ui =
            self.ui.insert(self.ui_command().spawn().map_err(|_| {
                dev_error("startup", "check the installed UI and Node.js, then rerun")
            })?);
        Self::ready(ui, &format!("http://{}/", self.ui_bind)).await
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

    /// Observe the owned server and UI; every unsolicited exit of either is a
    /// failure. Reaped statuses stay cached on the children, so `stop` still
    /// settles both.
    ///
    /// # Errors
    /// Returns redacted recovery guidance if waiting fails or a child exits.
    async fn exited(server: &mut Child, ui: &mut Child) -> Result<ExitCode, WyrdCliError> {
        tokio::select! {
            waited = server.wait() => waited,
            waited = ui.wait() => waited,
        }
        .map_err(|_| {
            dev_error(
                "server exit",
                "rerun; saved client configuration and committed server state remain",
            )
        })?;
        Err(dev_error(
            "server exit",
            "the foreground server or UI exited; rerun with the same tenant and configuration directory",
        ))
    }

    /// Wait for `child` to answer a successful `GET url`, refusing an occupied
    /// listener: a child that exits first fails readiness.
    ///
    /// # Errors
    /// Returns a redacted readiness failure if the child exits or is not ready
    /// within the existing two-minute local-server readiness budget.
    async fn ready(child: &mut Child, url: &str) -> Result<(), WyrdCliError> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .map_err(|_| dev_error("readiness", "check local HTTP connectivity and rerun"))?;
        tokio::time::timeout(Duration::from_mins(2), async {
            loop {
                if child.try_wait()
                    .map_err(|_| dev_error("readiness", "the server or UI could not be observed; rerun"))?.is_some() {
                    return Err(dev_error("readiness", "the server or UI exited before readiness; inspect its redacted diagnostics and rerun"));
                }
                if http.get(url).send().await
                    .is_ok_and(|response| response.status().is_success()) {
                    return Ok(());
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }).await.map_err(|_| dev_error("readiness", "the server or UI did not become ready within two minutes; inspect its diagnostics and rerun"))?
    }

    /// Settle the UI and the server together before returning to the caller.
    ///
    /// # Errors
    /// Returns the first shutdown failure; both children are still settled.
    async fn stop(&mut self) -> Result<(), WyrdCliError> {
        let (ui, server) = tokio::join!(settle(self.ui.take()), settle(self.child.take()));
        ui.and(server)
    }
}

/// Forward termination to an owned child and reap it.
///
/// # Errors
/// Returns a shutdown failure if signalling or reaping fails. A child
/// exceeding the server's 35-second drain budget plus ten-second grace is
/// killed and reaped; it cannot remain detached from the CLI.
async fn settle(child: Option<Child>) -> Result<(), WyrdCliError> {
    let Some(mut child) = child else {
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
                    "the server or UI could not be signalled; stop it before rerunning",
                )
            })?;
        if !signalled.success()
            && child
                .try_wait()
                .map_err(|_| dev_error("shutdown", "the server or UI could not be observed"))?
                .is_none()
        {
            child.kill().await.map_err(|_| {
                dev_error(
                    "shutdown",
                    "stop the remaining server or UI before rerunning",
                )
            })?;
        }
    }
    if let Ok(result) = tokio::time::timeout(Duration::from_secs(45), child.wait()).await {
        result.map_err(|_| {
            dev_error(
                "shutdown",
                "the server or UI could not be reaped; stop it before rerunning",
            )
        })?;
    } else {
        child.kill().await.map_err(|_| {
            dev_error(
                "shutdown",
                "stop the remaining server or UI before rerunning",
            )
        })?;
        return Err(dev_error(
            "shutdown",
            "the server or UI exceeded its drain deadline and was killed; rerun with the same configuration",
        ));
    }
    Ok(())
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

/// Name of the local deployment sealing key under the install root.
const SEALING_KEY_FILE: &str = "sealing.key";

/// Ensure the local deployment has the sealing key its server encrypts stored
/// secrets with, such as gateway managed-secret provider keys, before the
/// first boot, and return the file to supply as `WYRD_SEALING_KEY_FILE`.
///
/// A caller that already supplies `WYRD_SEALING_KEY_FILE` or
/// `WYRD_SEALING_KEY_BASE64` keeps it and gets `None`. Otherwise the first run
/// writes 32 random bytes, base64-encoded, to an owner-only `sealing.key`
/// under the install root and every later run reuses it, so stored secrets
/// keep opening across restarts and releases. The file is staged beside its
/// destination and moved into place without replacing an existing key, so an
/// interrupted or concurrent first run never leaves a partial key. The key is
/// never printed.
///
/// # Errors
/// Returns a secret-free local-persistence error when the key cannot be
/// staged or moved into place.
fn local_sealing_key(root: &Path) -> Result<Option<PathBuf>, WyrdCliError> {
    if std::env::var_os("WYRD_SEALING_KEY_FILE").is_some()
        || std::env::var_os("WYRD_SEALING_KEY_BASE64").is_some()
    {
        return Ok(None);
    }
    let path = root.join(SEALING_KEY_FILE);
    if path.is_file() {
        return Ok(Some(path));
    }
    let failed = || {
        dev_error(
            "local persistence",
            "the local sealing key could not be written; restore safe permissions on the Wyrd config directory and rerun",
        )
    };
    // `NamedTempFile` is created owner-only, the mode the server requires of
    // a mounted key file.
    let mut staged = tempfile::NamedTempFile::new_in(root).map_err(|_| failed())?;
    staged
        .write_all(
            BASE64_STANDARD
                .encode(rand::random::<[u8; 32]>())
                .as_bytes(),
        )
        .and_then(|()| staged.as_file().sync_all())
        .map_err(|_| failed())?;
    match staged.persist_noclobber(&path) {
        Ok(_) => Ok(Some(path)),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => Ok(Some(path)),
        Err(_) => Err(failed()),
    }
}

/// Validate external PostgreSQL and loopback HTTP, public gRPC, and UI inputs
/// before installation, returning the selected HTTP, gRPC, and UI listeners.
///
/// # Errors
/// Refuses absent/invalid database URLs and non-loopback or zero-port binds.
fn local_binds() -> Result<(SocketAddr, SocketAddr, SocketAddr), WyrdCliError> {
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
    Ok((
        loopback(
            "WYRD_SERVER_BIND",
            "127.0.0.1:8080",
            "WYRD_SERVER_BIND must be a loopback socket address with a nonzero port",
        )?,
        loopback(
            "WYRD_GRPC_BIND",
            "127.0.0.1:50051",
            "WYRD_GRPC_BIND must be a loopback socket address with a nonzero port",
        )?,
        loopback(
            "WYRD_UI_BIND",
            "127.0.0.1:3000",
            "WYRD_UI_BIND must be a loopback socket address with a nonzero port",
        )?,
    ))
}

/// Select one listener from `variable`, else the ordinary loopback default.
///
/// # Errors
/// Returns `refusal` for an unparsable, non-loopback, or zero-port address.
fn loopback(
    variable: &str,
    default: &str,
    refusal: &'static str,
) -> Result<SocketAddr, WyrdCliError> {
    std::env::var(variable)
        .unwrap_or_else(|_| default.to_owned())
        .parse::<SocketAddr>()
        .ok()
        .filter(|bind| bind.ip().is_loopback() && bind.port() != 0)
        .ok_or_else(|| dev_error("input", refusal))
}

/// Commit the validated key before its HTTP and gRPC endpoints through
/// existing file owners.
///
/// # Errors
/// Refuses conflicting keys or unsafe persistence; committed setup remains.
fn save_local_tenant(
    tenant: &ProvisionedTenant,
    name: &TenantSlug,
    endpoint: &str,
    grpc: SocketAddr,
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
            &format!("http://{grpc}"),
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
    Cards::connect_with(&client).await?.list(ListCardsRequest {
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
