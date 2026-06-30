//! OIDC provider discovery.

use std::time::Duration;

use serde::Deserialize;
use url::Url;

use crate::error::OidcError;

const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(10);

const ASYMMETRIC_ALGS: &[&str] = &[
    "RS256", "RS384", "RS512", "PS256", "PS384", "PS512", "ES256", "ES384", "ES512", "EdDSA",
];

/// Raw deserialization target for the OpenID Connect discovery document.
/// URLs are captured as strings and parsed after deserialization.
#[derive(Debug, Deserialize)]
struct RawProviderMetadata {
    issuer: String,
    authorization_endpoint: String,
    #[serde(default)]
    token_endpoint: Option<String>,
    jwks_uri: String,
    id_token_signing_alg_values_supported: Vec<String>,
}

/// Parsed OpenID Connect discovery document.
#[derive(Debug, Clone)]
pub struct ProviderMetadata {
    /// The `issuer` claim from the discovery document.
    pub issuer: String,
    /// Authorization endpoint URL.
    pub authorization_endpoint: Url,
    /// Token endpoint URL. Optional for issuers that omit it.
    pub token_endpoint: Option<Url>,
    /// JWKS URI for fetching signing keys.
    pub jwks_uri: Url,
    /// Advertised signing algorithms for ID tokens.
    pub id_token_signing_alg_values_supported: Vec<String>,
}

fn parse_raw_metadata(
    raw: RawProviderMetadata,
    issuer_str: &str,
) -> Result<ProviderMetadata, OidcError> {
    let authorization_endpoint =
        raw.authorization_endpoint
            .parse::<Url>()
            .map_err(|e| OidcError::Discovery {
                issuer: issuer_str.to_owned(),
                message: format!("authorization_endpoint is not a valid URL: {e}"),
            })?;
    let token_endpoint = raw
        .token_endpoint
        .as_deref()
        .map(|s| {
            s.parse::<Url>().map_err(|e| OidcError::Discovery {
                issuer: issuer_str.to_owned(),
                message: format!("token_endpoint is not a valid URL: {e}"),
            })
        })
        .transpose()?;
    let jwks_uri = raw
        .jwks_uri
        .parse::<Url>()
        .map_err(|e| OidcError::Discovery {
            issuer: issuer_str.to_owned(),
            message: format!("jwks_uri is not a valid URL: {e}"),
        })?;
    Ok(ProviderMetadata {
        issuer: raw.issuer,
        authorization_endpoint,
        token_endpoint,
        jwks_uri,
        id_token_signing_alg_values_supported: raw.id_token_signing_alg_values_supported,
    })
}

/// A discovered OIDC provider, ready for JWKS key resolution.
///
/// Constructed via [`OidcProvider::discover`]. The constructor fetches and
/// validates the `/.well-known/openid-configuration` document, including the
/// anti-spoofing issuer check.
#[derive(Debug)]
pub struct OidcProvider {
    /// Discovery document fields.
    pub metadata: ProviderMetadata,
    http: reqwest::Client,
}

impl OidcProvider {
    /// Discover an OIDC provider at the given base URL.
    ///
    /// Fetches `{issuer_url}/.well-known/openid-configuration`, validates that
    /// the returned `metadata.issuer` matches the requested URL (anti-spoofing),
    /// and confirms at least one asymmetric signing algorithm is advertised.
    ///
    /// The function accepts a [`url::Url`] rather than [`wyrd_spec::auth::oidc::IssuerUrl`]
    /// so that tests can use in-process mock servers that serve HTTP.
    ///
    /// # Errors
    /// Returns [`OidcError::Discovery`] on network or HTTP failure,
    /// [`OidcError::IssuerMismatch`] when the metadata `issuer` disagrees with
    /// the request URL, and [`OidcError::NoAsymmetricAlg`] when no supported
    /// asymmetric algorithm is advertised.
    #[tracing::instrument(level = "debug", skip(http), fields(issuer = %issuer_url), err)]
    pub async fn discover(issuer_url: Url, http: reqwest::Client) -> Result<Self, OidcError> {
        let issuer_str = issuer_url.as_str().trim_end_matches('/').to_owned();
        let discovery_url = format!("{issuer_str}/.well-known/openid-configuration");

        tracing::debug!(%issuer_str, "fetching OIDC discovery document");

        let response = http
            .get(&discovery_url)
            .timeout(DISCOVERY_TIMEOUT)
            .send()
            .await
            .map_err(|e| OidcError::Discovery {
                issuer: issuer_str.clone(),
                message: e.to_string(),
            })?;

        if !response.status().is_success() {
            return Err(OidcError::Discovery {
                issuer: issuer_str.clone(),
                message: format!("HTTP {}", response.status()),
            });
        }

        let raw: RawProviderMetadata = response.json().await.map_err(|e| OidcError::Discovery {
            issuer: issuer_str.clone(),
            message: e.to_string(),
        })?;

        // Anti-spoofing: the metadata issuer must match the URL we requested from.
        // Both sides are normalized to strip trailing slashes so comparison is canonical.
        let metadata_issuer = raw.issuer.trim_end_matches('/');
        if metadata_issuer != issuer_str {
            return Err(OidcError::IssuerMismatch {
                expected: issuer_str,
                found: raw.issuer.clone(),
            });
        }

        // At least one asymmetric algorithm must be advertised.
        let has_asymmetric = raw
            .id_token_signing_alg_values_supported
            .iter()
            .any(|alg| ASYMMETRIC_ALGS.contains(&alg.as_str()));
        if !has_asymmetric {
            return Err(OidcError::NoAsymmetricAlg { issuer: issuer_str });
        }

        let metadata = parse_raw_metadata(raw, &issuer_str)?;
        Ok(Self { metadata, http })
    }

