//! Wyrd client transport configuration types.

use serde::{Deserialize, Serialize};

use crate::error::WyrdClientError;

/// Default endpoint for [`GrpcConfig`]: the public gRPC port on the default
/// HTTP host, i.e. [`grpc_endpoint_for`] of [`HTTP_DEFAULT_BASE_URL`].
pub const GRPC_DEFAULT_ENDPOINT: &str = "http://localhost:50051";
/// Public gRPC port a deployment serves beside its HTTP `server_url`.
pub const GRPC_DEFAULT_PORT: u16 = 50051;

/// Derive the default gRPC endpoint from an HTTP `server_url`.
///
/// Keeps the URL's scheme and host and replaces its port and path with the
/// public gRPC port, so a client configured with only `server_url` reaches
/// the gRPC listener of the same deployment. An unparseable or host-less
/// `server_url` is returned unchanged so the transport reports it rather than
/// silently dialing a different address.
#[must_use]
pub fn grpc_endpoint_for(server_url: &str) -> String {
    match reqwest::Url::parse(server_url) {
        Ok(url) => match url.host_str() {
            Some(host) => format!("{}://{host}:{GRPC_DEFAULT_PORT}", url.scheme()),
            None => server_url.to_owned(),
        },
        Err(_) => server_url.to_owned(),
    }
}
/// Default per-call timeout (ms) for [`GrpcConfig`].
pub const GRPC_DEFAULT_TIMEOUT_MS: u64 = 30_000;
/// Default connect-retry budget for [`GrpcConfig`].
pub const GRPC_DEFAULT_CONNECT_RETRIES: u32 = 3;
/// Default HTTP/2 keepalive ping interval (ms) for [`GrpcConfig`].
pub const GRPC_DEFAULT_KEEPALIVE_INTERVAL_MS: u64 = 20_000;
/// Default HTTP/2 keepalive ping timeout (ms) for [`GrpcConfig`].
pub const GRPC_DEFAULT_KEEPALIVE_TIMEOUT_MS: u64 = 5_000;
/// Default maximum gRPC message size in bytes for [`GrpcConfig`] (4 MiB).
pub const GRPC_DEFAULT_MAX_MESSAGE_BYTES: usize = 4 * 1024 * 1024;

/// Configuration for the gRPC transport.
///
/// gRPC is the default transport for `wyrd-client`. The server-side endpoint
/// is the Wyrd ingest gateway (`vala-ingest`, PR4.2) or any compatible
/// tonic-based receiver.
///
/// This config carries transport wiring only. Authentication is owned by
/// `AuthMiddleware`, which injects the `x-wyrd-access-token` credential per
/// call; there is no auth field here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GrpcConfig {
    /// gRPC server endpoint. Must be `http://host:port` (plaintext) or
    /// `https://host:port` (TLS). Must be non-empty. TLS verifies the server
    /// against the platform trust store; point `SSL_CERT_FILE` or
    /// `SSL_CERT_DIR` at a private CA.
    ///
    /// Default: [`GRPC_DEFAULT_ENDPOINT`] (`"http://localhost:50051"`).
    pub endpoint: String,

    /// Per-call timeout in milliseconds.
    ///
    /// Default: [`GRPC_DEFAULT_TIMEOUT_MS`] (`30_000`, 30 s).
    pub timeout_ms: u64,

    /// Connection-level retry budget. The transport retries the initial
    /// connection up to this many times before surfacing an error.
    ///
    /// Default: [`GRPC_DEFAULT_CONNECT_RETRIES`] (`3`).
    pub connect_retries: u32,

    /// HTTP/2 keepalive ping interval in milliseconds. `0` disables pings.
    ///
    /// Default: [`GRPC_DEFAULT_KEEPALIVE_INTERVAL_MS`] (`20_000`, 20 s).
    #[serde(default = "default_keepalive_interval_ms")]
    pub keepalive_interval_ms: u64,

    /// HTTP/2 keepalive ping timeout in milliseconds. The connection is closed
    /// if the peer does not respond within this window.
    ///
    /// Default: [`GRPC_DEFAULT_KEEPALIVE_TIMEOUT_MS`] (`5_000`, 5 s).
    #[serde(default = "default_keepalive_timeout_ms")]
    pub keepalive_timeout_ms: u64,

    /// Maximum gRPC message size in bytes (both encoding and decoding).
    ///
    /// Default: [`GRPC_DEFAULT_MAX_MESSAGE_BYTES`] (`4_194_304`, 4 MiB).
    #[serde(default = "default_max_message_bytes")]
    pub max_message_bytes: usize,
}

