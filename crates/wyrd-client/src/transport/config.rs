//! Wyrd client transport configuration types.

use serde::{Deserialize, Serialize};
use wyrd_spec::security::{SecretRef, TlsConfig};

use crate::error::WyrdClientError;

/// Default endpoint for [`GrpcConfig`]. Matches predecessor parity row C in
/// `01-parity-audit.md`.
pub const GRPC_DEFAULT_ENDPOINT: &str = "http://localhost:50051";
/// Default per-call timeout (ms) for [`GrpcConfig`].
pub const GRPC_DEFAULT_TIMEOUT_MS: u64 = 30_000;
/// Default connect-retry budget for [`GrpcConfig`].
pub const GRPC_DEFAULT_CONNECT_RETRIES: u32 = 3;

/// Configuration for the gRPC transport.
///
/// gRPC is the default transport for `wyrd-client`. The server-side endpoint
/// is the Wyrd ingest gateway (`vala-ingest`, PR4.2) or any compatible
/// tonic-based receiver.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GrpcConfig {
    /// gRPC server endpoint. Must be `http://host:port` (plaintext) or
    /// `https://host:port` (TLS). Must be non-empty.
    ///
    /// Default: [`GRPC_DEFAULT_ENDPOINT`] (`"http://localhost:50051"`).
    pub endpoint: String,

    /// Per-call timeout in milliseconds.
    ///
    /// Default: [`GRPC_DEFAULT_TIMEOUT_MS`] (`30_000`, 30 s).
    pub timeout_ms: u64,

    /// Optional TLS configuration. When `None`, connections use the scheme
    /// in `endpoint` to decide TLS (`https://` implies TLS with the system CA
    /// pool, `http://` means plaintext).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls: Option<TlsConfig>,

    /// Optional bearer token or mTLS identity reference. The runtime passes
    /// this through the gRPC `Authorization` metadata header.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<SecretRef>,

    /// Connection-level retry budget. The transport retries the initial
    /// connection up to this many times before surfacing an error. Per-call
    /// retry logic lives in `QueueConfig`.
    ///
    /// Default: [`GRPC_DEFAULT_CONNECT_RETRIES`] (`3`).
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
    /// Validate the config. Returns `Err` if `endpoint` is empty or
    /// `timeout_ms` is zero. `connect_retries == 0` is permitted; it means
    /// fail on the first connect attempt.
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

/// Default base URL for [`HttpConfig`]. Matches the local-development
/// gateway port. HTTP is the secondary path; this default mirrors
/// [`GrpcConfig`] for parity with predecessor row D.
pub const HTTP_DEFAULT_BASE_URL: &str = "http://localhost:50050";
/// Default per-request timeout (ms) for [`HttpConfig`].
pub const HTTP_DEFAULT_TIMEOUT_MS: u64 = 30_000;

/// Configuration for the HTTP transport.
///
/// The HTTP transport is a fallback for environments where gRPC is unavailable
/// or blocked. Ingest routes are appended by the client at call time;
/// `base_url` is the common prefix.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HttpConfig {
    /// Base URL for all ingest routes. Must be non-empty. Ingest routes
    /// (`/api/v1/observations`, `/api/v1/records`, etc.) are appended at
    /// call time.
    ///
    /// Example: `"https://wyrd-ingest.example.com"`.
    /// Default: [`HTTP_DEFAULT_BASE_URL`] (`"http://localhost:50050"`).
    #[serde(default = "default_base_url")]
    pub base_url: String,

    /// Request timeout in milliseconds.
    ///
    /// Default: [`HTTP_DEFAULT_TIMEOUT_MS`] (`30_000`, 30 s).
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,

    /// Optional TLS configuration. When `None`, the system CA pool is used
    /// and the scheme in `base_url` governs whether TLS is active.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls: Option<TlsConfig>,

    /// Optional authorization credential. Passed through the `Authorization`
    /// HTTP header at request time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<SecretRef>,

    /// Whether to gzip-compress request bodies. Useful for high-volume
    /// payloads; the server must accept `Content-Encoding: gzip`.
    ///
    /// Default: `false`.
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
    /// Validate the config. Returns `Err` if `base_url` is empty or
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

