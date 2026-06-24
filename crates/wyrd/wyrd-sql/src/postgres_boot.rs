//! Postgres boot primitives for Wyrd server-tier storage.

mod role_bootstrap;

use std::env;
use std::fmt;
use std::fs;
use std::io;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::time::Duration;

use pg_embed::pg_enums::PgAuthMethod;
use pg_embed::pg_fetch::{PG_V17, PgFetchSettings};
use pg_embed::postgres::{PgEmbed, PgSettings};
use rand::distr::{Alphanumeric, SampleString};
use secrecy::{ExposeSecret, SecretString};
use sqlx::AssertSqlSafe;
use url::Url;

use crate::pool::build_pool;
use role_bootstrap::{
    WYRD_APP_ROLE, WYRD_DATABASE, WYRD_MIGRATOR_ROLE, WYRD_PLATFORM_ADMIN_ROLE, role_bootstrap_sql,
};

pub use crate::pool::PoolConfig;

/// Canonical Wyrd database URL. Carries the `wyrd_app` userinfo; migrator and
/// platform-admin DSNs are synthesized at boot by swapping userinfo with the
/// per-role passwords below.
pub const APP_DSN_ENV: &str = "WYRD_DATABASE_URL";
/// `wyrd_migrator` password for external Postgres. Required alongside
/// `WYRD_DATABASE_URL` whenever Wyrd applies migrations against external
/// Postgres.
pub const MIGRATOR_PASSWORD_ENV: &str = "WYRD_DATABASE_MIGRATOR_PASSWORD";
/// Optional `wyrd_platform_admin` password for external Postgres. Required
/// when the audited cross-tenant platform-admin surface is enabled.
pub const PLATFORM_ADMIN_PASSWORD_ENV: &str = "WYRD_DATABASE_PLATFORM_ADMIN_PASSWORD";

const XDG_DATA_HOME_ENV: &str = "XDG_DATA_HOME";
const HOME_ENV: &str = "HOME";
const WYRD_DEV_PG_CACHE_ENV: &str = "WYRD_DEV_PG_CACHE";
const GENERATED_PASSWORD_LEN: usize = 48;
const WYRD_CONFIG_BEGIN: &str = "# BEGIN WYRD EMBEDDED CONFIG";
const WYRD_CONFIG_END: &str = "# END WYRD EMBEDDED CONFIG";