fn default_keepalive_interval_ms() -> u64 {
    GRPC_DEFAULT_KEEPALIVE_INTERVAL_MS
}

fn default_keepalive_timeout_ms() -> u64 {
    GRPC_DEFAULT_KEEPALIVE_TIMEOUT_MS
}

fn default_max_message_bytes() -> usize {
    GRPC_DEFAULT_MAX_MESSAGE_BYTES
}

impl Default for GrpcConfig {
    fn default() -> Self {
        Self {
            endpoint: GRPC_DEFAULT_ENDPOINT.to_string(),
            timeout_ms: GRPC_DEFAULT_TIMEOUT_MS,
            connect_retries: GRPC_DEFAULT_CONNECT_RETRIES,
            keepalive_interval_ms: GRPC_DEFAULT_KEEPALIVE_INTERVAL_MS,
            keepalive_timeout_ms: GRPC_DEFAULT_KEEPALIVE_TIMEOUT_MS,
            max_message_bytes: GRPC_DEFAULT_MAX_MESSAGE_BYTES,
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
pub const HTTP_DEFAULT_BASE_URL: &str = "http://localhost:8080";
/// Default per-request timeout (ms) for [`HttpConfig`].
pub const HTTP_DEFAULT_TIMEOUT_MS: u64 = 30_000;

/// Configuration for the HTTP transport.
///
/// The HTTP transport is a fallback for environments where gRPC is unavailable
/// or blocked. Ingest routes are appended by the client at call time;
/// `base_url` is the common prefix.
///
/// This config carries transport wiring only. Authentication is owned by
/// `AuthMiddleware`, which injects the `x-wyrd-access-token` credential per
/// call; there is no auth field here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HttpConfig {
    /// Base URL for all ingest routes. Must be non-empty. Ingest routes
    /// (`/api/v1/observations`, `/api/v1/records`, etc.) are appended at
    /// call time.
    ///
    /// Example: `"https://wyrd-ingest.example.com"`.
    /// Default: [`HTTP_DEFAULT_BASE_URL`] (`"http://localhost:8080"`).
    #[serde(default = "default_base_url")]
    pub base_url: String,

    /// HTTP timeout in milliseconds with two uses: it bounds connection
    /// establishment for every call, and it bounds the total request/response
    /// duration only for retried JSON and control calls. Streaming transfers
    /// and terminal query streams have no total deadline from this value.
    ///
    /// Default: [`HTTP_DEFAULT_TIMEOUT_MS`] (`30_000`, 30 s).
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,

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
            compression: false,
        }
    }
}

impl HttpConfig {
    /// Validate the config. Returns `Err` if `base_url` is empty or
    /// `timeout_ms` is zero.
    ///
    /// `base_url` must parse as an absolute `https://` URL, or an `http://`
    /// URL whose host is loopback (`localhost` or a loopback IP), because
    /// credentials may be sent to `{base_url}/auth/token` or in request
    /// headers. The URL parser decides scheme and host, so spellings such as
    /// `HTTP://` are judged the same way the HTTP client will send them.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::Config`] for an empty or unparsable
    /// `base_url`, a scheme other than `http` or `https`, remote cleartext
    /// `http`, or a zero `timeout_ms`.
    pub fn validate(&self) -> Result<(), WyrdClientError> {
        let invalid = |reason: &str| WyrdClientError::Config {
            field: "http_config.base_url".to_string(),
            reason: reason.to_string(),
        };
        if self.base_url.is_empty() {
            return Err(invalid("must not be empty"));
        }
        if self.timeout_ms == 0 {
            return Err(WyrdClientError::Config {
                field: "http_config.timeout_ms".to_string(),
                reason: "must be at least 1".to_string(),
            });
        }
        let url = reqwest::Url::parse(&self.base_url)
            .map_err(|error| invalid(&format!("not an absolute URL: {error}")))?;
        match url.scheme() {
            "https" => Ok(()),
            "http" if is_loopback(&url) => Ok(()),
            "http" => Err(invalid(
                "remote cleartext HTTP is not allowed; use https:// or a loopback host",
            )),
            _ => Err(invalid(
                "the scheme must be https, or http for a loopback host",
            )),
        }
    }
}

