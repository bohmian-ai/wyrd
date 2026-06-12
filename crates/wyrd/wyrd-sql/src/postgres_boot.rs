//! Postgres boot primitives for Wyrd server-tier storage.

mod role_bootstrap;

use std::env;
use std::fmt;
use std::fs;
use std::io;
use std::net::TcpListener;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use pg_embed::pg_enums::PgAuthMethod;
use pg_embed::pg_fetch::{PG_V17, PgFetchSettings};
use pg_embed::postgres::{PgEmbed, PgSettings};
use rand::distr::{Alphanumeric, SampleString};
use secrecy::{ExposeSecret, SecretString};
use sqlx::AssertSqlSafe;
use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};

use role_bootstrap::{WYRD_DATABASE, role_bootstrap_sql};

/// Runtime application DSN environment variable.
pub const APP_DSN_ENV: &str = "WYRD_DATABASE_URL";
/// Boot-only migrator DSN environment variable.
pub const MIGRATOR_DSN_ENV: &str = "WYRD_DATABASE_URL_MIGRATOR";
/// Optional cross-tenant platform-admin DSN environment variable.
pub const PLATFORM_ADMIN_DSN_ENV: &str = "WYRD_DATABASE_URL_PLATFORM_ADMIN";

const XDG_DATA_HOME_ENV: &str = "XDG_DATA_HOME";
const HOME_ENV: &str = "HOME";
const GENERATED_PASSWORD_LEN: usize = 48;
const WYRD_CONFIG_BEGIN: &str = "# BEGIN WYRD EMBEDDED CONFIG";
const WYRD_CONFIG_END: &str = "# END WYRD EMBEDDED CONFIG";

/// Postgres boot errors.
#[derive(Debug, thiserror::Error)]
pub enum BootError {
    /// DSN environment variables were partially configured.
    #[error(
        "mixed database DSN configuration: set both WYRD_DATABASE_URL and \
         WYRD_DATABASE_URL_MIGRATOR for external Postgres, optionally set \
         WYRD_DATABASE_URL_PLATFORM_ADMIN, or leave all three unset for embedded mode"
    )]
    MixedDsnConfig,
    /// Embedded Postgres startup failed.
    #[error("embedded Postgres startup failed")]
    Embedded(#[source] pg_embed::pg_errors::Error),
    /// Embedded Postgres filesystem setup failed.
    #[error("embedded Postgres filesystem setup failed: {0}")]
    EmbeddedIo(#[source] io::Error),
    /// Database pool construction failed.
    #[error("database pool construction failed")]
    PoolConnect(#[source] sqlx::Error),
}

/// Resolved role-specific Postgres DSNs.
///
/// DSNs are secret-bearing because they normally include role passwords.
#[derive(Clone)]
pub struct ResolvedDsns {
    /// Runtime `wyrd_app` DSN. RLS applies to connections built from this DSN.
    pub app: SecretString,
    /// Boot-only `wyrd_migrator` DSN. This pool must be closed after migrations.
    pub migrator: SecretString,
    /// Optional audited cross-tenant `wyrd_platform_admin` DSN.
    pub platform_admin: Option<SecretString>,
}

impl fmt::Debug for ResolvedDsns {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResolvedDsns")
            .field("app", &"<redacted>")
            .field("migrator", &"<redacted>")
            .field(
                "platform_admin",
                &self.platform_admin.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

/// Postgres boot mode selected from deploy configuration.
pub enum PostgresBoot {
    /// External Postgres DSNs supplied by deploy configuration.
    External {
        /// Runtime `wyrd_app` DSN.
        app_dsn: SecretString,
        /// Boot-only `wyrd_migrator` DSN.
        migrator_dsn: SecretString,
        /// Optional audited `wyrd_platform_admin` DSN.
        platform_admin_dsn: Option<SecretString>,
    },
    /// Embedded Postgres handle.
    Embedded(EmbeddedPgHandle),
}

impl fmt::Debug for PostgresBoot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::External {
                platform_admin_dsn, ..
            } => f
                .debug_struct("PostgresBoot::External")
                .field("app_dsn", &"<redacted>")
                .field("migrator_dsn", &"<redacted>")
                .field(
                    "platform_admin_dsn",
                    &platform_admin_dsn.as_ref().map(|_| "<redacted>"),
                )
                .finish(),
            Self::Embedded(handle) => f
                .debug_tuple("PostgresBoot::Embedded")
                .field(handle)
                .finish(),
        }
    }
}

impl PostgresBoot {
    /// Resolve Postgres boot mode from Wyrd database DSN environment variables.
    ///
    /// External mode requires `WYRD_DATABASE_URL` and
    /// `WYRD_DATABASE_URL_MIGRATOR`; `WYRD_DATABASE_URL_PLATFORM_ADMIN` is
    /// optional. All three unset starts embedded Postgres, provisions roles,
    /// and derives the three role DSNs from the managed instance.
    ///
    /// # Errors
    /// Returns [`BootError::MixedDsnConfig`] for partial or incoherent DSN
    /// configuration. Returns embedded startup or filesystem errors when
    /// embedded mode is selected and the managed instance cannot be started.
    pub async fn from_env() -> Result<Self, BootError> {
        Self::from_optional_dsns(
            env::var(APP_DSN_ENV).ok(),
            env::var(MIGRATOR_DSN_ENV).ok(),
            env::var(PLATFORM_ADMIN_DSN_ENV).ok(),
        )
        .await
    }

    /// Start embedded Postgres with an explicit configuration.
    ///
    /// # Errors
    /// Returns an error when credentials cannot be persisted, pg_embed cannot
    /// initialize or start Postgres, the `wyrd` database cannot be created, or
    /// role bootstrap fails.
    pub async fn embedded(config: EmbeddedConfig) -> Result<Self, BootError> {
        let handle = EmbeddedPgHandle::start(config).await?;
        Ok(Self::Embedded(handle))
    }

    /// Return resolved role DSNs for the selected boot mode.
    ///
    /// # Errors
    /// This method is infallible for active boot modes; it returns a
    /// `Result` so future embedded handle failures can remain typed without an
    /// API break.
    pub fn dsns(&self) -> Result<ResolvedDsns, BootError> {
        match self {
            Self::External {
                app_dsn,
                migrator_dsn,
                platform_admin_dsn,
            } => Ok(ResolvedDsns {
                app: app_dsn.clone(),
                migrator: migrator_dsn.clone(),
                platform_admin: platform_admin_dsn.clone(),
            }),
            Self::Embedded(handle) => handle.resolved_dsns(),
        }
    }

    async fn from_optional_dsns(
        app: Option<String>,
        migrator: Option<String>,
        platform_admin: Option<String>,
    ) -> Result<Self, BootError> {
        match (app, migrator, platform_admin) {
            (Some(app_dsn), Some(migrator_dsn), platform_admin_dsn) => Ok(Self::External {
                app_dsn: SecretString::from(app_dsn),
                migrator_dsn: SecretString::from(migrator_dsn),
                platform_admin_dsn: platform_admin_dsn.map(SecretString::from),
            }),
            (None, None, None) => Self::embedded(EmbeddedConfig::default()).await,
            _ => Err(BootError::MixedDsnConfig),
        }
    }
}

/// Handle for a managed embedded Postgres instance.
pub struct EmbeddedPgHandle {
    config: EmbeddedConfig,
    credentials: EmbeddedRoleCredentials,
    port: u16,
    pg: Box<PgEmbed>,
}

impl fmt::Debug for EmbeddedPgHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EmbeddedPgHandle")
            .field("config", &self.config)
            .field("credentials", &"<redacted>")
            .field("port", &self.port)
            .finish_non_exhaustive()
    }
}

