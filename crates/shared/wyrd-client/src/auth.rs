//! Authentication middleware.
//!
//! [`AuthMiddleware`] is the single, `Arc`-shared auth path for the client. It
//! holds one durable secret (the resolved API key), exchanges it for a
//! short-lived access JWT at `{http.base_url}/auth/token`, caches the access
//! token, and refreshes it both proactively (at the 30s skew boundary) and
//! reactively (via [`AuthMiddleware::force_refresh`] after a `401`). Concurrent
//! callers single-flight one exchange instead of stampeding.
//!
//! Both durable-secret arms — [`ResolvedCredential::ApiKey`] and
//! [`ResolvedCredential::WorkloadJwt`] — exchange their secret for a short-lived
//! access token through the *same* cache and single-flight gate. A directly
//! supplied [`ResolvedCredential::BearerToken`] is passed through as-is.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use chrono::{DateTime, Utc};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::Mutex;
use uuid::Uuid;
use wyrd_spec::auth::{SecretBearer, TokenRequest, TokenResponse};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::TenantSlug;

use crate::config::{ClientConfig, TokenCacheMode};
use crate::error::{WyrdClientError, from_problem_json};
use crate::transport::credential::ResolvedCredential;

/// Fixed proactive-refresh skew. A cached access token is considered stale once
/// `now >= expires_at - SKEW`, so the client refreshes before the server would
/// reject it. The reactive `401` path is the clock-skew backstop.
const REFRESH_SKEW_SECONDS: i64 = 30;

/// Error surfaced by the auth path.
///
/// Two disjoint failure channels: [`AuthError::Client`] carries a client-local
/// [`WyrdClientError`] (a transport failure reaching `/auth/token` is
/// [`WyrdClientError::TransportDown`]); [`AuthError::Server`] carries a
/// server-reported [`WyrdError`] mapped from an `application/problem+json` body
/// via [`from_problem_json`]. A revoked or invalid API key is the only failure
/// that surfaces to the application.
#[derive(Debug, Error)]
pub enum AuthError {
    /// Client-local failure (transport down, unsupported credential).
    #[error(transparent)]
    Client(#[from] WyrdClientError),
    /// Server-reported failure mapped into the unified Wyrd error catalog.
    #[error(transparent)]
    Server(WyrdError),
}

/// One cached access token plus its expiry. The refresh token is never stored.
#[derive(Debug, Clone)]
struct CachedToken {
    access_token: SecretBearer,
    expires_at: DateTime<Utc>,
}

impl CachedToken {
    /// `true` once the token is within [`REFRESH_SKEW_SECONDS`] of expiry.
    fn is_stale(&self) -> bool {
        Utc::now() >= self.expires_at - chrono::Duration::seconds(REFRESH_SKEW_SECONDS)
    }
}

/// On-disk token record. Holds the access token and expiry only — never a
/// refresh token. `Debug` is redacted via [`SecretBearer`].
#[derive(Debug, Serialize, Deserialize)]
struct DiskTokenRecord {
    access_token: SecretBearer,
    expires_at: DateTime<Utc>,
}

/// Single, `Arc`-shared auth path: token exchange, cache, and refresh.
pub struct AuthMiddleware {
    credential: ResolvedCredential,
    http_base_url: String,
    http_client: reqwest::Client,
    cache: Mutex<Option<CachedToken>>,
    cache_mode: TokenCacheMode,
    /// Resolved on-disk token-cache path, computed once at construction in
    /// [`TokenCacheMode::Disk`] mode (`None` otherwise). Resolving here — rather
    /// than reading `~`/`HOME` on every persist — keeps the path stable and lets
    /// tests inject a unique path without mutating the process environment.
    cache_path: Option<PathBuf>,
    /// Set once the first short-TTL token is observed, so the operational
    /// warning fires at most once per middleware instance (see [`Self::exchange`]).
    short_ttl_warned: AtomicBool,
}

impl std::fmt::Debug for AuthMiddleware {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthMiddleware")
            .field("credential", &self.credential)
            .field("http_base_url", &self.http_base_url)
            .field("cache_mode", &self.cache_mode)
            .finish_non_exhaustive()
    }
}

