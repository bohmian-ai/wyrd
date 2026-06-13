//! Google OAuth token loading for ADC-style provider authentication.

use std::fmt;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::error::{ProviderError, ProviderResult};

const METADATA_PATH: &str = "/computeMetadata/v1/instance/service-accounts/default/token";
const JWT_SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";
const TOKEN_URI_ALLOWED_HOSTS: &[&str] = &["oauth2.googleapis.com", "accounts.google.com"];

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
    // tokio::sync::Mutex held across the async fetch to prevent concurrent
    // token refreshes for the same instance.
    cache: tokio::sync::Mutex<Option<CachedToken>>,
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
    client_email: Option<String>,
    private_key: Option<String>,
}

#[derive(Serialize)]
struct JwtClaims {
    iss: String,
    scope: String,
    aud: String,
    iat: u64,
    exp: u64,
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
        if let Some(path) = gcloud_adc_path()
            && path.exists()
        {
            return Self::from_credentials_file(path);
        }
        let base_url = std::env::var("GCE_METADATA_HOST")
            .map(|host| format!("http://{host}"))
            // GCE metadata uses plain HTTP by design; no https enforcement here.
            .unwrap_or_else(|_| "http://metadata.google.internal".to_owned());
        let parsed = Url::parse(&base_url).map_err(|_| {
            ProviderError::auth("google", "GCE_METADATA_HOST produced an invalid base URL")
        })?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(ProviderError::auth(
                "google",
                "GCE_METADATA_HOST must resolve to an http or https URL",
            ));
        }
        Self::from_metadata_server(base_url)
    }

    fn new(source: GoogleOAuthSource) -> ProviderResult<Self> {
        let http = reqwest::Client::builder()
            .build()
            .map_err(|error| ProviderError::decode("google", error))?;
        Ok(Self {
            source,
            http,
            cache: tokio::sync::Mutex::new(None),
        })
    }

    /// Returns a bearer token, reusing cached tokens until the refresh grace.
    ///
    /// The async mutex is held across the fetch so at most one in-flight token
    /// refresh happens per `GoogleOAuth` instance.
    pub async fn token(&self) -> ProviderResult<GoogleOAuthToken> {
        let mut cache = self.cache.lock().await;
        if let Some(cached) = cache.as_ref() {
            let refresh_grace = Duration::from_secs(30);
            if cached.expires_at > Instant::now() + refresh_grace {
                return Ok(cached.token.clone());
            }
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
        let expires_at = Instant::now() + Duration::from_secs(token.expires_in);
        *cache = Some(CachedToken {
            token: token.clone(),
            expires_at,
        });
        Ok(token)
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
        validate_token_uri(&token_uri)?;
        let Some(client_email) = parsed.client_email else {
            return Err(ProviderError::auth(
                "google",
                "ADC JSON service account must include client_email",
            ));
        };
        let Some(private_key) = parsed.private_key else {
            return Err(ProviderError::auth(
                "google",
                "ADC JSON service account must include private_key",
            ));
        };
        let assertion = sign_service_account_jwt(&client_email, &token_uri, &private_key)?;
        let response = self
            .http
            .post(&token_uri)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", &assertion),
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

fn validate_token_uri(token_uri: &str) -> ProviderResult<()> {
    let parsed = Url::parse(token_uri)
        .map_err(|_| ProviderError::auth("google", "token_uri is not a valid URL"))?;
    if parsed.scheme() != "https" {
        return Err(ProviderError::auth(
            "google",
            "token_uri must use https scheme",
        ));
    }
    let host = parsed.host_str().unwrap_or("");
    if !TOKEN_URI_ALLOWED_HOSTS.contains(&host) {
        return Err(ProviderError::auth(
            "google",
            format!(
                "token_uri hostname '{host}' is not allowed; permitted: {}",
                TOKEN_URI_ALLOWED_HOSTS.join(", ")
            ),
        ));
    }
    Ok(())
}

fn sign_service_account_jwt(
    client_email: &str,
    token_uri: &str,
    private_key: &str,
) -> ProviderResult<String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| {
            ProviderError::auth(
                "google",
                format!("system clock is before Unix epoch: {error}"),
            )
        })?
        .as_secs();
    let claims = JwtClaims {
        iss: client_email.to_owned(),
        scope: JWT_SCOPE.to_owned(),
        aud: token_uri.to_owned(),
        iat: now,
        exp: now + 3600,
    };
    let encoding_key = EncodingKey::from_rsa_pem(private_key.as_bytes()).map_err(|error| {
        ProviderError::auth(
            "google",
            format!("invalid private_key in ADC JSON: {error}"),
        )
    })?;
    encode(&Header::new(Algorithm::RS256), &claims, &encoding_key)
        .map_err(|error| ProviderError::auth("google", format!("JWT signing failed: {error}")))
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
