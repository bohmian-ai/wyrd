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
//! supplied [`ResolvedCredential::BearerToken`] is passed through as-is. A
//! [`ResolvedCredential::Renewable`] source mints its token in-process through
//! that same cache and gate, and its tokens are never written to disk.
//! [`ResolvedCredential::Delegated`] middleware, built by
//! [`AuthMiddleware::on_behalf_of`], re-runs its RFC 8693 exchange through the
//! same cache and gate, drawing the actor token from the acting middleware.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use chrono::{DateTime, Utc};
use secrecy::{ExposeSecret, SecretString};
use serde::Serialize;
use serde::de::DeserializeOwned;
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use uuid::Uuid;
use wyrd_spec::auth::{
    DeviceAuthorization, DeviceAuthorizationRequest, ExchangeTokenType, PlatformTokenRequest,
    PlatformTokenResponse, RevokeRefreshToken, SecretBearer, TokenAudience, TokenRequest,
    TokenResponse,
};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::TenantSlug;

use crate::config::{ClientConfig, TokenCacheMode};
use crate::credentials_file::CredentialsFile;
use crate::error::{WyrdClientError, from_problem_json};
use crate::transport::HttpConfig;
use crate::transport::credential::{AccessTokenSource, MintedAccessToken, ResolvedCredential};
use reqwest::{Client, Response};
use std::fmt::{Debug, Formatter, Result as FmtResult};

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

impl AuthError {
    /// Project this failure onto the stable Wyrd catalog.
    ///
    /// A server-reported failure already *is* a catalog error and passes
    /// through; a client-local one keeps its `WYRD_CLIENT_*` identity through
    /// the shared client-error projection. Every caller that surfaces an auth
    /// failure to an application — the HTTP transport, the platform handle —
    /// maps it here, so a rejected credential reads the same whichever layer
    /// noticed it.
    #[must_use]
    pub fn into_wyrd(self) -> WyrdError {
        match self {
            Self::Server(wyrd) => wyrd,
            Self::Client(client_error) => client_error.into(),
        }
    }
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

/// A renewable mint running on Tokio's blocking pool.
type PendingMint = JoinHandle<Result<MintedAccessToken, WyrdClientError>>;

/// The unauthenticated `/auth` surface of one Wyrd deployment.
///
/// Every grant that *mints* a Wyrd credential is presented without one: the API
/// key and workload exchanges behind [`AuthMiddleware`], an interactive OIDC
/// login, a refresh-token rotation, and the platform credential exchange all
/// POST to a route that no access token could reach. This type owns that one
/// wire path — the URL join, the POST, the `application/problem+json` mapping,
/// and the typed decode — so those callers do not each grow their own HTTP
/// client and their own idea of what a rejection looks like.
///
/// It is separate from [`crate::transport::HttpTransport`] because that layer
/// injects a bearer on every request, and none of these calls has one yet.
#[derive(Clone)]
pub struct TokenExchange {
    /// Deployment base URL, without a trailing slash.
    base_url: String,
    /// Shared connection pool for the exchange routes.
    http: Client,
}

impl Debug for TokenExchange {
    /// Prints the target without the pool, which carries no secret but no
    /// useful detail either.
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("TokenExchange")
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

impl TokenExchange {
    /// Bind an exchange to one deployment.
    ///
    /// Every route this type calls carries a secret — an API key, a workload
    /// assertion, a device code, or a refresh token — so the target goes
    /// through the same [`HttpConfig::validate`] rule as the authenticated
    /// transport before anything is built: remote cleartext `http://` is
    /// refused, HTTPS and loopback HTTP are accepted. This one check covers
    /// every caller, including the CLI login, logout, and refresh commands.
    ///
    /// Installs Wyrd's process TLS provider next, for the same reason the
    /// authenticated transport does: the provider is process-global and the
    /// first client to build must be the one that sets it.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::Config`] for an empty or remote cleartext
    /// `base_url` or a zero timeout, and [`WyrdClientError::TransportDown`]
    /// when another Rustls provider already owns the process or the HTTP
    /// client cannot be built.
    pub fn new(base_url: &str, timeout_ms: u64) -> Result<Self, WyrdClientError> {
        HttpConfig {
            base_url: base_url.to_owned(),
            timeout_ms,
            compression: false,
        }
        .validate()?;
        wyrd_tls::install_crypto_provider().map_err(|error| WyrdClientError::TransportDown {
            transport: "http".to_owned(),
            message: error.to_string(),
        })?;
        let http = reqwest::Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .build()
            .map_err(|err| WyrdClientError::TransportDown {
                transport: "http".to_owned(),
                message: format!("failed to build HTTP client: {err}"),
            })?;
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
            http,
        })
    }

