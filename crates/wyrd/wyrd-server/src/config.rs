//! Typed server configuration with TOML file + environment variable loading.
//!
//! Load order: env overrides > TOML file > compiled defaults.

use std::collections::HashMap;
use std::env;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use wyrd_spec::TenantSlug;
use wyrd_spec::auth::IssuerTokenPolicy;
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
    /// The signing-key file named by `WYRD_SIGNING_KEY_FILE` could not be read.
    #[error("signing-key file at {path} could not be read")]
    ReadSigningKey {
        /// Path that failed to read.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// The sealing-key file named by `WYRD_SEALING_KEY_FILE` could not be read.
    #[error("sealing-key file at {path} could not be read")]
    ReadSealingKey {
        /// Path that failed to read.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
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

/// Which network transports the server binds and serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
#[clap(rename_all = "snake_case")]
pub enum ServeMode {
    /// Serve HTTP and gRPC (default).
    #[default]
    Both,
    /// Serve HTTP only.
    Http,
    /// Serve gRPC only.
    Grpc,
}

impl ServeMode {
    /// True when this mode binds the HTTP listener.
    #[must_use]
    pub fn serves_http(self) -> bool {
        matches!(self, Self::Both | Self::Http)
    }

    /// True when this mode binds the gRPC listener.
    #[must_use]
    pub fn serves_grpc(self) -> bool {
        matches!(self, Self::Both | Self::Grpc)
    }
}

/// Transport-selection configuration.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServeConfig {
    /// Transports to bind. Defaults to `Both`.
    #[serde(default)]
    pub mode: ServeMode,
}

/// Prometheus metrics server configuration.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricsConfig {
    /// Whether to bind the metrics listener. Defaults to true.
    #[serde(default = "default_metrics_enabled")]
    pub enabled: bool,
    /// Explicit bind address. When `None`, resolves to loopback on the HTTP
    /// port + 1. The endpoint is unauthenticated, so the default deliberately
    /// stays on loopback rather than inheriting the (possibly public) HTTP IP.
    #[serde(default)]
    pub bind: Option<SocketAddr>,
}

fn default_metrics_enabled() -> bool {
    true
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: default_metrics_enabled(),
            bind: None,
        }
    }
}

impl MetricsConfig {
    /// Resolve the concrete metrics bind. When `bind` is unset, use
    /// **loopback** (`127.0.0.1`) on `http_bind`'s port + 1 — NOT `http_bind`'s
    /// IP, which may be `0.0.0.0`. Unauthenticated `/metrics` must not be
    /// public by default.
    ///
    /// Returns `None` when the auto-computed port would overflow (HTTP on port
    /// 65535 has no room for `+ 1`).
    #[must_use]
    pub fn resolved_bind(&self, http_bind: SocketAddr) -> Option<SocketAddr> {
        if let Some(bind) = self.bind {
            return Some(bind);
        }
        let port = http_bind.port().checked_add(1)?;
        Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port))
    }

    /// True when the resolved bind is exposed beyond loopback (public IP or the
    /// unspecified `0.0.0.0`/`::` address). Used to warn in production.
    ///
    /// Returns `false` when the bind address cannot be resolved (port overflow).
    #[must_use]
    pub fn is_public_bind(&self, http_bind: SocketAddr) -> bool {
        self.resolved_bind(http_bind)
            .map(|addr| !addr.ip().is_loopback())
            .unwrap_or(false)
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
    /// Transport-selection configuration.
    #[serde(default)]
    pub serve: ServeConfig,
    /// Prometheus metrics server configuration.
    #[serde(default)]
    pub metrics: MetricsConfig,
    /// Readiness probe configuration.
    #[serde(default)]
    pub readiness: ReadinessConfig,
    /// Authentication gate configuration.
    #[serde(default)]
    pub auth: AuthConfig,
    /// Trusted OIDC issuers for this deployment.
    #[serde(default)]
    pub trusted_issuers: Vec<IssuerEntry>,
    /// Workload identity bindings for this deployment.
    #[serde(default)]
    pub workload_bindings: Vec<WorkloadBindingEntry>,
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

/// Authentication gate configuration.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct AuthConfig {
    /// When true, auth routes that require preview-gated card registration are allowed.
    #[serde(default)]
    pub allow_preview: bool,
    /// Slug of the implicit tenant this self-hosted deployment serves.
    ///
    /// Boot has no request `Host` to derive the tenant from, so the operator
    /// declares it here (or via `WYRD_SERVER_TENANT_SLUG`). Required when any
    /// `[[trusted_issuers]]` or `[[workload_bindings]]` entry is configured; the
    /// slug is resolved at boot through the same `resolve_by_slug_for_app` path
    /// the request handlers use, so the bound tenant matches request-time lookups.
    #[serde(default)]
    pub tenant_slug: Option<TenantSlug>,
    /// Wyrd's own Ed25519 signing key PEM, used to mint and verify Wyrd JWTs.
    ///
    /// Env-injected only — never read from the TOML file. Loaded at config time
    /// from `WYRD_SIGNING_KEY_FILE` (path to a mounted secret; primary) or
    /// `WYRD_SIGNING_KEY_PEM` (inline PEM; fallback). The paired public key is
    /// derived from this private key at boot, so no public key is configured
    /// separately. `None` when unset; production boot fails closed without it.
    #[serde(skip)]
    pub signing_key: Option<SecretString>,
    /// Base64-encoded 32-byte AES-256-GCM sealing key for issuer client secrets.
    ///
    /// Env-injected only — never read from the TOML file. Loaded at config time
    /// from `WYRD_SEALING_KEY_FILE` (path to a mounted secret; primary) or
    /// `WYRD_SEALING_KEY_BASE64` (inline base64; fallback). Boot decodes this to
    /// a 32-byte key. `None` when unset; boot fails closed if any seeded issuer
    /// carries a client secret without a sealing key configured.
    ///
    /// **Single-key limitation:** there is currently no key-id column or keyring.
    /// All rows are encrypted under this one key; live rotation without downtime
    /// is not yet supported. If the key leaks: (1) rotate the client secrets at
    /// the IdP, (2) re-register the issuers with the new secrets, (3) rotate this
    /// env var. The existing `client_secret_enc` rows then encrypt stale secrets
    /// and are harmless. Tracked in issue #72.
    #[serde(skip)]
    pub sealing_key: Option<SecretString>,
}

