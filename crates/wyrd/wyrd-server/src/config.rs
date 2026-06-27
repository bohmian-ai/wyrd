//! Typed server configuration with TOML file + environment variable loading.
//!
//! Load order: env overrides > TOML file > compiled defaults.

use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;
use wyrd_telemetry::TelemetryConfig;

/// Errors raised during configuration loading or validation.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Config file at `path` could not be read from disk.
    #[error("config file at {path} could not be read")]
    ReadToml {
        /// Path that failed to read.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// Config file at `path` could not be parsed as TOML.
    #[error("config file at {path} could not be parsed as TOML")]
    ParseToml {
        /// Path that failed to parse.
        path: PathBuf,
        /// Underlying TOML deserialization error.
        #[source]
        source: toml::de::Error,
    },
    /// `WYRD_CONFIG_PATH` points at a path that does not exist on disk.
    #[error("WYRD_CONFIG_PATH points at {path}, which does not exist")]
    MissingExplicitTomlPath {
        /// The missing path.
        path: PathBuf,
    },
    /// An environment variable is set but contains no value.
    #[error("environment variable {key} is set but empty")]
    EmptyEnvVar {
        /// Variable name.
        key: String,
    },
    /// An environment variable value could not be parsed into the expected type.
    #[error("environment variable {key} could not be parsed: {message}")]
    BadEnvVar {
        /// Variable name.
        key: String,
        /// Human-readable parse failure description.
        message: String,
    },
    /// Two or more mutually exclusive environment variables were both set.
    #[error("environment variables {keys:?} are mutually exclusive; set only one")]
    ConflictingEnvVars {
        /// Variable names that conflict.
        keys: Vec<String>,
    },
    /// HTTP and gRPC bind addresses resolve to the same socket address.
    #[error("HTTP and gRPC bind addresses must differ ({bind})")]
    BindCollision {
        /// The colliding address.
        bind: SocketAddr,
    },
    /// A configuration field failed range or cross-field validation.
    #[error("validation failure: {message}")]
    Invalid {
        /// Human-readable description of the failure.
        message: String,
    },
}

// ──────────────────────────────────────────────────────────────────────────────
// Deployment profile
// ──────────────────────────────────────────────────────────────────────────────

/// Deployment profile that selects hardened or relaxed runtime defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentProfile {
    /// Local development defaults. Relaxed gates, reflection enabled.
    #[default]
    Development,
    /// Production hardened defaults. Reflection disabled, preview gates off.
    Production,
}

impl DeploymentProfile {
    /// Returns `true` when the active profile is `production`.
    #[must_use]
    pub fn is_production(self) -> bool {
        matches!(self, Self::Production)
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Config structs
// ──────────────────────────────────────────────────────────────────────────────

/// Top-level server configuration.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WyrdServerConfig {
    /// Active deployment profile.
    #[serde(default)]
    pub deployment_profile: DeploymentProfile,
    /// HTTP server bind configuration.
    #[serde(default)]
    pub http: HttpConfig,
    /// gRPC server bind configuration.
    #[serde(default)]
    pub grpc: GrpcConfig,
    /// Telemetry and tracing configuration.
    #[serde(default)]
    pub telemetry: TelemetryConfig,
    /// Database connection pool configuration.
    #[serde(default)]
    pub pools: PoolsConfig,
    /// Request limits and timeouts.
    #[serde(default)]
    pub limits: LimitsConfig,
    /// Graceful shutdown configuration.
    #[serde(default)]
    pub shutdown: ShutdownConfig,
    /// Readiness probe configuration.
    #[serde(default)]
    pub readiness: ReadinessConfig,
    /// Request ID propagation trust configuration.
    #[serde(default)]
    pub request_id: RequestIdConfig,
    /// Authentication gate configuration.
    #[serde(default)]
    pub auth: AuthConfig,
}

/// HTTP server bind configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpConfig {
    /// Socket address to bind the HTTP listener.
    #[serde(default = "default_http_bind")]
    pub bind: SocketAddr,
}

