//! Reqwest transport configuration for provider clients.

use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};

use crate::error::{ProviderError, ProviderResult};

const DEFAULT_USER_AGENT: &str = "skald-providers/0.0.1";

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
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(5),
            pool_max_idle_per_host: 8,
            user_agent: DEFAULT_USER_AGENT.to_owned(),
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
        wyrd_tls::install_crypto_provider().map_err(|error| ProviderError::Upstream {
            provider: "transport".to_owned(),
            status: 0,
            body: error.to_string(),
        })?;
        let mut headers = HeaderMap::new();
        let user_agent = HeaderValue::from_str(&config.user_agent)
            .map_err(|error| ProviderError::decode("transport", error))?;
        headers.insert(USER_AGENT, user_agent);

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(config.timeout)
            .connect_timeout(config.connect_timeout)
            .pool_max_idle_per_host(config.pool_max_idle_per_host)
            .gzip(true)
            .brotli(true)
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
    }

    #[test]
    fn transport_builds_reqwest_client() {
        let transport = HttpTransport::new(TransportConfig::default()).expect("transport builds");

        assert_eq!(transport.config().pool_max_idle_per_host, 8);
    }
}
