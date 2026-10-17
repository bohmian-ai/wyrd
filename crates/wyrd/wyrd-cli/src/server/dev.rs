//! Installed-server orchestration for local development. Durable operations
//! remain the installed server's `migrate` and `setup` commands.

use std::collections::HashMap;
use std::io::{IsTerminal as _, Write as _};
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
use wyrd_client::global_config::DevSection;
use wyrd_client::saved_login::{SavedLogins, canonical_origin};
use wyrd_client::{Gateway, GlobalConfig, WyrdClient};
use wyrd_spec::DataTenantId;
use wyrd_spec::gateway::ProviderDeploymentsApply;
use wyrd_spec::ids::TenantSlug;
use wyrd_spec::registry::ListCardsRequest;

use super::install::compatible;
use super::{InstalledServer, ServerInstaller};
use crate::client::map_client_error;
use crate::error::WyrdCliError;
use crate::gateway::{AuthoringApply, DeploymentFile, apply_deployments_with};
use crate::mcp::{self, McpHost};
use crate::platform::credential::PlatformEndpoint;
#[cfg(unix)]
use crate::terminal::Terminal;

/// Local inputs accepted by both installed CLI projections. Each resolves as
/// argument, canonical environment variable, saved `[dev]` setting, then a
/// terminal prompt; secrets are never accepted as arguments.
#[derive(Debug, Args)]
pub struct DevArgs {
    /// Tenant to provision on first use; an active tenant is never reissued.
    /// Defaults to the saved tenant, else is asked on a terminal.
    #[arg(long)]
    tenant: Option<TenantSlug>,
    /// Detected MCP hosts to connect; repeat to select several. Interactive
    /// runs offer detected hosts when omitted; scripts select none implicitly.
    #[arg(long = "host", value_enum)]
    hosts: Vec<McpHost>,
    /// Never prompt: report every unresolved input together and exit before
    /// any download, migration, or provisioning.
    #[arg(long)]
    non_interactive: bool,
    /// Loopback HTTP listener; overrides `WYRD_SERVER_BIND` and the saved one.
    #[arg(long, value_name = "ADDR")]
    bind: Option<String>,
    /// Loopback public gRPC listener; overrides `WYRD_GRPC_BIND` and the
    /// saved one.
    #[arg(long, value_name = "ADDR")]
    grpc_bind: Option<String>,
    /// Storage URL without credentials, query, or fragment; overrides
    /// `WYRD_STORAGE_URL` and the saved one.
    #[arg(long, value_name = "URL")]
    storage_url: Option<String>,
    /// Gateway document to apply after readiness, also on restart; its
    /// `local_env` credentials resolve from this process's environment.
    #[arg(long, value_name = "PATH", conflicts_with = "skip_gateway")]
    gateway_file: Option<PathBuf>,
    /// Skip the gateway offer.
    #[arg(long)]
    skip_gateway: bool,
}

/// Effective local deployment inputs, resolved once before any side effect,
/// supplied unchanged to every server child, and saved for restarts.
struct DevInputs {
    /// Migration-owner PostgreSQL URL (`WYRD_DATABASE_URL`).
    database: SecretString,
    /// Optional separate platform URL (`WYRD_PLATFORM_DATABASE_URL`).
    platform_database: Option<SecretString>,
    /// Tenant provisioned and served.
    tenant: TenantSlug,
    /// Loopback HTTP listener.
    bind: SocketAddr,
    /// Loopback public gRPC listener.
    grpc: SocketAddr,
    /// Effective `WYRD_STORAGE_URL`.
    storage_url: String,
    /// Effective absolute `WYRD_BIFROST_DATA_DIR`.
    bifrost: PathBuf,
    /// Explicitly requested gateway document, already resolved locally.
    gateway: Option<ProviderDeploymentsApply>,
    /// Whether questions may be asked on the invoking terminal.
    interactive: bool,
    /// Whether to offer detected provider keys: a fresh interactive setup
    /// without `--gateway-file` or `--skip-gateway`.
    offer_gateway: bool,
    /// Whether an earlier run already completed setup for this endpoint;
    /// it suppresses the automatic gateway and host offers.
    configured: bool,
}

