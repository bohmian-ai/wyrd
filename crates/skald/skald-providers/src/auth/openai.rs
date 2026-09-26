//! OpenAI and OpenAI-compatible authentication.

use std::fmt;

use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderName, HeaderValue};
use secrecy::{ExposeSecret, SecretString};
use url::Url;

use crate::error::{ProviderError, ProviderResult};

/// How an OpenAI-compatible endpoint receives its API key.
#[derive(Clone)]
enum Credential {
    /// The endpoint needs no authentication.
    None,
    /// `Authorization: Bearer <key>`, as OpenAI itself expects.
    Bearer(SecretString),
    /// The raw key in a provider-named header.
    Header(HeaderName, SecretString),
}

/// OpenAI auth material and endpoint configuration.
#[derive(Clone)]
pub struct OpenAiAuth {
    credential: Credential,
    organization: Option<String>,
    project: Option<String>,
    base_url: String,
}

impl OpenAiAuth {
    /// Creates OpenAI bearer auth from an API key.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self::with_credential(Credential::Bearer(super::secret(api_key)))
    }

    /// Creates auth for a compatible endpoint that reads the raw key from
    /// `header`.
    pub fn with_key_header(header: HeaderName, api_key: impl Into<String>) -> Self {
        Self::with_credential(Credential::Header(header, super::secret(api_key)))
    }

    /// Creates auth for a compatible endpoint that needs no credential.
    pub fn unauthenticated() -> Self {
        Self::with_credential(Credential::None)
    }

    /// Auth presenting `credential` to the default OpenAI base URL.
    fn with_credential(credential: Credential) -> Self {
        Self {
            credential,
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
            let parsed = Url::parse(&base_url)
                .map_err(|_| ProviderError::auth("openai", "OPENAI_BASE_URL is not a valid URL"))?;
            if parsed.scheme() != "https" {
                return Err(ProviderError::auth(
                    "openai",
                    "OPENAI_BASE_URL must use https scheme",
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

    /// Builds request headers; the credential header is marked sensitive so
    /// it never appears in debug output. Bearer, provider-named header, and
    /// unauthenticated credentials each add exactly their own header.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Decode`] when the credential, header name value,
    /// organization, or project is not a valid header value.
    pub fn headers(&self) -> ProviderResult<HeaderMap> {
        let mut headers = HeaderMap::new();
        let secret = |value: &str| {
            let mut value = HeaderValue::from_str(value)
                .map_err(|error| ProviderError::decode("openai", error))?;
            value.set_sensitive(true);
            Ok::<_, ProviderError>(value)
        };
        match &self.credential {
            Credential::None => {}
            Credential::Bearer(key) => {
                headers.insert(
                    AUTHORIZATION,
                    secret(&format!("Bearer {}", key.expose_secret()))?,
                );
            }
            Credential::Header(name, key) => {
                headers.insert(name.clone(), secret(key.expose_secret())?);
            }
        }
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

#[cfg(test)]
mod openai_auth_headers {
    use reqwest::header::{AUTHORIZATION, HeaderName};

    use super::OpenAiAuth;

    /// Bearer, named-header, and unauthenticated credentials each produce
    /// exactly their own sensitive header.
    #[test]
    fn credentials_produce_their_own_sensitive_header() {
        let bearer = OpenAiAuth::new("sk").headers().expect("headers");
        assert_eq!(bearer[AUTHORIZATION], "Bearer sk");
        assert!(bearer[AUTHORIZATION].is_sensitive());

        let named = OpenAiAuth::with_key_header(HeaderName::from_static("api-key"), "k")
            .headers()
            .expect("headers");
        assert_eq!(named["api-key"], "k");
        assert!(named["api-key"].is_sensitive());
        assert!(!named.contains_key(AUTHORIZATION));

        assert!(
            OpenAiAuth::unauthenticated()
                .headers()
                .expect("headers")
                .is_empty()
        );
    }
}