/// gRPC server bind configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrpcConfig {
    /// Socket address to bind the gRPC listener.
    #[serde(default = "default_grpc_bind")]
    pub bind: SocketAddr,
    /// Whether to expose gRPC server reflection.
    #[serde(default)]
    pub reflection_enabled: bool,
}

/// Database connection pool configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoolsConfig {
    /// Maximum number of pooled database connections.
    #[serde(default = "default_pool_max")]
    pub max_connections: u32,
    /// Maximum time in milliseconds to wait when acquiring a connection.
    #[serde(default = "default_pool_acquire_ms")]
    pub acquire_ms: u64,
}

/// Per-request limits and timeout configuration.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LimitsConfig {
    /// Maximum request body size in bytes.
    ///
    /// The body-limit middleware buffers up to this many bytes per request;
    /// worst-case process memory is `body_bytes × concurrency`.
    #[serde(default = "default_body_bytes")]
    pub body_bytes: usize,
    /// Per-request processing timeout in milliseconds.
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    /// Maximum number of in-flight concurrent requests.
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
}

impl LimitsConfig {
    /// Convert to the runtime-ready [`crate::state::LimitsConfig`] representation.
    #[must_use]
    pub fn into_state(self) -> crate::state::LimitsConfig {
        crate::state::LimitsConfig {
            body_bytes: self.body_bytes,
            timeout: Duration::from_millis(self.timeout_ms),
            concurrency: self.concurrency,
        }
    }
}

/// Graceful shutdown configuration.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShutdownConfig {
    /// Time in milliseconds to wait for in-flight requests to drain.
    #[serde(default = "default_shutdown_drain_ms")]
    pub drain_ms: u64,
}

/// Readiness probe configuration.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadinessConfig {
    /// Interval in milliseconds between readiness checks.
    #[serde(default = "default_readiness_tick_ms")]
    pub tick_ms: u64,
    /// Per-probe timeout in milliseconds.
    #[serde(default = "default_readiness_probe_timeout_ms")]
    pub probe_timeout_ms: u64,
}

/// Request ID propagation trust configuration.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestIdConfig {
    /// When true, inbound `X-Request-Id` headers are forwarded from trusted upstreams.
    #[serde(default)]
    pub trust_upstream: bool,
    /// CIDR-formatted allowlist of upstreams whose request IDs are trusted.
    #[serde(default)]
    pub trusted_upstreams: Vec<String>,
}

/// Authentication gate configuration.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct AuthConfig {
    /// When true, auth routes that require preview-gated card registration are allowed.
    #[serde(default)]
    pub allow_preview: bool,
}

// ──────────────────────────────────────────────────────────────────────────────
// Default helpers (private)
// ──────────────────────────────────────────────────────────────────────────────

fn default_http_bind() -> SocketAddr {
    "0.0.0.0:8080"
        .parse()
        .expect("static HTTP bind address is valid")
}

fn default_grpc_bind() -> SocketAddr {
    "127.0.0.1:50051"
        .parse()
        .expect("static gRPC bind address is valid")
}

fn default_pool_max() -> u32 {
    32
}

fn default_pool_acquire_ms() -> u64 {
    5_000
}

fn default_body_bytes() -> usize {
    1_048_576
}

fn default_timeout_ms() -> u64 {
    30_000
}

fn default_concurrency() -> usize {
    1_024
}

fn default_shutdown_drain_ms() -> u64 {
    15_000
}

fn default_readiness_tick_ms() -> u64 {
    5_000
}

fn default_readiness_probe_timeout_ms() -> u64 {
    1_500
}

// ──────────────────────────────────────────────────────────────────────────────
// Default impls
// ──────────────────────────────────────────────────────────────────────────────

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            bind: default_http_bind(),
        }
    }
}

impl Default for GrpcConfig {
    fn default() -> Self {
        Self {
            bind: default_grpc_bind(),
            reflection_enabled: false,
        }
    }
}

