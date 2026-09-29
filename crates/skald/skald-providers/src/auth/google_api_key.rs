//! Google AI Studio API-key authentication.

use std::fmt;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use secrecy::{ExposeSecret, SecretString};
use url::Url;

use crate::error::{ProviderError, ProviderResult};

/// Google AI Studio API-key auth.
#[derive(Clone)]
pub struct GoogleApiKeyAuth {
    api_key: SecretString,
    base_url: String,
}

impl GoogleApiKeyAuth {
    /// Creates Google API-key auth.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: super::secret(api_key),
            base_url: "https://generativelanguage.googleapis.com".to_owned(),
        }
    }

    /// Loads Google API-key auth from `GOOGLE_API_KEY` or `GEMINI_API_KEY`.
    pub fn from_env() -> ProviderResult<Self> {
        let api_key = std::env::var("GOOGLE_API_KEY")
            .or_else(|_| std::env::var("GEMINI_API_KEY"))
            .map_err(|_| {
                ProviderError::auth("google", "GOOGLE_API_KEY or GEMINI_API_KEY is not set")
            })?;
        let mut auth = Self::new(api_key);
        if let Ok(base_url) = std::env::var("GOOGLE_API_BASE_URL") {
            let parsed = Url::parse(&base_url).map_err(|_| {
                ProviderError::auth("google", "GOOGLE_API_BASE_URL is not a valid URL")
            })?;
            if parsed.scheme() != "https" {
                return Err(ProviderError::auth(
                    "google",
                    "GOOGLE_API_BASE_URL must use https scheme",
                ));
            }
            auth.base_url = base_url;
        }
        Ok(auth)
    }

    /// Overrides the base API URL, primarily for local tests.
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Returns the configured base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Returns the URL of `path` under the base URL.
    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// Builds the sensitive `x-goog-api-key` header.
    ///
    /// The key never enters the URL, where transport errors and access logs
    /// would record it.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Decode`] when the key is not a valid header
    /// value.
    pub fn headers(&self) -> ProviderResult<HeaderMap> {
        let mut value = HeaderValue::from_str(self.api_key.expose_secret())
            .map_err(|error| ProviderError::decode("google", error))?;
        value.set_sensitive(true);
        let mut headers = HeaderMap::new();
        headers.insert(HeaderName::from_static("x-goog-api-key"), value);
        Ok(headers)
    }
}

impl fmt::Debug for GoogleApiKeyAuth {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GoogleApiKeyAuth")
            .field("api_key", &"<redacted>")
            .field("base_url", &self.base_url)
            .finish()
    }
}