/// The CLI's owned foreground server, also settled after a failed startup.
struct DevServer {
    /// Verified installation selected without updating an existing release.
    installed: InstalledServer,
    /// Resolved inputs every child receives and restarts reuse.
    inputs: DevInputs,
    /// HTTP endpoint shared by setup, readiness, clients, and MCP hosts.
    endpoint: String,
    /// macOS memory detected once and supplied only to child processes.
    memory: Option<String>,
    /// Local deployment sealing key supplied as `WYRD_SEALING_KEY_FILE`;
    /// `None` when the caller already supplies the server's sealing key.
    sealing_key: Option<PathBuf>,
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
    /// reissue an active tenant's key. Interruption is observed from before
    /// the first question, so an unanswered concealed question is dropped with
    /// echo restored; later, cancellation settles the owned server.
    ///
    /// # Panics
    /// Panics if the compile-time package version is not valid semver.
    pub async fn run(self) -> Result<ExitCode, WyrdCliError> {
        let installer = ServerInstaller::official()?;
        let interrupted = interruption();
        tokio::pin!(interrupted);
        // Biased: the signal listeners register before any question is asked.
        let inputs = tokio::select! {
            biased;
            signal = &mut interrupted => {
                eprintln!("Local setup stopped before installation, migration, or provisioning; rerun the same command.");
                return signal.map(|()| ExitCode::from(130)).map_err(|source| WyrdCliError::Io { source });
            }
            inputs = self.resolve(installer.root()) => inputs?,
        };
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
        let mut server = DevServer::new(installed, inputs, sealing_key)?;
        let result = tokio::select! {
            biased;
            signal = &mut interrupted => {
                eprintln!("Local startup stopped. Committed setup steps remain; rerun the same command. An active tenant's key is never reissued.");
                signal.map(|()| ExitCode::from(130)).map_err(|source| WyrdCliError::Io { source })
            }
            result = server.run(&self.hosts) => result,
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

    /// Resolve every input before any side effect, asking for a missing
    /// database URL (concealed) and tenant only on a usable terminal.
    ///
    /// Unattended runs report all unresolved inputs together. Unset storage
    /// selects absolute defaults beneath the server directory `root`, which
    /// are created; an existing working-directory `.wyrd` is never adopted.
    ///
    /// # Errors
    /// Refuses unsafe local files, invalid or missing inputs, a saved key or
    /// login that belongs to another tenant, and an unreadable gateway file.
    async fn resolve(&self, root: &Path) -> Result<DevInputs, WyrdCliError> {
        // Check local files before any server operation can issue a key.
        let environment = Environment::Process;
        let key = GlobalConfig::local_api_key(&environment).map_err(map_client_error)?;
        let global = GlobalConfig::load().map_err(|_| dev_error("local config", "config.toml is invalid; correct it or use a separate WYRD_CONFIG_HOME; no server setup has run"))?;
        let (saved_database, saved_platform) =
            GlobalConfig::local_databases(&environment).map_err(map_client_error)?;
        let saved = global.dev;
        let bind = loopback(
            first(
                self.bind.as_deref(),
                "WYRD_SERVER_BIND",
                saved.bind.as_deref(),
            ),
            "127.0.0.1:8080",
            "--bind / WYRD_SERVER_BIND must be a loopback socket address with a nonzero port",
        )?;
        let grpc = loopback(
            first(
                self.grpc_bind.as_deref(),
                "WYRD_GRPC_BIND",
                saved.grpc_bind.as_deref(),
            ),
            "127.0.0.1:50051",
            "--grpc-bind / WYRD_GRPC_BIND must be a loopback socket address with a nonzero port",
        )?;
        if let Some(storage) = &self.storage_url
            && !url::Url::parse(storage).is_ok_and(|url| {
                url.username().is_empty()
                    && url.password().is_none()
                    && url.query().is_none()
                    && url.fragment().is_none()
            })
        {
            return Err(dev_error(
                "input",
                "--storage-url must be a URL without credentials, query, or fragment; supply credential-bearing storage through WYRD_STORAGE_URL and its provider's environment",
            ));
        }
        let gateway = self
            .gateway_file
            .as_deref()
            .map(|path| {
                Ok::<_, WyrdCliError>(match DeploymentFile::read(path)? {
                    DeploymentFile::Apply(apply) => apply,
                    DeploymentFile::One(deployment) => ProviderDeploymentsApply {
                        credentials: Vec::new(),
                        deployments: vec![deployment],
                    },
                    DeploymentFile::Many(deployments) => ProviderDeploymentsApply {
                        credentials: Vec::new(),
                        deployments,
                    },
                })
            })
            .transpose()?;
        let saved_tenant = saved
            .tenant
            .as_deref()
            .map(str::parse::<TenantSlug>)
            .transpose()
            .map_err(|_| dev_error("local config", "config.toml [dev] tenant is invalid; correct it or use a separate WYRD_CONFIG_HOME"))?;
        let interactive = cfg!(unix) && !self.non_interactive && std::io::stdin().is_terminal();
        let database = secret_var("WYRD_DATABASE_URL").or(saved_database);
        let tenant = self.tenant.clone().or_else(|| saved_tenant.clone());
        let (database, tenant) = match (database, tenant) {
            (Some(database), Some(tenant)) => (database, tenant),
            #[cfg(unix)]
            (database, tenant) if interactive => {
                let terminal = Terminal::open().map_err(|source| WyrdCliError::Io { source })?;
                let database = match database {
                    Some(database) => database,
                    None => terminal
                        .conceal("PostgreSQL URL for WYRD_DATABASE_URL (input hidden): ")
                        .await
                        .map_err(|source| WyrdCliError::Io { source })?,
                };
                let tenant = match tenant {
                    Some(tenant) => tenant,
                    None => terminal
                        .ask("Tenant slug to provision: ")
                        .await
                        .map_err(|source| WyrdCliError::Io { source })?
                        .trim()
                        .parse()
                        .map_err(|_| dev_error("input", "the tenant slug must be 3-64 lowercase letters, digits, '_' or '-', starting with a letter"))?,
                };
                (database, tenant)
            }
            (None, None) => {
                return Err(dev_error(
                    "input",
                    "unresolved inputs: set WYRD_DATABASE_URL and pass --tenant, or run on a terminal to be asked; nothing has run",
                ));
            }
            (None, Some(_)) => {
                return Err(dev_error(
                    "input",
                    "unresolved input: set WYRD_DATABASE_URL to an externally managed PostgreSQL database, or run on a terminal to be asked; nothing has run",
                ));
            }
            (Some(_), None) => {
                return Err(dev_error(
                    "input",
                    "unresolved input: pass --tenant, or run on a terminal to be asked; nothing has run",
                ));
            }
        };
        if !url::Url::parse(database.expose_secret())
            .is_ok_and(|url| matches!(url.scheme(), "postgres" | "postgresql"))
        {
            return Err(dev_error(
                "input",
                "the database URL must be a postgres:// URL for an externally managed database whose migration login owns its objects",
            ));
        }
        if key.is_some() && saved_tenant.as_ref().is_some_and(|saved| *saved != tenant) {
            return Err(dev_error(
                "saved credential",
                "the saved key belongs to the saved [dev] tenant and is preserved; use a separate WYRD_CONFIG_HOME for another tenant",
            ));
        }
        let endpoint = format!("http://{bind}");
        validate_saved_login(&endpoint, &tenant, global.client.tenant.as_deref())?;
        let (storage_url, bifrost) = local_data(
            first(
                self.storage_url.as_deref(),
                "WYRD_STORAGE_URL",
                saved.storage_url.as_deref(),
            ),
            first(
                None,
                "WYRD_BIFROST_DATA_DIR",
                saved.bifrost_data_dir.as_deref(),
            ),
            root,
        )?;
        let configured =
            key.is_some() && global.client.http_url.as_deref() == Some(endpoint.as_str());
        Ok(DevInputs {
            offer_gateway: interactive && !self.skip_gateway && !configured,
            configured,
            database,
            platform_database: secret_var("WYRD_PLATFORM_DATABASE_URL").or(saved_platform),
            tenant,
            bind,
            grpc,
            storage_url,
            bifrost,
            gateway,
            interactive,
        })
    }
}

/// Refuse an unrelated saved login for `endpoint` before setup.
///
/// # Errors
/// Returns saved-login validation errors and a refusal naming the
/// `WYRD_CONFIG_HOME` separation when a login selects another tenant.
fn validate_saved_login(
    endpoint: &str,
    tenant: &TenantSlug,
    selector: Option<&str>,
) -> Result<(), WyrdCliError> {
    if let Some(dir) = Environment::Process.config_dir() {
        let origin = canonical_origin(endpoint).map_err(map_client_error)?;
        if SavedLogins::at(dir)
            .select(&origin, selector)
            .map_err(map_client_error)?
            .is_some_and(|login| login.tenant_key != *tenant)
        {
            return Err(dev_error(
                "saved login",
                "an unrelated login for this endpoint is preserved; use a separate WYRD_CONFIG_HOME for local setup",
            ));
        }
    }
    Ok(())
}

/// The first supplied value: `argument`, then environment `variable`, then
/// the `saved` setting.
fn first(argument: Option<&str>, variable: &str, saved: Option<&str>) -> Option<String> {
    argument
        .map(str::to_owned)
        .or_else(|| {
            std::env::var(variable)
                .ok()
                .filter(|value| !value.is_empty())
        })
        .or_else(|| saved.map(str::to_owned))
}

/// Read a secret-bearing environment variable; empty means unset.
fn secret_var(variable: &str) -> Option<SecretString> {
    std::env::var(variable)
        .ok()
        .filter(|value| !value.is_empty())
        .map(SecretString::from)
}

/// Select the storage URL and absolute Bifrost root, defaulting each to a
/// created directory beneath the server directory `root`.
///
/// A defaulted storage selection is explained once, with its absolute
/// location; an existing working-directory `.wyrd` is named but not adopted.
///
/// # Errors
/// Refuses an unresolvable Bifrost path and default directories that cannot
/// be created.
fn local_data(
    storage: Option<String>,
    bifrost: Option<String>,
    root: &Path,
) -> Result<(String, PathBuf), WyrdCliError> {
    let created = |path: PathBuf| {
        std::fs::create_dir_all(&path).map(|()| path).map_err(|_| {
            dev_error(
                "local persistence",
                "the local data directory could not be created; restore safe permissions on the Wyrd config directory or set WYRD_STORAGE_URL and WYRD_BIFROST_DATA_DIR",
            )
        })
    };
    let bifrost = match bifrost {
        Some(path) => std::path::absolute(path).map_err(|_| {
            dev_error(
                "input",
                "WYRD_BIFROST_DATA_DIR must name a resolvable directory",
            )
        })?,
        None => created(root.join("data").join("bifrost"))?,
    };
    if let Some(storage) = storage {
        return Ok((storage, bifrost));
    }
    let path = created(root.join("data").join("storage"))?;
    println!(
        "WYRD_STORAGE_URL is unset; using local storage at {} (saved as [dev] storage_url).",
        path.display()
    );
    if Path::new(".wyrd").is_dir() {
        println!(
            "The existing ./.wyrd directory is not adopted; pass --storage-url and set WYRD_BIFROST_DATA_DIR to use it."
        );
    }
    let storage = url::Url::from_file_path(&path)
        .map_err(|()| dev_error("input", "the local storage path is not absolute"))?;
    Ok((storage.to_string(), bifrost))
}

impl DevServer {
    /// Retain a compatible installed binary, its loopback listener settings,
    /// and the local sealing key every child receives.
    ///
    /// # Errors
    /// Refuses a release outside the CLI's supported version line.
    ///
    /// # Panics
    /// Panics if the compile-time package version is not valid semver.
    fn new(
        installed: InstalledServer,
        inputs: DevInputs,
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
            endpoint: format!("http://{}", inputs.bind),
            inputs,
            memory: None,
            sealing_key,
            child: None,
        })
    }

