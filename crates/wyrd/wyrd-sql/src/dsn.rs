//! Feature-free Postgres DSN resolution for configured external databases.

use std::env;
use std::fmt;

use secrecy::{ExposeSecret, SecretString};
use url::Url;

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
/// `wyrd_catalog_app` password for external Postgres. Required alongside
/// `WYRD_DATABASE_URL`; the Iceberg catalog connects with this role.
pub const CATALOG_APP_PASSWORD_ENV: &str = "WYRD_DATABASE_CATALOG_APP_PASSWORD";

/// Runtime role for RLS-enforced application traffic.
pub const WYRD_APP_ROLE: &str = "wyrd_app";
/// Boot/reset role for migration and superuser test reset.
pub const WYRD_MIGRATOR_ROLE: &str = "wyrd_migrator";
/// Audited cross-tenant platform-admin role.
pub const WYRD_PLATFORM_ADMIN_ROLE: &str = "wyrd_platform_admin";
/// Bifrost Iceberg catalog application role.
pub const WYRD_CATALOG_APP_ROLE: &str = "wyrd_catalog_app";

const CATALOG_OPTIONS: &str =
    "options=-c%20role%3Dwyrd_catalog%20-c%20search_path%3Diceberg_catalog";

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
    /// Bifrost catalog DSN using `wyrd_catalog_app` plus catalog role/search path options.
    pub catalog_app: SecretString,
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
            .field("catalog_app", &"<redacted>")
            .finish()
    }
}

/// External DSN resolution errors.
#[derive(Debug, thiserror::Error)]
pub enum DsnError {
    /// DSN environment variables were partially configured.
    #[error(
        "mixed database configuration: set WYRD_DATABASE_URL with \
         WYRD_DATABASE_MIGRATOR_PASSWORD, WYRD_DATABASE_CATALOG_APP_PASSWORD, \
	         (and optionally WYRD_DATABASE_PLATFORM_ADMIN_PASSWORD) for external \
	         Postgres, or \
         leave all database DSN/password vars unset for embedded mode"
    )]
    Mixed,
    /// The canonical `WYRD_DATABASE_URL` could not be parsed.
    #[error("invalid WYRD_DATABASE_URL: {0}")]
    InvalidUrl(#[source] url::ParseError),
}

/// Resolve externally configured role DSNs without selecting an embedded fallback.
///
/// Returns `Ok(Some(_))` when external DSNs are fully configured, `Ok(None)`
/// when all database DSN/password values are absent, and `Err` for partial or invalid config.
///
/// # Errors
/// Returns [`DsnError::Mixed`] for partial configuration and
/// [`DsnError::InvalidUrl`] when `app` is not a valid URL.
pub fn resolve_external_dsns(
    app: Option<String>,
    migrator_password: Option<SecretString>,
    platform_admin_password: Option<SecretString>,
    catalog_app_password: Option<SecretString>,
) -> Result<Option<ResolvedDsns>, DsnError> {
    match (
        app,
        migrator_password,
        platform_admin_password,
        catalog_app_password,
    ) {
        (Some(app_url), Some(migrator_pw), platform_admin_pw, Some(catalog_pw)) => {
            let base = Url::parse(&app_url).map_err(DsnError::InvalidUrl)?;
            let migrator_dsn = role_dsn(&base, WYRD_MIGRATOR_ROLE, &migrator_pw);
            let platform_admin_dsn = platform_admin_pw
                .as_ref()
                .map(|pw| role_dsn(&base, WYRD_PLATFORM_ADMIN_ROLE, pw));
            let catalog_app_dsn = catalog_role_dsn(&base, &catalog_pw);
            Ok(Some(ResolvedDsns {
                app: SecretString::from(app_url),
                migrator: migrator_dsn,
                platform_admin: platform_admin_dsn,
                catalog_app: catalog_app_dsn,
            }))
        }
        (None, None, None, None) => Ok(None),
        _ => Err(DsnError::Mixed),
    }
}

/// Resolve externally configured role DSNs from Wyrd database environment vars.
///
/// # Errors
/// Returns [`DsnError`] when the environment is partially configured or the app
/// DSN is invalid.
pub fn resolve_external_dsns_from_env() -> Result<Option<ResolvedDsns>, DsnError> {
    resolve_external_dsns(
        env::var(APP_DSN_ENV).ok(),
        env::var(MIGRATOR_PASSWORD_ENV).ok().map(SecretString::from),
        env::var(PLATFORM_ADMIN_PASSWORD_ENV)
            .ok()
            .map(SecretString::from),
        env::var(CATALOG_APP_PASSWORD_ENV)
            .ok()
            .map(SecretString::from),
    )
}

/// Synthesize a role DSN by replacing the userinfo of `base` with `role` and
/// `password`. Host, port, database, and query string are preserved.
pub fn role_dsn(base: &Url, role: &str, password: &SecretString) -> SecretString {
    let mut dsn = base.clone();
    dsn.set_username(role)
        .expect("role name is URL-safe ASCII per role constants");
    dsn.set_password(Some(password.expose_secret()))
        .expect("password is URL-safe via set_password percent-encoding");
    SecretString::from(String::from(dsn))
}

/// Synthesize the Bifrost catalog DSN with role and search-path options.
#[must_use]
pub fn catalog_role_dsn(base: &Url, password: &SecretString) -> SecretString {
    with_catalog_options(&role_dsn(base, WYRD_CATALOG_APP_ROLE, password))
}

/// Append catalog role/search-path options to a DSN.
#[must_use]
pub fn with_catalog_options(dsn: &SecretString) -> SecretString {
    let dsn = dsn.expose_secret();
    let separator = if dsn.contains('?') { '&' } else { '?' };
    SecretString::from(format!("{dsn}{separator}{CATALOG_OPTIONS}"))
}

/// Synthesize a role DSN from a secret-bearing base DSN string.
///
/// # Errors
/// Returns [`DsnError::InvalidUrl`] when `base` is not a valid URL.
pub fn role_dsn_from_base(
    base: &SecretString,
    role: &str,
    password: &SecretString,
) -> Result<SecretString, DsnError> {
    let base = Url::parse(base.expose_secret()).map_err(DsnError::InvalidUrl)?;
    Ok(role_dsn(&base, role, password))
}
