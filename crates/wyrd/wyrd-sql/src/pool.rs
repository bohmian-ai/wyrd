//! Role-specific Postgres pool configuration and builders.

use std::env;
use std::str::FromStr;
use std::time::Duration;

use sqlx::Connection as _;
use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};

/// Idle time after which a pooled connection is pinged before it is handed out.
///
/// A connection released moments ago is live with overwhelming probability,
/// so pinging it only adds a round-trip to every hot-path acquire. Past this
/// threshold the server, a proxy, or the network may have dropped it, so the
/// ping runs and a dead connection is closed before any caller sees it.
const VALIDATE_IDLE_AFTER: Duration = Duration::from_secs(1);

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
    /// Whether a connection idle longer than [`VALIDATE_IDLE_AFTER`] is
    /// pinged before it is handed out; a failed ping closes it and the pool
    /// acquires another.
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
            idle_timeout: Some(Duration::from_mins(5)),
            max_lifetime: Some(Duration::from_mins(30)),
            statement_cache_capacity: 256,
            test_before_acquire: true,
        }
    }

    /// Defaults for the one-off migration pool opened with the database-owner
    /// login by `wyrd-server migrate`.
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

    /// Defaults for the `wyrd_platform_admin` pool.
    #[must_use]
    pub fn platform_admin_defaults() -> Self {
        Self {
            max_connections: 2,
            min_connections: 0,
            acquire_timeout: Duration::from_secs(5),
            idle_timeout: Some(Duration::from_mins(1)),
            max_lifetime: Some(Duration::from_mins(15)),
            statement_cache_capacity: 64,
            test_before_acquire: true,
        }
    }

    /// Runtime pool config from `WYRD_DB_*` env vars.
    #[must_use]
    pub fn app_from_env() -> Self {
        Self::from_env_with_suffix(Self::app_defaults(), "")
    }

    /// Migration owner pool config from `WYRD_DB_*_MIGRATOR` env vars.
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
        Self::from_lookup(defaults, suffix, |name| env::var(name).ok())
    }

    /// Override a default pool config from `WYRD_DB_*{suffix}` values.
    ///
    /// `lookup` returns the value of a variable name; unset, unparseable, or
    /// out-of-range values keep the matching field of `defaults`. Production
    /// callers use [`PoolConfig::from_env_with_suffix`]; tests pass a map.
    #[must_use]
    pub fn from_lookup(
        defaults: Self,
        suffix: &str,
        lookup: impl Fn(&str) -> Option<String>,
    ) -> Self {
        let value = |base: &str| lookup(&format!("{base}{suffix}"));
        Self {
            max_connections: parse_or(value("WYRD_DB_MAX_CONNECTIONS"), defaults.max_connections),
            min_connections: parse_or(value("WYRD_DB_MIN_CONNECTIONS"), defaults.min_connections),
            acquire_timeout: parse_secs(value("WYRD_DB_ACQUIRE_TIMEOUT_SECS"))
                .unwrap_or(defaults.acquire_timeout),
            idle_timeout: parse_opt_secs(value("WYRD_DB_IDLE_TIMEOUT_SECS"), defaults.idle_timeout),
            max_lifetime: parse_opt_secs(value("WYRD_DB_MAX_LIFETIME_SECS"), defaults.max_lifetime),
            statement_cache_capacity: parse_or(
                value("WYRD_DB_STATEMENT_CACHE_CAPACITY"),
                defaults.statement_cache_capacity,
            ),
            test_before_acquire: parse_bool(value("WYRD_DB_TEST_BEFORE_ACQUIRE"))
                .unwrap_or(defaults.test_before_acquire),
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
/// SQLx's own `test_before_acquire` pings on every acquire; when the config
/// enables validation the pool instead pings only connections idle longer
/// than [`VALIDATE_IDLE_AFTER`], through the `before_acquire` hook.
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

    let pool = PgPoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .acquire_timeout(config.acquire_timeout)
        .idle_timeout(config.idle_timeout)
        .max_lifetime(config.max_lifetime)
        .test_before_acquire(false);
    let pool = if config.test_before_acquire {
        pool.before_acquire(|conn, meta| {
            Box::pin(async move {
                if meta.idle_for > VALIDATE_IDLE_AFTER {
                    conn.ping().await?;
                }
                Ok(true)
            })
        })
    } else {
        pool
    };
    pool.connect_with(options).await
}

/// Parse `value` as `T`, keeping `default` when it is unset or invalid.
fn parse_or<T: std::str::FromStr>(value: Option<String>, default: T) -> T {
    value
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

/// Parse a finite, non-negative number of seconds.
fn parse_secs(value: Option<String>) -> Option<Duration> {
    value
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
        .map(Duration::from_secs_f64)
}

/// Parse an optional timeout where `off` disables it; invalid values keep
/// `default`.
fn parse_opt_secs(value: Option<String>, default: Option<Duration>) -> Option<Duration> {
    match value {
        Some(value) if value.eq_ignore_ascii_case("off") => None,
        value => parse_secs(value).or(default),
    }
}

/// Parse a boolean spelled `true/1/yes/on` or `false/0/no/off`.
fn parse_bool(value: Option<String>) -> Option<bool> {
    match value?.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use sqlx::postgres::PgPoolOptions;

    use super::{PoolConfig, VALIDATE_IDLE_AFTER, connect_pool};
    use crate::dsn::APP_DSN_ENV;

    /// A connection killed while idle past the threshold is replaced, not
    /// handed out.
    ///
    /// The single pooled backend is terminated from a second pool, then left
    /// idle beyond [`VALIDATE_IDLE_AFTER`]. The next acquire must ping it,
    /// discard it, and answer from a fresh backend; without the idle-gated
    /// validation the query would fail on the dead socket. Skipped when no
    /// Postgres DSN is configured.
    ///
    /// # Panics
    ///
    /// Panics when a pool cannot connect, a query fails, or the acquire
    /// returns the terminated backend.
    #[tokio::test(flavor = "current_thread")]
    async fn idle_dead_connection_is_replaced_before_acquire() {
        let Ok(url) = std::env::var(APP_DSN_ENV) else {
            return;
        };
        let config = PoolConfig {
            max_connections: 1,
            min_connections: 0,
            ..PoolConfig::app_defaults()
        };
        let pool = connect_pool(&url, config).await.expect("validated pool");
        let backend_pid = "SELECT pg_backend_pid()";
        let victim: i32 = sqlx::query_scalar(backend_pid)
            .fetch_one(&pool)
            .await
            .expect("first backend");
        let killer = PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .expect("killer pool");
        let terminated: bool = sqlx::query_scalar("SELECT pg_terminate_backend($1)")
            .bind(victim)
            .fetch_one(&killer)
            .await
            .expect("terminate backend");
        assert!(terminated, "the pooled backend is terminated");
        // Elapses the idle threshold the validation keys on; not a synchronization wait.
        tokio::time::sleep(VALIDATE_IDLE_AFTER + Duration::from_millis(200)).await;
        let replacement: i32 = sqlx::query_scalar(backend_pid)
            .fetch_one(&pool)
            .await
            .expect("an idle dead connection is replaced before acquire");
        assert_ne!(replacement, victim);
    }

    #[test]
    fn pool_defaults_match_locked_values() {
        assert_eq!(
            PoolConfig::app_defaults(),
            PoolConfig {
                max_connections: 32,
                min_connections: 2,
                acquire_timeout: Duration::from_secs(5),
                idle_timeout: Some(Duration::from_mins(5)),
                max_lifetime: Some(Duration::from_mins(30)),
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
                idle_timeout: Some(Duration::from_mins(1)),
                max_lifetime: Some(Duration::from_mins(15)),
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
        {
            let cfg = config(
                PoolConfig::platform_admin_defaults(),
                "_PLATFORM_ADMIN",
                &vars,
            );

            assert_eq!(cfg.max_connections, 4);
            assert_eq!(cfg.min_connections, 1);
            assert_eq!(cfg.acquire_timeout, Duration::from_millis(2_500));
            assert_eq!(cfg.idle_timeout, None);
            assert_eq!(cfg.max_lifetime, Some(Duration::from_secs(30)));
            assert_eq!(cfg.statement_cache_capacity, 12);
            assert!(!cfg.test_before_acquire);
        }
    }

    #[test]
    fn pool_env_overrides_cover_app_and_migrator_roles() {
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

        {
            let app = config(PoolConfig::app_defaults(), "", &vars);
            assert_eq!(app.max_connections, 40);
            assert_eq!(app.min_connections, 3);
            assert_eq!(app.acquire_timeout, Duration::from_millis(1_500));
            assert_eq!(app.idle_timeout, Some(Duration::from_secs(20)));
            assert_eq!(app.max_lifetime, None);
            assert_eq!(app.statement_cache_capacity, 128);
            assert!(!app.test_before_acquire);

            let migrator = config(PoolConfig::migrator_defaults(), "_MIGRATOR", &vars);
            assert_eq!(migrator.max_connections, 3);
            assert_eq!(migrator.min_connections, 0);
            assert_eq!(migrator.acquire_timeout, Duration::from_secs(12));
            assert_eq!(migrator.idle_timeout, Some(Duration::from_secs(15)));
            assert_eq!(migrator.max_lifetime, Some(Duration::from_secs(25)));
            assert_eq!(migrator.statement_cache_capacity, 0);
            assert!(migrator.test_before_acquire);
        }
    }

    #[test]
    fn suffixed_pool_vars_fall_back_to_role_defaults_not_app_env() {
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

        assert_eq!(
            config(PoolConfig::migrator_defaults(), "_MIGRATOR", &vars),
            PoolConfig::migrator_defaults()
        );
        assert_eq!(
            config(
                PoolConfig::platform_admin_defaults(),
                "_PLATFORM_ADMIN",
                &vars
            ),
            PoolConfig::platform_admin_defaults()
        );
    }

    /// Build a pool config from `defaults` and exactly the `vars` given.
    fn config(defaults: PoolConfig, suffix: &str, vars: &[(&str, Option<&str>)]) -> PoolConfig {
        PoolConfig::from_lookup(defaults, suffix, |name| {
            vars.iter()
                .find(|(key, _)| *key == name)
                .and_then(|(_, value)| value.map(str::to_owned))
        })
    }
}