// ──────────────────────────────────────────────────────────────────────────────
// Trusted-issuer and workload-binding config DTOs
// ──────────────────────────────────────────────────────────────────────────────

/// How this deployment authenticates to an OIDC provider's token endpoint.
///
/// Maps 1:1 to `wyrd_auth_oidc::ClientAuth`. Boot converts this DTO and moves
/// secret values into the domain type. Secret-bearing variants store a redacted
/// [`SecretString`]; the value is never shown in `Debug` output.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientAuthEntry {
    /// HTTP Basic auth with a shared client secret (RFC 6749 §2.3.1).
    SecretBasic(SecretString),
    /// Secret sent in the token-endpoint POST body (RFC 6749 §2.3.1).
    SecretPost(SecretString),
    /// Private-key JWT client assertion (RFC 7523).
    PrivateKeyJwt,
    /// Public PKCE-only client — no client secret or assertion.
    Public,
}

/// Dotted claim paths for extracting normalized claims from verified tokens.
///
/// `subject` defaults to `"sub"`. No path-format validation is performed here;
/// boot wraps these strings via `wyrd_auth_oidc::ClaimPath::new`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimMappingEntry {
    /// Claim path for the external subject. Defaults to `"sub"`.
    #[serde(default = "ClaimMappingEntry::default_subject")]
    pub subject: String,
    /// Optional claim path for an email address.
    pub email: Option<String>,
    /// Optional claim path for a groups/roles array (e.g. `realm_access.roles`).
    pub groups: Option<String>,
}

impl ClaimMappingEntry {
    fn default_subject() -> String {
        "sub".to_string()
    }
}

impl Default for ClaimMappingEntry {
    fn default() -> Self {
        Self {
            subject: Self::default_subject(),
            email: None,
            groups: None,
        }
    }
}

/// Config DTO for one `[[trusted_issuers]]` entry.
///
/// Maps to `wyrd_auth_oidc::TrustedIssuer` minus boot-derived fields
/// (`jwks_uri`, `tenant_id`). Boot commit 02 converts this DTO and fills those
/// fields via OIDC discovery.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssuerEntry {
    /// OIDC issuer URL (normalized by boot; must be non-empty).
    pub issuer: String,
    /// Wyrd's OAuth 2.0 `client_id` at this IdP.
    pub client_id: String,
    /// Expected `aud` claim value in tokens from this issuer.
    pub expected_audience: String,
    /// How Wyrd authenticates to this IdP's token endpoint.
    pub client_auth: ClientAuthEntry,
    /// Claim path mapping. Defaults to `{ subject = "sub" }`.
    #[serde(default)]
    pub claim_mapping: ClaimMappingEntry,
    /// Per-issuer map from IdP group strings to Wyrd role names.
    #[serde(default)]
    pub group_role_map: HashMap<String, Vec<String>>,
    /// Baseline Wyrd role names granted to every federated user from this issuer.
    #[serde(default)]
    pub default_roles: Vec<String>,
    /// Whether tokens from this issuer represent human users or machine workloads.
    #[serde(default)]
    pub principal_kind: IssuerTokenPolicy,
    /// JWKS key-cache TTL in seconds. `None` means boot applies its default.
    pub jwks_ttl_secs: Option<u64>,
}