    /// Construct each child with the same resolved inputs as canonical
    /// server environment variables, never arguments, preserving production
    /// hardening. The selected listeners override any server-file setting,
    /// and the local sealing key, when the caller supplied none, is the
    /// server's ordinary `WYRD_SEALING_KEY_FILE`.
    fn command(&self) -> Command {
        let inputs = &self.inputs;
        let mut command = Command::new(self.installed.binary());
        command
            .kill_on_drop(true)
            .stdin(Stdio::null())
            .env("WYRD_DATABASE_URL", inputs.database.expose_secret())
            .env("WYRD_SERVER_BIND", inputs.bind.to_string())
            .env("WYRD_GRPC_BIND", inputs.grpc.to_string())
            .env("WYRD_STORAGE_URL", &inputs.storage_url)
            .env("WYRD_BIFROST_DATA_DIR", &inputs.bifrost);
        if let Some(platform) = &inputs.platform_database {
            command.env("WYRD_PLATFORM_DATABASE_URL", platform.expose_secret());
        }
        if let Some(memory) = &self.memory {
            command.env("WYRD_BIFROST_MEMORY_LIMIT_BYTES", memory);
        }
        if let Some(sealing_key) = &self.sealing_key {
            command.env("WYRD_SEALING_KEY_FILE", sealing_key);
        }
        command
    }

