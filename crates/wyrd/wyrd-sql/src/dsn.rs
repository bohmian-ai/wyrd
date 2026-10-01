//! Postgres DSN resolution for the one-off migration and the serving process.
//!
//! Serving Wyrd receives exactly two logins: the RLS-bound `wyrd_app` role
//! through [`APP_DSN_ENV`] and the explicitly privileged `wyrd_platform_admin`
//! role through [`PLATFORM_DSN_ENV`]. The one-off `wyrd-server migrate`
//! process instead reads the existing database-owner login from
//! [`APP_DSN_ENV`]; the owner credential is never part of a serving
//! environment.

use std::env;
use std::fmt;

use secrecy::{ExposeSecret, SecretString};
use url::Url;

/// Serving `wyrd_app` DSN; in the one-off migration process, the owner DSN.
pub const APP_DSN_ENV: &str = "WYRD_DATABASE_URL";
/// Serving `wyrd_platform_admin` DSN for platform and Iceberg catalog work.
pub const PLATFORM_DSN_ENV: &str = "WYRD_PLATFORM_DATABASE_URL";

/// Runtime role for RLS-enforced application traffic.
pub const WYRD_APP_ROLE: &str = "wyrd_app";
/// Audited cross-tenant platform-admin role; also owns the Iceberg catalog.
pub const WYRD_PLATFORM_ADMIN_ROLE: &str = "wyrd_platform_admin";

/// Session options that point a platform-admin connection at the catalog schema.
const CATALOG_OPTIONS: &str = "options=-c%20search_path%3Diceberg_catalog";

/// Resolved serving DSNs.
///
/// DSNs are secret-bearing because they normally include role passwords.
#[derive(Clone)]
pub struct ResolvedDsns {
    /// Runtime `wyrd_app` DSN. RLS applies to connections built from this DSN.
    pub app: SecretString,
    /// Audited cross-tenant `wyrd_platform_admin` DSN, also used (with
    /// [`with_catalog_options`]) by the Bifrost Iceberg catalog.
    pub platform_admin: SecretString,
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
    /// Both values are required and must parse as URLs, so a serving process
    /// fails at boot rather than at its first platform or catalog operation.
    ///
    /// # Errors
    /// Returns [`DsnError::Missing`] naming the absent variable, or
    /// [`DsnError::InvalidUrl`] when a value does not parse.
    pub fn resolve(app: Option<String>, platform_admin: Option<String>) -> Result<Self, DsnError> {
        Ok(Self {
            app: parsed(APP_DSN_ENV, app)?,
            platform_admin: parsed(PLATFORM_DSN_ENV, platform_admin)?,
        })
    }

    /// Build the serving DSN set from [`APP_DSN_ENV`] and [`PLATFORM_DSN_ENV`].
    ///
    /// # Errors
    /// Returns [`DsnError`] as [`Self::resolve`] does.
    pub fn from_env() -> Result<Self, DsnError> {
        Self::resolve(env::var(APP_DSN_ENV).ok(), env::var(PLATFORM_DSN_ENV).ok())
    }

    /// Return the Bifrost Iceberg catalog DSN: the platform-admin login with
    /// its search path pinned to the catalog schema.
    #[must_use]
    pub fn catalog(&self) -> SecretString {
        with_catalog_options(&self.platform_admin)
    }
}

/// Read the one-off migration owner DSN from [`APP_DSN_ENV`].
///
/// # Errors
/// Returns [`DsnError::Missing`] when the variable is unset, or
/// [`DsnError::InvalidUrl`] when it does not parse.
pub fn owner_dsn_from_env() -> Result<SecretString, DsnError> {
    parsed(APP_DSN_ENV, env::var(APP_DSN_ENV).ok())
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

/// Append the catalog search-path option to a DSN.
#[must_use]
pub fn with_catalog_options(dsn: &SecretString) -> SecretString {
    let dsn = dsn.expose_secret();
    let separator = if dsn.contains('?') { '&' } else { '?' };
    SecretString::from(format!("{dsn}{separator}{CATALOG_OPTIONS}"))
}

#[cfg(test)]
mod tests {
    use secrecy::ExposeSecret;

    use super::{DsnError, ResolvedDsns};

    /// Both serving DSNs are required, and the catalog DSN is the platform
    /// login with only the catalog search path added.
    #[test]
    fn serving_dsns_require_both_logins_and_derive_the_catalog() {
        let missing = ResolvedDsns::resolve(Some("postgres://a@h/wyrd".to_owned()), None);
        assert!(matches!(
            missing,
            Err(DsnError::Missing("WYRD_PLATFORM_DATABASE_URL"))
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
            Some("postgres://wyrd_app:a@h/wyrd".to_owned()),
            Some("postgres://wyrd_platform_admin:p@h/wyrd?sslmode=require".to_owned()),
        )
        .expect("both DSNs resolve");
        assert_eq!(
            dsns.catalog().expose_secret(),
            "postgres://wyrd_platform_admin:p@h/wyrd?sslmode=require&options=-c%20search_path%3Diceberg_catalog"
        );
    }
}
