//! Google OAuth token loading for ADC-style mocked provider tests.

use std::fmt;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use base64::Engine;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use secrecy::SecretString;
use serde::Deserialize;

use crate::error::{ProviderError, ProviderResult};

const METADATA_PATH: &str = "/computeMetadata/v1/instance/service-accounts/default/token";

/// Cached OAuth bearer token.
#[derive(Clone)]
pub struct GoogleOAuthToken {
    /// Access token value.
    pub access_token: SecretString,
    /// Seconds until expiry.
    pub expires_in: u64,
}

impl fmt::Debug for GoogleOAuthToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GoogleOAuthToken")
            .field("access_token", &"<redacted>")
            .field("expires_in", &self.expires_in)
            .finish()
    }
}

/// Google OAuth ADC loader with a small in-memory token cache.
pub struct GoogleOAuth {
    source: GoogleOAuthSource,
    http: reqwest::Client,
    cache: Mutex<Option<CachedToken>>,
}

#[derive(Clone)]
enum GoogleOAuthSource {
    InlineJson(String),
    CredentialsFile(PathBuf),
    Metadata { base_url: String },
}

struct CachedToken {
    token: GoogleOAuthToken,
    expires_at: Instant,
}

#[derive(Deserialize)]
struct TokenJson {
    access_token: Option<String>,
    expires_in: Option<u64>,
    token_uri: Option<String>,
}

impl GoogleOAuth {
    /// Creates an OAuth loader from inline account JSON.
    pub fn from_account_json(json: impl Into<String>) -> ProviderResult<Self> {
        Self::new(GoogleOAuthSource::InlineJson(json.into()))
    }

    /// Creates an OAuth loader from an ADC credentials file.
    pub fn from_credentials_file(path: impl Into<PathBuf>) -> ProviderResult<Self> {
        Self::new(GoogleOAuthSource::CredentialsFile(path.into()))
    }

    /// Creates an OAuth loader using a metadata-server base URL.
    pub fn from_metadata_server(base_url: impl Into<String>) -> ProviderResult<Self> {
        Self::new(GoogleOAuthSource::Metadata {
            base_url: base_url.into(),
        })
    }