    /// Migrate, serve, save settings, provision, persist, authenticate a new
    /// client, apply any requested gateway document, and wait.
    ///
    /// Settings are saved before provisioning so a protected-file failure
    /// surfaces before a one-time key is issued. Only first setup offers the
    /// gateway and detected hosts; `configured` restarts skip both offers
    /// unless a gateway file or hosts were selected explicitly. The serving
    /// child stays observed during every offer: its exit cancels any
    /// unanswered question.
    ///
    /// # Errors
    /// Returns a stage-specific recovery message without child secret output.
    /// Committed server operations survive a failed later stage. The child
    /// remains owned by this handle and is settled by `stop` on every exit.
    ///
    /// # Panics
    /// Panics if the serving child is not retained after readiness.
    async fn run(&mut self, hosts: &[McpHost]) -> Result<ExitCode, WyrdCliError> {
        self.start().await?;
        self.save_settings()?;
        let tenant = self.provision(&self.inputs.tenant).await?;
        self.save_tenant(&tenant).await?;
        drop(tenant);
        let client = saved_client().await?;
        let Self {
            child,
            endpoint,
            inputs,
            ..
        } = self;
        let child = child
            .as_mut()
            .expect("the server is retained after readiness");
        let gateway = tokio::select! {
            configured = inputs.configure_gateway(client) => configured?,
            exited = Self::exited(child) => return exited,
        };
        if !inputs.configured || !hosts.is_empty() {
            tokio::select! {
                offered = mcp::offer_local(hosts, endpoint, inputs.interactive) => offered?,
                exited = Self::exited(child) => return exited,
            }
        }
        let files = Environment::Process
            .config_dir()
            .map_or_else(String::new, |dir| dir.display().to_string());
        println!(
            "Local Wyrd ready at {endpoint} for tenant {}.\n  gRPC: http://{}\n  Storage: {}\n  Bifrost data: {}\n  Saved settings: {files}/config.toml ([client], [dev]); key and database URLs: {files}/credentials.toml\n  Gateway: {gateway}\nFresh SDK and CLI processes use these saved settings without exporting a key or endpoint. Press Ctrl-C to stop.",
            inputs.tenant,
            inputs.grpc,
            inputs.storage_url,
            inputs.bifrost.display(),
        );
        Self::exited(child).await
    }

