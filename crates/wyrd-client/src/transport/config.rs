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
