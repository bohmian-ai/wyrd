//! JWKS key cache with TTL expiry and refetch-on-unknown-kid.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use jsonwebtoken::DecodingKey;
use moka::future::Cache;
use serde::Deserialize;
use url::Url;

use crate::error::OidcError;

/// Key identifier extracted from a JWT header, scoped to OIDC key management.
///
/// Intentionally separate from `wyrd_auth_verify::Kid` to avoid a dependency
/// cycle: commit 03 makes `wyrd-auth-verify` depend on this crate (F09 rule).
/// The verifier converts between the two types at its boundary only.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OidcKid(String);

impl OidcKid {
    /// Wrap any string as a key identifier. No format validation is imposed;
    /// OIDC kid values may be UUIDs, URLs, or arbitrary strings.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Borrow as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for OidcKid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// --------------------------------------------------------------------------
// JWKS parsing
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct Jwks {
    keys: Vec<Jwk>,
}

#[derive(Debug, Deserialize)]
struct Jwk {
    kty: String,
    #[serde(default)]
    kid: Option<String>,
    // RSA
    #[serde(default)]
    n: Option<String>,
    #[serde(default)]
    e: Option<String>,
    // EC and OKP
    #[serde(default)]
    x: Option<String>,
    #[serde(default)]
    y: Option<String>,
    // EC curve and OKP curve discriminant
    #[serde(default)]
    crv: Option<String>,
}

type KeyMap = Arc<HashMap<OidcKid, Arc<DecodingKey>>>;

fn decoding_key_from_jwk(
    jwk: &Jwk,
    issuer: &str,
) -> Result<Option<(OidcKid, Arc<DecodingKey>)>, OidcError> {
    let kid = match &jwk.kid {
        Some(k) => OidcKid::new(k.clone()),
        // Skip keys without a kid; we cannot look them up by header kid.
        None => return Ok(None),
    };

    let dk = match jwk.kty.as_str() {
        "RSA" => {
            let n = jwk.n.as_deref().ok_or_else(|| OidcError::JwksUnavailable {
                issuer: issuer.to_owned(),
                message: "RSA key missing 'n' component".to_owned(),
            })?;
            let e = jwk.e.as_deref().ok_or_else(|| OidcError::JwksUnavailable {
                issuer: issuer.to_owned(),
                message: "RSA key missing 'e' component".to_owned(),
            })?;
            DecodingKey::from_rsa_components(n, e).map_err(|err| OidcError::JwksUnavailable {
                issuer: issuer.to_owned(),
                message: format!("RSA key construction failed: {err}"),
            })?
        }
        "EC" => {
            let x = jwk.x.as_deref().ok_or_else(|| OidcError::JwksUnavailable {
                issuer: issuer.to_owned(),
                message: "EC key missing 'x' component".to_owned(),
            })?;
            let y = jwk.y.as_deref().ok_or_else(|| OidcError::JwksUnavailable {
                issuer: issuer.to_owned(),
                message: "EC key missing 'y' component".to_owned(),
            })?;
            DecodingKey::from_ec_components(x, y).map_err(|err| OidcError::JwksUnavailable {
                issuer: issuer.to_owned(),
                message: format!("EC key construction failed for crv {:?}: {err}", jwk.crv),
            })?
        }
        "OKP" => {
            let x = jwk.x.as_deref().ok_or_else(|| OidcError::JwksUnavailable {
                issuer: issuer.to_owned(),
                message: "OKP key missing 'x' component".to_owned(),
            })?;
            DecodingKey::from_ed_components(x).map_err(|err| OidcError::JwksUnavailable {
                issuer: issuer.to_owned(),
                message: format!("OKP/Ed25519 key construction failed: {err}"),
            })?
        }
        // Unknown kty — skip silently; don't fail the whole JWKS fetch.
        _ => return Ok(None),
    };

    Ok(Some((kid, Arc::new(dk))))
}

async fn fetch_jwks(
    issuer: &str,
    jwks_uri: &Url,
    http: &reqwest::Client,
    timeout: Duration,
) -> Result<KeyMap, OidcError> {
    tracing::debug!(%issuer, %jwks_uri, "fetching JWKS");

    let response = http
        .get(jwks_uri.as_str())
        .timeout(timeout)
        .send()
        .await
        .map_err(|e| OidcError::JwksUnavailable {
            issuer: issuer.to_owned(),
            message: e.to_string(),
        })?;

    if !response.status().is_success() {
        return Err(OidcError::JwksUnavailable {
            issuer: issuer.to_owned(),
            message: format!("HTTP {}", response.status()),
        });
    }

    let jwks: Jwks = response
        .json()
        .await
        .map_err(|e| OidcError::JwksUnavailable {
            issuer: issuer.to_owned(),
            message: format!("JWKS JSON parse failed: {e}"),
        })?;

    let mut map = HashMap::new();
    for jwk in &jwks.keys {
        match decoding_key_from_jwk(jwk, issuer) {
            Ok(Some((kid, dk))) => {
                map.insert(kid, dk);
            }
            Ok(None) => {}
            Err(e) => {
                // Log and skip malformed keys rather than rejecting the whole set.
                tracing::warn!(%issuer, error = %e, "skipping malformed JWK entry");
            }
        }
    }

    Ok(Arc::new(map))
}

// --------------------------------------------------------------------------
// JwksCache
// --------------------------------------------------------------------------

/// Async JWKS key cache backed by moka, with per-jwks-uri TTL and
/// refetch-on-unknown-kid.
///
/// Concurrent misses for the same `jwks_uri` are coalesced by moka into a
/// single in-flight fetch (single-flight). No `Arc<Mutex<T>>` is needed;
/// moka manages the concurrent access internally.
pub struct JwksCache {
    inner: Cache<String, KeyMap>,
    http: reqwest::Client,
    fetch_timeout: Duration,
}

impl JwksCache {
    /// Construct a cache with a given key TTL and per-fetch timeout.
    ///
    /// `max_issuers` bounds the number of cached issuer key sets. Entries
    /// beyond the capacity are evicted by LRU before TTL expiry.
    pub fn new(http: reqwest::Client, ttl: Duration, fetch_timeout: Duration) -> Self {
        let inner = Cache::builder().max_capacity(256).time_to_live(ttl).build();
        Self {
            inner,
            http,
            fetch_timeout,
        }
    }