    /// The normalized deployment base URL this exchange targets.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Exchange one tenant grant at `/auth/token`.
    ///
    /// Returns the server's response whole, refresh token included. The
    /// middleware drops the refresh token because it caches nothing durable; an
    /// interactive caller that must show the operator their new refresh token
    /// needs it, which is why the discarding happens in the caller and not here.
    ///
    /// # Errors
    /// Returns [`AuthError::Server`] with the stable Wyrd error for a rejected
    /// grant, and [`AuthError::Client`] when the server cannot be reached or its
    /// body cannot be decoded.
    pub async fn exchange(&self, request: &TokenRequest) -> Result<TokenResponse, AuthError> {
        self.post("/auth/token", request).await
    }

    /// Exchange a platform credential for a short-lived platform session.
    ///
    /// The one platform call that reads credential material; every later
    /// platform request presents the returned session on the canonical header.
    ///
    /// # Errors
    /// Returns [`AuthError::Server`] with the plane's indistinguishable
    /// unauthenticated error for every credential rejection, and
    /// [`AuthError::Client`] for a transport or decode failure.
    pub async fn platform_session(
        &self,
        request: &PlatformTokenRequest,
    ) -> Result<PlatformTokenResponse, AuthError> {
        self.post("/auth/platform/token", request).await
    }

    /// Begin a device login at `tenant_route_key` (RFC 8628 §3.1); poll it
    /// with [`TokenRequest::DeviceCode`] through [`Self::exchange`].
    ///
    /// # Errors
    /// Returns [`AuthError::Server`] when the tenant offers no SSO login or
    /// the server refuses, and [`AuthError::Client`] for a transport or decode
    /// failure.
    pub async fn device_authorization(
        &self,
        tenant_route_key: &TenantSlug,
    ) -> Result<DeviceAuthorization, AuthError> {
        self.post(
            "/auth/device_authorization",
            &DeviceAuthorizationRequest {
                tenant_route_key: tenant_route_key.clone(),
            },
        )
        .await
    }

    /// Ask the server to end the login `refresh_token` belongs to, so neither
    /// it nor any successor renews again; idempotent on the server.
    ///
    /// # Errors
    /// Returns [`AuthError::Server`] when the server fails and
    /// [`AuthError::Client`] for a transport failure.
    pub async fn revoke_refresh_token(
        &self,
        refresh_token: &SecretBearer,
    ) -> Result<(), AuthError> {
        self.post_no_content(
            "/auth/revoke",
            &RevokeRefreshToken {
                refresh_token: refresh_token.clone(),
            },
        )
        .await
    }

    /// POST a JSON body to one unauthenticated `/auth` path whose success
    /// carries no body.
    ///
    /// # Errors
    /// Returns [`AuthError::Server`] for a non-success status and
    /// [`AuthError::Client`] for a transport or decode failure.
    async fn post_no_content<S: Serialize>(&self, path: &str, body: &S) -> Result<(), AuthError> {
        let url = format!("{}{path}", self.base_url);
        let response = self
            .http
            .post(&url)
            .json(body)
            .send()
            .await
            .map_err(transport_down)?;
        if response.status().is_success() {
            return Ok(());
        }
        Self::decode::<serde_json::Value>(response)
            .await
            .map(|_| ())
    }

    /// POST a JSON body to one unauthenticated `/auth` path and decode the reply.
    ///
    /// # Errors
    /// Returns [`AuthError::Server`] for a non-success status and
    /// [`AuthError::Client`] for a transport or decode failure.
    async fn post<S, D>(&self, path: &str, body: &S) -> Result<D, AuthError>
    where
        S: Serialize,
        D: DeserializeOwned,
    {
        let url = format!("{}{path}", self.base_url);
        let response = self
            .http
            .post(&url)
            .json(body)
            .send()
            .await
            .map_err(transport_down)?;
        Self::decode(response).await
    }

