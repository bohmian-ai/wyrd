//! Anthropic API-key authentication.

use std::fmt;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use secrecy::{ExposeSecret, SecretString};

use crate::error::{ProviderError, ProviderResult};

/// Anthropic auth material and endpoint configuration.
#[derive(Clone)]
pub struct AnthropicAuth {
    api_key: SecretString,
    version: String,
    betas: Option<String>,
    base_url: String,
}

impl AnthropicAuth {
    /// Creates Anthropic auth from an API key.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: super::secret(api_key),
            version: "2023-06-01".to_owned(),
            betas: None,
            base_url: "https://api.anthropic.com".to_owned(),
        }
    }

    /// Loads Anthropic auth from environment variables.
    pub fn from_env() -> ProviderResult<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .map_err(|_| ProviderError::auth("anthropic", "ANTHROPIC_API_KEY is not set"))?;
        let mut auth = Self::new(api_key);
        if let Ok(version) = std::env::var("ANTHROPIC_VERSION") {
            auth.version = version;
        }
        auth.betas = std::env::var("ANTHROPIC_BETA").ok();
        if let Ok(base_url) = std::env::var("ANTHROPIC_BASE_URL") {
            auth.base_url = base_url;
        }
        Ok(auth)
    }

    /// Overrides the base API URL, primarily for local tests.
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Sets the optional beta header value.
    pub fn with_betas(mut self, betas: impl Into<String>) -> Self {
        self.betas = Some(betas.into());
        self
    }

    /// Returns the configured base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Builds Anthropic request headers with secrets redacted from errors.
    pub fn headers(&self) -> ProviderResult<HeaderMap> {
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("x-api-key"),
            HeaderValue::from_str(self.api_key.expose_secret())
                .map_err(|error| ProviderError::decode("anthropic", error))?,
        );
        headers.insert(
            HeaderName::from_static("anthropic-version"),
            HeaderValue::from_str(&self.version)
                .map_err(|error| ProviderError::decode("anthropic", error))?,
        );
        if let Some(betas) = &self.betas {
            headers.insert(
                HeaderName::from_static("anthropic-beta"),
                HeaderValue::from_str(betas)
                    .map_err(|error| ProviderError::decode("anthropic", error))?,
            );
        }
        Ok(headers)
    }
}

impl fmt::Debug for AnthropicAuth {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnthropicAuth")
            .field("api_key", &"<redacted>")
            .field("version", &self.version)
            .field("betas", &self.betas)
            .field("base_url", &self.base_url)
            .finish()
    }
}