    /// Resolve a [`DecodingKey`] by `kid` for the given issuer.
    ///
    /// On a cache hit the key is returned immediately. On a miss the JWKS is
    /// fetched (coalesced with concurrent misses). If the kid is absent from
    /// the freshly fetched set the cache is invalidated and the JWKS is
    /// refetched exactly once; still absent → [`OidcError::UnknownKid`].
    ///
    /// # Errors
    /// - [`OidcError::JwksUnavailable`] when the JWKS endpoint is unreachable.
    /// - [`OidcError::UnknownKid`] when the kid is absent after one refetch.
    #[tracing::instrument(level = "debug", skip(self), fields(%issuer, kid = %kid), err)]
    pub async fn key(
        &self,
        issuer: &str,
        jwks_uri: &Url,
        kid: &OidcKid,
    ) -> Result<Arc<DecodingKey>, OidcError> {
        let map = self.fetch_cached(issuer, jwks_uri).await?;

        if let Some(dk) = map.get(kid) {
            return Ok(Arc::clone(dk));
        }

        // Unknown kid — could be key rotation. Invalidate and refetch once.
        tracing::debug!(%issuer, %kid, "unknown kid, triggering JWKS refetch");
        self.inner.invalidate(&jwks_uri.to_string()).await;

        let map = self.fetch_cached(issuer, jwks_uri).await?;

        map.get(kid)
            .map(Arc::clone)
            .ok_or_else(|| OidcError::UnknownKid {
                issuer: issuer.to_owned(),
                kid: kid.as_str().to_owned(),
            })
    }

