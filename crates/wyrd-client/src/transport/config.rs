//! Wyrd client transport configuration types.

use serde::{Deserialize, Serialize};
use wyrd_spec::security::{SecretRef, TlsConfig};

use crate::error::WyrdClientError;

/// Default endpoint for [`GrpcConfig`].
pub const GRPC_DEFAULT_ENDPOINT: &str = "http://localhost:50051";
/// Default per-call timeout, in milliseconds, for [`GrpcConfig`].
pub const GRPC_DEFAULT_TIMEOUT_MS: u64 = 30_000;
/// Default connect-retry budget for [`GrpcConfig`].
pub const GRPC_DEFAULT_CONNECT_RETRIES: u32 = 3;
/// Default base URL for [`HttpConfig`].
pub const HTTP_DEFAULT_BASE_URL: &str = "http://localhost:50050";
/// Default per-request timeout, in milliseconds, for [`HttpConfig`].
pub const HTTP_DEFAULT_TIMEOUT_MS: u64 = 30_000;
/// Default buffer label used by [`MockConfig::default`].
pub const MOCK_DEFAULT_LABEL: &str = "default";

/// Configuration for the gRPC transport.
///
/// gRPC is the default transport for `wyrd-client`. The server-side endpoint
/// is the Wyrd ingest gateway or any compatible tonic-based receiver.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GrpcConfig {
    /// gRPC server endpoint. Must be `http://host:port` or `https://host:port`.
    pub endpoint: String,

    /// Per-call timeout in milliseconds. Must be at least 1.
    pub timeout_ms: u64,

    /// Optional TLS configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls: Option<TlsConfig>,

    /// Optional bearer token or mTLS identity reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<SecretRef>,

    /// Connection-level retry budget.
    pub connect_retries: u32,
}

impl Default for GrpcConfig {
    fn default() -> Self {
        Self {
            endpoint: GRPC_DEFAULT_ENDPOINT.to_string(),
            timeout_ms: GRPC_DEFAULT_TIMEOUT_MS,
            tls: None,
            auth: None,
            connect_retries: GRPC_DEFAULT_CONNECT_RETRIES,
        }
    }
}

impl GrpcConfig {
    /// Validate the config.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::Config`] when `endpoint` is empty or
    /// `timeout_ms` is zero.
    pub fn validate(&self) -> Result<(), WyrdClientError> {
        if self.endpoint.is_empty() {
            return Err(WyrdClientError::Config {
                field: "grpc_config.endpoint".to_string(),
                reason: "must not be empty".to_string(),
            });
        }
        if self.timeout_ms == 0 {
            return Err(WyrdClientError::Config {
                field: "grpc_config.timeout_ms".to_string(),
                reason: "must be at least 1".to_string(),
            });
        }
        Ok(())
    }
}

/// Configuration for the HTTP transport.
///
/// The HTTP transport is a fallback for environments where gRPC is unavailable
/// or blocked. Ingest routes are appended by the client at call time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HttpConfig {
    /// Base URL for all ingest routes. Must be non-empty.
    #[serde(default = "default_base_url")]
    pub base_url: String,

    /// Request timeout in milliseconds. Must be at least 1.
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,

    /// Optional TLS configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls: Option<TlsConfig>,

    /// Optional authorization credential reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<SecretRef>,

    /// Whether to gzip-compress request bodies.
    #[serde(default)]
    pub compression: bool,
}

fn default_base_url() -> String {
    HTTP_DEFAULT_BASE_URL.to_string()
}

fn default_timeout_ms() -> u64 {
    HTTP_DEFAULT_TIMEOUT_MS
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            base_url: HTTP_DEFAULT_BASE_URL.to_string(),
            timeout_ms: HTTP_DEFAULT_TIMEOUT_MS,
            tls: None,
            auth: None,
            compression: false,
        }
    }
}

impl HttpConfig {
    /// Validate the config.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::Config`] when `base_url` is empty or
    /// `timeout_ms` is zero.
    pub fn validate(&self) -> Result<(), WyrdClientError> {
        if self.base_url.is_empty() {
            return Err(WyrdClientError::Config {
                field: "http_config.base_url".to_string(),
                reason: "must not be empty".to_string(),
            });
        }
        if self.timeout_ms == 0 {
            return Err(WyrdClientError::Config {
                field: "http_config.timeout_ms".to_string(),
                reason: "must be at least 1".to_string(),
            });
        }
        Ok(())
    }
}