/// Postgres boot errors.
#[derive(Debug, thiserror::Error)]
pub enum BootError {
    /// DSN environment variables were partially configured.
    #[error(
        "mixed database configuration: set WYRD_DATABASE_URL with \
         WYRD_DATABASE_MIGRATOR_PASSWORD (and optionally \
         WYRD_DATABASE_PLATFORM_ADMIN_PASSWORD) for external Postgres, or \
         leave all three unset for embedded mode"
    )]
    MixedDsnConfig,
    /// The canonical `WYRD_DATABASE_URL` could not be parsed.
    #[error("invalid WYRD_DATABASE_URL: {0}")]
    InvalidDatabaseUrl(#[source] url::ParseError),
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
    /// Resolve Postgres boot mode from Wyrd database environment variables.
    ///
    /// External mode requires `WYRD_DATABASE_URL` (the `wyrd_app` DSN) and
    /// `WYRD_DATABASE_MIGRATOR_PASSWORD`; `WYRD_DATABASE_PLATFORM_ADMIN_PASSWORD`
    /// is optional. The migrator and platform-admin DSNs are synthesized from
    /// the canonical URL by swapping the userinfo segment to the matching role
    /// name and password. All three unset starts embedded Postgres, provisions
    /// roles, and derives the three role DSNs from the managed instance.
    ///
    /// # Errors
    /// Returns [`BootError::MixedDsnConfig`] for partial or incoherent
    /// configuration. Returns [`BootError::InvalidDatabaseUrl`] when the
    /// canonical URL cannot be parsed. Returns embedded startup or filesystem
    /// errors when embedded mode is selected and the managed instance cannot
    /// be started.
    pub async fn from_env() -> Result<Self, BootError> {
        Self::from_optional_dsns(
            env::var(APP_DSN_ENV).ok(),
            env::var(MIGRATOR_PASSWORD_ENV).ok().map(SecretString::from),
            env::var(PLATFORM_ADMIN_PASSWORD_ENV)
                .ok()
                .map(SecretString::from),
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
        migrator_password: Option<SecretString>,
        platform_admin_password: Option<SecretString>,
    ) -> Result<Self, BootError> {
        match (app, migrator_password, platform_admin_password) {
            (Some(app_url), Some(migrator_pw), platform_admin_pw) => {
                let base = Url::parse(&app_url).map_err(BootError::InvalidDatabaseUrl)?;
                let migrator_dsn = role_dsn(&base, WYRD_MIGRATOR_ROLE, &migrator_pw);
                let platform_admin_dsn = platform_admin_pw
                    .as_ref()
                    .map(|pw| role_dsn(&base, WYRD_PLATFORM_ADMIN_ROLE, pw))
                    .map(SecretString::from);
                Ok(Self::External {
                    app_dsn: SecretString::from(app_url),
                    migrator_dsn: SecretString::from(migrator_dsn),
                    platform_admin_dsn,
                })
            }
            (None, None, None) => Self::embedded(EmbeddedConfig::default()).await,
            _ => Err(BootError::MixedDsnConfig),
        }
    }
}

/// Synthesize a role DSN by replacing the userinfo of `base` with `role` and
/// `password`. Host, port, database, and query string are preserved. The role
/// name comes from the schema-side constants in `role_bootstrap`, not from
/// operator configuration.
fn role_dsn(base: &Url, role: &str, password: &SecretString) -> String {
    let mut dsn = base.clone();
    dsn.set_username(role)
        .expect("role name is URL-safe ASCII per role_bootstrap constants");
    dsn.set_password(Some(password.expose_secret()))
        .expect("password is URL-safe via set_password percent-encoding");
    dsn.into()
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
        let (port, port_lock) = resolve_port(config.port)?;
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
        drop(port_lock);
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
            WYRD_APP_ROLE,
            self.credentials.app.expose_secret(),
            self.port,
            WYRD_DATABASE,
        );
        let migrator = embedded_dsn(
            WYRD_MIGRATOR_ROLE,
            self.credentials.migrator.expose_secret(),
            self.port,
            WYRD_DATABASE,
        );
        let platform_admin = embedded_dsn(
            WYRD_PLATFORM_ADMIN_ROLE,
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
    /// Persisted `wyrd_catalog_app` password path.
    pub catalog_app_secret: PathBuf,
    /// Persisted `vala_recovery` password path.
    pub recovery_secret: PathBuf,
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
            catalog_app_secret: role_credentials.join("wyrd_catalog_app.secret"),
            recovery_secret: role_credentials.join("vala_recovery.secret"),
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
    pub(crate) catalog_app: SecretString,
    pub(crate) recovery: SecretString,
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
            catalog_app: read_or_create_secret(&dirs.catalog_app_secret)?,
            recovery: read_or_create_secret(&dirs.recovery_secret)?,
        })
    }
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
    let postgres_pool = build_pool(&postgres_dsn, PoolConfig::migrator_defaults())
        .await
        .map_err(BootError::PoolConnect)?;

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
    let wyrd_pool = build_pool(&wyrd_dsn, PoolConfig::migrator_defaults())
        .await
        .map_err(BootError::PoolConnect)?;
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