    /// Save the resolved settings and database URLs through the protected
    /// configuration owner once migration has proven the database.
    ///
    /// # Errors
    /// Refuses unsafe or unwritable configuration before any key is issued.
    fn save_settings(&self) -> Result<(), WyrdCliError> {
        let inputs = &self.inputs;
        GlobalConfig::save_local_dev(
            &Environment::Process,
            &DevSection {
                tenant: Some(inputs.tenant.to_string()),
                bind: Some(inputs.bind.to_string()),
                grpc_bind: Some(inputs.grpc.to_string()),
                storage_url: Some(inputs.storage_url.clone()),
                bifrost_data_dir: Some(inputs.bifrost.display().to_string()),
            },
            &inputs.database,
            inputs.platform_database.as_ref(),
        )
        .map_err(|_| dev_error("local persistence", "local settings could not be saved; restore safe permissions on the Wyrd config directory and rerun. No tenant key has been issued"))
    }

    /// Commit the issued, saved, or recovered key before its endpoints
    /// through the existing file owners.
    ///
    /// An active tenant whose one-time key was never saved needs platform
    /// recovery: only an interactive run holding `WYRD_PLATFORM_CREDENTIAL`
    /// that confirms rotation calls the existing platform recovery. Every
    /// other run fails with that guidance; nothing is reissued implicitly.
    ///
    /// # Errors
    /// Refuses conflicting keys, unsafe persistence, and a lost key without
    /// confirmed authority; committed setup remains.
    async fn save_tenant(&self, tenant: &ProvisionedTenant) -> Result<(), WyrdCliError> {
        let saved = match (
            GlobalConfig::local_api_key(&Environment::Process).map_err(map_client_error)?,
            &tenant.key,
        ) {
            (None, None) => Some(self.recover(tenant).await?),
            (saved, _) => saved,
        };
        let candidate = tenant.credential(saved.as_ref())?;
        println!(
            "Tenant {} ({}) is active; saving local client configuration.",
            self.inputs.tenant, tenant.id
        );
        GlobalConfig::save_local(
            &Environment::Process,
            &self.endpoint,
            &format!("http://{}", self.inputs.grpc),
            candidate,
        )
        .map_err(|_| dev_error("local persistence", "the tenant may already be active; restore safe local file permissions and rerun. If its one-time key was not saved, use explicit platform tenant recover-admin; rerunning never replaces it"))
    }