    /// Map one response onto the catalog or the typed success body.
    ///
    /// A non-success status is read as `application/problem+json` so every
    /// caller reports the server's own stable code rather than inventing a
    /// status-shaped error of its own.
    ///
    /// # Errors
    /// Returns [`AuthError::Server`] for a non-success status and
    /// [`AuthError::Client`] when the body cannot be read or decoded.
    async fn decode<D: DeserializeOwned>(response: Response) -> Result<D, AuthError> {
        if !response.status().is_success() {
            let body = response
                .json::<serde_json::Value>()
                .await
                .map_err(transport_down)?;
            return Err(AuthError::Server(from_problem_json(&body)));
        }
        response.json::<D>().await.map_err(transport_down)
    }
}

/// Single, `Arc`-shared auth path: token exchange, cache, and refresh.
pub struct AuthMiddleware {
    credential: ResolvedCredential,
    /// The unauthenticated `/auth` surface this middleware exchanges against.
    exchange: TokenExchange,
    /// Total deadline of one `/auth/token` exchange, the same
    /// `HttpConfig::timeout_ms` that bounds the exchange client.
    exchange_timeout: Duration,
    cache: Mutex<Option<CachedToken>>,
    /// The renewable mint started under the `cache` gate and not yet
    /// consumed. Locked only while `cache` is held. It outlives a cancelled
    /// waiter, so the next caller awaits that same mint instead of starting a
    /// concurrent one.
    pending_mint: Mutex<Option<PendingMint>>,
    /// The credential file an API key's access token is cached in, beside
    /// that key, so every process using it reuses the token; set only in
    /// [`TokenCacheMode::Disk`] mode.
    credentials: Option<CredentialsFile>,
    /// Set once the first short-TTL token is observed, so the operational
    /// warning fires at most once per middleware instance (see [`Self::exchange`]).
    short_ttl_warned: AtomicBool,
}

impl Debug for AuthMiddleware {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("AuthMiddleware")
            .field("credential", &self.credential)
            .field("http_base_url", &self.exchange.base_url)
            .field("credentials", &self.credentials)
            .finish_non_exhaustive()
    }
}

impl AuthMiddleware {
    /// Build the middleware from a [`ClientConfig`] and a resolved credential.
    ///
    /// The API key is sent only to `{config.http.base_url}/auth/token`. In
    /// [`TokenCacheMode::Disk`] mode a non-stale access token cached in
    /// `credentials.toml` beside that same API key is loaded as the initial
    /// cache, so the token survives process restarts.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::TransportDown`] when another Rustls provider
    /// already owns the process or the underlying Reqwest client cannot be
    /// constructed.
    pub fn new(
        config: &ClientConfig,
        credential: ResolvedCredential,
    ) -> Result<Arc<Self>, WyrdClientError> {
        let credentials = if config.token_cache == TokenCacheMode::Disk {
            CredentialsFile::locate()
        } else {
            None
        };
        Self::build(config, credential, credentials)
    }

