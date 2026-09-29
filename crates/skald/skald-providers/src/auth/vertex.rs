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
    ///
    /// The base URL is the location's regional endpoint, which serves models
    /// deployed in that location; the `global` location, or one that is not a
    /// plain hostname label, uses the global endpoint.
    pub fn new(
        project: impl Into<String>,
        location: impl Into<String>,
        oauth: GoogleOAuth,
    ) -> Self {
        let location = location.into();
        let regional = location != "global"
            && !location.is_empty()
            && location
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-');
        let base_url = if regional {
            format!("https://{location}-aiplatform.googleapis.com")
        } else {
            "https://aiplatform.googleapis.com".to_owned()
        };
        Self {
            project: project.into(),
            location,
            base_url,
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

    /// Returns the configured base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
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

#[cfg(test)]
mod vertex_endpoint {
    use crate::auth::{GoogleOAuth, VertexAuth};

    /// Regional locations use their regional host; `global` and unsafe
    /// location text use the global host.
    #[test]
    fn locations_select_regional_or_global_hosts() {
        let auth = |location: &str| {
            VertexAuth::new(
                "p",
                location,
                GoogleOAuth::from_access_token("t").expect("oauth"),
            )
        };
        assert_eq!(
            auth("us-central1").base_url(),
            "https://us-central1-aiplatform.googleapis.com"
        );
        assert_eq!(
            auth("global").base_url(),
            "https://aiplatform.googleapis.com"
        );
        assert_eq!(
            auth("evil.example/x").base_url(),
            "https://aiplatform.googleapis.com"
        );
    }
}