/// Whether the parsed `url` names a loopback host: `localhost` or a loopback
/// IP address. The parser has already lowercased the host.
fn is_loopback(url: &reqwest::Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    host == "localhost"
        || host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
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
/// `MockConfig::fail_on_drain` lets tests inject deterministic drain
/// failures — a Wyrd-native addition the predecessor mock did not support.
///
/// # Default
///
/// `MockConfig::default()` returns `{ label: "default", fail_on_drain: None }`.
/// The default is a working config — it does not need `validate()` and is
/// safe to use directly in tests that do not care about label isolation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MockConfig {
    /// Buffer label. Multiple `MockConfig` instances with the same label share
    /// the same in-memory buffer in `wyrd-client::transport::mock`. Use
    /// distinct labels to isolate test scenarios running in the same process.
    pub label: String,

    /// Inject a drain failure. When `Some(n)`, the `nth` call to drain
    /// returns `Err`; all other calls succeed normally.
    /// Counting starts from 1 (i.e. `Some(1)` fails the first drain).
    /// `None` means the mock always succeeds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fail_on_drain: Option<u32>,
}

impl Default for MockConfig {
    fn default() -> Self {
        Self {
            label: MOCK_DEFAULT_LABEL.to_string(),
            fail_on_drain: None,
        }
    }
}

/// Wyrd client transport selection.
///
/// gRPC is the default. Variants for queue transports (Kafka, RabbitMQ,
/// Redis) are intentionally absent in this phase.
///
/// Wire shape: `{"transport": "grpc", "params": { ... }}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "transport", content = "params", rename_all = "snake_case")]
pub enum TransportConfig {
    /// gRPC transport (tonic). The default transport.
    Grpc(GrpcConfig),
    /// HTTP transport (reqwest). Secondary path for gRPC-restricted environments.
    Http(HttpConfig),
    /// In-memory loopback transport for tests. Always available.
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

#[cfg(test)]
mod tests {
    use super::HttpConfig;

    /// The base-URL decision for `base_url`, through [`HttpConfig::validate`].
    fn accepts(base_url: &str) -> bool {
        HttpConfig {
            base_url: base_url.to_owned(),
            ..HttpConfig::default()
        }
        .validate()
        .is_ok()
    }

    /// Remote cleartext in any scheme spelling, malformed targets, and
    /// unsupported schemes are refused.
    #[test]
    fn remote_cleartext_malformed_and_unsupported_targets_are_refused() {
        for url in [
            "http://wyrd.example.com",
            "http://wyrd.example.com:8080/api",
            "http://10.0.0.5:8080",
            "HTTP://wyrd.example.com",
            "Http://wyrd.example.com",
            "http://localhost.example.com",
            "http://127.0.0.1.example.com",
            "wyrd.example.com",
            "https://",
            "http//wyrd.example.com",
            "ftp://wyrd.example.com",
            "file:///etc/passwd",
        ] {
            assert!(!accepts(url), "{url} must be refused");
        }
    }

    /// HTTPS in any spelling and loopback HTTP are accepted.
    #[test]
    fn https_and_loopback_http_are_accepted() {
        for url in [
            "https://wyrd.example.com",
            "HTTPS://Wyrd.Example.com/api",
            "http://localhost:8080",
            "HTTP://LOCALHOST:8080",
            "http://127.0.0.1:8080",
            "http://[::1]:8080",
        ] {
            assert!(accepts(url), "{url} must be accepted");
        }
    }
}