impl EmbeddedPgHandle {
    async fn start(mut config: EmbeddedConfig) -> Result<Self, BootError> {
        let dirs = config.data_dirs();
        let credentials = EmbeddedRoleCredentials::load_or_create(&dirs, &config)?;
        let port = resolve_port(config.port)?;
        config.port = port;

        let pg_settings = PgSettings {
            database_dir: dirs.data.clone(),
            port,
            user: config.superuser.clone(),
            password: credentials.superuser.expose_secret().to_owned(),
            auth_method: PgAuthMethod::MD5,
            persistent: true,
            timeout: Some(Duration::from_secs(30)),
            migration_dir: None,
        };
        let fetch_settings = PgFetchSettings {
            version: PG_V17,
            ..Default::default()
        };

        let mut pg = PgEmbed::new(pg_settings, fetch_settings)
            .await
            .map_err(BootError::Embedded)?;
        pg.setup().await.map_err(BootError::Embedded)?;
        write_embedded_postgres_config(&dirs, config.max_connections)?;
        pg.start_db().await.map_err(BootError::Embedded)?;

        write_pidfile(&dirs)?;
        bootstrap_embedded_database(port, &config.superuser, &credentials).await?;

        Ok(Self {
            config,
            credentials,
            port,
            pg: Box::new(pg),
        })
    }

    /// Borrow the embedded configuration.
    #[must_use]
    pub fn config(&self) -> &EmbeddedConfig {
        &self.config
    }

    /// Return embedded-mode DSNs.
    pub fn resolved_dsns(&self) -> Result<ResolvedDsns, BootError> {
        let app = embedded_dsn(
            role_bootstrap::WYRD_APP_ROLE,
            self.credentials.app.expose_secret(),
            self.port,
            WYRD_DATABASE,
        );
        let migrator = embedded_dsn(
            role_bootstrap::WYRD_MIGRATOR_ROLE,
            self.credentials.migrator.expose_secret(),
            self.port,
            WYRD_DATABASE,
        );
        let platform_admin = embedded_dsn(
            role_bootstrap::WYRD_PLATFORM_ADMIN_ROLE,
            self.credentials.platform_admin.expose_secret(),
            self.port,
            WYRD_DATABASE,
        );

        Ok(ResolvedDsns {
            app: SecretString::from(app),
            migrator: SecretString::from(migrator),
            platform_admin: Some(SecretString::from(platform_admin)),
        })
    }

