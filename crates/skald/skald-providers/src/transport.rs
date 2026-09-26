//! Reqwest transport configuration for provider clients.

use std::time::Duration;

use reqwest::ClientBuilder;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};

use crate::error::{ProviderError, ProviderResult};

const DEFAULT_USER_AGENT: &str = "skald-providers/0.0.1";

/// Default bound on one decoded provider answer.
const DEFAULT_MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;

/// HTTP transport configuration shared by provider clients.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportConfig {
    /// Total request timeout.
    pub timeout: Duration,
    /// TCP/TLS connection timeout.
    pub connect_timeout: Duration,
    /// Maximum idle pooled connections per host.
    pub pool_max_idle_per_host: usize,
    /// Default user-agent header value.
    pub user_agent: String,
    /// Largest decoded answer body a client reads before failing.
    pub max_response_bytes: usize,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(5),
            pool_max_idle_per_host: 8,
            user_agent: DEFAULT_USER_AGENT.to_owned(),
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
        }
    }
}

/// Thin wrapper around `reqwest::Client` with inspectable config.
#[derive(Debug, Clone)]
pub struct HttpTransport {
    client: reqwest::Client,
    config: TransportConfig,
}

impl HttpTransport {
    /// Builds a reqwest client with the supplied provider-safe defaults.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Upstream`] when another Rustls provider already
    /// owns the process or Reqwest cannot build the client. Invalid user-agent
    /// values are returned as provider decode errors.
    pub fn new(config: TransportConfig) -> ProviderResult<Self> {
        Self::customized(config, |builder| builder)
    }

    /// Builds the client from `config`, then lets `customize` add egress
    /// policy such as a DNS resolver, redirect policy, or proxy settings.
    ///
    /// Every provider transport is built here, so the configured timeouts,
    /// pool, user agent, and decompression always apply.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Upstream`] when another Rustls provider already
    /// owns the process or Reqwest cannot build the client. Invalid user-agent
    /// values are returned as provider decode errors.
    pub fn customized(
        config: TransportConfig,
        customize: impl FnOnce(ClientBuilder) -> ClientBuilder,
    ) -> ProviderResult<Self> {
        wyrd_tls::install_crypto_provider().map_err(|error| ProviderError::Upstream {
            provider: "transport".to_owned(),
            status: 0,
            body: error.to_string(),
        })?;
        let mut headers = HeaderMap::new();
        let user_agent = HeaderValue::from_str(&config.user_agent)
            .map_err(|error| ProviderError::decode("transport", error))?;
        headers.insert(USER_AGENT, user_agent);

        let builder = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(config.timeout)
            .connect_timeout(config.connect_timeout)
            .pool_max_idle_per_host(config.pool_max_idle_per_host)
            .gzip(true)
            .brotli(true);
        let client = customize(builder)
            .build()
            .map_err(|error| ProviderError::decode("transport", error))?;

        Ok(Self { client, config })
    }

    /// Returns the configured reqwest client.
    pub const fn client(&self) -> &reqwest::Client {
        &self.client
    }

    /// Returns the transport configuration used to build this client.
    pub const fn config(&self) -> &TransportConfig {
        &self.config
    }
}

#[cfg(test)]
mod transport_config {
    use std::time::Duration;

    use crate::{HttpTransport, TransportConfig};

    #[test]
    fn transport_defaults_match_plan() {
        let config = TransportConfig::default();

        assert_eq!(config.timeout, Duration::from_secs(30));
        assert_eq!(config.connect_timeout, Duration::from_secs(5));
        assert_eq!(config.pool_max_idle_per_host, 8);
        assert!(config.user_agent.contains("skald-providers"));
        assert_eq!(config.max_response_bytes, 16 * 1024 * 1024);
    }

    #[test]
    fn transport_builds_reqwest_client() {
        let transport = HttpTransport::new(TransportConfig::default()).expect("transport builds");

        assert_eq!(transport.config().pool_max_idle_per_host, 8);
    }
}
