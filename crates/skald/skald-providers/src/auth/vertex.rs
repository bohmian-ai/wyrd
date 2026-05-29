//! Vertex AI bearer-token authentication.

use std::fmt;

use reqwest::header::HeaderMap;

use crate::auth::google_oauth::{GoogleOAuth, oauth_headers};
use crate::error::ProviderResult;

/// Vertex auth and endpoint path components.
pub struct VertexAuth {
    project: String,
    location: String,
    base_url: String,
    oauth: GoogleOAuth,
}

impl VertexAuth {
    /// Creates Vertex auth from explicit project, location, and OAuth loader.
    pub fn new(
        project: impl Into<String>,
        location: impl Into<String>,
        oauth: GoogleOAuth,
    ) -> Self {
        Self {
            project: project.into(),
            location: location.into(),
            base_url: "https://aiplatform.googleapis.com".to_owned(),
            oauth,
        }
    }

    /// Loads Vertex auth from environment variables and ADC.
    pub fn from_env() -> ProviderResult<Self> {
        let project = std::env::var("GOOGLE_CLOUD_PROJECT")
            .or_else(|_| std::env::var("VERTEX_PROJECT"))
            .map_err(|_| crate::ProviderError::auth("vertex", "GOOGLE_CLOUD_PROJECT is not set"))?;
        let location = std::env::var("GOOGLE_CLOUD_LOCATION")
            .or_else(|_| std::env::var("VERTEX_LOCATION"))
            .unwrap_or_else(|_| "us-central1".to_owned());
        Ok(Self::new(project, location, GoogleOAuth::from_env()?))
    }

    /// Overrides the base API URL, primarily for local tests.
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Returns a Vertex model endpoint URL for the supplied operation suffix.
    pub fn model_url(&self, model: &str, suffix: &str) -> String {
        format!(
            "{}/v1/projects/{}/locations/{}/publishers/google/models/{}:{}",
            self.base_url,
            urlencoding::encode(&self.project),
            urlencoding::encode(&self.location),
            urlencoding::encode(model),
            suffix,
        )
    }

    /// Builds Vertex bearer headers.
    pub async fn headers(&self) -> ProviderResult<HeaderMap> {
        oauth_headers("vertex", &self.oauth).await
    }
}

impl fmt::Debug for VertexAuth {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VertexAuth")
            .field("project", &self.project)
            .field("location", &self.location)
            .field("base_url", &self.base_url)
            .field("oauth", &"<redacted>")
            .finish()
    }
}