    /// Borrow the underlying `pg-embed` handle.
    #[must_use]
    pub fn pg(&self) -> &PgEmbed {
        &self.pg
    }

    /// Borrow the port selected for embedded Postgres.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }
}

/// Embedded Postgres configuration.
#[derive(Clone)]
pub struct EmbeddedConfig {
    /// Root data directory, defaulting to `$XDG_DATA_HOME/wyrd/pg`.
    pub data_dir: PathBuf,
    /// Requested port. `0` means an ephemeral port.
    pub port: u16,
    /// Cluster superuser name.
    pub superuser: String,
    /// Optional superuser password. When unset, a per-data-dir password is
    /// generated and persisted.
    pub superuser_password: Option<SecretString>,
    /// Embedded cluster `max_connections` setting.
    pub max_connections: u32,
}

impl Default for EmbeddedConfig {
    fn default() -> Self {
        Self {
            data_dir: default_embedded_data_dir(),
            port: 0,
            superuser: "wyrd".to_owned(),
            superuser_password: None,
            max_connections: 100,
        }
    }
}

impl fmt::Debug for EmbeddedConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EmbeddedConfig")
            .field("data_dir", &self.data_dir)
            .field("port", &self.port)
            .field("superuser", &self.superuser)
            .field(
                "superuser_password",
                &self.superuser_password.as_ref().map(|_| "<redacted>"),
            )
            .field("max_connections", &self.max_connections)
            .finish()
    }
}

impl EmbeddedConfig {
    /// Return the filesystem layout used by embedded mode.
    #[must_use]
    pub fn data_dirs(&self) -> EmbeddedDataDirs {
        EmbeddedDataDirs::new(self.data_dir.clone())
    }
}

/// Embedded Postgres data-directory layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedDataDirs {
    /// Root directory for the managed embedded cluster.
    pub root: PathBuf,
    /// Postgres `PGDATA` directory.
    pub data: PathBuf,
    /// Process lock or pid file.
    pub lock_file: PathBuf,
    /// Persisted cluster superuser secret path.
    pub superuser_secret: PathBuf,
    /// Directory for generated role credentials.
    pub role_credentials: PathBuf,
    /// Persisted `wyrd_migrator` password path.
    pub migrator_secret: PathBuf,
    /// Persisted `wyrd_app` password path.
    pub app_secret: PathBuf,
    /// Persisted `wyrd_platform_admin` password path.
    pub platform_admin_secret: PathBuf,
}

impl EmbeddedDataDirs {
    fn new(root: PathBuf) -> Self {
        let role_credentials = root.join("role-credentials");
        Self {
            data: root.join("data"),
            lock_file: root.join("pg.lock"),
            superuser_secret: root.join("superuser.secret"),
            migrator_secret: role_credentials.join("wyrd_migrator.secret"),
            app_secret: role_credentials.join("wyrd_app.secret"),
            platform_admin_secret: role_credentials.join("wyrd_platform_admin.secret"),
            role_credentials,
            root,
        }
    }
}

pub(crate) struct EmbeddedRoleCredentials {
    pub(crate) superuser: SecretString,
    pub(crate) migrator: SecretString,
    pub(crate) app: SecretString,
    pub(crate) platform_admin: SecretString,
}

impl EmbeddedRoleCredentials {
    fn load_or_create(dirs: &EmbeddedDataDirs, config: &EmbeddedConfig) -> Result<Self, BootError> {
        fs::create_dir_all(&dirs.root).map_err(BootError::EmbeddedIo)?;
        fs::create_dir_all(&dirs.role_credentials).map_err(BootError::EmbeddedIo)?;

        let superuser = match &config.superuser_password {
            Some(password) => {
                persist_secret_if_missing(&dirs.superuser_secret, password.expose_secret())?;
                password.clone()
            }
            None => read_or_create_secret(&dirs.superuser_secret)?,
        };

        Ok(Self {
            superuser,
            migrator: read_or_create_secret(&dirs.migrator_secret)?,
            app: read_or_create_secret(&dirs.app_secret)?,
            platform_admin: read_or_create_secret(&dirs.platform_admin_secret)?,
        })
    }
}