/// Configuration for the mock in-memory loopback transport.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MockConfig {
    /// Buffer label used by the mock recorder implementation.
    pub label: String,

    /// Inject a flush failure on the `nth` flush. `None` means never fail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fail_on_flush: Option<u32>,
}

impl Default for MockConfig {
    fn default() -> Self {
        Self {
            label: MOCK_DEFAULT_LABEL.to_string(),
            fail_on_flush: None,
        }
    }
}

/// Wyrd client transport selection.
///
/// gRPC is the default. Queue-backed transports are intentionally absent in
/// this phase and are reserved for a follow-on transport plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "transport", content = "params", rename_all = "snake_case")]
pub enum TransportConfig {
    /// gRPC transport. Gated on `transport-grpc`.
    Grpc(GrpcConfig),
    /// HTTP transport. Gated on `transport-http`.
    Http(HttpConfig),
    /// In-memory loopback transport for tests. Always available.
    Mock(MockConfig),
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self::Grpc(GrpcConfig::default())
    }
}

impl TransportConfig {
    /// Short ASCII name matching the serde tag.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Grpc(_) => "grpc",
            Self::Http(_) => "http",
            Self::Mock(_) => "mock",
        }
    }

    /// Cargo feature that gates this variant, if any.
    #[must_use]
    pub const fn required_feature(&self) -> Option<&'static str> {
        match self {
            Self::Grpc(_) => Some("transport-grpc"),
            Self::Http(_) => Some("transport-http"),
            Self::Mock(_) => None,
        }
    }

    /// Returns `true` if the selected transport is compiled into this build.
    ///
    /// `WyrdClient::new()` uses this to raise
    /// `WYRD_CLIENT_400_TRANSPORT_FEATURE_DISABLED` when a config names a
    /// transport that the local build excludes.
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        match self {
            Self::Grpc(_) => cfg!(feature = "transport-grpc"),
            Self::Http(_) => cfg!(feature = "transport-http"),
            Self::Mock(_) => true,
        }
    }
}

/// Shared queue policy for all per-record queues.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QueueConfig {
    /// Transport variant this queue sends to.
    pub transport: TransportConfig,

    /// Flush when the buffer reaches this row count. Must be at least 1.
    pub flush_max_rows: usize,

    /// Flush at most every N milliseconds regardless of buffer fill.
    pub flush_interval_ms: u64,

    /// Bounded channel capacity from the user thread to the flush worker.
    pub channel_capacity: usize,

    /// Optional drop gate. `None` means no sampling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_ratio: Option<f64>,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            transport: TransportConfig::default(),
            flush_max_rows: 10_000,
            flush_interval_ms: 5_000,
            channel_capacity: 100,
            sample_ratio: None,
        }
    }
}

impl QueueConfig {
    /// Validate the queue policy.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::Config`] when any numeric lower bound fails
    /// or `sample_ratio` is outside `[0.0, 1.0]`.
    pub fn validate(&self) -> Result<(), WyrdClientError> {
        if self.flush_max_rows < 1 {
            return Err(WyrdClientError::Config {
                field: "queue_config.flush_max_rows".to_string(),
                reason: "must be >= 1".to_string(),
            });
        }
        if self.flush_interval_ms < 1 {
            return Err(WyrdClientError::Config {
                field: "queue_config.flush_interval_ms".to_string(),
                reason: "must be >= 1".to_string(),
            });
        }
        if self.channel_capacity < 1 {
            return Err(WyrdClientError::Config {
                field: "queue_config.channel_capacity".to_string(),
                reason: "must be >= 1".to_string(),
            });
        }
        if let Some(ratio) = self.sample_ratio {
            if !(0.0..=1.0).contains(&ratio) {
                return Err(WyrdClientError::Config {
                    field: "queue_config.sample_ratio".to_string(),
                    reason: format!("must be in [0.0, 1.0], got {ratio}"),
                });
            }
        }
        Ok(())
    }
}