    async fn fetch_cached(&self, issuer: &str, jwks_uri: &Url) -> Result<KeyMap, OidcError> {
        let cache_key = jwks_uri.to_string();
        let issuer_owned = issuer.to_owned();
        let jwks_uri_owned = jwks_uri.clone();
        let http = self.http.clone();
        let timeout = self.fetch_timeout;

        self.inner
            .try_get_with(cache_key, async move {
                fetch_jwks(&issuer_owned, &jwks_uri_owned, &http, timeout).await
            })
            .await
            .map_err(|e| (*e).clone())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    // Ed25519 public key x-component (base64url, no padding).
    // Matches the PUBLIC_KEY_PEM used in wyrd-auth-verify tests:
    // MCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=
    const ED_X: &str = "WhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ-DZ8Vw";

    // RSA-2048 test components from RFC 7517 Appendix A.1.
    const RSA_N: &str = "0vx7agoebGcQSuuPiLJXZptN9nndrQmbXEps2aiAFbWhM78LhWx4cbbfAAtVT86zwu1RK7aPFFxuhDR1L6tSoc_BJECPebWKRXjBZCiFV4n3oknjhMstn64tZ_2W-5JsGY4Hc5n9yBXArwl93lqt7_RN5w6Cf0h4QyQ5v-65YGjQR0_FDW2QvzqY368QQMicAtaSqzs8KJZgnYb9c7d0zgdAZHzu6qMQvRL5hajrn1n91CbOpbISD08qNLyrdkt-bFTWhAI4vMQFh6WeZu0fM4lFd2NcRwr3XPksINHaQ-G_xBniIqbw0Ls1jF44-csFCur-kEgU8awapJzKnqDKgw";
    const RSA_E: &str = "AQAB";

    // EC P-256 test components from RFC 7517 Appendix A.2.
    const EC_X: &str = "f83OJ3D2xF1Bg8vub9tLe1gHMzV76e8Tus9uPHvRVEU";
    const EC_Y: &str = "x_FEzRu9m36HLN_tue659LNpXW6pCyStikYjKIWI5a0";

    fn ed_jwks(kid: &str) -> serde_json::Value {
        serde_json::json!({
            "keys": [{
                "kty": "OKP",
                "crv": "Ed25519",
                "kid": kid,
                "x": ED_X
            }]
        })
    }

    fn rsa_jwks(kid: &str) -> serde_json::Value {
        serde_json::json!({
            "keys": [{
                "kty": "RSA",
                "kid": kid,
                "n": RSA_N,
                "e": RSA_E
            }]
        })
    }

    fn ec_jwks(kid: &str) -> serde_json::Value {
        serde_json::json!({
            "keys": [{
                "kty": "EC",
                "crv": "P-256",
                "kid": kid,
                "x": EC_X,
                "y": EC_Y
            }]
        })
    }

    fn cache() -> JwksCache {
        JwksCache::new(
            reqwest::Client::new(),
            Duration::from_secs(300),
            Duration::from_secs(5),
        )
    }

    #[tokio::test]
    async fn resolves_ed25519_key() {
        let server = MockServer::start().await;
        let kid = "ed-key-1";
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ed_jwks(kid)))
            .mount(&server)
            .await;

        let jwks_uri: Url = format!("{}/jwks", server.uri()).parse().unwrap();
        let cache = cache();
        cache
            .key("test-issuer", &jwks_uri, &OidcKid::new(kid))
            .await
            .expect("Ed25519 key resolution should succeed");
    }

    #[tokio::test]
    async fn resolves_rsa_key() {
        let server = MockServer::start().await;
        let kid = "rsa-key-1";
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(rsa_jwks(kid)))
            .mount(&server)
            .await;