/// Role-specific Postgres pool configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolConfig {
    /// Maximum connections held by the pool.
    pub max_connections: u32,
    /// Minimum idle connections maintained by the pool.
    pub min_connections: u32,
    /// Maximum time to wait for an available connection.
    pub acquire_timeout: Duration,
    /// Idle connection timeout. `None` disables idle reaping.
    pub idle_timeout: Option<Duration>,
    /// Maximum connection lifetime. `None` disables lifetime recycling.
    pub max_lifetime: Option<Duration>,
    /// Per-connection SQLx statement cache capacity.
    pub statement_cache_capacity: usize,
    /// Whether SQLx tests a connection before handing it out.
    pub test_before_acquire: bool,
}

impl PoolConfig {
    /// Defaults for the runtime `wyrd_app` pool.
    #[must_use]
    pub fn app_defaults() -> Self {
        Self {
            max_connections: 32,
            min_connections: 2,
            acquire_timeout: Duration::from_secs(5),
            idle_timeout: Some(Duration::from_secs(300)),
            max_lifetime: Some(Duration::from_secs(1_800)),
            statement_cache_capacity: 256,
            test_before_acquire: true,
        }
    }

    /// Defaults for the boot-only `wyrd_migrator` pool.
    #[must_use]
    pub fn migrator_defaults() -> Self {
        Self {
            max_connections: 2,
            min_connections: 1,
            acquire_timeout: Duration::from_secs(10),
            idle_timeout: None,
            max_lifetime: None,
            statement_cache_capacity: 0,
            test_before_acquire: false,
        }
    }

    /// Defaults for the optional `wyrd_platform_admin` pool.
    #[must_use]
    pub fn platform_admin_defaults() -> Self {
        Self {
            max_connections: 2,
            min_connections: 0,
            acquire_timeout: Duration::from_secs(5),
            idle_timeout: Some(Duration::from_secs(60)),
            max_lifetime: Some(Duration::from_secs(900)),
            statement_cache_capacity: 64,
            test_before_acquire: true,
        }
    }

    /// Runtime pool config from `WYRD_DB_*` env vars.
    #[must_use]
    pub fn from_env() -> Self {
        Self::app_from_env()
    }

    /// Runtime pool config from `WYRD_DB_*` env vars.
    #[must_use]
    pub fn app_from_env() -> Self {
        Self::from_env_with_suffix(Self::app_defaults(), "")
    }

    /// Migrator pool config from `WYRD_DB_*_MIGRATOR` env vars.
    #[must_use]
    pub fn migrator_from_env() -> Self {
        Self::from_env_with_suffix(Self::migrator_defaults(), "_MIGRATOR")
    }

    /// Platform-admin pool config from `WYRD_DB_*_PLATFORM_ADMIN` env vars.
    #[must_use]
    pub fn platform_admin_from_env() -> Self {
        Self::from_env_with_suffix(Self::platform_admin_defaults(), "_PLATFORM_ADMIN")
    }

    fn from_env_with_suffix(defaults: Self, suffix: &str) -> Self {
        Self {
            max_connections: env_u32("WYRD_DB_MAX_CONNECTIONS", suffix, defaults.max_connections),
            min_connections: env_u32("WYRD_DB_MIN_CONNECTIONS", suffix, defaults.min_connections),
            acquire_timeout: env_secs(
                "WYRD_DB_ACQUIRE_TIMEOUT_SECS",
                suffix,
                defaults.acquire_timeout,
            ),
            idle_timeout: env_opt_secs("WYRD_DB_IDLE_TIMEOUT_SECS", suffix, defaults.idle_timeout),
            max_lifetime: env_opt_secs("WYRD_DB_MAX_LIFETIME_SECS", suffix, defaults.max_lifetime),
            statement_cache_capacity: env_usize(
                "WYRD_DB_STATEMENT_CACHE_CAPACITY",
                suffix,
                defaults.statement_cache_capacity,
            ),
            test_before_acquire: env_bool(
                "WYRD_DB_TEST_BEFORE_ACQUIRE",
                suffix,
                defaults.test_before_acquire,
            ),
        }
    }
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self::app_defaults()
    }
}

/// Build a Postgres pool with the supplied role-specific config.
///
/// # Errors
/// Returns [`BootError::PoolConnect`] when the DSN cannot be parsed or SQLx
/// cannot connect.
pub async fn build_pool(database_url: &str, config: PoolConfig) -> Result<PgPool, BootError> {
    connect_pool(database_url, config)
        .await
        .map_err(BootError::PoolConnect)
}

pub(crate) async fn connect_pool(
    database_url: &str,
    config: PoolConfig,
) -> Result<PgPool, sqlx::Error> {
    let options = PgConnectOptions::from_str(database_url)?
        .statement_cache_capacity(config.statement_cache_capacity);

    PgPoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .acquire_timeout(config.acquire_timeout)
        .idle_timeout(config.idle_timeout)
        .max_lifetime(config.max_lifetime)
        .test_before_acquire(config.test_before_acquire)
        .connect_with(options)
        .await
}