    /// Rotate a lost tenant-admin key through the existing platform recovery
    /// after explicit terminal confirmation with `WYRD_PLATFORM_CREDENTIAL`.
    ///
    /// # Errors
    /// Refuses without a terminal, platform credential, or confirmation, and
    /// returns the platform's refusal.
    async fn recover(&self, tenant: &ProvisionedTenant) -> Result<SecretString, WyrdCliError> {
        #[cfg(unix)]
        if self.inputs.interactive && secret_var("WYRD_PLATFORM_CREDENTIAL").is_some() {
            let answer = Terminal::open()
                .map_err(|source| WyrdCliError::Io { source })?
                .ask(&format!(
                    "Tenant {} is active but its one-time key was not saved. Rotate its administrative key with WYRD_PLATFORM_CREDENTIAL? [y/N]: ",
                    self.inputs.tenant
                ))
                .await
                .map_err(|source| WyrdCliError::Io { source })?;
            if answer.trim().eq_ignore_ascii_case("y") {
                let server = url::Url::parse(&self.endpoint)
                    .map_err(|_| dev_error("recovery", "the local endpoint is not a URL"))?;
                let recovered = PlatformEndpoint { server }
                    .connect()
                    .await?
                    .recover_tenant_admin(tenant.id)
                    .await
                    .map_err(|source| WyrdCliError::Server { source })?;
                return Ok(SecretString::from(recovered.credential.expose().to_owned()));
            }
        }
        Err(dev_error(
            "saved credential",
            "the tenant is active but its one-time key was not saved and nothing was reissued; rerun on a terminal with WYRD_PLATFORM_CREDENTIAL set and confirm rotation, or use wyrd platform tenant recover-admin",
        ))
    }