fn resolve_port(configured: u16) -> Result<(u16, Option<TcpListener>), BootError> {
    if configured != 0 {
        return Ok((configured, None));
    }

    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(BootError::EmbeddedIo)?;
    let port = listener.local_addr().map_err(BootError::EmbeddedIo)?.port();
    Ok((port, Some(listener)))
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

    let tmp = path.with_extension("conf.tmp");
    fs::write(&tmp, updated).map_err(BootError::EmbeddedIo)?;
    fs::rename(&tmp, &path).map_err(BootError::EmbeddedIo)
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

fn read_or_create_secret(path: &Path) -> Result<SecretString, BootError> {
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

fn persist_secret_if_missing(path: &Path, value: &str) -> Result<(), BootError> {
    match fs::metadata(path) {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => write_secret(path, value),
        Err(error) => Err(BootError::EmbeddedIo(error)),
    }
}

fn write_secret(path: &Path, value: &str) -> Result<(), BootError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(BootError::EmbeddedIo)?;
    }

    write_secret_file(path, value)?;
    set_secret_permissions(path)
}

#[cfg(unix)]
fn write_secret_file(path: &Path, value: &str) -> Result<(), BootError> {
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
fn write_secret_file(path: &Path, value: &str) -> Result<(), BootError> {
    // Credential files are not ACL-restricted on non-Unix; embedded mode is dev-only on these platforms.
    fs::write(path, value).map_err(BootError::EmbeddedIo)
}

#[cfg(unix)]
fn set_secret_permissions(path: &Path) -> Result<(), BootError> {
    use std::os::unix::fs::PermissionsExt;

    let permissions = fs::Permissions::from_mode(0o600);
    fs::set_permissions(path, permissions).map_err(BootError::EmbeddedIo)
}

#[cfg(not(unix))]
fn set_secret_permissions(_path: &Path) -> Result<(), BootError> {
    Ok(())
}

fn generate_password() -> String {
    Alphanumeric.sample_string(&mut rand::rng(), GENERATED_PASSWORD_LEN)
}

fn default_embedded_data_dir() -> PathBuf {
    if let Some(override_dir) = env::var_os(WYRD_DEV_PG_CACHE_ENV).filter(|v| !v.is_empty()) {
        return PathBuf::from(override_dir);
    }

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

    use secrecy::ExposeSecret;

    use super::{
        APP_DSN_ENV, BootError, EmbeddedConfig, EmbeddedRoleCredentials, MIGRATOR_PASSWORD_ENV,
        PLATFORM_ADMIN_PASSWORD_ENV, PostgresBoot, SecretString, write_embedded_postgres_config,
    };

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    const APP_URL: &str = "postgres://wyrd_app:app-secret@localhost/wyrd";
    const MIGRATOR_PW: &str = "migrator-secret";
    const ADMIN_PW: &str = "admin-secret";
    const EXPECTED_MIGRATOR_DSN: &str = "postgres://wyrd_migrator:migrator-secret@localhost/wyrd";
    const EXPECTED_ADMIN_DSN: &str = "postgres://wyrd_platform_admin:admin-secret@localhost/wyrd";

    #[tokio::test]
    async fn external_with_app_and_migrator_password_allows_dedicated_mode() {
        let boot = PostgresBoot::from_optional_dsns(
            Some(APP_URL.to_owned()),
            Some(SecretString::from(MIGRATOR_PW.to_owned())),
            None,
        )
        .await
        .expect("external boot resolves");
        let dsns = boot.dsns().expect("external dsns resolve");

        assert_eq!(dsns.app.expose_secret(), APP_URL);
        assert_eq!(dsns.migrator.expose_secret(), EXPECTED_MIGRATOR_DSN);
        assert!(dsns.platform_admin.is_none());
    }

    #[tokio::test]
    async fn external_with_all_three_inputs_resolves() {
        let boot = PostgresBoot::from_optional_dsns(
            Some(APP_URL.to_owned()),
            Some(SecretString::from(MIGRATOR_PW.to_owned())),
            Some(SecretString::from(ADMIN_PW.to_owned())),
        )
        .await
        .expect("external boot resolves");
        let dsns = boot.dsns().expect("external dsns resolve");

        assert_eq!(dsns.app.expose_secret(), APP_URL);
        assert_eq!(dsns.migrator.expose_secret(), EXPECTED_MIGRATOR_DSN);
        assert_eq!(
            dsns.platform_admin
                .as_ref()
                .expect("admin dsn resolves")
                .expose_secret(),
            EXPECTED_ADMIN_DSN
        );
    }

    #[tokio::test]
    async fn mixed_inputs_are_rejected() {
        let cases: [(Option<&str>, Option<&str>, Option<&str>); 5] = [
            (Some(APP_URL), None, None),
            (None, Some(MIGRATOR_PW), None),
            (Some(APP_URL), None, Some(ADMIN_PW)),
            (None, Some(MIGRATOR_PW), Some(ADMIN_PW)),
            (None, None, Some(ADMIN_PW)),
        ];
        for (app, migrator, admin) in cases {
            let result = PostgresBoot::from_optional_dsns(
                app.map(str::to_owned),
                migrator.map(|pw| SecretString::from(pw.to_owned())),
                admin.map(|pw| SecretString::from(pw.to_owned())),
            )
            .await;

            assert!(matches!(result, Err(BootError::MixedDsnConfig)));
        }
    }

    #[tokio::test]
    async fn invalid_database_url_returns_typed_error() {
        let result = PostgresBoot::from_optional_dsns(
            Some("not-a-url".to_owned()),
            Some(SecretString::from(MIGRATOR_PW.to_owned())),
            None,
        )
        .await;
        assert!(matches!(result, Err(BootError::InvalidDatabaseUrl(_))));
    }

    #[tokio::test]
    async fn synthesized_dsns_preserve_host_port_and_database() {
        let boot = PostgresBoot::from_optional_dsns(
            Some(
                "postgres://wyrd_app:app-pw@db.example.com:6543/wyrd_prod?sslmode=require"
                    .to_owned(),
            ),
            Some(SecretString::from(MIGRATOR_PW.to_owned())),
            Some(SecretString::from(ADMIN_PW.to_owned())),
        )
        .await
        .expect("external boot resolves");
        let dsns = boot.dsns().expect("external dsns resolve");

        assert_eq!(
            dsns.migrator.expose_secret(),
            "postgres://wyrd_migrator:migrator-secret@db.example.com:6543/wyrd_prod?sslmode=require"
        );
        assert_eq!(
            dsns.platform_admin
                .as_ref()
                .expect("admin dsn resolves")
                .expose_secret(),
            "postgres://wyrd_platform_admin:admin-secret@db.example.com:6543/wyrd_prod?sslmode=require"
        );
    }

    #[test]
    fn debug_output_redacts_secret_dsns() {
        let boot = PostgresBoot::External {
            app_dsn: APP_URL.to_owned().into(),
            migrator_dsn: EXPECTED_MIGRATOR_DSN.to_owned().into(),
            platform_admin_dsn: Some(EXPECTED_ADMIN_DSN.to_owned().into()),
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

    #[tokio::test(flavor = "current_thread")]
    async fn from_env_uses_wyrd_database_url_and_password_names() {
        let previous = {
            let _guard = ENV_LOCK.lock().expect("env lock is not poisoned");
            let previous = snapshot_env(&[
                APP_DSN_ENV,
                MIGRATOR_PASSWORD_ENV,
                PLATFORM_ADMIN_PASSWORD_ENV,
            ]);
            set_env(APP_DSN_ENV, Some(APP_URL));
            set_env(MIGRATOR_PASSWORD_ENV, Some(MIGRATOR_PW));
            set_env(PLATFORM_ADMIN_PASSWORD_ENV, None);
            previous
        };
        // current_thread flavor: from_env() reads env vars before its first await,
        // so no other tokio task can observe the env between setup and the read.
        let boot = PostgresBoot::from_env()
            .await
            .expect("external env resolves");

        {
            let _guard = ENV_LOCK.lock().expect("env lock is not poisoned");
            restore_env(previous);
        }

        let dsns = boot.dsns().expect("external dsns resolve");
        assert_eq!(dsns.app.expose_secret(), APP_URL);
        assert_eq!(dsns.migrator.expose_secret(), EXPECTED_MIGRATOR_DSN);
    }

    use std::path::PathBuf;

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