async fn bootstrap_embedded_database(
    port: u16,
    superuser: &str,
    credentials: &EmbeddedRoleCredentials,
) -> Result<(), BootError> {
    let postgres_dsn = embedded_dsn(
        superuser,
        credentials.superuser.expose_secret(),
        port,
        "postgres",
    );
    let postgres_pool = build_pool(&postgres_dsn, PoolConfig::migrator_defaults()).await?;

    let exists: (bool,) =
        sqlx::query_as("SELECT EXISTS (SELECT 1 FROM pg_database WHERE datname = $1)")
            .bind(WYRD_DATABASE)
            .fetch_one(&postgres_pool)
            .await
            .map_err(BootError::PoolConnect)?;
    if !exists.0 {
        sqlx::query("CREATE DATABASE wyrd")
            .execute(&postgres_pool)
            .await
            .map_err(BootError::PoolConnect)?;
    }
    postgres_pool.close().await;

    let wyrd_dsn = embedded_dsn(
        superuser,
        credentials.superuser.expose_secret(),
        port,
        WYRD_DATABASE,
    );
    let wyrd_pool = build_pool(&wyrd_dsn, PoolConfig::migrator_defaults()).await?;
    sqlx::raw_sql(AssertSqlSafe(role_bootstrap_sql(credentials)))
        .execute(&wyrd_pool)
        .await
        .map_err(BootError::PoolConnect)?;
    wyrd_pool.close().await;

    Ok(())
}

fn embedded_dsn(user: &str, password: &str, port: u16, database: &str) -> String {
    format!(
        "postgres://{}:{}@localhost:{}/{}",
        urlencoding::encode(user),
        urlencoding::encode(password),
        port,
        urlencoding::encode(database),
    )
}

fn resolve_port(configured: u16) -> Result<u16, BootError> {
    if configured != 0 {
        return Ok(configured);
    }

    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(BootError::EmbeddedIo)?;
    let port = listener.local_addr().map_err(BootError::EmbeddedIo)?.port();
    drop(listener);
    Ok(port)
}

fn write_pidfile(dirs: &EmbeddedDataDirs) -> Result<(), BootError> {
    fs::write(&dirs.lock_file, std::process::id().to_string()).map_err(BootError::EmbeddedIo)?;
    set_secret_permissions(&dirs.lock_file)
}

fn write_embedded_postgres_config(
    dirs: &EmbeddedDataDirs,
    max_connections: u32,
) -> Result<(), BootError> {
    let path = dirs.data.join("postgresql.conf");
    let existing = fs::read_to_string(&path).map_err(BootError::EmbeddedIo)?;
    let without_wyrd = remove_wyrd_config_block(&existing);
    let updated = format!(
        "{without_wyrd}\n{WYRD_CONFIG_BEGIN}\nmax_connections = {max_connections}\n{WYRD_CONFIG_END}\n"
    );

    fs::write(path, updated).map_err(BootError::EmbeddedIo)
}

fn remove_wyrd_config_block(input: &str) -> String {
    let mut output = Vec::new();
    let mut skipping = false;

    for line in input.lines() {
        match line {
            WYRD_CONFIG_BEGIN => skipping = true,
            WYRD_CONFIG_END => skipping = false,
            _ if !skipping => output.push(line),
            _ => {}
        }
    }

    output.join("\n").trim_end().to_owned()
}

fn read_or_create_secret(path: &PathBuf) -> Result<SecretString, BootError> {
    match fs::read_to_string(path) {
        Ok(value) => Ok(SecretString::from(
            value.trim_end_matches(['\r', '\n']).to_owned(),
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let value = generate_password();
            write_secret(path, &value)?;
            Ok(SecretString::from(value))
        }
        Err(error) => Err(BootError::EmbeddedIo(error)),
    }
}

fn persist_secret_if_missing(path: &PathBuf, value: &str) -> Result<(), BootError> {
    match fs::metadata(path) {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => write_secret(path, value),
        Err(error) => Err(BootError::EmbeddedIo(error)),
    }
}

fn write_secret(path: &PathBuf, value: &str) -> Result<(), BootError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(BootError::EmbeddedIo)?;
    }

    write_secret_file(path, value)?;
    set_secret_permissions(path)
}

#[cfg(unix)]
fn write_secret_file(path: &PathBuf, value: &str) -> Result<(), BootError> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(path)
        .map_err(BootError::EmbeddedIo)?;
    file.write_all(value.as_bytes())
        .map_err(BootError::EmbeddedIo)
}

#[cfg(not(unix))]
fn write_secret_file(path: &PathBuf, value: &str) -> Result<(), BootError> {
    fs::write(path, value).map_err(BootError::EmbeddedIo)
}

#[cfg(unix)]
fn set_secret_permissions(path: &PathBuf) -> Result<(), BootError> {
    use std::os::unix::fs::PermissionsExt;

    let permissions = fs::Permissions::from_mode(0o600);
    fs::set_permissions(path, permissions).map_err(BootError::EmbeddedIo)
}