    /// Reserve HTTP through migration, then start and await the ordinary server.
    ///
    /// # Errors
    /// Refuses occupied listeners, invalid memory input, migration, or readiness.
    /// Cancellation leaves committed migrations intact and the child owned here.
    async fn start(&mut self) -> Result<(), WyrdCliError> {
        let reservation = std::net::TcpListener::bind(self.inputs.bind)
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
    /// The reaped status stays cached on `child`, so `stop` still settles it.
    ///
    /// # Errors
    /// Returns redacted recovery guidance if waiting fails or the server exits.
    async fn exited(child: &mut Child) -> Result<ExitCode, WyrdCliError> {
        child.wait().await.map_err(|_| {
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
    ///
    /// # Panics
    /// Panics if called before `start` has spawned and retained the child.
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

/// Select one listener from the resolved `value`, else the ordinary
/// loopback `default`.
///
/// # Errors
/// Returns `refusal` for an unparsable, non-loopback, or zero-port address.
fn loopback(
    value: Option<String>,
    default: &str,
    refusal: &'static str,
) -> Result<SocketAddr, WyrdCliError> {
    value
        .as_deref()
        .unwrap_or(default)
        .parse::<SocketAddr>()
        .ok()
        .filter(|bind| bind.ip().is_loopback() && bind.port() != 0)
        .ok_or_else(|| dev_error("input", refusal))
}

/// Builds a deployment's authentication naming its credential.
type ProviderAuth = fn(&str) -> serde_json::Value;

/// Known provider keys the gateway offer detects: provider (also its
/// adapter and default deployment name), variables in precedence order, and
/// the deployment authentication naming its credential.
const PROVIDERS: [(&str, &[&str], ProviderAuth); 4] = [
    (
        "openai",
        &["OPENAI_API_KEY"],
        |credential| serde_json::json!({"bearer": {"credential": credential}}),
    ),
    (
        "anthropic",
        &["ANTHROPIC_API_KEY"],
        |credential| serde_json::json!({"api_key_header": {"header": "x-api-key", "credential": credential}}),
    ),
    (
        "gemini",
        &["GOOGLE_API_KEY", "GEMINI_API_KEY"],
        |credential| serde_json::json!({"api_key_header": {"header": "x-goog-api-key", "credential": credential}}),
    ),
    (
        "jev",
        &["TYPESAFE_API_KEY"],
        |credential| serde_json::json!({"bearer": {"credential": credential}}),
    ),
];

impl DevInputs {
    /// Apply the explicitly requested gateway document, or on a fresh
    /// interactive setup the approved offer, through the same atomic owner as
    /// `wyrd gateway deployment put`, then confirm every applied deployment
    /// reads back through the redacted administration surface.
    ///
    /// Returns the summary line. No provider is called; key validity is not
    /// claimed.
    ///
    /// # Errors
    /// Returns terminal failures, an invalid offered document, the apply
    /// error, and a failed read-back. A refusal applies nothing, but an error
    /// can also follow a commit whose response was lost, so the failure is
    /// reported as uncertain; reapplying the same named document is safe.
    async fn configure_gateway(&self, client: WyrdClient) -> Result<String, WyrdCliError> {
        let offered;
        let apply = match &self.gateway {
            Some(apply) => apply,
            None if self.offer_gateway => match offer_gateway().await? {
                Some(apply) => {
                    offered = apply;
                    &offered
                }
                None => return Ok("skipped".to_owned()),
            },
            None => {
                return Ok(
                    "skipped; configure it later with wyrd gateway deployment put --file"
                        .to_owned(),
                );
            }
        };
        let gateway = Gateway::connect_with(&client).await?;
        let names: Vec<String> = apply
            .deployments
            .iter()
            .map(|deployment| deployment.name.to_string())
            .collect();
        if let Err(source) = apply_deployments_with(client, apply).await {
            eprintln!(
                "Gateway setup did not complete; tenant setup remains. A refused document applies nothing, but if the response was lost the gateway changes may have committed. Inspect them with wyrd gateway deployment list once the server is running, then rerun: reapplying the same named document is safe. Or pass --skip-gateway."
            );
            return Err(WyrdCliError::Server { source });
        }
        let committed = gateway.deployments().await?;
        if !names.iter().all(|name| {
            committed
                .iter()
                .any(|deployment| deployment.name.as_str() == name)
        }) {
            return Err(dev_error(
                "gateway",
                "the applied deployments could not be read back; inspect wyrd gateway deployment list and rerun",
            ));
        }
        Ok(format!("configured deployments {}", names.join(", ")))
    }
}

/// Offer detected provider keys on the terminal and build the approved
/// composite document with `local_env` credentials, resolved exactly as a
/// `--gateway-file` document. Only names are shown, never values.
///
/// Returns `None` when no key is detected or the offer is declined.
///
/// # Errors
/// Returns terminal failures and an invalid resulting document.
#[cfg(unix)]
async fn offer_gateway() -> Result<Option<ProviderDeploymentsApply>, WyrdCliError> {
    let io = |source| WyrdCliError::Io { source };
    let detected: Vec<(&str, &str, ProviderAuth)> = PROVIDERS
        .iter()
        .filter_map(|(provider, variables, auth)| {
            variables
                .iter()
                .find(|variable| secret_var(variable).is_some())
                .map(|variable| (*provider, *variable, *auth))
        })
        .collect();
    if detected.is_empty() {
        println!(
            "No provider key variables (OPENAI_API_KEY, ANTHROPIC_API_KEY, GOOGLE_API_KEY or GEMINI_API_KEY, TYPESAFE_API_KEY) are set; gateway setup skipped."
        );
        return Ok(None);
    }
    let terminal = Terminal::open().map_err(io)?;
    println!("Detected provider keys:");
    for (index, (provider, variable, _)) in detected.iter().enumerate() {
        println!(
            "  {}) {provider} from {variable} as deployment {provider}",
            index + 1
        );
    }
    let chosen = loop {
        let answer = terminal
            .ask("Configure which providers in the gateway? [numbers, all, or empty for none]: ")
            .await
            .map_err(io)?;
        if let Some(chosen) = mcp::selected(&detected, &answer) {
            break chosen;
        }
        println!("Enter numbers between 1 and {}.", detected.len());
    };
    if chosen.is_empty() {
        println!("No provider selected; gateway setup skipped.");
        return Ok(None);
    }
    let (mut credentials, mut deployments) = (Vec::new(), Vec::new());
    for (provider, variable, auth) in chosen {
        let name = terminal
            .ask(&format!("Deployment name for {provider} [{provider}]: "))
            .await
            .map_err(io)?;
        let name = Some(name.trim())
            .filter(|name| !name.is_empty())
            .unwrap_or(provider);
        let credential = format!("{provider}-key");
        credentials.push(serde_json::json!({"name": credential, "provider": provider, "source": {"local_env": variable}}));
        deployments.push(
            serde_json::json!({"name": name, "adapter": provider, "auth": auth(&credential)}),
        );
    }
    let document = serde_json::json!({"credentials": credentials, "deployments": deployments});
    let apply = serde_json::from_value::<AuthoringApply>(document)
        .map_err(|_| dev_error("gateway", "a deployment name is invalid; use 3-64 lowercase letters, digits, '_' or '-', starting with a letter"))?
        .resolve(&Environment::Process)
        .map_err(|_| dev_error("gateway", "the selected provider keys did not form a valid gateway document; rerun or pass --skip-gateway"))?;
    let names: Vec<String> = apply
        .deployments
        .iter()
        .map(|deployment| deployment.name.to_string())
        .collect();
    let confirm = terminal
        .ask(&format!(
            "Apply deployments {} (an existing deployment or credential with the same name is replaced)? [Y/n]: ",
            names.join(", ")
        ))
        .await
        .map_err(io)?;
    Ok((!confirm.trim().eq_ignore_ascii_case("n")).then_some(apply))
}

/// Without a terminal reader there is nothing to offer.
///
/// # Errors
/// Never fails.
#[cfg(not(unix))]
async fn offer_gateway() -> Result<Option<ProviderDeploymentsApply>, WyrdCliError> {
    Ok(None)
}

/// Assemble a client from fresh saved-file inputs only, authenticate it, and
/// perform a bounded read before returning it for gateway setup.
///
/// # Errors
/// Returns redacted guidance for unusable configuration or credentials.
async fn saved_client() -> Result<WyrdClient, WyrdCliError> {
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
    Ok(client)
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