/// Config DTO for one `[[workload_bindings]]` entry.
///
/// Maps to `wyrd_auth_oidc::WorkloadBinding` minus boot-derived fields
/// (`tenant_id`, resolved `card_ref`). Boot commit 03 converts this DTO.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkloadBindingEntry {
    /// OIDC issuer URL of the issuer that signs tokens for this workload.
    pub issuer: String,
    /// Verified external subject claim value (e.g. a Kubernetes service account).
    pub subject: String,
    /// Optional audience constraint for additional lookup precision.
    pub audience: Option<String>,
    /// Card kind backing the workload principal — `service` or `agent`. Any
    /// other kind is rejected at boot (it can never resolve a principal).
    pub kind: String,
    /// Card name.
    pub name: String,
    /// Space that pins the card identity.
    pub space: String,
    /// Exact card version (no version range).
    pub version: String,
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
        // deployment_profile (APP_ENV: development | staging | production).
        // staging and production both select the hardened production profile, so
        // both fail closed without a signing key; only development is lenient.
        if let Some(val) = env_opt("APP_ENV")? {
            self.deployment_profile = match val.as_str() {
                "development" => DeploymentProfile::Development,
                "staging" | "production" => DeploymentProfile::Production,
                _ => {
                    return Err(ConfigError::BadEnvVar {
                        key: "APP_ENV".to_string(),
                        message: format!(
                            "expected 'development', 'staging', or 'production', got {val:?}"
                        ),
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

        // serve.mode
        if let Some(val) = env_opt("WYRD_SERVE_MODE")? {
            self.serve.mode = match val.as_str() {
                "both" => ServeMode::Both,
                "http" => ServeMode::Http,
                "grpc" => ServeMode::Grpc,
                _ => {
                    return Err(ConfigError::BadEnvVar {
                        key: "WYRD_SERVE_MODE".to_string(),
                        message: format!("expected 'both', 'http', or 'grpc', got {val:?}"),
                    });
                }
            };
        }

        // metrics.enabled
        if let Some(val) = env_opt("WYRD_METRICS_ENABLED")? {
            self.metrics.enabled = parse_flag(&val, "WYRD_METRICS_ENABLED")?;
        }

        // metrics.bind
        if let Some(val) = env_opt("WYRD_METRICS_BIND")? {
            self.metrics.bind =
                Some(
                    val.parse::<SocketAddr>()
                        .map_err(|e| ConfigError::BadEnvVar {
                            key: "WYRD_METRICS_BIND".to_string(),
                            message: e.to_string(),
                        })?,
                );
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

        // auth.allow_preview
        if let Some(val) = env_opt("WYRD_AUTH_ALLOW_PREVIEW")? {
            self.auth.allow_preview = parse_flag(&val, "WYRD_AUTH_ALLOW_PREVIEW")?;
        }

        // auth.tenant_slug
        if let Some(val) = env_opt("WYRD_SERVER_TENANT_SLUG")? {
            let slug = TenantSlug::new(val).map_err(|e| ConfigError::BadEnvVar {
                key: "WYRD_SERVER_TENANT_SLUG".to_string(),
                message: e.to_string(),
            })?;
            self.auth.tenant_slug = Some(slug);
        }

        // auth.signing_key (WYRD_SIGNING_KEY_FILE primary, WYRD_SIGNING_KEY_PEM fallback)
        if let Some(key) = load_signing_key()? {
            self.auth.signing_key = Some(key);
        }

        // auth.sealing_key (WYRD_SEALING_KEY_FILE primary, WYRD_SEALING_KEY_BASE64 fallback)
        if let Some(key) = load_sealing_key()? {
            self.auth.sealing_key = Some(key);
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

        // 1b. Metrics bind must not collide with HTTP or gRPC when enabled.
        if self.metrics.enabled {
            let metrics_bind =
                self.metrics
                    .resolved_bind(self.http.bind)
                    .ok_or_else(|| ConfigError::Invalid {
                        message:
                            "metrics port arithmetic overflow: HTTP is on port 65535, leaving \
                              no room for the auto-computed metrics port (http_port + 1)"
                                .to_owned(),
                    })?;
            if metrics_bind == self.http.bind || metrics_bind == self.grpc.bind {
                return Err(ConfigError::BindCollision { bind: metrics_bind });
            }
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

        // 10. Production profile hardening.
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

        // 16. Each trusted_issuers entry must have non-empty required fields and
        //     coherent client_auth (secret present iff secret_basic/secret_post).
        for (idx, issuer) in self.trusted_issuers.iter().enumerate() {
            let loc = |field: &str| format!("trusted_issuers[{idx}].{field}");

            if issuer.issuer.is_empty() {
                return Err(ConfigError::Invalid {
                    message: format!("{} must not be empty", loc("issuer")),
                });
            }
            if issuer.client_id.is_empty() {
                return Err(ConfigError::Invalid {
                    message: format!("{} must not be empty", loc("client_id")),
                });
            }
            if issuer.expected_audience.is_empty() {
                return Err(ConfigError::Invalid {
                    message: format!("{} must not be empty", loc("expected_audience")),
                });
            }
            match &issuer.client_auth {
                ClientAuthEntry::SecretBasic(s) | ClientAuthEntry::SecretPost(s) => {
                    if s.expose_secret().is_empty() {
                        return Err(ConfigError::Invalid {
                            message: format!("{} secret must not be empty", loc("client_auth")),
                        });
                    }
                }
                ClientAuthEntry::PrivateKeyJwt | ClientAuthEntry::Public => {}
            }
        }

        // 17. Each workload_bindings entry must have all card-target fields non-empty.
        for (idx, binding) in self.workload_bindings.iter().enumerate() {
            let loc = |field: &str| format!("workload_bindings[{idx}].{field}");

            for (field, value) in [
                ("issuer", binding.issuer.as_str()),
                ("subject", binding.subject.as_str()),
                ("kind", binding.kind.as_str()),
                ("name", binding.name.as_str()),
                ("space", binding.space.as_str()),
                ("version", binding.version.as_str()),
            ] {
                if value.is_empty() {
                    return Err(ConfigError::Invalid {
                        message: format!("{} must not be empty", loc(field)),
                    });
                }
            }
        }

        // 18. Issuers/bindings require an explicit implicit-tenant slug. Boot
        //     binds every issuer/binding to this tenant via the same slug path
        //     the request handlers use; without it boot can resolve no tenant
        //     and must fail closed, so reject the config here.
        if (!self.trusted_issuers.is_empty() || !self.workload_bindings.is_empty())
            && self.auth.tenant_slug.is_none()
        {
            return Err(ConfigError::Invalid {
                message: "[auth] tenant_slug is required when trusted_issuers or \
                          workload_bindings are configured"
                    .to_string(),
            });
        }

        // Metrics endpoint is unauthenticated; warn if it is exposed beyond
        // loopback in production. Not an error — routable scrape is a valid,
        // network-policy-protected choice — but it must be deliberate.
        if self.metrics.enabled
            && self.deployment_profile.is_production()
            && self.metrics.is_public_bind(self.http.bind)
            && let Some(bind) = self.metrics.resolved_bind(self.http.bind)
        {
            tracing::warn!(
                %bind,
                "unauthenticated /metrics is bound to a non-loopback address in \
                 production; ensure a network policy restricts scrape access"
            );
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

/// Load Wyrd's own signing-key PEM from the environment.
///
/// `WYRD_SIGNING_KEY_FILE` (a path to a mounted secret) is the primary source;
/// `WYRD_SIGNING_KEY_PEM` (inline PEM) is the fallback. The file form is
/// preferred because a k8s Secret volume keeps the PEM out of the process
/// environment and `env` dumps. Setting both is a configuration error.
fn load_signing_key() -> Result<Option<SecretString>, ConfigError> {
    let file = env_opt("WYRD_SIGNING_KEY_FILE")?;
    let inline = env_opt("WYRD_SIGNING_KEY_PEM")?;
    match (file, inline) {
        (Some(_), Some(_)) => Err(ConfigError::ConflictingEnvVars {
            keys: vec![
                "WYRD_SIGNING_KEY_FILE".to_string(),
                "WYRD_SIGNING_KEY_PEM".to_string(),
            ],
        }),
        (Some(path), None) => {
            let path = PathBuf::from(path);
            let pem = std::fs::read_to_string(&path)
                .map_err(|source| ConfigError::ReadSigningKey { path, source })?;
            Ok(Some(SecretString::from(pem)))
        }
        (None, Some(pem)) => Ok(Some(SecretString::from(pem))),
        (None, None) => Ok(None),
    }
}

/// Load the base64-encoded issuer sealing key from the environment.
///
/// `WYRD_SEALING_KEY_FILE` (a path to a mounted secret) is the primary source;
/// `WYRD_SEALING_KEY_BASE64` (inline base64) is the fallback. The file form is
/// preferred for the same reason as the signing key. Setting both is a
/// configuration error. Surrounding whitespace (e.g. a trailing newline in a
/// mounted secret file) is trimmed; boot decodes the base64 to a 32-byte key.
fn load_sealing_key() -> Result<Option<SecretString>, ConfigError> {
    let file = env_opt("WYRD_SEALING_KEY_FILE")?;
    let inline = env_opt("WYRD_SEALING_KEY_BASE64")?;
    match (file, inline) {
        (Some(_), Some(_)) => Err(ConfigError::ConflictingEnvVars {
            keys: vec![
                "WYRD_SEALING_KEY_FILE".to_string(),
                "WYRD_SEALING_KEY_BASE64".to_string(),
            ],
        }),
        (Some(path), None) => {
            let path = PathBuf::from(path);
            let encoded = std::fs::read_to_string(&path)
                .map_err(|source| ConfigError::ReadSealingKey { path, source })?;
            Ok(Some(SecretString::from(encoded.trim().to_owned())))
        }
        (None, Some(encoded)) => Ok(Some(SecretString::from(encoded.trim().to_owned()))),
        (None, None) => Ok(None),
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

    // ── 7. shutdown.drain_ms over 60k cap → Invalid ───────────────────────────

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

    // ── 8. Empty env var → EmptyEnvVar ────────────────────────────────────────

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

    // ── 9. tick_ms too low → Invalid ─────────────────────────────────────────

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

    // ── 10. Bad APP_ENV value → BadEnvVar ────────────────────────────────────

    #[test]
    fn bad_app_env_value() {
        let _guard = ENV_LOCK.lock().unwrap();
        temp_env::with_vars([("APP_ENV", Some("qa"))], || {
            let mut cfg = WyrdServerConfig::default();
            let err = cfg
                .apply_env_overrides()
                .expect_err("bad profile must error");
            assert!(
                matches!(err, ConfigError::BadEnvVar { ref key, .. } if key == "APP_ENV"),
                "expected BadEnvVar, got {err:?}"
            );
        });
    }

    #[test]
    fn app_env_staging_selects_hardened_production_profile() {
        let _guard = ENV_LOCK.lock().unwrap();
        temp_env::with_vars([("APP_ENV", Some("staging"))], || {
            let mut cfg = WyrdServerConfig::default();
            cfg.apply_env_overrides()
                .expect("staging is a valid APP_ENV");
            assert!(
                cfg.deployment_profile.is_production(),
                "staging must map to the hardened production profile"
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

    // ── 16. Absent trusted_issuers defaults to empty ──────────────────────────

    #[test]
    fn trusted_issuers_absent_defaults_to_empty() {
        let cfg = WyrdServerConfig::default();
        assert!(cfg.trusted_issuers.is_empty());
    }

    // ── 17. Absent workload_bindings defaults to empty ────────────────────────

    #[test]
    fn workload_bindings_absent_defaults_to_empty() {
        let cfg = WyrdServerConfig::default();
        assert!(cfg.workload_bindings.is_empty());
    }

    // ── 18. Populated [[trusted_issuers]] round-trips through TOML ───────────

    #[test]
    fn trusted_issuer_toml_round_trip() {
        let toml = r#"
            [[trusted_issuers]]
            issuer = "https://idp.example.com/realms/acme"
            client_id = "wyrd-client"
            expected_audience = "wyrd-client"
            client_auth = { secret_post = "my-secret" }
            principal_kind = "human"
            claim_mapping = { subject = "sub", email = "email", groups = "realm_access.roles" }
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        assert_eq!(cfg.trusted_issuers.len(), 1);
        let entry = &cfg.trusted_issuers[0];
        assert_eq!(entry.issuer, "https://idp.example.com/realms/acme");
        assert_eq!(entry.client_id, "wyrd-client");
        assert_eq!(entry.expected_audience, "wyrd-client");
        assert_eq!(entry.claim_mapping.subject, "sub");
        assert_eq!(entry.claim_mapping.email.as_deref(), Some("email"));
        assert_eq!(
            entry.claim_mapping.groups.as_deref(),
            Some("realm_access.roles")
        );
    }

    // ── 19. Populated [[workload_bindings]] round-trips through TOML ─────────

    #[test]
    fn workload_binding_toml_round_trip() {
        let toml = r#"
            [[workload_bindings]]
            issuer = "https://idp.example.com"
            subject = "system:serviceaccount:default/my-sa"
            audience = "my-audience"
            kind = "service"
            name = "my-model"
            space = "prod"
            version = "1.0.0"
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        assert_eq!(cfg.workload_bindings.len(), 1);
        let binding = &cfg.workload_bindings[0];
        assert_eq!(binding.issuer, "https://idp.example.com");
        assert_eq!(binding.subject, "system:serviceaccount:default/my-sa");
        assert_eq!(binding.audience.as_deref(), Some("my-audience"));
        assert_eq!(binding.kind, "service");
        assert_eq!(binding.name, "my-model");
        assert_eq!(binding.space, "prod");
        assert_eq!(binding.version, "1.0.0");
    }

    // ── 20. Unknown field in issuer entry rejected ────────────────────────────

    #[test]
    fn issuer_entry_unknown_field_rejected() {
        let toml = r#"
            [[trusted_issuers]]
            issuer = "https://idp.example.com"
            client_id = "wyrd"
            expected_audience = "wyrd"
            client_auth = "public"
            unknown_field = "oops"
        "#;
        let err = from_toml_str(toml).expect_err("unknown field must fail");
        assert!(
            matches!(err, ConfigError::ParseToml { .. }),
            "expected ParseToml, got {err:?}"
        );
    }

    // ── 21. Unknown field in binding entry rejected ───────────────────────────

    #[test]
    fn workload_binding_entry_unknown_field_rejected() {
        let toml = r#"
            [[workload_bindings]]
            issuer = "https://idp.example.com"
            subject = "system:serviceaccount:ns/sa"
            kind = "model"
            name = "my-model"
            space = "prod"
            version = "1.0.0"
            extra_field = "bad"
        "#;
        let err = from_toml_str(toml).expect_err("unknown field must fail");
        assert!(
            matches!(err, ConfigError::ParseToml { .. }),
            "expected ParseToml, got {err:?}"
        );
    }

    // ── 22. ClientAuthEntry variants all parse correctly ─────────────────────

    #[test]
    fn client_auth_variants_parse() {
        let cases = [
            (r#"client_auth = "public""#, "public"),
            (r#"client_auth = "private_key_jwt""#, "private_key_jwt"),
            (r#"client_auth = { secret_post = "s" }"#, "secret_post"),
            (r#"client_auth = { secret_basic = "s" }"#, "secret_basic"),
        ];
        for (auth_str, label) in cases {
            let toml = format!(
                r#"
                    [[trusted_issuers]]
                    issuer = "https://idp.example.com"
                    client_id = "wyrd"
                    expected_audience = "wyrd"
                    {auth_str}
                "#
            );
            let cfg = from_toml_str(&toml)
                .unwrap_or_else(|e| panic!("{label} variant must parse: {e:?}"));
            assert_eq!(cfg.trusted_issuers.len(), 1, "{label}");
        }
    }

    // ── 23. IssuerTokenPolicy variants parse correctly ───────────────────────

    #[test]
    fn principal_kind_variants_parse() {
        for kind in ["human", "workload"] {
            let toml = format!(
                r#"
                    [[trusted_issuers]]
                    issuer = "https://idp.example.com"
                    client_id = "wyrd"
                    expected_audience = "wyrd"
                    client_auth = "public"
                    principal_kind = "{kind}"
                "#
            );
            let cfg = from_toml_str(&toml)
                .unwrap_or_else(|e| panic!("principal_kind = {kind:?} must parse: {e:?}"));
            assert_eq!(cfg.trusted_issuers.len(), 1);
        }
    }

    // ── 24. Empty issuer URL → Invalid ───────────────────────────────────────

    #[test]
    fn issuer_entry_empty_issuer_invalid() {
        let toml = r#"
            [[trusted_issuers]]
            issuer = ""
            client_id = "wyrd"
            expected_audience = "wyrd"
            client_auth = "public"
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        let err = cfg.validate().expect_err("empty issuer must fail");
        assert!(
            matches!(err, ConfigError::Invalid { ref message } if message.contains("issuer")),
            "expected Invalid(issuer), got {err:?}"
        );
    }

    // ── 25. Empty client_id → Invalid ────────────────────────────────────────

    #[test]
    fn issuer_entry_empty_client_id_invalid() {
        let toml = r#"
            [[trusted_issuers]]
            issuer = "https://idp.example.com"
            client_id = ""
            expected_audience = "wyrd"
            client_auth = "public"
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        let err = cfg.validate().expect_err("empty client_id must fail");
        assert!(
            matches!(err, ConfigError::Invalid { ref message } if message.contains("client_id")),
            "expected Invalid(client_id), got {err:?}"
        );
    }

    // ── 26. Empty expected_audience → Invalid ─────────────────────────────────

    #[test]
    fn issuer_entry_empty_expected_audience_invalid() {
        let toml = r#"
            [[trusted_issuers]]
            issuer = "https://idp.example.com"
            client_id = "wyrd"
            expected_audience = ""
            client_auth = "public"
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        let err = cfg
            .validate()
            .expect_err("empty expected_audience must fail");
        assert!(
            matches!(err, ConfigError::Invalid { ref message } if message.contains("expected_audience")),
            "expected Invalid(expected_audience), got {err:?}"
        );
    }

    // ── 27. Empty secret in secret_post → Invalid ─────────────────────────────

    #[test]
    fn issuer_entry_empty_secret_post_invalid() {
        let toml = r#"
            [[trusted_issuers]]
            issuer = "https://idp.example.com"
            client_id = "wyrd"
            expected_audience = "wyrd"
            client_auth = { secret_post = "" }
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        let err = cfg.validate().expect_err("empty secret must fail");
        assert!(
            matches!(err, ConfigError::Invalid { ref message } if message.contains("client_auth")),
            "expected Invalid(client_auth), got {err:?}"
        );
    }

    // ── 28. Empty issuer in workload binding → Invalid ────────────────────────

    #[test]
    fn binding_entry_empty_issuer_invalid() {
        let toml = r#"
            [[workload_bindings]]
            issuer = ""
            subject = "system:serviceaccount:default/my-sa"
            kind = "model"
            name = "my-model"
            space = "prod"
            version = "1.0.0"
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        let err = cfg.validate().expect_err("empty binding issuer must fail");
        assert!(
            matches!(err, ConfigError::Invalid { ref message } if message.contains("issuer")),
            "expected Invalid(issuer), got {err:?}"
        );
    }

    // ── 29. Empty subject in workload binding → Invalid ───────────────────────

    #[test]
    fn binding_entry_empty_subject_invalid() {
        let toml = r#"
            [[workload_bindings]]
            issuer = "https://idp.example.com"
            subject = ""
            kind = "model"
            name = "my-model"
            space = "prod"
            version = "1.0.0"
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        let err = cfg.validate().expect_err("empty subject must fail");
        assert!(
            matches!(err, ConfigError::Invalid { ref message } if message.contains("subject")),
            "expected Invalid(subject), got {err:?}"
        );
    }

    // ── 30. Empty card-target field in workload binding → Invalid ─────────────

    #[test]
    fn binding_entry_empty_version_invalid() {
        let toml = r#"
            [[workload_bindings]]
            issuer = "https://idp.example.com"
            subject = "system:serviceaccount:default/my-sa"
            kind = "model"
            name = "my-model"
            space = "prod"
            version = ""
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        let err = cfg.validate().expect_err("empty version must fail");
        assert!(
            matches!(err, ConfigError::Invalid { ref message } if message.contains("version")),
            "expected Invalid(version), got {err:?}"
        );
    }

    // ── 31. Client secret never appears in Debug output ───────────────────────

    #[test]
    fn client_secret_not_in_debug_output() {
        let toml = r#"
            [[trusted_issuers]]
            issuer = "https://idp.example.com"
            client_id = "wyrd"
            expected_audience = "wyrd"
            client_auth = { secret_post = "super-secret-value" }
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        let debug = format!("{cfg:?}");
        assert!(
            !debug.contains("super-secret-value"),
            "secret must be redacted in Debug output, got: {debug}"
        );
    }

    // ── 32. trusted_issuers present without tenant_slug → Invalid ─────────────

    #[test]
    fn issuers_without_tenant_slug_invalid() {
        let toml = r#"
            [[trusted_issuers]]
            issuer = "https://idp.example.com"
            client_id = "wyrd"
            expected_audience = "wyrd"
            client_auth = "public"
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        let err = cfg.validate().expect_err("missing tenant_slug must fail");
        assert!(
            matches!(err, ConfigError::Invalid { ref message } if message.contains("tenant_slug")),
            "expected Invalid(tenant_slug), got {err:?}"
        );
    }

    // ── 33. workload_bindings present without tenant_slug → Invalid ───────────

    #[test]
    fn bindings_without_tenant_slug_invalid() {
        let toml = r#"
            [[workload_bindings]]
            issuer = "https://idp.example.com"
            subject = "system:serviceaccount:default/my-sa"
            kind = "model"
            name = "my-model"
            space = "prod"
            version = "1.0.0"
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        let err = cfg.validate().expect_err("missing tenant_slug must fail");
        assert!(
            matches!(err, ConfigError::Invalid { ref message } if message.contains("tenant_slug")),
            "expected Invalid(tenant_slug), got {err:?}"
        );
    }

    // ── 34. tenant_slug present satisfies the issuer requirement ──────────────

    #[test]
    fn issuers_with_tenant_slug_valid() {
        let toml = r#"
            [auth]
            tenant_slug = "acme"

            [[trusted_issuers]]
            issuer = "https://idp.example.com"
            client_id = "wyrd"
            expected_audience = "wyrd"
            client_auth = "public"
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        cfg.validate().expect("tenant_slug present must validate");
        assert_eq!(
            cfg.auth.tenant_slug.as_ref().map(TenantSlug::as_str),
            Some("acme")
        );
    }

    // ── 35. WYRD_SERVER_TENANT_SLUG overrides [auth] tenant_slug ──────────────

    #[test]
    fn env_tenant_slug_override() {
        let _guard = ENV_LOCK.lock().unwrap();
        temp_env::with_vars([("WYRD_SERVER_TENANT_SLUG", Some("from-env"))], || {
            let mut cfg = WyrdServerConfig::default();
            cfg.apply_env_overrides().expect("apply succeeds");
            assert_eq!(
                cfg.auth.tenant_slug.as_ref().map(TenantSlug::as_str),
                Some("from-env")
            );
        });
    }

    // ── 36. ServeMode defaults to Both ───────────────────────────────────────

    #[test]
    fn serve_mode_defaults_to_both() {
        let cfg = WyrdServerConfig::default();
        assert_eq!(cfg.serve.mode, ServeMode::Both);
    }

    // ── 37. WYRD_SERVE_MODE env override ────────────────────────────────────

    #[test]
    fn env_serve_mode_overrides() {
        let _guard = ENV_LOCK.lock().unwrap();
        temp_env::with_vars([("WYRD_SERVE_MODE", Some("grpc"))], || {
            let mut cfg = WyrdServerConfig::default();
            cfg.apply_env_overrides().expect("apply succeeds");
            assert_eq!(cfg.serve.mode, ServeMode::Grpc);
        });
        temp_env::with_vars([("WYRD_SERVE_MODE", Some("http"))], || {
            let mut cfg = WyrdServerConfig::default();
            cfg.apply_env_overrides().expect("apply succeeds");
            assert_eq!(cfg.serve.mode, ServeMode::Http);
        });
        temp_env::with_vars([("WYRD_SERVE_MODE", Some("both"))], || {
            let mut cfg = WyrdServerConfig::default();
            cfg.apply_env_overrides().expect("apply succeeds");
            assert_eq!(cfg.serve.mode, ServeMode::Both);
        });
        temp_env::with_vars([("WYRD_SERVE_MODE", Some("invalid"))], || {
            let mut cfg = WyrdServerConfig::default();
            let err = cfg.apply_env_overrides().expect_err("bad value must error");
            assert!(
                matches!(err, ConfigError::BadEnvVar { ref key, .. } if key == "WYRD_SERVE_MODE"),
                "expected BadEnvVar(WYRD_SERVE_MODE), got {err:?}"
            );
        });
    }

    // ── 38. MetricsConfig resolved_bind uses loopback + http_port+1 ──────────

    #[test]
    fn metrics_bind_defaults_to_loopback_port_plus_one() {
        let cfg = MetricsConfig::default();
        let http_bind: SocketAddr = "0.0.0.0:8080".parse().unwrap();
        let resolved = cfg
            .resolved_bind(http_bind)
            .expect("port 8080 + 1 must not overflow");
        assert_eq!(
            resolved,
            "127.0.0.1:8081".parse::<SocketAddr>().unwrap(),
            "default metrics bind must be loopback:http_port+1"
        );
    }

    #[test]
    fn metrics_bind_overflow_at_port_65535_returns_none() {
        let cfg = MetricsConfig::default();
        let http_bind: SocketAddr = "0.0.0.0:65535".parse().unwrap();
        assert!(
            cfg.resolved_bind(http_bind).is_none(),
            "port 65535 + 1 overflows u16 and must return None"
        );
    }

    // ── 39. MetricsConfig.is_public_bind correctness ─────────────────────────

    #[test]
    fn metrics_default_bind_is_not_public() {
        let http_bind: SocketAddr = "0.0.0.0:8080".parse().unwrap();
        assert!(
            !MetricsConfig::default().is_public_bind(http_bind),
            "default metrics bind must not be public"
        );
        let explicit_public = MetricsConfig {
            enabled: true,
            bind: Some("0.0.0.0:9000".parse().unwrap()),
        };
        assert!(
            explicit_public.is_public_bind(http_bind),
            "explicit 0.0.0.0 metrics bind must be public"
        );
    }

    // ── 40. Metrics bind collision with HTTP → BindCollision ─────────────────

    #[test]
    fn metrics_bind_collision_with_http_invalid() {
        // The default HTTP bind is 0.0.0.0:8080, gRPC is 127.0.0.1:50051.
        // Set metrics.bind to the HTTP port explicitly to trigger a collision.
        let toml = r#"
            [metrics]
            bind = "0.0.0.0:8080"
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        let err = cfg
            .validate()
            .expect_err("metrics-HTTP collision must fail");
        assert!(
            matches!(err, ConfigError::BindCollision { .. }),
            "expected BindCollision, got {err:?}"
        );
    }

    // ── 41. ServeMode truth table ─────────────────────────────────────────────

    #[test]
    fn serve_mode_truth_table() {
        assert!(ServeMode::Both.serves_http());
        assert!(ServeMode::Both.serves_grpc());
        assert!(ServeMode::Http.serves_http());
        assert!(!ServeMode::Http.serves_grpc());
        assert!(!ServeMode::Grpc.serves_http());
        assert!(ServeMode::Grpc.serves_grpc());
    }
}