impl AuthMiddleware {
    /// Build the middleware from a [`ClientConfig`] and a resolved credential.
    ///
    /// The API key is sent only to `{config.http.base_url}/auth/token`. In
    /// [`TokenCacheMode::Disk`] mode an existing, non-stale on-disk record is
    /// loaded as the initial cache so an access token survives process
    /// restarts.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::TransportDown`] when another Rustls provider
    /// already owns the process or the underlying Reqwest client cannot be
    /// constructed.
    pub fn new(
        config: &ClientConfig,
        credential: ResolvedCredential,
    ) -> Result<Arc<Self>, WyrdClientError> {
        let cache_path = if config.token_cache == TokenCacheMode::Disk {
            token_cache_path()
        } else {
            None
        };
        Self::build(config, credential, cache_path)
    }

    /// Construct from a resolved cache path. `new` resolves the production
    /// `~/.config/wyrd/token_cache.json`; tests inject a unique path so they
    /// never touch the process environment (and stay parallel-safe).
    ///
    /// # Errors
    ///
    /// Returns [`WyrdClientError::TransportDown`] when another Rustls provider
    /// already owns the process or the authentication HTTP client cannot be
    /// built.
    fn build(
        config: &ClientConfig,
        credential: ResolvedCredential,
        cache_path: Option<PathBuf>,
    ) -> Result<Arc<Self>, WyrdClientError> {
        wyrd_tls::install_crypto_provider().map_err(|error| WyrdClientError::TransportDown {
            transport: "http".to_owned(),
            message: error.to_string(),
        })?;
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.http.timeout_ms))
            .build()
            .map_err(|err| WyrdClientError::TransportDown {
                transport: "http".to_owned(),
                message: format!("failed to build HTTP client: {err}"),
            })?;

        let initial = cache_path.as_deref().and_then(load_disk_record);

        Ok(Arc::new(Self {
            credential,
            http_base_url: config.http.base_url.clone(),
            http_client,
            cache: Mutex::new(initial),
            cache_mode: config.token_cache.clone(),
            cache_path,
            short_ttl_warned: AtomicBool::new(false),
        }))
    }

    /// Test-only constructor that injects an explicit token-cache path,
    /// avoiding any `HOME`/`~` resolution so disk-cache tests are hermetic.
    #[cfg(test)]
    fn new_with_cache_path(
        config: &ClientConfig,
        credential: ResolvedCredential,
        cache_path: Option<PathBuf>,
    ) -> Result<Arc<Self>, WyrdClientError> {
        Self::build(config, credential, cache_path)
    }

    /// Return the current access token, exchanging or refreshing as needed.
    ///
    /// For an [`ResolvedCredential::ApiKey`]: reads the cache and returns the
    /// cached token when it is still fresh; otherwise takes the exchange gate,
    /// re-reads under the gate (so N racing callers cause exactly one
    /// `/auth/token` round-trip), and exchanges the API key once. A
    /// [`ResolvedCredential::BearerToken`] is returned directly without
    /// exchange. A [`ResolvedCredential::WorkloadJwt`] follows the same
    /// cache/single-flight path as the API key, exchanging the ambient OIDC
    /// assertion via the `jwt-bearer` grant.
    ///
    /// # Errors
    /// Returns [`AuthError::Client`] on transport failure or an invalid tenant
    /// slug, or [`AuthError::Server`] when the server rejects the credential.
    pub async fn bearer(&self) -> Result<SecretBearer, AuthError> {
        match &self.credential {
            ResolvedCredential::BearerToken(token) => {
                Ok(SecretBearer::new(token.expose_secret().to_owned()))
            }
            ResolvedCredential::WorkloadJwt { jwt, tenant } => {
                let mut cache = self.cache.lock().await;
                if let Some(entry) = cache.as_ref()
                    && !entry.is_stale()
                {
                    return Ok(entry.access_token.clone());
                }
                self.exchange_workload_and_store(jwt, tenant, &mut cache)
                    .await
            }
            ResolvedCredential::ApiKey(api_key) => {
                let mut cache = self.cache.lock().await;
                if let Some(entry) = cache.as_ref()
                    && !entry.is_stale()
                {
                    return Ok(entry.access_token.clone());
                }
                self.exchange_and_store(api_key, &mut cache).await
            }
        }
    }

    /// Unconditionally re-exchange the credential, replacing the cache.
    ///
    /// The reactive `401` path: the HTTP/gRPC transport calls this after a
    /// rejected request, then retries once. Takes the same single-flight gate
    /// as [`AuthMiddleware::bearer`].
    ///
    /// # Errors
    /// Returns [`AuthError::Client`] on transport failure or an unsupported
    /// credential, or [`AuthError::Server`] when the server rejects the key.
    pub async fn force_refresh(&self) -> Result<SecretBearer, AuthError> {
        match &self.credential {
            ResolvedCredential::BearerToken(token) => {
                Ok(SecretBearer::new(token.expose_secret().to_owned()))
            }
            ResolvedCredential::WorkloadJwt { jwt, tenant } => {
                let mut cache = self.cache.lock().await;
                self.exchange_workload_and_store(jwt, tenant, &mut cache)
                    .await
            }
            ResolvedCredential::ApiKey(api_key) => {
                let mut cache = self.cache.lock().await;
                self.exchange_and_store(api_key, &mut cache).await
            }
        }
    }

    /// Exchange the API key, persist the result, and replace the in-memory
    /// cache. The shared post-exchange tail of [`AuthMiddleware::bearer`] and
    /// [`AuthMiddleware::force_refresh`]; the caller holds the exchange gate.
    async fn exchange_and_store(
        &self,
        api_key: &SecretString,
        cache: &mut Option<CachedToken>,
    ) -> Result<SecretBearer, AuthError> {
        let entry = self.exchange(api_key).await?;
        Ok(self.store(entry, cache))
    }

    /// Exchange the workload JWT, persist the result, and replace the in-memory
    /// cache. The [`ResolvedCredential::WorkloadJwt`] analogue of
    /// [`AuthMiddleware::exchange_and_store`]; the caller holds the exchange gate
    /// and both credentials share the one cache.
    async fn exchange_workload_and_store(
        &self,
        jwt: &SecretString,
        tenant: &str,
        cache: &mut Option<CachedToken>,
    ) -> Result<SecretBearer, AuthError> {
        let entry = self.exchange_workload(jwt, tenant).await?;
        Ok(self.store(entry, cache))
    }

    /// Persist a freshly exchanged token and replace the in-memory cache,
    /// returning the access bearer. The shared cache-write tail of both the
    /// API-key and workload exchange paths.
    fn store(&self, entry: CachedToken, cache: &mut Option<CachedToken>) -> SecretBearer {
        let bearer = entry.access_token.clone();
        self.persist(&entry);
        *cache = Some(entry);
        bearer
    }

    /// Forward an inbound `wyrd-request-id` or mint a fresh UUIDv7.
    ///
    /// The id spine is independent of `traceparent`. A non-empty `inbound` id is
    /// forwarded verbatim; otherwise a new UUIDv7 is minted.
    #[must_use]
    pub fn request_id(&self, inbound: Option<&str>) -> String {
        let id = match inbound {
            Some(value) if !value.is_empty() => value.to_owned(),
            _ => Uuid::now_v7().to_string(),
        };
        tracing::trace!(target: "wyrd.client.request", wyrd_request_id = %id, "resolved wyrd-request-id");
        id
    }

    /// Exchange the API key for an access token at `/auth/token`.
    async fn exchange(&self, api_key: &SecretString) -> Result<CachedToken, AuthError> {
        let request = TokenRequest::WyrdApiKey {
            api_key: SecretBearer::new(api_key.expose_secret().to_owned()),
        };
        self.post_token_request(request).await
    }

    /// Exchange the workload OIDC assertion for an access token at `/auth/token`
    /// via the `jwt-bearer` grant.
    ///
    /// The tenant slug is validated client-side first: an invalid slug is a
    /// client-local [`WyrdClientError::Config`] surfaced before any network call.
    async fn exchange_workload(
        &self,
        jwt: &SecretString,
        tenant: &str,
    ) -> Result<CachedToken, AuthError> {
        let tenant = TenantSlug::new(tenant.to_owned()).map_err(|err| {
            AuthError::Client(WyrdClientError::Config {
                field: "tenant".to_owned(),
                reason: format!("invalid workload tenant slug: {err}"),
            })
        })?;
        let request = TokenRequest::JwtBearer {
            assertion: SecretBearer::new(jwt.expose_secret().to_owned()),
            tenant: Some(tenant),
        };
        self.post_token_request(request).await
    }

    /// POST a token request to `/auth/token`, map a non-2xx body via
    /// [`from_problem_json`] into [`AuthError::Server`], and decode the success
    /// body into a [`CachedToken`]. The shared POST/decode tail of every grant.
    async fn post_token_request(&self, request: TokenRequest) -> Result<CachedToken, AuthError> {
        let url = format!("{}/auth/token", self.http_base_url.trim_end_matches('/'));
        let response = self
            .http_client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(transport_down)?;

        if !response.status().is_success() {
            let body = response
                .json::<serde_json::Value>()
                .await
                .map_err(transport_down)?;
            return Err(AuthError::Server(from_problem_json(&body)));
        }

        let token = response
            .json::<TokenResponse>()
            .await
            .map_err(transport_down)?;
        self.warn_if_short_ttl(token.expires_at);
        // `token.refresh_token` is intentionally dropped here: never cached,
        // never written to disk. The durable secret is re-exchanged instead.
        Ok(CachedToken {
            access_token: token.access_token,
            expires_at: token.expires_at,
        })
    }

    /// Warn once if the issued access TTL is below `2 × REFRESH_SKEW_SECONDS`.
    ///
    /// The proactive-refresh skew is a fixed [`REFRESH_SKEW_SECONDS`]. When the
    /// server issues a token whose lifetime is under twice that, the cache is
    /// effectively always stale and every call re-exchanges. This is a server
    /// misconfiguration, not a client bug, so the client logs an operational
    /// warning (once) rather than changing the documented skew.
    fn warn_if_short_ttl(&self, expires_at: DateTime<Utc>) {
        let ttl = expires_at - Utc::now();
        if ttl < chrono::Duration::seconds(2 * REFRESH_SKEW_SECONDS)
            && !self.short_ttl_warned.swap(true, Ordering::Relaxed)
        {
            tracing::warn!(
                target: "wyrd.client.auth",
                ttl_seconds = ttl.num_seconds(),
                refresh_skew_seconds = REFRESH_SKEW_SECONDS,
                "server-issued access TTL is below 2x the refresh skew; the token \
                 cache will refresh on nearly every call"
            );
        }
    }

    /// Write the access token to disk in [`TokenCacheMode::Disk`] mode.
    ///
    /// Last-writer-wins, no file lock. Best-effort: a write failure does not
    /// fail the exchange.
    fn persist(&self, entry: &CachedToken) {
        if self.cache_mode != TokenCacheMode::Disk {
            return;
        }
        if let Some(path) = self.cache_path.as_deref() {
            let _ = write_disk_record(path, entry);
        }
    }
}

