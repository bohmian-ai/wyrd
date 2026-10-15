//! TypeSafe Jev bearer API-key authentication.

use std::fmt;

use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use secrecy::{ExposeSecret, SecretString};

use crate::error::{ProviderError, ProviderResult};

/// Jev auth material and endpoint configuration.
///
/// Presents the TypeSafe API key as `Authorization: Bearer <key>` to the
/// production TypeSafe API unless [`Self::with_base_url`] overrides it.
#[derive(Clone)]
pub struct JevAuth {
    /// TypeSafe API key, redacted from `Debug`.
    api_key: SecretString,
    /// API base URL the `systemone` route is joined to.
    base_url: String,
}

impl JevAuth {
    /// Creates Jev auth from an API key against the production TypeSafe API.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: super::secret(api_key),
            base_url: "https://api.typesafe.ai".to_owned(),
        }
    }

    /// Loads Jev auth from `TYPESAFE_API_KEY`.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Auth`] when the variable is unset or empty.
    pub fn from_env() -> ProviderResult<Self> {
        let api_key = std::env::var("TYPESAFE_API_KEY")
            .ok()
            .filter(|key| !key.is_empty())
            .ok_or_else(|| ProviderError::auth("jev", "TYPESAFE_API_KEY is not set"))?;
        Ok(Self::new(api_key))
    }

    /// Overrides the base API URL, primarily for local tests and gateways.
    #[must_use]
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Returns the configured base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Builds the bearer header, marked sensitive so it never appears in
    /// debug output.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Decode`] when the key is not a valid header
    /// value.
    pub fn headers(&self) -> ProviderResult<HeaderMap> {
        let mut value = HeaderValue::from_str(&format!("Bearer {}", self.api_key.expose_secret()))
            .map_err(|error| ProviderError::decode("jev", error))?;
        value.set_sensitive(true);
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, value);
        Ok(headers)
    }
}

impl fmt::Debug for JevAuth {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("JevAuth")
            .field("api_key", &"<redacted>")
            .field("base_url", &self.base_url)
            .finish()
    }
}