        let jwks_uri: Url = format!("{}/jwks", server.uri()).parse().unwrap();
        let cache = cache();
        cache
            .key("test-issuer", &jwks_uri, &OidcKid::new(kid))
            .await
            .expect("RSA key resolution should succeed");
    }

    #[tokio::test]
    async fn resolves_ec_key() {
        let server = MockServer::start().await;
        let kid = "ec-key-1";
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ec_jwks(kid)))
            .mount(&server)
            .await;

        let jwks_uri: Url = format!("{}/jwks", server.uri()).parse().unwrap();
        let cache = cache();
        cache
            .key("test-issuer", &jwks_uri, &OidcKid::new(kid))
            .await
            .expect("EC key resolution should succeed");
    }

    #[tokio::test]
    async fn refetch_on_unknown_kid_succeeds_after_rotation() {
        let server = MockServer::start().await;
        let old_kid = "old-key";
        let new_kid = "new-key";

        // First call: serve the old key set.
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ed_jwks(old_kid)))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        // Second call (after rotation): serve the new key set.
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ed_jwks(new_kid)))
            .mount(&server)
            .await;

        let jwks_uri: Url = format!("{}/jwks", server.uri()).parse().unwrap();
        let cache = cache();

        // Populate the cache with the old key set.
        cache
            .key("test-issuer", &jwks_uri, &OidcKid::new(old_kid))
            .await
            .expect("old kid resolves on initial fetch");

        // Now request the new kid — should trigger exactly one refetch.
        cache
            .key("test-issuer", &jwks_uri, &OidcKid::new(new_kid))
            .await
            .expect("new kid should resolve after one JWKS refetch");
    }

    #[tokio::test]
    async fn unknown_kid_after_refetch_returns_error() {
        let server = MockServer::start().await;
        let kid = "key-1";
        let unknown = "does-not-exist";

        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ed_jwks(kid)))
            .mount(&server)
            .await;

        let jwks_uri: Url = format!("{}/jwks", server.uri()).parse().unwrap();
        let cache = cache();

        let err = cache
            .key("test-issuer", &jwks_uri, &OidcKid::new(unknown))
            .await
            .err()
            .expect("unknown kid should fail after refetch");

        assert!(
            matches!(err, OidcError::UnknownKid { ref kid, .. } if kid == unknown),
            "expected UnknownKid, got {err:?}"
        );
    }

    #[tokio::test]
    async fn concurrent_misses_coalesce_to_single_fetch() {
        let server = MockServer::start().await;
        let kid = "key-coalesce";

        // Add a 50ms delay so all spawned tasks can call try_get_with before
        // the first fetch completes, giving moka time to coalesce them.
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(ed_jwks(kid))
                    .set_delay(Duration::from_millis(50)),
            )
            .mount(&server)
            .await;

        let jwks_uri: Url = format!("{}/jwks", server.uri()).parse().unwrap();
        let cache = Arc::new(cache());
        let oidc_kid = OidcKid::new(kid);

        let handles: Vec<_> = (0..10)
            .map(|_| {
                let cache = Arc::clone(&cache);
                let kid = oidc_kid.clone();
                let uri = jwks_uri.clone();
                tokio::spawn(async move { cache.key("test-issuer", &uri, &kid).await })
            })
            .collect();

        let mut ok_count = 0usize;
        for h in handles {
            if h.await.expect("task did not panic").is_ok() {
                ok_count += 1;
            }
        }
        assert_eq!(ok_count, 10, "all concurrent tasks should resolve the key");

        // Exactly one HTTP request should have been made (moka coalesced the rest).
        let received = server
            .received_requests()
            .await
            .expect("wiremock tracks requests");
        assert_eq!(
            received.len(),
            1,
            "moka should coalesce 10 concurrent misses into 1 fetch, got {}",
            received.len()
        );
    }

    #[tokio::test]
    async fn jwks_unavailable_on_network_error() {
        // Point at a port nothing is listening on.
        let jwks_uri: Url = "http://127.0.0.1:1".parse().unwrap();
        let cache = cache();

        let err = cache
            .key("test-issuer", &jwks_uri, &OidcKid::new("any"))
            .await
            .err()
            .expect("unreachable endpoint should fail");

        assert!(
            matches!(err, OidcError::JwksUnavailable { .. }),
            "expected JwksUnavailable, got {err:?}"
        );
    }
}