#[cfg(not(unix))]
fn set_secret_permissions(_path: &PathBuf) -> Result<(), BootError> {
    Ok(())
}

fn generate_password() -> String {
    Alphanumeric.sample_string(&mut rand::rng(), GENERATED_PASSWORD_LEN)
}

fn env_name(base: &str, suffix: &str) -> String {
    format!("{base}{suffix}")
}

fn env_u32(base: &str, suffix: &str, default: u32) -> u32 {
    env::var(env_name(base, suffix))
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(default)
}

fn env_usize(base: &str, suffix: &str, default: usize) -> usize {
    env::var(env_name(base, suffix))
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
}

fn env_secs(base: &str, suffix: &str, default: Duration) -> Duration {
    env::var(env_name(base, suffix))
        .ok()
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
        .map(Duration::from_secs_f64)
        .unwrap_or(default)
}

fn env_opt_secs(base: &str, suffix: &str, default: Option<Duration>) -> Option<Duration> {
    env::var(env_name(base, suffix))
        .ok()
        .and_then(|value| {
            if value.eq_ignore_ascii_case("off") {
                Some(None)
            } else {
                value
                    .parse::<f64>()
                    .ok()
                    .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
                    .map(Duration::from_secs_f64)
                    .map(Some)
            }
        })
        .unwrap_or(default)
}

fn env_bool(base: &str, suffix: &str, default: bool) -> bool {
    env::var(env_name(base, suffix))
        .ok()
        .and_then(|value| match value.to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Some(true),
            "false" | "0" | "no" | "off" => Some(false),
            _ => None,
        })
        .unwrap_or(default)
}

