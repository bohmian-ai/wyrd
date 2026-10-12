//! Postgres DSN resolution for the one-off migration and the serving process.
//!
//! [`APP_DSN_ENV`] is the one required database URL for every server mode.
//! [`PLATFORM_DSN_ENV`] optionally names a separate platform login; when it is
//! unset, platform work uses the [`APP_DSN_ENV`] login. Operators create both
//! logins; Wyrd names no role. Scope comes from the pool: tenant pools connect
//! with [`APP_DSN_ENV`] as supplied and bind a tenant per transaction, while
//! platform and catalog pools connect with [`OPERATOR_SETTING`] on, which the
//! `operator_access` row-level-security policies admit only for the login that
//! owns Wyrd's objects. The one-off `wyrd-server migrate` process runs as the
//! platform login.

use std::env;
use std::fmt;

use secrecy::{ExposeSecret, SecretString};
use url::Url;

/// Required database DSN: the tenant login, and also the platform login when
/// [`PLATFORM_DSN_ENV`] is unset.
pub const APP_DSN_ENV: &str = "WYRD_DATABASE_URL";
/// Optional platform DSN for migration, platform, and Iceberg catalog work;
/// when unset, that work uses [`APP_DSN_ENV`].
pub const PLATFORM_DSN_ENV: &str = "WYRD_PLATFORM_DATABASE_URL";
/// Session setting that marks an operator session for `operator_access`
/// policies; platform, catalog, and migration sessions set it to `on`.
pub const OPERATOR_SETTING: &str = "app.operator";

/// Session options that mark a platform connection as an operator session.
const OPERATOR_OPTIONS: &str = "options=-c%20app.operator%3Don";
/// Session options for an operator session pinned to the catalog schema.
const CATALOG_OPTIONS: &str = "options=-c%20app.operator%3Don%20-c%20search_path%3Diceberg_catalog";

/// Resolved serving DSNs.
///
/// DSNs are secret-bearing because they normally include role passwords.
#[derive(Clone)]
pub struct ResolvedDsns {
    /// [`APP_DSN_ENV`] as supplied: the tenant login, and also the platform
    /// login when [`Self::platform_admin`] is `None`.
    pub app: SecretString,
    /// Explicit separate platform login. `None` means platform and catalog
    /// work share [`Self::app`]; see [`Self::platform`].
    pub platform_admin: Option<SecretString>,
}

impl fmt::Debug for ResolvedDsns {
    /// Render the DSN set without exposing any credential.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResolvedDsns")
            .field("app", &"<redacted>")
            .field("platform_admin", &"<redacted>")
            .finish()
    }
}

impl ResolvedDsns {
    /// Build the serving DSN set from explicit values.
    ///
    /// `app` is required; an absent `platform_admin` makes both pools share
    /// the `app` login. Every present value must parse as a URL, so a serving
    /// process fails at boot rather than at its first platform or catalog
    /// operation.
    ///
    /// # Errors
    /// Returns [`DsnError::Missing`] when `app` is absent, or
    /// [`DsnError::InvalidUrl`] naming the variable whose value does not parse.
    pub fn resolve(app: Option<String>, platform_admin: Option<String>) -> Result<Self, DsnError> {
        Ok(Self {
            app: parsed(APP_DSN_ENV, app)?,
            platform_admin: platform_admin
                .map(|value| parsed(PLATFORM_DSN_ENV, Some(value)))
                .transpose()?,
        })
    }

    /// Build the serving DSN set from [`APP_DSN_ENV`] and [`PLATFORM_DSN_ENV`].
    ///
    /// # Errors
    /// Returns [`DsnError`] as [`Self::resolve`] does.
    pub fn from_env() -> Result<Self, DsnError> {
        Self::resolve(env::var(APP_DSN_ENV).ok(), env::var(PLATFORM_DSN_ENV).ok())
    }

    /// Return the platform login: the explicit platform DSN, or [`Self::app`]
    /// when none was configured. Migration connects with this login.
    #[must_use]
    pub fn platform_login(&self) -> &SecretString {
        self.platform_admin.as_ref().unwrap_or(&self.app)
    }

    /// Return the platform pool DSN: [`Self::platform_login`] as an operator
    /// session ([`OPERATOR_SETTING`] on).
    #[must_use]
    pub fn platform(&self) -> SecretString {
        with_options(self.platform_login(), OPERATOR_OPTIONS)
    }