    /// Resolves the supported ADC chain without making network calls yet.
    pub fn from_env() -> ProviderResult<Self> {
        if let Ok(encoded) = std::env::var("GOOGLE_ACCOUNT_JSON_BASE64") {
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|error| ProviderError::decode("google", error))?;
            let json = String::from_utf8(decoded)
                .map_err(|error| ProviderError::decode("google", error))?;
            return Self::from_account_json(json);
        }
        if let Ok(path) = std::env::var("GOOGLE_APPLICATION_CREDENTIALS") {
            return Self::from_credentials_file(path);
        }
        if let Some(path) = gcloud_adc_path() {
            if path.exists() {
                return Self::from_credentials_file(path);
            }
        }
        let base_url = std::env::var("GCE_METADATA_HOST")
            .map(|host| format!("http://{host}"))
            .unwrap_or_else(|_| "http://metadata.google.internal".to_owned());
        Self::from_metadata_server(base_url)
    }

    fn new(source: GoogleOAuthSource) -> ProviderResult<Self> {
        let http = reqwest::Client::builder()
            .build()
            .map_err(|error| ProviderError::decode("google", error))?;
        Ok(Self {
            source,
            http,
            cache: Mutex::new(None),
        })
    }

    /// Returns a bearer token, reusing cached tokens until the refresh grace.
    pub async fn token(&self) -> ProviderResult<GoogleOAuthToken> {
        if let Some(token) = self.cached_token() {
            return Ok(token);
        }

        let token = match &self.source {
            GoogleOAuthSource::InlineJson(json) => self.token_from_json(json).await?,
            GoogleOAuthSource::CredentialsFile(path) => {
                let json = std::fs::read_to_string(path)
                    .map_err(|error| ProviderError::auth("google", error.to_string()))?;
                self.token_from_json(&json).await?
            }
            GoogleOAuthSource::Metadata { base_url } => self.token_from_metadata(base_url).await?,
        };
        self.store_token(token.clone());
        Ok(token)
    }

    fn cached_token(&self) -> Option<GoogleOAuthToken> {
        let cache = self.lock_cache();
        let cached = cache.as_ref()?;
        let refresh_grace = Duration::from_secs(30);
        if cached.expires_at > Instant::now() + refresh_grace {
            Some(cached.token.clone())
        } else {
            None
        }
    }

    fn store_token(&self, token: GoogleOAuthToken) {
        let expires_at = Instant::now() + Duration::from_secs(token.expires_in);
        *self.lock_cache() = Some(CachedToken { token, expires_at });
    }

    fn lock_cache(&self) -> MutexGuard<'_, Option<CachedToken>> {
        match self.cache.lock() {
            Ok(cache) => cache,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    async fn token_from_json(&self, json: &str) -> ProviderResult<GoogleOAuthToken> {
        let parsed: TokenJson =
            serde_json::from_str(json).map_err(|error| ProviderError::decode("google", error))?;
        if let Some(access_token) = parsed.access_token {
            return Ok(GoogleOAuthToken {
                access_token: super::secret(access_token),
                expires_in: parsed.expires_in.unwrap_or(3600),
            });
        }
        let Some(token_uri) = parsed.token_uri else {
            return Err(ProviderError::auth(
                "google",
                "ADC JSON did not contain access_token or token_uri",
            ));
        };
        let response = self
            .http
            .post(token_uri)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", "mocked-jwt-assertion"),
            ])
            .send()
            .await
            .map_err(|error| map_reqwest_error("google", error))?;
        decode_token_response(response).await
    }

    async fn token_from_metadata(&self, base_url: &str) -> ProviderResult<GoogleOAuthToken> {
        let url = format!("{base_url}{METADATA_PATH}");
        let response = self
            .http
            .get(url)
            .header("Metadata-Flavor", "Google")
            .send()
            .await
            .map_err(|error| map_reqwest_error("google", error))?;
        decode_token_response(response).await
    }
}

impl fmt::Debug for GoogleOAuth {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GoogleOAuth")
            .field("source", &"<redacted>")
            .field("cache", &"<redacted>")
            .finish()
    }
}

async fn decode_token_response(response: reqwest::Response) -> ProviderResult<GoogleOAuthToken> {
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| map_reqwest_error("google", error))?;
    if !status.is_success() {
        return Err(ProviderError::from_status("google", status, body, None));
    }
    let parsed: TokenJson =
        serde_json::from_str(&body).map_err(|error| ProviderError::decode("google", error))?;
    let Some(access_token) = parsed.access_token else {
        return Err(ProviderError::decode(
            "google",
            "token response missing access_token",
        ));
    };
    Ok(GoogleOAuthToken {
        access_token: super::secret(access_token),
        expires_in: parsed.expires_in.unwrap_or(3600),
    })
}

pub(crate) async fn oauth_headers(
    provider: &str,
    oauth: &GoogleOAuth,
) -> ProviderResult<HeaderMap> {
    let token = oauth.token().await?;
    let mut headers = HeaderMap::new();
    let value = format!(
        "Bearer {}",
        secrecy::ExposeSecret::expose_secret(&token.access_token)
    );
    headers.insert(
        reqwest::header::AUTHORIZATION,
        HeaderValue::from_str(&value).map_err(|error| ProviderError::decode(provider, error))?,
    );
    headers.insert(
        HeaderName::from_static("x-goog-api-client"),
        HeaderValue::from_static("skald-providers"),
    );
    Ok(headers)
}

fn map_reqwest_error(provider: &str, error: reqwest::Error) -> ProviderError {
    if error.is_timeout() {
        ProviderError::timeout(provider)
    } else {
        ProviderError::upstream(provider, 0, error.to_string())
    }
}

fn gcloud_adc_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(
        PathBuf::from(home)
            .join(".config")
            .join("gcloud")
            .join("application_default_credentials.json"),
    )
}
