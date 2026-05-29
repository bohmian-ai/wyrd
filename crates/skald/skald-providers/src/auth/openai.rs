//! OpenAI bearer-token authentication.

use std::fmt;

use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderName, HeaderValue};
use secrecy::{ExposeSecret, SecretString};

use crate::error::{ProviderError, ProviderResult};

/// OpenAI auth material and endpoint configuration.
#[derive(Clone)]
pub struct OpenAiAuth {
    api_key: SecretString,
    organization: Option<String>,
    project: Option<String>,
    base_url: String,
}

impl OpenAiAuth {
    /// Creates OpenAI auth from an API key.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: super::secret(api_key),
            organization: None,
            project: None,
            base_url: "https://api.openai.com/v1".to_owned(),
        }
    }

    /// Loads OpenAI auth from environment variables.
    pub fn from_env() -> ProviderResult<Self> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| ProviderError::auth("openai", "OPENAI_API_KEY is not set"))?;
        let mut auth = Self::new(api_key);
        auth.organization = std::env::var("OPENAI_ORG_ID").ok();
        auth.project = std::env::var("OPENAI_PROJECT_ID").ok();
        if let Ok(base_url) = std::env::var("OPENAI_BASE_URL") {
            auth.base_url = base_url;
        }
        Ok(auth)
    }

    /// Overrides the base API URL, primarily for local tests.
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Sets the optional organization header.
    pub fn with_organization(mut self, organization: impl Into<String>) -> Self {
        self.organization = Some(organization.into());
        self
    }

    /// Sets the optional project header.
    pub fn with_project(mut self, project: impl Into<String>) -> Self {
        self.project = Some(project.into());
        self
    }

    /// Returns the configured base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Builds OpenAI request headers with secrets redacted from errors.
    pub fn headers(&self) -> ProviderResult<HeaderMap> {
        let mut headers = HeaderMap::new();
        let value = format!("Bearer {}", self.api_key.expose_secret());
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&value)
                .map_err(|error| ProviderError::decode("openai", error))?,
        );
        if let Some(organization) = &self.organization {
            headers.insert(
                HeaderName::from_static("openai-organization"),
                HeaderValue::from_str(organization)
                    .map_err(|error| ProviderError::decode("openai", error))?,
            );
        }
        if let Some(project) = &self.project {
            headers.insert(
                HeaderName::from_static("openai-project"),
                HeaderValue::from_str(project)
                    .map_err(|error| ProviderError::decode("openai", error))?,
            );
        }
        Ok(headers)
    }
}

impl fmt::Debug for OpenAiAuth {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiAuth")
            .field("api_key", &"<redacted>")
            .field("organization", &self.organization)
            .field("project", &self.project)
            .field("base_url", &self.base_url)
            .finish()
    }
}