    /// Whether the tenant and platform pools share one login, which selects
    /// the tenant-login readiness checks.
    #[must_use]
    pub fn shares_login(&self) -> bool {
        self.platform_admin.is_none()
    }

    /// Return the Bifrost Iceberg catalog DSN: the platform operator session
    /// with its search path pinned to the catalog schema.
    #[must_use]
    pub fn catalog(&self) -> SecretString {
        with_options(self.platform_login(), CATALOG_OPTIONS)
    }
}

/// DSN resolution errors.
#[derive(Debug, thiserror::Error)]
pub enum DsnError {
    /// A required database URL variable is unset.
    #[error("{0} is not set")]
    Missing(&'static str),
    /// A database URL could not be parsed.
    #[error("invalid {name}: {source}")]
    InvalidUrl {
        /// Environment variable that carried the value.
        name: &'static str,
        /// Parser failure.
        #[source]
        source: url::ParseError,
    },
}

/// Require `value` and prove it parses as a URL before wrapping it as a secret.
///
/// # Errors
/// Returns [`DsnError::Missing`] or [`DsnError::InvalidUrl`] for `name`.
fn parsed(name: &'static str, value: Option<String>) -> Result<SecretString, DsnError> {
    let value = value.ok_or(DsnError::Missing(name))?;
    Url::parse(&value).map_err(|source| DsnError::InvalidUrl { name, source })?;
    Ok(SecretString::from(value))
}

/// Append one encoded `options` query parameter to a DSN.
fn with_options(dsn: &SecretString, options: &str) -> SecretString {
    let dsn = dsn.expose_secret();
    let separator = if dsn.contains('?') { '&' } else { '?' };
    SecretString::from(format!("{dsn}{separator}{options}"))
}

#[cfg(test)]
mod tests {
    use secrecy::ExposeSecret;

    use super::{DsnError, ResolvedDsns};

    /// Explicit serving DSNs are validated and kept separate; the platform and
    /// catalog DSNs are the platform login as an operator session.
    #[test]
    fn explicit_serving_dsns_stay_separate_and_derive_the_catalog() {
        assert!(matches!(
            ResolvedDsns::resolve(
                Some("postgres://a@h/w".to_owned()),
                Some("not a url".to_owned())
            ),
            Err(DsnError::InvalidUrl {
                name: "WYRD_PLATFORM_DATABASE_URL",
                ..
            })
        ));
        assert!(matches!(
            ResolvedDsns::resolve(
                Some("not a url".to_owned()),
                Some("postgres://p@h/w".to_owned())
            ),
            Err(DsnError::InvalidUrl {
                name: "WYRD_DATABASE_URL",
                ..
            })
        ));

        let dsns = ResolvedDsns::resolve(
            Some("postgres://tenant:a@h/wyrd".to_owned()),
            Some("postgres://platform:p@h/wyrd?sslmode=require".to_owned()),
        )
        .expect("both DSNs resolve");
        assert!(!dsns.shares_login());
        assert_eq!(dsns.app.expose_secret(), "postgres://tenant:a@h/wyrd");
        assert_eq!(
            dsns.platform().expose_secret(),
            "postgres://platform:p@h/wyrd?sslmode=require&options=-c%20app.operator%3Don"
        );
        assert_eq!(
            dsns.catalog().expose_secret(),
            "postgres://platform:p@h/wyrd?sslmode=require&options=-c%20app.operator%3Don%20-c%20search_path%3Diceberg_catalog"
        );
    }

    /// An unset platform URL resolves to the database URL for platform and
    /// catalog work, while the database URL is still required and validated.
    #[test]
    fn platform_url_falls_back_to_database_url() {
        let dsns = ResolvedDsns::resolve(Some("postgres://wyrd:a@h/wyrd".to_owned()), None)
            .expect("the database URL alone resolves");
        assert!(dsns.shares_login());
        assert_eq!(
            dsns.platform_login().expose_secret(),
            "postgres://wyrd:a@h/wyrd"
        );
        assert_eq!(
            dsns.platform().expose_secret(),
            "postgres://wyrd:a@h/wyrd?options=-c%20app.operator%3Don"
        );
        assert_eq!(
            dsns.catalog().expose_secret(),
            "postgres://wyrd:a@h/wyrd?options=-c%20app.operator%3Don%20-c%20search_path%3Diceberg_catalog"
        );
        assert!(matches!(
            ResolvedDsns::resolve(None, None),
            Err(DsnError::Missing("WYRD_DATABASE_URL"))
        ));
    }
}