    /// Borrow the HTTP client for downstream use (e.g. JWKS fetches).
    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    fn asymmetric_discovery_body(issuer: &str) -> serde_json::Value {
        serde_json::json!({
            "issuer": issuer,
            "authorization_endpoint": format!("{issuer}/authorize"),
            "token_endpoint": format!("{issuer}/token"),
            "jwks_uri": format!("{issuer}/jwks"),
            "id_token_signing_alg_values_supported": ["RS256", "EdDSA"]
        })
    }

    #[tokio::test]
    async fn discover_happy_path() {
        let server = MockServer::start().await;
        let issuer = server.uri();
        Mock::given(method("GET"))
            .and(path("/.well-known/openid-configuration"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(asymmetric_discovery_body(&issuer)),
            )
            .mount(&server)
            .await;

        let issuer_url: Url = issuer.parse().expect("server uri is valid url");
        let provider = OidcProvider::discover(issuer_url, reqwest::Client::new())
            .await
            .expect("discover should succeed");

        assert_eq!(provider.metadata.issuer, server.uri());
        assert!(provider.metadata.jwks_uri.as_str().ends_with("/jwks"));
        assert!(
            provider
                .metadata
                .id_token_signing_alg_values_supported
                .contains(&"RS256".to_owned())
        );
    }

    #[tokio::test]
    async fn discover_rejects_issuer_mismatch() {
        let server = MockServer::start().await;
        let issuer = server.uri();
        // Return a different issuer in the document than we requested.
        let body = serde_json::json!({
            "issuer": "https://evil.example.com",
            "authorization_endpoint": format!("{issuer}/authorize"),
            "jwks_uri": format!("{issuer}/jwks"),
            "id_token_signing_alg_values_supported": ["RS256"]
        });
        Mock::given(method("GET"))
            .and(path("/.well-known/openid-configuration"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;

        let issuer_url: Url = issuer.parse().expect("server uri is valid url");
        let err = OidcProvider::discover(issuer_url, reqwest::Client::new())
            .await
            .expect_err("issuer mismatch should fail");

        assert!(
            matches!(err, OidcError::IssuerMismatch { .. }),
            "expected IssuerMismatch, got {err:?}"
        );
    }

    #[tokio::test]
    async fn discover_rejects_symmetric_only_algs() {
        let server = MockServer::start().await;
        let issuer = server.uri();
        let body = serde_json::json!({
            "issuer": issuer,
            "authorization_endpoint": format!("{issuer}/authorize"),
            "jwks_uri": format!("{issuer}/jwks"),
            "id_token_signing_alg_values_supported": ["HS256", "HS512"]
        });
        Mock::given(method("GET"))
            .and(path("/.well-known/openid-configuration"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;

        let issuer_url: Url = issuer.parse().expect("server uri is valid url");
        let err = OidcProvider::discover(issuer_url, reqwest::Client::new())
            .await
            .expect_err("symmetric-only algs should fail");

        assert!(
            matches!(err, OidcError::NoAsymmetricAlg { .. }),
            "expected NoAsymmetricAlg, got {err:?}"
        );
    }

    #[tokio::test]
    async fn discover_rejects_non_200_response() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/.well-known/openid-configuration"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let issuer_url: Url = server.uri().parse().expect("server uri is valid url");
        let err = OidcProvider::discover(issuer_url, reqwest::Client::new())
            .await
            .expect_err("503 should fail");

        assert!(
            matches!(err, OidcError::Discovery { .. }),
            "expected Discovery error, got {err:?}"
        );
    }

    #[tokio::test]
    async fn discover_normalizes_trailing_slash() {
        let server = MockServer::start().await;
        let issuer = server.uri();
        // Mock returns issuer without trailing slash.
        Mock::given(method("GET"))
            .and(path("/.well-known/openid-configuration"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(asymmetric_discovery_body(&issuer)),
            )
            .mount(&server)
            .await;

        // Request URL has a trailing slash.
        let with_slash = format!("{issuer}/");
        let issuer_url: Url = with_slash.parse().expect("url is valid");
        let provider = OidcProvider::discover(issuer_url, reqwest::Client::new())
            .await
            .expect("trailing slash should be normalized away");

        assert_eq!(provider.metadata.issuer, issuer);
    }
}