/// Default buffer label used by [`MockConfig::default`].
pub const MOCK_DEFAULT_LABEL: &str = "default";

/// Configuration for the mock (in-memory loopback) transport.
///
/// Use this transport in unit tests where you want to assert what was
/// published without touching a network. `wyrd-client::transport::mock`
/// records every flushed envelope into an in-memory buffer keyed by
/// [`MockConfig::label`] and provides `mock::drain(label)` to retrieve
/// flushed records.
///
/// The mock transport is always available in `wyrd-client` (no feature gate).
/// `MockConfig::fail_on_flush` lets tests inject deterministic flush
/// failures - a Wyrd-native addition the predecessor mock did not support.
///
/// # Default
///
/// `MockConfig::default()` returns `{ label: "default", fail_on_flush: None }`.
/// The default is a working config - it does not need `validate()` and is
/// safe to use directly in tests that do not care about label isolation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MockConfig {
    /// Buffer label. Multiple `MockConfig` instances with the same label share
    /// the same in-memory buffer in `wyrd-client::transport::mock`. Use
    /// distinct labels to isolate test scenarios running in the same process.
    pub label: String,

    /// Inject a flush failure. When `Some(n)`, the `nth` call to
    /// `Flushable::flush` returns `Err`; all other calls succeed normally.
    /// Counting starts from 1 (i.e. `Some(1)` fails the first flush).
    /// `Some(0)` is treated equivalently to `None` (the counter starts at 1
    /// after increment, so the 0th call never matches).
    /// `None` means the mock always succeeds.
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
/// gRPC is the default (Steven, 2026-06-02). Variants for queue transports
/// (Kafka, RabbitMQ, Redis) are intentionally absent in this phase; they
/// return in a dedicated follow-on plan when consumers need queue
/// publication.
///
/// Wire shape: `{"transport": "grpc", "params": { ... }}`.
///
/// # Feature gating
///
/// `is_enabled()` is feature-aware natively. `Grpc` is gated on
/// `cfg!(feature = "transport-grpc")`, `Http` on
/// `cfg!(feature = "transport-http")`, and `Mock` is always available.
/// `WyrdClient::new()` (PR4.0 section 22) calls this method and returns
/// `WYRD_CLIENT_400_TRANSPORT_FEATURE_DISABLED { transport, required_feature }`
/// when the selected variant is not compiled into the local build.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "transport", content = "params", rename_all = "snake_case")]
pub enum TransportConfig {
    /// gRPC transport (tonic). The default transport. Gated on `transport-grpc`.
    Grpc(GrpcConfig),
    /// HTTP transport (reqwest). Secondary path for gRPC-restricted environments.
    /// Gated on `transport-http`.
    Http(HttpConfig),
    /// In-memory loopback transport for tests. Always available; no feature gate.
    Mock(MockConfig),
}

impl Default for TransportConfig {
    /// Returns `TransportConfig::Grpc(GrpcConfig::default())`.
    fn default() -> Self {
        Self::Grpc(GrpcConfig::default())
    }
}