impl Default for PoolsConfig {
    fn default() -> Self {
        Self {
            max_connections: default_pool_max(),
            acquire_ms: default_pool_acquire_ms(),
        }
    }
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            body_bytes: default_body_bytes(),
            timeout_ms: default_timeout_ms(),
            concurrency: default_concurrency(),
        }
    }
}

impl Default for ShutdownConfig {
    fn default() -> Self {
        Self {
            drain_ms: default_shutdown_drain_ms(),
        }
    }
}

impl Default for ReadinessConfig {
    fn default() -> Self {
        Self {
            tick_ms: default_readiness_tick_ms(),
            probe_timeout_ms: default_readiness_probe_timeout_ms(),
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Loader
// ──────────────────────────────────────────────────────────────────────────────

impl WyrdServerConfig {
    /// Load configuration using the standard load order: env > TOML > defaults.
    ///
    /// # Errors
    /// Returns [`ConfigError`] when the TOML file cannot be read or parsed,
    /// an environment variable is malformed, or validation fails.
    pub fn load() -> Result<Self, ConfigError> {
        let mut config = match Self::pick_toml_path()? {
            Some(path) => {
                let contents =
                    std::fs::read_to_string(&path).map_err(|source| ConfigError::ReadToml {
                        path: path.clone(),
                        source,
                    })?;
                toml::from_str::<Self>(&contents)
                    .map_err(|source| ConfigError::ParseToml { path, source })?
            }
            None => Self::default(),
        };
        config.apply_env_overrides()?;
        config.validate()?;
        Ok(config)
    }

    /// Resolve the TOML config file path.
    ///
    /// Priority: `WYRD_CONFIG_PATH` env var → `/etc/wyrd/server.toml` →
    /// `~/.config/wyrd/server.toml`. Returns `None` if no file is found.
    fn pick_toml_path() -> Result<Option<PathBuf>, ConfigError> {
        if let Some(val) = env_opt("WYRD_CONFIG_PATH")? {
            let path = PathBuf::from(val);
            if !path.exists() {
                return Err(ConfigError::MissingExplicitTomlPath { path });
            }
            return Ok(Some(path));
        }

        let candidates = [
            PathBuf::from("/etc/wyrd/server.toml"),
            PathBuf::from(shellexpand::tilde("~/.config/wyrd/server.toml").as_ref()),
        ];

        for path in &candidates {
            if path.exists() {
                return Ok(Some(path.clone()));
            }
        }

        Ok(None)
    }

    /// Apply environment variable overrides to the loaded configuration.
    ///
    /// Unset variables are silently skipped. Empty variables produce
    /// [`ConfigError::EmptyEnvVar`].
    fn apply_env_overrides(&mut self) -> Result<(), ConfigError> {
        // deployment_profile
        if let Some(val) = env_opt("WYRD_DEPLOYMENT_PROFILE")? {
            self.deployment_profile = match val.as_str() {
                "development" => DeploymentProfile::Development,
                "production" => DeploymentProfile::Production,
                _ => {
                    return Err(ConfigError::BadEnvVar {
                        key: "WYRD_DEPLOYMENT_PROFILE".to_string(),
                        message: format!("expected 'development' or 'production', got {val:?}"),
                    });
                }
            };
        }

        // http.bind
        if let Some(val) = env_opt("WYRD_SERVER_BIND")? {
            self.http.bind = val
                .parse::<SocketAddr>()
                .map_err(|e| ConfigError::BadEnvVar {
                    key: "WYRD_SERVER_BIND".to_string(),
                    message: e.to_string(),
                })?;
        }

        // grpc.bind
        if let Some(val) = env_opt("WYRD_GRPC_BIND")? {
            self.grpc.bind = val
                .parse::<SocketAddr>()
                .map_err(|e| ConfigError::BadEnvVar {
                    key: "WYRD_GRPC_BIND".to_string(),
                    message: e.to_string(),
                })?;
        }

        // grpc.reflection_enabled
        if let Some(val) = env_opt("WYRD_GRPC_REFLECTION")? {
            self.grpc.reflection_enabled = parse_flag(&val, "WYRD_GRPC_REFLECTION")?;
        }

        // telemetry.endpoint
        if let Some(val) = env_opt("WYRD_OTLP_ENDPOINT")? {
            self.telemetry.endpoint = Some(val);
        }

        // telemetry.service_name
        if let Some(val) = env_opt("WYRD_SERVICE_NAME")? {
            self.telemetry.service_name = Some(val);
        }

        // telemetry.filter
        if let Some(val) = env_opt("WYRD_LOG")? {
            self.telemetry.filter = val;
        }

        // telemetry.protocol — only "grpc" is accepted from env
        if let Some(val) = env_opt("WYRD_OTLP_PROTOCOL")? {
            if val != "grpc" {
                return Err(ConfigError::BadEnvVar {
                    key: "WYRD_OTLP_PROTOCOL".to_string(),
                    message: format!(
                        "only 'grpc' is accepted via env var (use telemetry.protocol = 'http' in wyrd.toml for HTTP OTLP), got {val:?}"
                    ),
                });
            }
            self.telemetry.protocol = wyrd_telemetry::OtlpProtocol::Grpc;
        }

        // telemetry.sample_ratio
        if let Some(val) = env_opt("WYRD_OTLP_SAMPLE_RATIO")? {
            self.telemetry.sample_ratio =
                Some(val.parse::<f64>().map_err(|e| ConfigError::BadEnvVar {
                    key: "WYRD_OTLP_SAMPLE_RATIO".to_string(),
                    message: e.to_string(),
                })?);
        }

        // telemetry.export_timeout_ms
        if let Some(val) = env_opt("WYRD_OTLP_EXPORT_TIMEOUT_MS")? {
            self.telemetry.export_timeout_ms =
                Some(val.parse::<u64>().map_err(|e| ConfigError::BadEnvVar {
                    key: "WYRD_OTLP_EXPORT_TIMEOUT_MS".to_string(),
                    message: e.to_string(),
                })?);
        }

        // pools.max_connections
        if let Some(val) = env_opt("WYRD_PG_POOL_MAX")? {
            self.pools.max_connections =
                val.parse::<u32>().map_err(|e| ConfigError::BadEnvVar {
                    key: "WYRD_PG_POOL_MAX".to_string(),
                    message: e.to_string(),
                })?;
        }

        // pools.acquire_ms
        if let Some(val) = env_opt("WYRD_PG_POOL_ACQUIRE_MS")? {
            self.pools.acquire_ms = val.parse::<u64>().map_err(|e| ConfigError::BadEnvVar {
                key: "WYRD_PG_POOL_ACQUIRE_MS".to_string(),
                message: e.to_string(),
            })?;
        }

        // limits.body_bytes
        if let Some(val) = env_opt("WYRD_HTTP_BODY_LIMIT_BYTES")? {
            self.limits.body_bytes = val.parse::<usize>().map_err(|e| ConfigError::BadEnvVar {
                key: "WYRD_HTTP_BODY_LIMIT_BYTES".to_string(),
                message: e.to_string(),
            })?;
        }

        // limits.timeout_ms
        if let Some(val) = env_opt("WYRD_HTTP_REQUEST_TIMEOUT_MS")? {
            self.limits.timeout_ms = val.parse::<u64>().map_err(|e| ConfigError::BadEnvVar {
                key: "WYRD_HTTP_REQUEST_TIMEOUT_MS".to_string(),
                message: e.to_string(),
            })?;
        }

        // limits.concurrency
        if let Some(val) = env_opt("WYRD_HTTP_CONCURRENCY_LIMIT")? {
            self.limits.concurrency = val.parse::<usize>().map_err(|e| ConfigError::BadEnvVar {
                key: "WYRD_HTTP_CONCURRENCY_LIMIT".to_string(),
                message: e.to_string(),
            })?;
        }

        // shutdown.drain_ms
        if let Some(val) = env_opt("WYRD_SHUTDOWN_DRAIN_MS")? {
            self.shutdown.drain_ms = val.parse::<u64>().map_err(|e| ConfigError::BadEnvVar {
                key: "WYRD_SHUTDOWN_DRAIN_MS".to_string(),
                message: e.to_string(),
            })?;
        }

        // readiness.tick_ms
        if let Some(val) = env_opt("WYRD_READINESS_TICK_MS")? {
            self.readiness.tick_ms = val.parse::<u64>().map_err(|e| ConfigError::BadEnvVar {
                key: "WYRD_READINESS_TICK_MS".to_string(),
                message: e.to_string(),
            })?;
        }

        // readiness.probe_timeout_ms
        if let Some(val) = env_opt("WYRD_READINESS_PROBE_TIMEOUT_MS")? {
            self.readiness.probe_timeout_ms =
                val.parse::<u64>().map_err(|e| ConfigError::BadEnvVar {
                    key: "WYRD_READINESS_PROBE_TIMEOUT_MS".to_string(),
                    message: e.to_string(),
                })?;
        }

        // request_id.trust_upstream
        if let Some(val) = env_opt("WYRD_TRUSTED_REQUEST_ID_PROPAGATION")? {
            self.request_id.trust_upstream =
                parse_flag(&val, "WYRD_TRUSTED_REQUEST_ID_PROPAGATION")?;
        }

        // request_id.trusted_upstreams (comma-separated CIDRs)
        if let Some(val) = env_opt("WYRD_TRUSTED_UPSTREAMS")? {
            self.request_id.trusted_upstreams = val
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }

        // auth.allow_preview
        if let Some(val) = env_opt("WYRD_AUTH_ALLOW_PREVIEW")? {
            self.auth.allow_preview = parse_flag(&val, "WYRD_AUTH_ALLOW_PREVIEW")?;
        }

        Ok(())
    }

    /// Validate the final assembled configuration.
    ///
    /// # Errors
    /// Returns [`ConfigError`] for any violated constraint.
    fn validate(&self) -> Result<(), ConfigError> {
        // 1. HTTP and gRPC bind addresses must differ.
        if self.http.bind == self.grpc.bind {
            return Err(ConfigError::BindCollision {
                bind: self.http.bind,
            });
        }

        // 2. pools.max_connections >= 2
        if self.pools.max_connections < 2 {
            return Err(ConfigError::Invalid {
                message: format!(
                    "pools.max_connections must be >= 2, got {}",
                    self.pools.max_connections
                ),
            });
        }

        // 3. pools.acquire_ms in [100, 60_000]
        if self.pools.acquire_ms < 100 || self.pools.acquire_ms > 60_000 {
            return Err(ConfigError::Invalid {
                message: format!(
                    "pools.acquire_ms must be in [100, 60000], got {}",
                    self.pools.acquire_ms
                ),
            });
        }

        // 4. limits.body_bytes >= 1 MiB
        if self.limits.body_bytes < 1_048_576 {
            return Err(ConfigError::Invalid {
                message: format!(
                    "limits.body_bytes must be >= 1048576 (1 MiB), got {}",
                    self.limits.body_bytes
                ),
            });
        }

        // 5. limits.timeout_ms in [1_000, 600_000]
        if self.limits.timeout_ms < 1_000 || self.limits.timeout_ms > 600_000 {
            return Err(ConfigError::Invalid {
                message: format!(
                    "limits.timeout_ms must be in [1000, 600000], got {}",
                    self.limits.timeout_ms
                ),
            });
        }

        // 6. limits.concurrency in [1, 1_048_576]
        if self.limits.concurrency < 1 || self.limits.concurrency > 1_048_576 {
            return Err(ConfigError::Invalid {
                message: format!(
                    "limits.concurrency must be in [1, 1048576], got {}",
                    self.limits.concurrency
                ),
            });
        }

        // 7. shutdown.drain_ms in [1_000, 60_000]
        if self.shutdown.drain_ms < 1_000 || self.shutdown.drain_ms > 60_000 {
            return Err(ConfigError::Invalid {
                message: format!(
                    "shutdown.drain_ms must be in [1000, 60000], got {}",
                    self.shutdown.drain_ms
                ),
            });
        }

        // 8. readiness.tick_ms in [500, 60_000]
        if self.readiness.tick_ms < 500 || self.readiness.tick_ms > 60_000 {
            return Err(ConfigError::Invalid {
                message: format!(
                    "readiness.tick_ms must be in [500, 60000], got {}",
                    self.readiness.tick_ms
                ),
            });
        }

        // 9. readiness.probe_timeout_ms in [100, 10_000]
        if self.readiness.probe_timeout_ms < 100 || self.readiness.probe_timeout_ms > 10_000 {
            return Err(ConfigError::Invalid {
                message: format!(
                    "readiness.probe_timeout_ms must be in [100, 10000], got {}",
                    self.readiness.probe_timeout_ms
                ),
            });
        }

        // 10. All trusted_upstreams must parse as valid CIDRs.
        for cidr in &self.request_id.trusted_upstreams {
            cidr.parse::<ipnetwork::IpNetwork>()
                .map_err(|e| ConfigError::Invalid {
                    message: format!("trusted upstream {cidr:?} is not a valid CIDR: {e}"),
                })?;
        }

        // 11. trust_upstream requires at least one trusted_upstream entry.
        if self.request_id.trust_upstream && self.request_id.trusted_upstreams.is_empty() {
            return Err(ConfigError::Invalid {
                message: "request_id.trust_upstream is true but trusted_upstreams is empty"
                    .to_string(),
            });
        }

        // 12. Production profile hardening.
        if self.deployment_profile.is_production() {
            if self.grpc.reflection_enabled {
                return Err(ConfigError::Invalid {
                    message: "grpc.reflection_enabled must be false in production profile"
                        .to_string(),
                });
            }
            if self.auth.allow_preview {
                return Err(ConfigError::Invalid {
                    message: "auth.allow_preview must be false in production profile".to_string(),
                });
            }
        }

        // 13. telemetry.service_name, when Some, must be non-empty.
        if let Some(name) = &self.telemetry.service_name
            && name.is_empty()
        {
            return Err(ConfigError::Invalid {
                message: "telemetry.service_name must not be empty when set".to_string(),
            });
        }

        // 14. telemetry.endpoint, when set, must be a valid URL with an
        //     accepted scheme and no userinfo, query string, or fragment.
        if let Some(endpoint) = &self.telemetry.endpoint {
            validate_otlp_endpoint(endpoint).map_err(|msg| ConfigError::Invalid {
                message: format!("telemetry.endpoint is invalid: {msg}"),
            })?;
        }

        // 15. telemetry.sample_ratio, when set, must be in [0.0, 1.0].
        if let Some(ratio) = self.telemetry.sample_ratio
            && !(0.0..=1.0).contains(&ratio)
        {
            return Err(ConfigError::Invalid {
                message: format!("telemetry.sample_ratio must be in [0.0, 1.0], got {ratio}"),
            });
        }

        Ok(())
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ──────────────────────────────────────────────────────────────────────────────

/// Read an environment variable, returning `None` if unset and
/// `Err(EmptyEnvVar)` if set but empty.
fn env_opt(key: &str) -> Result<Option<String>, ConfigError> {
    match env::var(key) {
        Ok(v) if v.is_empty() => Err(ConfigError::EmptyEnvVar {
            key: key.to_string(),
        }),
        Ok(v) => Ok(Some(v)),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(env::VarError::NotUnicode(_)) => Err(ConfigError::BadEnvVar {
            key: key.to_string(),
            message: "value is not valid UTF-8".to_string(),
        }),
    }
}

/// Parse a boolean flag from a `0`/`1` string.
fn parse_flag(val: &str, key: &str) -> Result<bool, ConfigError> {
    match val {
        "1" => Ok(true),
        "0" => Ok(false),
        _ => Err(ConfigError::BadEnvVar {
            key: key.to_string(),
            message: format!("expected 0 or 1, got {val:?}"),
        }),
    }
}

/// Validate an OTLP endpoint URL string.
///
/// Accepted schemes: `http`, `https`, `grpc`. Userinfo, query string, and
/// fragment are rejected.
fn validate_otlp_endpoint(url_str: &str) -> Result<(), String> {
    let url = url::Url::parse(url_str).map_err(|e| e.to_string())?;
    let scheme = url.scheme();
    if !["http", "https", "grpc"].contains(&scheme) {
        return Err(format!(
            "scheme must be http, https, or grpc; got {scheme:?}"
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("userinfo (username or password) is not permitted".to_string());
    }
    if url.query().is_some() {
        return Err("query string is not permitted".to_string());
    }
    if url.fragment().is_some() {
        return Err("fragment is not permitted".to_string());
    }
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// Serialize env-var tests so concurrent test threads cannot interfere.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    // ── helpers ──────────────────────────────────────────────────────────────

    /// Parse TOML from a string without performing any file I/O.
    fn from_toml_str(s: &str) -> Result<WyrdServerConfig, ConfigError> {
        toml::from_str::<WyrdServerConfig>(s).map_err(|source| ConfigError::ParseToml {
            path: PathBuf::from("<test-string>"),
            source,
        })
    }

    // ── 1. Default config validates ───────────────────────────────────────────

    #[test]
    fn default_config_validates() {
        let cfg = WyrdServerConfig::default();
        cfg.validate().expect("default config must be valid");
    }

    // ── 2. TOML with unknown legacy field fails with ParseToml ────────────────

    #[test]
    fn unknown_toml_field_fails_parse_toml() {
        let toml = r#"
            port = 9090
        "#;
        let err = from_toml_str(toml).expect_err("unknown field must fail");
        assert!(
            matches!(err, ConfigError::ParseToml { .. }),
            "expected ParseToml, got {err:?}"
        );
    }

    // ── 3. WYRD_SERVER_BIND overrides HTTP bind ───────────────────────────────

    #[test]
    fn env_server_bind_overrides_http() {
        let _guard = ENV_LOCK.lock().unwrap();
        temp_env::with_vars([("WYRD_SERVER_BIND", Some("127.0.0.1:9999"))], || {
            let mut cfg = WyrdServerConfig::default();
            cfg.apply_env_overrides().expect("apply succeeds");
            assert_eq!(
                cfg.http.bind,
                "127.0.0.1:9999".parse::<SocketAddr>().unwrap()
            );
        });
    }

    // ── 4. Same-port collision triggers BindCollision ─────────────────────────

    #[test]
    fn same_port_triggers_bind_collision() {
        let _guard = ENV_LOCK.lock().unwrap();
        temp_env::with_vars(
            [
                ("WYRD_SERVER_BIND", Some("127.0.0.1:8080")),
                ("WYRD_GRPC_BIND", Some("127.0.0.1:8080")),
            ],
            || {
                let mut cfg = WyrdServerConfig::default();
                cfg.apply_env_overrides().expect("apply succeeds");
                let err = cfg.validate().expect_err("collision must fail");
                assert!(
                    matches!(err, ConfigError::BindCollision { .. }),
                    "expected BindCollision, got {err:?}"
                );
            },
        );
    }

    // ── 5. Production profile + reflection enabled → Invalid ─────────────────

    #[test]
    fn production_reflection_invalid() {
        let toml = r#"
            deployment_profile = "production"
            [grpc]
            bind = "127.0.0.1:50051"
            reflection_enabled = true
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        let err = cfg.validate().expect_err("must fail");
        assert!(
            matches!(err, ConfigError::Invalid { .. }),
            "expected Invalid, got {err:?}"
        );
    }

    // ── 6. Production profile + allow_preview → Invalid ──────────────────────

    #[test]
    fn production_allow_preview_invalid() {
        let toml = r#"
            deployment_profile = "production"
            [auth]
            allow_preview = true
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        let err = cfg.validate().expect_err("must fail");
        assert!(
            matches!(err, ConfigError::Invalid { .. }),
            "expected Invalid, got {err:?}"
        );
    }

    // ── 7. trust_upstream without trusted_upstreams → Invalid ────────────────

    #[test]
    fn trust_upstream_without_entries_invalid() {
        let toml = r#"
            [request_id]
            trust_upstream = true
            trusted_upstreams = []
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        let err = cfg.validate().expect_err("must fail");
        assert!(
            matches!(err, ConfigError::Invalid { .. }),
            "expected Invalid, got {err:?}"
        );
    }

    // ── 8. Invalid CIDR in trusted_upstreams → Invalid ───────────────────────

    #[test]
    fn invalid_cidr_invalid() {
        let toml = r#"
            [request_id]
            trust_upstream = true
            trusted_upstreams = ["not-a-cidr"]
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        let err = cfg.validate().expect_err("must fail");
        assert!(
            matches!(err, ConfigError::Invalid { .. }),
            "expected Invalid, got {err:?}"
        );
    }

    // ── 9. shutdown.drain_ms over 60k cap → Invalid ───────────────────────────

    #[test]
    fn shutdown_drain_ms_over_cap_invalid() {
        let toml = r#"
            [shutdown]
            drain_ms = 999999
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        let err = cfg.validate().expect_err("must fail");
        assert!(
            matches!(err, ConfigError::Invalid { .. }),
            "expected Invalid, got {err:?}"
        );
    }

    // ── 10. Empty env var → EmptyEnvVar ───────────────────────────────────────

    #[test]
    fn empty_env_var_produces_error() {
        let _guard = ENV_LOCK.lock().unwrap();
        temp_env::with_vars([("WYRD_SERVER_BIND", Some(""))], || {
            let err = env_opt("WYRD_SERVER_BIND").expect_err("empty var must error");
            assert!(
                matches!(err, ConfigError::EmptyEnvVar { ref key } if key == "WYRD_SERVER_BIND"),
                "expected EmptyEnvVar, got {err:?}"
            );
        });
    }

    // ── 11. tick_ms too low → Invalid ────────────────────────────────────────

    #[test]
    fn tick_ms_too_low_invalid() {
        let toml = r#"
            [readiness]
            tick_ms = 10
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        let err = cfg.validate().expect_err("must fail");
        assert!(
            matches!(err, ConfigError::Invalid { .. }),
            "expected Invalid, got {err:?}"
        );
    }

    // ── 12. Bad deployment profile env var → BadEnvVar ───────────────────────

    #[test]
    fn bad_deployment_profile_env_var() {
        let _guard = ENV_LOCK.lock().unwrap();
        temp_env::with_vars([("WYRD_DEPLOYMENT_PROFILE", Some("staging"))], || {
            let mut cfg = WyrdServerConfig::default();
            let err = cfg
                .apply_env_overrides()
                .expect_err("bad profile must error");
            assert!(
                matches!(err, ConfigError::BadEnvVar { ref key, .. } if key == "WYRD_DEPLOYMENT_PROFILE"),
                "expected BadEnvVar, got {err:?}"
            );
        });
    }

    // ── 13. OTLP endpoint with userinfo → Invalid ────────────────────────────

    #[test]
    fn otlp_endpoint_userinfo_invalid() {
        let err = validate_otlp_endpoint("http://user:pass@collector.example.com:4317")
            .expect_err("userinfo must fail");
        assert!(
            err.contains("userinfo"),
            "message should mention userinfo: {err}"
        );
    }

    // ── 14. OTLP endpoint with query string → Invalid ────────────────────────

    #[test]
    fn otlp_endpoint_query_invalid() {
        let err = validate_otlp_endpoint("http://collector.example.com:4317?foo=bar")
            .expect_err("query must fail");
        assert!(err.contains("query"), "message should mention query: {err}");
    }

    // ── 15. Valid OTLP endpoint path → OK ────────────────────────────────────

    #[test]
    fn valid_otlp_endpoint_with_path() {
        validate_otlp_endpoint("http://collector.example.com:4317/v1/traces")
            .expect("endpoint with path must be valid");
        validate_otlp_endpoint("grpc://localhost:4317").expect("grpc endpoint must be valid");
        validate_otlp_endpoint("https://otel.example.com").expect("https endpoint must be valid");
    }
}
