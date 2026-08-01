//! Role-specific Postgres pool configuration and builders.

use std::env;
use std::str::FromStr;
use std::time::Duration;

use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};

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

    /// Override a default pool config from `WYRD_DB_*{suffix}` env vars.
    #[must_use]
    pub fn from_env_with_suffix(defaults: Self, suffix: &str) -> Self {
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
/// Returns [`sqlx::Error::Configuration`] when another Rustls provider already
/// owns the process or the DSN cannot be parsed. Other [`sqlx::Error`] variants
/// report pool construction or connection failures. Cancellation may leave
/// connections opened by SQLx for the pool to close during drop.
pub async fn build_pool(database_url: &str, config: PoolConfig) -> Result<PgPool, sqlx::Error> {
    connect_pool(database_url, config).await
}

/// Build the runtime `wyrd_app` pool from `WYRD_DB_*` tuning.
///
/// # Errors
/// Returns [`sqlx::Error::Configuration`] when another Rustls provider already
/// owns the process or the DSN cannot be parsed. Other [`sqlx::Error`] variants
/// report pool construction or connection failures.
pub async fn build_app_pool(database_url: &str) -> Result<PgPool, sqlx::Error> {
    build_pool(database_url, PoolConfig::app_from_env()).await
}

/// Build the boot-only `wyrd_migrator` pool from `WYRD_DB_*_MIGRATOR` tuning.
///
/// # Errors
/// Returns [`sqlx::Error::Configuration`] when another Rustls provider already
/// owns the process or the DSN cannot be parsed. Other [`sqlx::Error`] variants
/// report pool construction or connection failures.
pub async fn build_migrator_pool(database_url: &str) -> Result<PgPool, sqlx::Error> {
    build_pool(database_url, PoolConfig::migrator_from_env()).await
}

/// Build the audited `wyrd_platform_admin` pool from
/// `WYRD_DB_*_PLATFORM_ADMIN` tuning.
///
/// # Errors
/// Returns [`sqlx::Error::Configuration`] when another Rustls provider already
/// owns the process or the DSN cannot be parsed. Other [`sqlx::Error`] variants
/// report pool construction or connection failures.
pub async fn build_platform_admin_pool(database_url: &str) -> Result<PgPool, sqlx::Error> {
    build_pool(database_url, PoolConfig::platform_admin_from_env()).await
}

/// Connect a role-configured Postgres pool after claiming Wyrd's TLS provider.
///
/// The provider is installed before SQLx parses or connects the DSN, ensuring
/// every TLS-capable pool observes the same process-wide crypto implementation.
///
/// # Errors
///
/// Returns [`sqlx::Error::Configuration`] when another Rustls provider already
/// owns the process or the DSN cannot be parsed. Other [`sqlx::Error`] variants
/// report pool construction or connection failures. Cancellation may leave
/// connections opened by SQLx for the pool to close during drop.
pub(crate) async fn connect_pool(
    database_url: &str,
    config: PoolConfig,
) -> Result<PgPool, sqlx::Error> {
    wyrd_tls::install_crypto_provider()
        .map_err(|error| sqlx::Error::Configuration(Box::new(error)))?;
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

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::Duration;

    use super::PoolConfig;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

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

    /// SQLx reaches its TLS connection path before any tonic/server initialization.
    #[tokio::test]
    async fn sql_tls_path_initializes_provider_standalone() {
        let config = PoolConfig {
            max_connections: 1,
            min_connections: 0,
            acquire_timeout: Duration::from_millis(50),
            idle_timeout: None,
            max_lifetime: None,
            statement_cache_capacity: 0,
            test_before_acquire: false,
        };
        let error = super::connect_pool(
            "postgres://wyrd:wyrd@127.0.0.1:1/wyrd?sslmode=require",
            config,
        )
        .await
        .expect_err("closed local port rejects after TLS provider initialization");
        assert!(matches!(
            error,
            sqlx::Error::Io(_) | sqlx::Error::PoolTimedOut
        ));
        wyrd_tls::install_crypto_provider().expect("SQL boundary retained AWS-LC ownership");
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

    #[test]
    fn pool_env_overrides_cover_app_and_migrator_roles() {
        let _guard = ENV_LOCK.lock().expect("env lock is not poisoned");
        let vars = [
            ("WYRD_DB_MAX_CONNECTIONS", Some("40")),
            ("WYRD_DB_MIN_CONNECTIONS", Some("3")),
            ("WYRD_DB_ACQUIRE_TIMEOUT_SECS", Some("1.5")),
            ("WYRD_DB_IDLE_TIMEOUT_SECS", Some("20")),
            ("WYRD_DB_MAX_LIFETIME_SECS", Some("off")),
            ("WYRD_DB_STATEMENT_CACHE_CAPACITY", Some("128")),
            ("WYRD_DB_TEST_BEFORE_ACQUIRE", Some("false")),
            ("WYRD_DB_MAX_CONNECTIONS_MIGRATOR", Some("3")),
            ("WYRD_DB_MIN_CONNECTIONS_MIGRATOR", Some("0")),
            ("WYRD_DB_ACQUIRE_TIMEOUT_SECS_MIGRATOR", Some("12")),
            ("WYRD_DB_IDLE_TIMEOUT_SECS_MIGRATOR", Some("15")),
            ("WYRD_DB_MAX_LIFETIME_SECS_MIGRATOR", Some("25")),
            ("WYRD_DB_STATEMENT_CACHE_CAPACITY_MIGRATOR", Some("0")),
            ("WYRD_DB_TEST_BEFORE_ACQUIRE_MIGRATOR", Some("true")),
        ];

        with_env(&vars, || {
            let app = PoolConfig::app_from_env();
            assert_eq!(app.max_connections, 40);
            assert_eq!(app.min_connections, 3);
            assert_eq!(app.acquire_timeout, Duration::from_millis(1_500));
            assert_eq!(app.idle_timeout, Some(Duration::from_secs(20)));
            assert_eq!(app.max_lifetime, None);
            assert_eq!(app.statement_cache_capacity, 128);
            assert!(!app.test_before_acquire);

            let migrator = PoolConfig::migrator_from_env();
            assert_eq!(migrator.max_connections, 3);
            assert_eq!(migrator.min_connections, 0);
            assert_eq!(migrator.acquire_timeout, Duration::from_secs(12));
            assert_eq!(migrator.idle_timeout, Some(Duration::from_secs(15)));
            assert_eq!(migrator.max_lifetime, Some(Duration::from_secs(25)));
            assert_eq!(migrator.statement_cache_capacity, 0);
            assert!(migrator.test_before_acquire);
        });
    }

    #[test]
    fn suffixed_pool_vars_fall_back_to_role_defaults_not_app_env() {
        let _guard = ENV_LOCK.lock().expect("env lock is not poisoned");
        let vars = [
            ("WYRD_DB_MAX_CONNECTIONS", Some("99")),
            ("WYRD_DB_MIN_CONNECTIONS", Some("9")),
            ("WYRD_DB_ACQUIRE_TIMEOUT_SECS", Some("9")),
            ("WYRD_DB_IDLE_TIMEOUT_SECS", Some("9")),
            ("WYRD_DB_MAX_LIFETIME_SECS", Some("9")),
            ("WYRD_DB_STATEMENT_CACHE_CAPACITY", Some("9")),
            ("WYRD_DB_TEST_BEFORE_ACQUIRE", Some("false")),
            ("WYRD_DB_MAX_CONNECTIONS_MIGRATOR", None),
            ("WYRD_DB_MIN_CONNECTIONS_MIGRATOR", None),
            ("WYRD_DB_ACQUIRE_TIMEOUT_SECS_MIGRATOR", None),
            ("WYRD_DB_IDLE_TIMEOUT_SECS_MIGRATOR", None),
            ("WYRD_DB_MAX_LIFETIME_SECS_MIGRATOR", None),
            ("WYRD_DB_STATEMENT_CACHE_CAPACITY_MIGRATOR", None),
            ("WYRD_DB_TEST_BEFORE_ACQUIRE_MIGRATOR", None),
            ("WYRD_DB_MAX_CONNECTIONS_PLATFORM_ADMIN", None),
            ("WYRD_DB_MIN_CONNECTIONS_PLATFORM_ADMIN", None),
            ("WYRD_DB_ACQUIRE_TIMEOUT_SECS_PLATFORM_ADMIN", None),
            ("WYRD_DB_IDLE_TIMEOUT_SECS_PLATFORM_ADMIN", None),
            ("WYRD_DB_MAX_LIFETIME_SECS_PLATFORM_ADMIN", None),
            ("WYRD_DB_STATEMENT_CACHE_CAPACITY_PLATFORM_ADMIN", None),
            ("WYRD_DB_TEST_BEFORE_ACQUIRE_PLATFORM_ADMIN", None),
        ];

        with_env(&vars, || {
            assert_eq!(
                PoolConfig::migrator_from_env(),
                PoolConfig::migrator_defaults()
            );
            assert_eq!(
                PoolConfig::platform_admin_from_env(),
                PoolConfig::platform_admin_defaults()
            );
        });
    }

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
}