impl TransportConfig {
    /// Short ASCII name matching the serde tag. Values: `"grpc"`, `"http"`,
    /// `"mock"`.
    ///
    /// Useful for log annotations and metric labels.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Grpc(_) => "grpc",
            Self::Http(_) => "http",
            Self::Mock(_) => "mock",
        }
    }

    /// Returns the Cargo feature name that gates this variant, or `None` if
    /// the variant is always available. Used by `WyrdClient::new()` (PR4.0)
    /// to fill the `required_feature` field of
    /// `WYRD_CLIENT_400_TRANSPORT_FEATURE_DISABLED`.
    pub fn required_feature(&self) -> Option<&'static str> {
        match self {
            Self::Grpc(_) => Some("transport-grpc"),
            Self::Http(_) => Some("transport-http"),
            Self::Mock(_) => None,
        }
    }

    /// Returns `true` if the variant is compiled into the current build.
    ///
    /// `Mock` is always enabled. `Grpc` is enabled when the `transport-grpc`
    /// Cargo feature is on; `Http` requires `transport-http`.
    ///
    /// `WyrdClient::new()` (PR4.0 section 22) calls this and returns
    /// `Err(WyrdClientError::TransportFeatureDisabled { transport, required_feature })`
    /// when the selected variant is compiled out, rather than panicking at
    /// first use. Note: `WyrdClientError::TransportFeatureDisabled` is not
    /// yet in the enum — that variant lands in PR4.0 when `WyrdClient::new()`
    /// is implemented.
    pub fn is_enabled(&self) -> bool {
        match self {
            Self::Grpc(_) => cfg!(feature = "transport-grpc"),
            Self::Http(_) => cfg!(feature = "transport-http"),
            Self::Mock(_) => true,
        }
    }

    /// Validate the wrapped transport config.
    ///
    /// Delegates to the inner config's `validate()`. `Mock` always returns
    /// `Ok(())` — the mock transport has no invalid field combinations.
    pub fn validate(&self) -> Result<(), WyrdClientError> {
        match self {
            Self::Grpc(c) => c.validate(),
            Self::Http(c) => c.validate(),
            Self::Mock(_) => Ok(()),
        }
    }
}

/// Shared queue policy for all per-record queues.
///
/// Every per-record queue in `wyrd-client` (`PsiFeatureQueue`,
/// `SpcFeatureQueue`, `CustomMetricQueue`, `EvalRecordQueue`,
/// `AgentTaskQueue`, `DatasetQueue`, `ObservationQueue`, `SpanQueue`,
/// `QueueBus`) embeds a `QueueConfig` and uses it to decide when to flush.
///
/// # Validation
///
/// Call [`QueueConfig::validate`] before passing a config to a queue
/// constructor. The constructor in `wyrd-client` must call `validate` and
/// propagate the error rather than panicking.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QueueConfig {
    /// Transport variant this queue sends to.
    pub transport: TransportConfig,

    /// Flush when the buffer reaches this row count. Must be at least 1.
    ///
    /// Default: `10_000` (predecessor parity).
    pub flush_max_rows: usize,

    /// Flush at most every N milliseconds regardless of buffer fill. Must be
    /// at least 1. Default: `5_000` (predecessor parity).
    pub flush_interval_ms: u64,

    /// Bounded channel capacity from the user thread to the flush worker.
    /// Must be at least 1. Default: `100` (predecessor parity).
    pub channel_capacity: usize,

    /// Optional drop gate. When `Some(p)`, each record is kept with
    /// probability `p` and dropped otherwise. `p` must be in `[0.0, 1.0]`.
    /// `None` means no sampling (all records kept).
    ///
    /// Predecessor behavior scattered this across per-queue types. Wyrd
    /// lifts it to `QueueConfig` so all queues share one policy.
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
    /// Validate the config. Returns `Err` on any of:
    ///
    /// - `flush_max_rows < 1`
    /// - `flush_interval_ms < 1`
    /// - `channel_capacity < 1`
    /// - `sample_ratio` is `Some(p)` with `p < 0.0`, `p > 1.0`, or `p` is NaN
    ///
    /// Note: this method does not recursively validate the embedded
    /// `transport`. Call `transport.validate()` (or the inner
    /// `GrpcConfig::validate` / `HttpConfig::validate`) separately before
    /// passing to a queue constructor. Callers who skip this step may receive
    /// `Ok(())` here but see a `TransportDown` error at connect time.
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
        if let Some(ratio) = self.sample_ratio
            && !(0.0..=1.0).contains(&ratio)
        {
            return Err(WyrdClientError::Config {
                field: "queue_config.sample_ratio".to_string(),
                reason: format!("must be in [0.0, 1.0], got {ratio}"),
            });
        }
        Ok(())
    }
}