fn default_embedded_data_dir() -> PathBuf {
    env::var_os(XDG_DATA_HOME_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os(HOME_ENV)
                .filter(|value| !value.is_empty())
                .map(|home| PathBuf::from(home).join(".local").join("share"))
        })
        .unwrap_or_else(|| PathBuf::from(".local").join("share"))
        .join("wyrd")
        .join("pg")
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::Duration;

    use secrecy::ExposeSecret;

    use super::{
        APP_DSN_ENV, BootError, EmbeddedConfig, EmbeddedRoleCredentials, MIGRATOR_DSN_ENV,
        PLATFORM_ADMIN_DSN_ENV, PoolConfig, PostgresBoot, write_embedded_postgres_config,
    };

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    const APP_DSN: &str = "postgres://wyrd_app:app-secret@localhost/wyrd";
    const MIGRATOR_DSN: &str = "postgres://wyrd_migrator:migrator-secret@localhost/wyrd";
    const ADMIN_DSN: &str = "postgres://wyrd_platform_admin:admin-secret@localhost/wyrd";

    #[tokio::test]
    async fn external_with_app_and_migrator_allows_dedicated_mode() {
        let boot = PostgresBoot::from_optional_dsns(
            Some(APP_DSN.to_owned()),
            Some(MIGRATOR_DSN.to_owned()),
            None,
        )
        .await
        .expect("external boot resolves");
        let dsns = boot.dsns().expect("external dsns resolve");

        assert_eq!(dsns.app.expose_secret(), APP_DSN);
        assert_eq!(dsns.migrator.expose_secret(), MIGRATOR_DSN);
        assert!(dsns.platform_admin.is_none());
    }

    #[tokio::test]
    async fn external_with_all_three_dsns_resolves() {
        let boot = PostgresBoot::from_optional_dsns(
            Some(APP_DSN.to_owned()),
            Some(MIGRATOR_DSN.to_owned()),
            Some(ADMIN_DSN.to_owned()),
        )
        .await
        .expect("external boot resolves");
        let dsns = boot.dsns().expect("external dsns resolve");

        assert_eq!(dsns.app.expose_secret(), APP_DSN);
        assert_eq!(dsns.migrator.expose_secret(), MIGRATOR_DSN);
        assert_eq!(
            dsns.platform_admin
                .as_ref()
                .expect("admin dsn resolves")
                .expose_secret(),
            ADMIN_DSN
        );
    }

    #[tokio::test]
    async fn mixed_dsn_presence_is_rejected() {
        for (app, migrator, admin) in [
            (Some(APP_DSN), None, None),
            (None, Some(MIGRATOR_DSN), None),
            (Some(APP_DSN), None, Some(ADMIN_DSN)),
            (None, Some(MIGRATOR_DSN), Some(ADMIN_DSN)),
            (None, None, Some(ADMIN_DSN)),
        ] {
            let result = PostgresBoot::from_optional_dsns(
                app.map(str::to_owned),
                migrator.map(str::to_owned),
                admin.map(str::to_owned),
            )
            .await;

            assert!(matches!(result, Err(BootError::MixedDsnConfig)));
        }
    }

    #[test]
    fn debug_output_redacts_secret_dsns() {
        let boot = PostgresBoot::External {
            app_dsn: APP_DSN.to_owned().into(),
            migrator_dsn: MIGRATOR_DSN.to_owned().into(),
            platform_admin_dsn: Some(ADMIN_DSN.to_owned().into()),
        };
        let rendered = format!("{boot:?}");

        assert!(rendered.contains("<redacted>"));
        assert!(!rendered.contains("app-secret"));
        assert!(!rendered.contains("migrator-secret"));
        assert!(!rendered.contains("admin-secret"));
    }

    #[test]
    fn embedded_layout_matches_locked_paths() {
        let config = EmbeddedConfig {
            data_dir: "/tmp/wyrd/pg".into(),
            ..EmbeddedConfig::default()
        };
        let dirs = config.data_dirs();

        assert_eq!(dirs.data, PathBuf::from("/tmp/wyrd/pg/data"));
        assert_eq!(dirs.lock_file, PathBuf::from("/tmp/wyrd/pg/pg.lock"));
        assert_eq!(
            dirs.superuser_secret,
            PathBuf::from("/tmp/wyrd/pg/superuser.secret")
        );
        assert_eq!(
            dirs.migrator_secret,
            PathBuf::from("/tmp/wyrd/pg/role-credentials/wyrd_migrator.secret")
        );
        assert_eq!(
            dirs.app_secret,
            PathBuf::from("/tmp/wyrd/pg/role-credentials/wyrd_app.secret")
        );
        assert_eq!(
            dirs.platform_admin_secret,
            PathBuf::from("/tmp/wyrd/pg/role-credentials/wyrd_platform_admin.secret")
        );
    }

    #[test]
    fn embedded_credentials_are_created_and_reused() {
        let root = test_dir("embedded-credentials");
        let _ = std::fs::remove_dir_all(&root);
        let config = EmbeddedConfig {
            data_dir: root.join("pg"),
            ..EmbeddedConfig::default()
        };
        let dirs = config.data_dirs();

        let first =
            EmbeddedRoleCredentials::load_or_create(&dirs, &config).expect("credentials create");
        let first_app = first.app.expose_secret().to_owned();
        let first_migrator = first.migrator.expose_secret().to_owned();
        let first_admin = first.platform_admin.expose_secret().to_owned();

        let second =
            EmbeddedRoleCredentials::load_or_create(&dirs, &config).expect("credentials reload");

        assert_eq!(second.app.expose_secret(), &first_app);
        assert_eq!(second.migrator.expose_secret(), &first_migrator);
        assert_eq!(second.platform_admin.expose_secret(), &first_admin);
        assert!(dirs.superuser_secret.exists());
        assert!(dirs.migrator_secret.exists());
        assert!(dirs.app_secret.exists());
        assert!(dirs.platform_admin_secret.exists());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn embedded_secret_files_are_0600_on_unix() {
        use std::os::unix::fs::PermissionsExt;

        let root = test_dir("embedded-permissions");
        let _ = std::fs::remove_dir_all(&root);
        let config = EmbeddedConfig {
            data_dir: root.join("pg"),
            ..EmbeddedConfig::default()
        };
        let dirs = config.data_dirs();

        EmbeddedRoleCredentials::load_or_create(&dirs, &config).expect("credentials create");
        let mode = std::fs::metadata(&dirs.app_secret)
            .expect("app secret metadata")
            .permissions()
            .mode()
            & 0o777;

        assert_eq!(mode, 0o600);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn embedded_postgres_config_block_is_idempotent() {
        let root = test_dir("embedded-config");
        let _ = std::fs::remove_dir_all(&root);
        let config = EmbeddedConfig {
            data_dir: root.join("pg"),
            ..EmbeddedConfig::default()
        };
        let dirs = config.data_dirs();
        std::fs::create_dir_all(&dirs.data).expect("data dir creates");
        std::fs::write(
            dirs.data.join("postgresql.conf"),
            "# base config\n# BEGIN WYRD EMBEDDED CONFIG\nmax_connections = 20\n# END WYRD EMBEDDED CONFIG\n",
        )
        .expect("config writes");

        write_embedded_postgres_config(&dirs, 100).expect("config updates");

        let updated =
            std::fs::read_to_string(dirs.data.join("postgresql.conf")).expect("config reads");
        assert!(updated.contains("# base config"));
        assert!(updated.contains("max_connections = 100"));
        assert!(!updated.contains("max_connections = 20"));
        assert_eq!(updated.matches("BEGIN WYRD EMBEDDED CONFIG").count(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pool_defaults_match_locked_values() {
        assert_eq!(
            PoolConfig::app_defaults(),
            PoolConfig {
                max_connections: 32,
                min_connections: 2,
                acquire_timeout: Duration::from_secs(5),
                idle_timeout: Some(Duration::from_secs(300)),
                max_lifetime: Some(Duration::from_secs(1_800)),
                statement_cache_capacity: 256,
                test_before_acquire: true,
            }
        );
        assert_eq!(
            PoolConfig::migrator_defaults(),
            PoolConfig {
                max_connections: 2,
                min_connections: 1,
                acquire_timeout: Duration::from_secs(10),
                idle_timeout: None,
                max_lifetime: None,
                statement_cache_capacity: 0,
                test_before_acquire: false,
            }
        );
        assert_eq!(
            PoolConfig::platform_admin_defaults(),
            PoolConfig {
                max_connections: 2,
                min_connections: 0,
                acquire_timeout: Duration::from_secs(5),
                idle_timeout: Some(Duration::from_secs(60)),
                max_lifetime: Some(Duration::from_secs(900)),
                statement_cache_capacity: 64,
                test_before_acquire: true,
            }
        );
    }

    #[test]
    fn pool_env_overrides_use_role_suffixes() {
        let _guard = ENV_LOCK.lock().expect("env lock is not poisoned");
        let vars = [
            ("WYRD_DB_MAX_CONNECTIONS_PLATFORM_ADMIN", Some("4")),
            ("WYRD_DB_MIN_CONNECTIONS_PLATFORM_ADMIN", Some("1")),
            ("WYRD_DB_ACQUIRE_TIMEOUT_SECS_PLATFORM_ADMIN", Some("2.5")),
            ("WYRD_DB_IDLE_TIMEOUT_SECS_PLATFORM_ADMIN", Some("off")),
            ("WYRD_DB_MAX_LIFETIME_SECS_PLATFORM_ADMIN", Some("30")),
            (
                "WYRD_DB_STATEMENT_CACHE_CAPACITY_PLATFORM_ADMIN",
                Some("12"),
            ),
            ("WYRD_DB_TEST_BEFORE_ACQUIRE_PLATFORM_ADMIN", Some("false")),
        ];
        with_env(&vars, || {
            let cfg = PoolConfig::platform_admin_from_env();

            assert_eq!(cfg.max_connections, 4);
            assert_eq!(cfg.min_connections, 1);
            assert_eq!(cfg.acquire_timeout, Duration::from_millis(2_500));
            assert_eq!(cfg.idle_timeout, None);
            assert_eq!(cfg.max_lifetime, Some(Duration::from_secs(30)));
            assert_eq!(cfg.statement_cache_capacity, 12);
            assert!(!cfg.test_before_acquire);
        });
    }

    #[tokio::test(flavor = "current_thread")]
    async fn from_env_uses_wyrd_database_url_names() {
        let previous = {
            let _guard = ENV_LOCK.lock().expect("env lock is not poisoned");
            let previous = snapshot_env(&[APP_DSN_ENV, MIGRATOR_DSN_ENV, PLATFORM_ADMIN_DSN_ENV]);
            set_env(APP_DSN_ENV, Some(APP_DSN));
            set_env(MIGRATOR_DSN_ENV, Some(MIGRATOR_DSN));
            set_env(PLATFORM_ADMIN_DSN_ENV, None);
            previous
        };

        let boot = PostgresBoot::from_env()
            .await
            .expect("external env resolves");

        {
            let _guard = ENV_LOCK.lock().expect("env lock is not poisoned");
            restore_env(previous);
        }

        assert!(matches!(boot, PostgresBoot::External { .. }));
    }

    use std::path::PathBuf;

    fn with_env(vars: &[(&str, Option<&str>)], f: impl FnOnce()) {
        let previous = snapshot_env(&vars.iter().map(|(name, _)| *name).collect::<Vec<_>>());
        for (name, value) in vars {
            set_env(name, *value);
        }
        f();
        restore_env(previous);
    }

    fn snapshot_env(names: &[&str]) -> Vec<(String, Option<std::ffi::OsString>)> {
        names
            .iter()
            .map(|name| ((*name).to_owned(), std::env::var_os(name)))
            .collect()
    }

    fn restore_env(values: Vec<(String, Option<std::ffi::OsString>)>) {
        for (name, value) in values {
            match value {
                Some(value) => {
                    // SAFETY: ENV_LOCK serializes all environment mutations in this test module.
                    unsafe {
                        std::env::set_var(name, value);
                    }
                }
                None => {
                    // SAFETY: ENV_LOCK serializes all environment mutations in this test module.
                    unsafe {
                        std::env::remove_var(name);
                    }
                }
            }
        }
    }

    fn set_env(name: &str, value: Option<&str>) {
        match value {
            Some(value) => {
                // SAFETY: ENV_LOCK serializes all environment mutations in this test module.
                unsafe {
                    std::env::set_var(name, value);
                }
            }
            None => {
                // SAFETY: ENV_LOCK serializes all environment mutations in this test module.
                unsafe {
                    std::env::remove_var(name);
                }
            }
        }
    }

    fn test_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "wyrd-sql-{name}-{}-{}",
            std::process::id(),
            super::generate_password()
        ))
    }
}