/// Map a `reqwest` transport error to [`WyrdClientError::TransportDown`].
fn transport_down(err: reqwest::Error) -> AuthError {
    AuthError::Client(WyrdClientError::TransportDown {
        transport: "http".to_owned(),
        message: err.to_string(),
    })
}

/// Path of the on-disk token cache: `~/.config/wyrd/token_cache.json`.
fn token_cache_path() -> Option<PathBuf> {
    let expanded = shellexpand::tilde("~/.config/wyrd/token_cache.json");
    Some(PathBuf::from(expanded.as_ref()))
}

/// Load a non-stale token record from the given path, if present. Best-effort.
fn load_disk_record(path: &std::path::Path) -> Option<CachedToken> {
    let content = std::fs::read_to_string(path).ok()?;
    let record: DiskTokenRecord = serde_json::from_str(&content).ok()?;
    let entry = CachedToken {
        access_token: record.access_token,
        expires_at: record.expires_at,
    };
    if entry.is_stale() {
        return None;
    }
    Some(entry)
}

/// Write the token record at mode `0600` (owner read/write only).
fn write_disk_record(path: &std::path::Path, entry: &CachedToken) -> std::io::Result<()> {
    use std::io::Write as _;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let record = DiskTokenRecord {
        access_token: entry.access_token.clone(),
        expires_at: entry.expires_at,
    };
    let bytes = serde_json::to_vec(&record)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(&bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use chrono::Utc;
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    use tokio::net::TcpListener;
    use uuid::Uuid;

    use super::{AuthError, AuthMiddleware, CachedToken};
    use crate::config::{ClientConfig, TokenCacheMode};
    use crate::error::WyrdClientError;
    use crate::transport::credential::ResolvedCredential;
    use wyrd_spec::auth::SecretBearer;

    struct MockServer {
        base_url: String,
        hits: Arc<AtomicUsize>,
        _handle: tokio::task::JoinHandle<()>,
    }

    async fn spawn_mock(status_line: &'static str, body: String) -> MockServer {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_loop = hits.clone();
        let response = format!(
            "{status_line}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body,
        );
        let handle = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                hits_loop.fetch_add(1, Ordering::SeqCst);
                let response = response.clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 2048];
                    let _ = stream.read(&mut buf).await;
                    let _ = stream.write_all(response.as_bytes()).await;
                    let _ = stream.flush().await;
                });
            }
        });
        MockServer {
            base_url: format!("http://{addr}"),
            hits,
            _handle: handle,
        }
    }

    fn token_body(access: &str, expires_in_secs: i64) -> String {
        let expires_at = Utc::now() + chrono::Duration::seconds(expires_in_secs);
        serde_json::json!({
            "access_token": access,
            "refresh_token": "refresh-should-drop",
            "token_type": "Bearer",
            "expires_at": expires_at,
        })
        .to_string()
    }

    fn config_for(base_url: String, cache: TokenCacheMode) -> ClientConfig {
        let mut config = ClientConfig::default();
        config.http.base_url = base_url;
        config.token_cache = cache;
        config
    }

    fn api_key_credential() -> ResolvedCredential {
        ResolvedCredential::ApiKey("api-key-value".to_owned().into())
    }

    fn workload_jwt_credential(tenant: &str) -> ResolvedCredential {
        ResolvedCredential::WorkloadJwt {
            jwt: "workload-oidc-assertion".to_owned().into(),
            tenant: tenant.to_owned(),
        }
    }

    #[tokio::test]
    async fn exchange_caches_access_token_and_drops_refresh() {
        let mock = spawn_mock("HTTP/1.1 200 OK", token_body("access-123", 3600)).await;
        let mw = AuthMiddleware::new(
            &config_for(mock.base_url.clone(), TokenCacheMode::InMemory),
            api_key_credential(),
        )
        .expect("middleware builds");

        let bearer = mw.bearer().await.expect("exchange succeeds");
        assert_eq!(bearer.expose(), "access-123");
        assert_eq!(mock.hits.load(Ordering::SeqCst), 1);

        // Second call is served from cache: no new round-trip.
        let again = mw.bearer().await.expect("cached");
        assert_eq!(again.expose(), "access-123");
        assert_eq!(
            mock.hits.load(Ordering::SeqCst),
            1,
            "fresh token must be served from cache"
        );
    }

    #[test]
    fn is_stale_true_at_and_within_skew_boundary() {
        let inside = CachedToken {
            access_token: SecretBearer::new("x".to_owned()),
            expires_at: Utc::now() + chrono::Duration::seconds(20),
        };
        assert!(inside.is_stale(), "20s < 30s skew must be stale");

        let boundary = CachedToken {
            access_token: SecretBearer::new("x".to_owned()),
            expires_at: Utc::now() + chrono::Duration::seconds(30),
        };
        assert!(boundary.is_stale(), "exactly at 30s skew boundary is stale");
    }

    #[test]
    fn is_stale_false_outside_skew() {
        let fresh = CachedToken {
            access_token: SecretBearer::new("x".to_owned()),
            expires_at: Utc::now() + chrono::Duration::seconds(120),
        };
        assert!(!fresh.is_stale());
    }

    #[tokio::test]
    async fn force_refresh_re_exchanges_once() {
        let mock = spawn_mock("HTTP/1.1 200 OK", token_body("access-1", 3600)).await;
        let mw = AuthMiddleware::new(
            &config_for(mock.base_url.clone(), TokenCacheMode::InMemory),
            api_key_credential(),
        )
        .expect("middleware builds");

        let _ = mw.bearer().await.expect("first exchange");
        assert_eq!(mock.hits.load(Ordering::SeqCst), 1);
        let _ = mw.force_refresh().await.expect("forced re-exchange");
        assert_eq!(
            mock.hits.load(Ordering::SeqCst),
            2,
            "force_refresh must perform exactly one additional exchange"
        );
    }

    #[tokio::test]
    async fn stale_token_re_exchanges_without_error() {
        // expires_at is immediately within the 30s skew, so every read is stale.
        let mock = spawn_mock("HTTP/1.1 200 OK", token_body("access-stale", 5)).await;
        let mw = AuthMiddleware::new(
            &config_for(mock.base_url.clone(), TokenCacheMode::InMemory),
            api_key_credential(),
        )
        .expect("middleware builds");

        let first = mw.bearer().await.expect("first exchange ok");
        let second = mw.bearer().await.expect("stale token never errors");
        assert_eq!(first.expose(), second.expose());
        assert_eq!(
            mock.hits.load(Ordering::SeqCst),
            2,
            "a stale cached token must trigger a fresh exchange, not an error"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_bearer_callers_single_flight() {
        let mock = spawn_mock("HTTP/1.1 200 OK", token_body("access-shared", 3600)).await;
        let mw = AuthMiddleware::new(
            &config_for(mock.base_url.clone(), TokenCacheMode::InMemory),
            api_key_credential(),
        )
        .expect("middleware builds");

        let mut handles = Vec::new();
        for _ in 0..16 {
            let mw = mw.clone();
            handles.push(tokio::spawn(async move {
                mw.bearer()
                    .await
                    .expect("concurrent bearer ok")
                    .expose()
                    .to_owned()
            }));
        }
        for handle in handles {
            assert_eq!(handle.await.expect("task joins"), "access-shared");
        }
        assert_eq!(
            mock.hits.load(Ordering::SeqCst),
            1,
            "N concurrent callers on an empty cache must trigger exactly one exchange"
        );
    }

    #[tokio::test]
    async fn revoked_key_maps_to_wyrd_error() {
        let body = serde_json::json!({
            "type": "https://wyrd.dev/problems/WYRD_AUTH_401_API_KEY_INVALID",
            "title": "API key not found, revoked, or hash mismatch",
            "status": 401,
            "code": "WYRD_AUTH_401_API_KEY_INVALID",
            "detail": "API key revoked",
            "details": {},
        })
        .to_string();
        let mock = spawn_mock("HTTP/1.1 401 Unauthorized", body).await;
        let mw = AuthMiddleware::new(
            &config_for(mock.base_url.clone(), TokenCacheMode::InMemory),
            api_key_credential(),
        )
        .expect("middleware builds");

        let err = mw.bearer().await.expect_err("revoked key must fail");
        match err {
            AuthError::Server(wyrd) => {
                // The mapper reconstructs the typed variant for this catalog
                // code, so it keeps its real code and 401 status rather than
                // collapsing into the 502 catch-all.
                assert_eq!(
                    wyrd.code(),
                    "WYRD_AUTH_401_API_KEY_INVALID",
                    "the revoked-key code must be preserved in the mapped WyrdError"
                );
                assert_eq!(
                    wyrd.status(),
                    401,
                    "revoked-key status must be 401, not the 502 catch-all"
                );
            }
            AuthError::Client(other) => panic!("expected server error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn workload_exchange_caches_and_serves_from_shared_cache() {
        let mock = spawn_mock("HTTP/1.1 200 OK", token_body("workload-access", 3600)).await;
        let mw = AuthMiddleware::new(
            &config_for(mock.base_url.clone(), TokenCacheMode::InMemory),
            workload_jwt_credential("acme"),
        )
        .expect("middleware builds");

        let bearer = mw.bearer().await.expect("workload exchange succeeds");
        assert_eq!(bearer.expose(), "workload-access");
        assert_eq!(mock.hits.load(Ordering::SeqCst), 1);

        // Second call is served from the same cache the API-key path uses.
        let again = mw.bearer().await.expect("cached");
        assert_eq!(again.expose(), "workload-access");
        assert_eq!(
            mock.hits.load(Ordering::SeqCst),
            1,
            "a fresh workload token must be served from cache, not re-exchanged"
        );
    }

    #[tokio::test]
    async fn workload_force_refresh_re_exchanges_once() {
        let mock = spawn_mock("HTTP/1.1 200 OK", token_body("workload-1", 3600)).await;
        let mw = AuthMiddleware::new(
            &config_for(mock.base_url.clone(), TokenCacheMode::InMemory),
            workload_jwt_credential("acme"),
        )
        .expect("middleware builds");

        let _ = mw.bearer().await.expect("first workload exchange");
        assert_eq!(mock.hits.load(Ordering::SeqCst), 1);
        let _ = mw
            .force_refresh()
            .await
            .expect("forced workload re-exchange");
        assert_eq!(
            mock.hits.load(Ordering::SeqCst),
            2,
            "force_refresh must perform exactly one additional workload exchange"
        );
    }

    #[tokio::test]
    async fn workload_invalid_tenant_slug_is_client_error_preflight() {
        let mock = spawn_mock("HTTP/1.1 200 OK", token_body("never-issued", 3600)).await;
        let mw = AuthMiddleware::new(
            &config_for(mock.base_url.clone(), TokenCacheMode::InMemory),
            workload_jwt_credential("INVALID SLUG!"),
        )
        .expect("middleware builds");

        let err = mw
            .bearer()
            .await
            .expect_err("invalid tenant slug must fail");
        match err {
            AuthError::Client(WyrdClientError::Config { field, .. }) => {
                assert_eq!(field, "tenant");
            }
            other => panic!("expected client-local config error, got {other:?}"),
        }
        assert_eq!(
            mock.hits.load(Ordering::SeqCst),
            0,
            "an invalid tenant slug must be rejected before any network call"
        );
    }

    #[tokio::test]
    async fn workload_server_rejection_maps_to_server_error() {
        let body = serde_json::json!({
            "type": "https://wyrd.dev/problems/WYRD_AUTH_401_INVALID_TOKEN",
            "title": "Token rejected",
            "status": 401,
            "code": "WYRD_AUTH_401_INVALID_TOKEN",
            "detail": "workload assertion rejected",
            "details": {},
        })
        .to_string();
        let mock = spawn_mock("HTTP/1.1 401 Unauthorized", body).await;
        let mw = AuthMiddleware::new(
            &config_for(mock.base_url.clone(), TokenCacheMode::InMemory),
            workload_jwt_credential("acme"),
        )
        .expect("middleware builds");

        let err = mw
            .bearer()
            .await
            .expect_err("server rejection of workload assertion must fail");
        match err {
            AuthError::Server(_) => {}
            AuthError::Client(other) => panic!("expected server error, got {other:?}"),
        }
    }

    #[test]
    fn cached_token_debug_redacts_secret() {
        let entry = CachedToken {
            access_token: SecretBearer::new("top-secret-token".to_owned()),
            expires_at: Utc::now(),
        };
        let debug = format!("{entry:?}");
        assert!(debug.contains("REDACTED"));
        assert!(!debug.contains("top-secret-token"));
    }

    #[tokio::test]
    async fn disk_cache_writes_secure_record_without_refresh_token() {
        // Inject a unique cache path instead of mutating the global HOME env
        // var, so the test is hermetic and safe under the parallel workspace
        // test runner (the production path resolves ~/.config/wyrd).
        let dir = std::env::temp_dir().join(format!(
            "wyrd_auth_disk_{}_{}",
            std::process::id(),
            Uuid::now_v7()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("token_cache.json");

        let mock = spawn_mock("HTTP/1.1 200 OK", token_body("disk-access", 3600)).await;
        let mw = AuthMiddleware::new_with_cache_path(
            &config_for(mock.base_url.clone(), TokenCacheMode::Disk),
            api_key_credential(),
            Some(path.clone()),
        )
        .expect("middleware builds");

        let _ = mw.bearer().await.expect("exchange ok");

        let metadata = std::fs::metadata(&path).expect("token cache file exists");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(
                metadata.permissions().mode() & 0o777,
                0o600,
                "token cache must be mode 0600"
            );
        }

        let contents = std::fs::read_to_string(&path).expect("read token cache");
        assert!(
            contents.contains("disk-access"),
            "access token must be persisted"
        );
        assert!(
            !contents.contains("refresh-should-drop"),
            "refresh token must never be written to disk"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn request_id_forwards_supplied_id() {
        let mw = AuthMiddleware::new(&ClientConfig::default(), api_key_credential())
            .expect("middleware builds");
        assert_eq!(mw.request_id(Some("inbound-abc")), "inbound-abc");
        // Empty inbound is treated as absent and mints a fresh id.
        assert_ne!(mw.request_id(Some("")), "");
    }

    #[test]
    fn request_id_mints_uuid_v7_when_absent() {
        let mw = AuthMiddleware::new(&ClientConfig::default(), api_key_credential())
            .expect("middleware builds");
        let id = mw.request_id(None);
        let parsed = Uuid::parse_str(&id).expect("minted id is a valid uuid");
        assert_eq!(
            parsed.get_version_num(),
            7,
            "minted request id must be UUIDv7"
        );
    }
}