    /// Construct over a resolved credential file. `new` locates the user's
    /// `credentials.toml`; tests inject one in a temporary directory so they
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
        credentials: Option<CredentialsFile>,
    ) -> Result<Arc<Self>, WyrdClientError> {
        let exchange = TokenExchange::new(&config.http.base_url, config.http.timeout_ms)?;
        let exchange_timeout = Duration::from_millis(config.http.timeout_ms);

        let initial = match (&credentials, &credential) {
            (Some(file), ResolvedCredential::ApiKey(api_key)) => file
                .cached_api_key_token(api_key)
                .map(|(access_token, expires_at)| CachedToken {
                    access_token,
                    expires_at,
                })
                .filter(|entry| !entry.is_stale()),
            _ => None,
        };

        Ok(Arc::new(Self {
            credential,
            exchange,
            exchange_timeout,
            cache: Mutex::new(initial),
            pending_mint: Mutex::new(None),
            credentials,
            short_ttl_warned: AtomicBool::new(false),
        }))
    }

    /// Test-only constructor that injects the credential file, avoiding any
    /// `HOME`/`~` resolution so disk-cache tests are hermetic.
    ///
    /// # Errors
    /// The errors of [`Self::new`].
    #[cfg(test)]
    fn new_with_credentials(
        config: &ClientConfig,
        credential: ResolvedCredential,
        credentials: Option<CredentialsFile>,
    ) -> Result<Arc<Self>, WyrdClientError> {
        Self::build(config, credential, credentials)
    }

    /// Derive a middleware in which this one acts for the holder of
    /// `subject_token` against `audience`.
    ///
    /// The derived middleware shares this one's `/auth` connection pool and
    /// caches the delegated token in memory only, never on disk. Each exchange
    /// presents this middleware's current bearer as the RFC 8693 actor token,
    /// so the actor's own refresh keeps working underneath. No network call is
    /// made here.
    #[must_use]
    pub fn on_behalf_of(
        self: &Arc<Self>,
        subject_token: SecretString,
        audience: TokenAudience,
    ) -> Arc<Self> {
        Arc::new(Self {
            credential: ResolvedCredential::Delegated {
                subject_token,
                audience,
                actor: Arc::clone(self),
            },
            exchange: self.exchange.clone(),
            exchange_timeout: self.exchange_timeout,
            cache: Mutex::new(None),
            pending_mint: Mutex::new(None),
            credentials: None,
            short_ttl_warned: AtomicBool::new(false),
        })
    }

    /// The credential this middleware authenticates with.
    ///
    /// Exposed so a client-tier owner can fingerprint the secret material it
    /// is already bound to — the Bifrost [`ClientScope`] keys its producer
    /// pool on `(base URL, credential fingerprint)` — without re-resolving the
    /// credential chain and risking a different answer than the live transport
    /// uses.
    ///
    /// [`ClientScope`]: crate::bifrost::ClientScope
    #[must_use]
    pub fn credential(&self) -> &ResolvedCredential {
        &self.credential
    }

    /// The HTTP base URL this middleware exchanges tokens against.
    ///
    /// The same normalized URL [`HttpTransport`](crate::transport::HttpTransport)
    /// joins relative paths onto, so a scope derived from it matches the plane
    /// requests actually reach.
    #[must_use]
    pub fn base_url(&self) -> &str {
        self.exchange.base_url()
    }

    /// The longest one token exchange may take.
    ///
    /// A caller that bounds an operation including [`Self::bearer`] or
    /// [`Self::force_refresh`], such as a retrying transport, sizes its budget
    /// from this value instead of restating the HTTP configuration.
    #[must_use]
    pub(crate) fn exchange_timeout(&self) -> Duration {
        self.exchange_timeout
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
    /// assertion via the `jwt-bearer` grant. A [`ResolvedCredential::Delegated`]
    /// follows it too, running the RFC 8693 token exchange. A
    /// [`ResolvedCredential::Renewable`] source reuses its cached token too
    /// until the refresh skew.
    ///
    /// # Errors
    /// Returns [`AuthError::Client`] on transport failure or an invalid tenant
    /// slug, or [`AuthError::Server`] when the server rejects the credential.
    pub async fn bearer(&self) -> Result<SecretBearer, AuthError> {
        match &self.credential {
            ResolvedCredential::Renewable(source) => {
                let mut cache = self.cache.lock().await;
                if let Some(entry) = cache.as_ref()
                    && !entry.is_stale()
                {
                    return Ok(entry.access_token.clone());
                }
                self.mint_into(source, &mut cache).await
            }
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
            ResolvedCredential::Delegated {
                subject_token,
                audience,
                actor,
            } => {
                let mut cache = self.cache.lock().await;
                if let Some(entry) = cache.as_ref()
                    && !entry.is_stale()
                {
                    return Ok(entry.access_token.clone());
                }
                let entry = self
                    .exchange_delegated(subject_token, *audience, actor)
                    .await?;
                Ok(self.store(entry, &mut cache))
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
            ResolvedCredential::Renewable(source) => {
                let mut cache = self.cache.lock().await;
                self.mint_into(source, &mut cache).await
            }
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
            ResolvedCredential::Delegated {
                subject_token,
                audience,
                actor,
            } => {
                let mut cache = self.cache.lock().await;
                let entry = self
                    .exchange_delegated(subject_token, *audience, actor)
                    .await?;
                Ok(self.store(entry, &mut cache))
            }
        }
    }

    /// Exchange the API key, cache the result beside that key in
    /// `credentials.toml` ([`Self::persist`]), and replace the in-memory
    /// cache. The shared post-exchange tail of [`AuthMiddleware::bearer`] and
    /// [`AuthMiddleware::force_refresh`]; the caller holds the exchange gate.
    ///
    /// # Errors
    /// The exchange's [`AuthError`]; a failed cache write is not an error.
    async fn exchange_and_store(
        &self,
        api_key: &SecretString,
        cache: &mut Option<CachedToken>,
    ) -> Result<SecretBearer, AuthError> {
        let entry = self.exchange(api_key).await?;
        self.persist(api_key, &entry).await;
        Ok(self.store(entry, cache))
    }

    /// Exchange the workload JWT and replace the in-memory cache. The [`ResolvedCredential::WorkloadJwt`] analogue of
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

    /// Mint a fresh token from `source` and replace the in-memory cache.
    ///
    /// The [`ResolvedCredential::Renewable`] tail of [`AuthMiddleware::bearer`]
    /// and [`AuthMiddleware::force_refresh`]; the caller holds the exchange gate,
    /// so racing callers mint once. Unlike an exchanged token, a minted token is
    /// never persisted: the source mints again after a restart.
    ///
    /// The synchronous [`AccessTokenSource::mint`] may block, so it runs on
    /// Tokio's blocking pool and this future stays pollable: a caller's timeout
    /// can abandon it while the mint is still running. The running mint is kept
    /// in `pending_mint` until a caller receives its result, so a caller
    /// arriving after a cancelled one awaits that mint rather than starting
    /// another; at most one mint runs per middleware.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::Client`] with the source's error when minting fails;
    /// the cache is left unchanged.
    ///
    /// # Cancellation
    ///
    /// Dropping the future releases the gate without touching the cache. An
    /// already started mint keeps running and stays pending; its token is
    /// installed only by the next caller that awaits it.
    ///
    /// # Panics
    ///
    /// Panics when the source's `mint` panics.
    async fn mint_into(
        &self,
        source: &Arc<dyn AccessTokenSource>,
        cache: &mut Option<CachedToken>,
    ) -> Result<SecretBearer, AuthError> {
        let mut pending = self.pending_mint.lock().await;
        let mint = pending.get_or_insert_with(|| {
            let source = Arc::clone(source);
            tokio::task::spawn_blocking(move || source.mint())
        });
        let joined = mint.await;
        *pending = None;
        let minted = joined.expect("renewable mint task joins")?;
        let bearer = minted.access_token.clone();
        *cache = Some(CachedToken {
            access_token: minted.access_token,
            expires_at: minted.expires_at,
        });
        Ok(bearer)
    }

    /// Replace the in-memory cache with a freshly exchanged token,
    /// returning the access bearer. The shared cache-write tail of every
    /// exchange path.
    fn store(&self, entry: CachedToken, cache: &mut Option<CachedToken>) -> SecretBearer {
        let bearer = entry.access_token.clone();
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

    /// Run the RFC 8693 exchange: `actor`'s current bearer acts for the holder
    /// of `subject_token`, bound to `audience`.
    ///
    /// # Errors
    /// Returns the actor's own [`AuthError`] when it cannot produce a bearer,
    /// and otherwise the exchange's transport failure or server refusal.
    async fn exchange_delegated(
        &self,
        subject_token: &SecretString,
        audience: TokenAudience,
        actor: &AuthMiddleware,
    ) -> Result<CachedToken, AuthError> {
        let request = TokenRequest::TokenExchange {
            subject_token: SecretBearer::new(subject_token.expose_secret().to_owned()),
            subject_token_type: ExchangeTokenType::AccessToken,
            // Boxed because the actor may itself be delegated, which makes
            // `bearer` recursive.
            actor_token: Box::pin(actor.bearer()).await?,
            actor_token_type: ExchangeTokenType::AccessToken,
            audience,
        };
        self.post_token_request(request).await
    }

    /// POST a token request to `/auth/token`, map a non-2xx body via
    /// [`from_problem_json`] into [`AuthError::Server`], and decode the success
    /// body into a [`CachedToken`]. The shared POST/decode tail of every grant.
    ///
    /// # Errors
    /// Returns the [`AuthError`] the exchange produced: a transport failure, or
    /// [`AuthError::Server`] carrying the server's problem-json refusal.
    async fn post_token_request(&self, request: TokenRequest) -> Result<CachedToken, AuthError> {
        let token = self.exchange.exchange(&request).await?;
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

    /// Cache an API key's freshly exchanged access token beside that key in
    /// `credentials.toml`, in [`TokenCacheMode::Disk`] mode.
    ///
    /// The write takes the credential file's lock and may wait for another
    /// process, so it runs on the blocking pool. Best-effort: a key that is
    /// not `[default].api_key`, an unsafe file, or a failed write leaves the
    /// token in memory only and never fails the exchange.
    async fn persist(&self, api_key: &SecretString, entry: &CachedToken) {
        let Some(file) = self.credentials.clone() else {
            return;
        };
        let api_key = api_key.clone();
        let access_token = entry.access_token.clone();
        let expires_at = entry.expires_at;
        let written = tokio::task::spawn_blocking(move || {
            file.cache_api_key_token(&api_key, &access_token, expires_at)
        })
        .await;
        if let Ok(Err(error)) = written {
            tracing::debug!(target: "wyrd.client.auth", %error, "API key access token not cached");
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

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt as _;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use chrono::Utc;
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    use tokio::net::TcpListener;
    use uuid::Uuid;

    use super::{AuthError, AuthMiddleware, CachedToken, TokenExchange};
    use crate::config::{ClientConfig, TokenCacheMode};
    use crate::credentials_file::CredentialsFile;
    use crate::error::WyrdClientError;
    use crate::saved_login::{SavedLogin, SavedLogins};
    use crate::transport::credential::{AccessTokenSource, MintedAccessToken, ResolvedCredential};
    use wyrd_spec::auth::SecretBearer;
    use wyrd_spec::ids::TenantSlug;

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

    /// A delegated middleware exchanges once (after the actor's own exchange),
    /// serves the result from its memory cache, re-exchanges exactly once on a
    /// forced refresh while reusing the actor's cached bearer, and never prints
    /// the subject token.
    ///
    /// # Panics
    /// Panics when an exchange fails or a hit count or redaction check differs.
    #[tokio::test]
    async fn on_behalf_of_caches_the_exchange_and_redacts_the_subject() {
        let mock = spawn_mock("HTTP/1.1 200 OK", token_body("delegated-1", 3600)).await;
        let actor = AuthMiddleware::new(
            &config_for(mock.base_url.clone(), TokenCacheMode::InMemory),
            api_key_credential(),
        )
        .expect("middleware builds");
        let delegated = actor.on_behalf_of(
            "subject-secret".to_owned().into(),
            wyrd_spec::auth::TokenAudience::Bifrost,
        );

        let bearer = delegated.bearer().await.expect("delegated exchange");
        assert_eq!(bearer.expose(), "delegated-1");
        assert_eq!(mock.hits.load(Ordering::SeqCst), 2, "actor then exchange");
        delegated.bearer().await.expect("cached");
        assert_eq!(mock.hits.load(Ordering::SeqCst), 2, "served from cache");
        delegated.force_refresh().await.expect("forced re-exchange");
        assert_eq!(
            mock.hits.load(Ordering::SeqCst),
            3,
            "only the delegated exchange repeats"
        );

        assert!(delegated.credentials.is_none(), "never persisted to disk");
        let debug = format!("{delegated:?}");
        assert!(!debug.contains("subject-secret"), "{debug}");
        assert!(debug.contains("Bifrost"), "{debug}");
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

    /// Write a private `credentials.toml` holding `[default].api_key` and a
    /// user comment into a fresh directory.
    ///
    /// # Panics
    ///
    /// Panics when the temporary directory or file cannot be created.
    fn credentials_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("credentials.toml");
        std::fs::write(
            &path,
            "# kept by the user\n[default]\napi_key = \"api-key-value\"\n",
        )
        .expect("write credentials");
        #[cfg(unix)]
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .expect("chmod credentials");
        dir
    }

    /// An API key's exchanged token is cached in `credentials.toml` beside
    /// that key, reused by a later middleware, and never written for another
    /// key or into a separate file.
    ///
    /// # Panics
    ///
    /// Panics when the cache is missing, insecure, drops user content, holds
    /// the refresh token, or is not reused.
    #[tokio::test]
    async fn disk_cache_writes_token_beside_its_api_key() {
        let dir = credentials_dir();
        let path = dir.path().join("credentials.toml");
        let mock = spawn_mock("HTTP/1.1 200 OK", token_body("disk-access", 3600)).await;
        let config = config_for(mock.base_url.clone(), TokenCacheMode::Disk);
        let file = || Some(CredentialsFile::at(dir.path().to_path_buf()));

        let mw = AuthMiddleware::new_with_credentials(&config, api_key_credential(), file())
            .expect("middleware builds");
        mw.bearer().await.expect("exchange ok");

        #[cfg(unix)]
        {
            let mode = std::fs::metadata(&path)
                .expect("credentials")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "credentials.toml must stay 0600");
        }
        let contents = std::fs::read_to_string(&path).expect("read credentials");
        assert!(contents.contains("# kept by the user"), "{contents}");
        assert!(contents.contains("api_key = \"api-key-value\""));
        assert!(contents.contains("access_token = \"disk-access\""));
        assert!(contents.contains("access_expires_at"));
        assert!(
            !contents.contains("refresh-should-drop"),
            "refresh token never written"
        );
        assert!(
            !dir.path().join("tokens").exists(),
            "no separate token cache"
        );

        let reused = AuthMiddleware::new_with_credentials(&config, api_key_credential(), file())
            .expect("middleware builds");
        reused.bearer().await.expect("cached bearer");
        assert_eq!(
            mock.hits.load(Ordering::SeqCst),
            1,
            "served from credentials.toml"
        );

        let other = AuthMiddleware::new_with_credentials(
            &config,
            ResolvedCredential::ApiKey("other-key".to_owned().into()),
            file(),
        )
        .expect("middleware builds");
        other.bearer().await.expect("other key exchanges");
        assert_eq!(
            mock.hits.load(Ordering::SeqCst),
            2,
            "another key never reuses it"
        );
        assert_eq!(
            std::fs::read_to_string(&path).expect("read credentials"),
            contents,
            "another key never writes the cache"
        );
    }

    /// Source double minting sequentially numbered tokens of a fixed lifetime.
    struct CountingSource {
        /// Tokens minted so far.
        minted: AtomicUsize,
        /// Lifetime of every minted token.
        ttl_seconds: i64,
    }

    impl CountingSource {
        /// Builds one shared source minting tokens valid for `ttl_seconds`.
        fn new(ttl_seconds: i64) -> Arc<Self> {
            Arc::new(Self {
                minted: AtomicUsize::new(0),
                ttl_seconds,
            })
        }
    }

    impl AccessTokenSource for CountingSource {
        /// Names the fixed test identity.
        fn identity(&self) -> &str {
            "test-system-producer"
        }

        /// Mints `minted-<n>` expiring after the configured lifetime.
        ///
        /// # Errors
        ///
        /// This test source never fails.
        fn mint(&self) -> Result<MintedAccessToken, WyrdClientError> {
            let ordinal = self.minted.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(MintedAccessToken {
                access_token: SecretBearer::new(format!("minted-{ordinal}")),
                expires_at: Utc::now() + chrono::Duration::seconds(self.ttl_seconds),
            })
        }
    }

    /// A renewable source reuses the exchange cache, proactive refresh,
    /// single-flight, and forced refresh, and never touches the network.
    ///
    /// # Panics
    ///
    /// Panics when a fresh token re-mints, a stale or forced read does not,
    /// or concurrent readers on an empty cache mint more than once.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn renewable_source_uses_the_shared_refresh_lifecycle() {
        let config = config_for("http://127.0.0.1:9".to_owned(), TokenCacheMode::InMemory);
        let fresh = CountingSource::new(900);
        let mw = AuthMiddleware::new(&config, ResolvedCredential::Renewable(fresh.clone()))
            .expect("middleware builds");
        let mut readers = Vec::new();
        for _ in 0..16 {
            let mw = Arc::clone(&mw);
            readers.push(tokio::spawn(async move {
                mw.bearer().await.expect("mint").expose().to_owned()
            }));
        }
        for reader in readers {
            assert_eq!(reader.await.expect("reader joins"), "minted-1");
        }
        assert_eq!(fresh.minted.load(Ordering::SeqCst), 1, "single-flight mint");
        assert_eq!(
            mw.force_refresh().await.expect("forced mint").expose(),
            "minted-2"
        );
        assert_eq!(mw.bearer().await.expect("cached").expose(), "minted-2");

        let stale = CountingSource::new(5);
        let mw = AuthMiddleware::new(&config, ResolvedCredential::Renewable(stale.clone()))
            .expect("middleware builds");
        mw.bearer().await.expect("first mint");
        assert_eq!(
            mw.bearer().await.expect("proactive refresh").expose(),
            "minted-2",
            "a token inside the refresh skew is replaced before use"
        );
    }

    /// A saved login's token is used as saved until it nears expiry, with no
    /// server call. Renewal then happens under the file lock: a removed
    /// login asks for `wyrd auth login`, an unreachable server leaves the
    /// record unchanged for a later retry, and an unsafe file fails closed.
    ///
    /// # Panics
    ///
    /// Panics when a fresh token is not reused or a renewal failure is not
    /// reported.
    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn saved_login_token_is_reused_until_expiry() {
        let dir = tempfile::tempdir().expect("tempdir");
        let origin = "http://127.0.0.1:9";
        let login = SavedLogin {
            origin: origin.to_owned(),
            tenant_key: "acme".parse::<TenantSlug>().expect("slug"),
            access_token: SecretBearer::new("first".to_owned()),
            access_expires_at: Utc::now() + chrono::Duration::hours(1),
            refresh_token: SecretBearer::new("refresh".to_owned()),
        };
        let store = SavedLogins::at(dir.path().to_path_buf());
        store.save(login.clone()).expect("saves");
        let source = || {
            let exchange = TokenExchange::new(origin, 1_000).expect("exchange builds");
            AuthMiddleware::new(
                &config_for(origin.to_owned(), TokenCacheMode::InMemory),
                ResolvedCredential::Renewable(store.source(&login, exchange)),
            )
            .expect("middleware builds")
        };
        let refused = |result: Result<SecretBearer, AuthError>| match result {
            Err(AuthError::Client(error)) => error.to_string(),
            other => panic!("the renewal is refused, got {other:?}"),
        };
        assert_eq!(source().bearer().await.expect("saved").expose(), "first");

        let expired = SavedLogin {
            access_expires_at: Utc::now(),
            ..login.clone()
        };
        store.save(expired.clone()).expect("saves");
        let error = refused(source().bearer().await);
        assert!(
            !error.contains("saved user login"),
            "transport error: {error}"
        );
        assert_eq!(store.list().expect("lists"), vec![expired]);

        std::fs::set_permissions(
            dir.path().join("credentials.toml"),
            std::fs::Permissions::from_mode(0o644),
        )
        .expect("chmod");
        assert!(refused(source().bearer().await).contains("(unsafe_store)"));
        std::fs::set_permissions(
            dir.path().join("credentials.toml"),
            std::fs::Permissions::from_mode(0o600),
        )
        .expect("chmod");
        store
            .remove(origin, &login.tenant_key)
            .expect("removes")
            .expect("was saved");
        assert!(refused(source().bearer().await).contains("(logged_out)"));
    }

    /// A minted token stays in memory even when the client caches on disk.
    ///
    /// # Panics
    ///
    /// Panics when the disk-mode middleware changes `credentials.toml`.
    #[tokio::test]
    async fn renewable_tokens_are_never_persisted() {
        let dir = credentials_dir();
        let before = std::fs::read_to_string(dir.path().join("credentials.toml")).expect("read");
        let mw = AuthMiddleware::new_with_credentials(
            &config_for("http://127.0.0.1:9".to_owned(), TokenCacheMode::Disk),
            ResolvedCredential::Renewable(CountingSource::new(900)),
            Some(CredentialsFile::at(dir.path().to_path_buf())),
        )
        .expect("middleware builds");
        mw.bearer().await.expect("mint");
        let after = std::fs::read_to_string(dir.path().join("credentials.toml")).expect("read");
        assert_eq!(before, after, "a minted capture token must not reach disk");
        assert!(format!("{mw:?}").contains("test-system-producer"));
    }

    /// Every secret-bearing `/auth` route refuses a remote cleartext target
    /// at construction, before any request, while HTTPS and loopback HTTP
    /// stay usable.
    #[test]
    fn token_exchange_refuses_remote_cleartext() {
        for remote in ["http://wyrd.example.com", "http://10.0.0.5:8080/"] {
            let error = TokenExchange::new(remote, 30_000).expect_err("remote http is refused");
            assert!(
                matches!(&error, WyrdClientError::Config { reason, .. } if reason.contains("cleartext")),
                "{error:?}"
            );
        }
        for allowed in [
            "https://wyrd.example.com",
            "http://localhost:8080",
            "http://127.0.0.1:9000/",
            "http://[::1]:8080",
        ] {
            TokenExchange::new(allowed, 30_000).expect("allowed target builds");
        }
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
