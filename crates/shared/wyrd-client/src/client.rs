//! Assembled Wyrd client.
//!
//! [`WyrdClient`] is the single entry point a caller constructs once and then
//! uses for every request. It wires the four client layers together —
//! [`ClientConfig`] resolution, credential resolution, [`AuthMiddleware`] token
//! exchange/refresh, and the [`HttpTransport`] retry loop — behind one
//! constructor, so callers do not hand-assemble the stack per use site.
//!
//! The façade owns no request behavior of its own: `request_json`,
//! `request_arrow`, and `submit_idempotent` delegate to the held
//! [`HttpTransport`], which already injects the `x-wyrd-access-token` bearer and
//! applies the retry policy. gRPC is connected lazily via
//! `WyrdClient::connect_grpc`, sharing the same [`AuthMiddleware`] so the HTTP
//! and gRPC planes authenticate through one token path.

use std::sync::Arc;

use secrecy::SecretString;
#[cfg(feature = "internal")]
use serde::Serialize;
#[cfg(feature = "internal")]
use serde::de::DeserializeOwned;
use wyrd_spec::auth::{SecretBearer, TokenAudience};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::CardUid;
#[cfg(feature = "internal")]
use wyrd_spec::request_id::RequestId;

use crate::auth::{AuthError, AuthMiddleware};
use crate::config::ClientConfig;
use crate::environment::Environment;
use crate::error::WyrdClientError;
use crate::global_config::GlobalConfig;
use crate::transport::config::{GrpcConfig, HttpConfig};
use crate::transport::credential::{BrowserSession, ResolvedCredential};
#[cfg(feature = "internal")]
use crate::transport::http::ArrowResponse;
use crate::transport::http::HttpTransport;

/// Assembled Wyrd client: a single handle over the resolved transport + auth
/// stack.
///
/// Construct once with [`WyrdClient::from_global`] (or
/// [`WyrdClient::with_config`]) and reuse it for every request. Cloning is
/// cheap — the underlying `reqwest::Client` and [`AuthMiddleware`] are shared
/// handles, so a clone reuses the same connection pool and token cache rather
/// than re-resolving credentials or redialing.
#[derive(Debug, Clone)]
pub struct WyrdClient {
    /// Shared authentication middleware; crate capabilities read its
    /// credential and scope from here rather than through the `internal`
    /// accessor.
    pub(crate) auth: Arc<AuthMiddleware>,
    /// Authenticated HTTP transport every crate capability sends its requests
    /// through; the public raw-request methods are `internal`-only wrappers
    /// over it.
    pub(crate) http: HttpTransport,
    /// Held for the lazy gRPC dial; only the gRPC plane reads it.
    pub(crate) grpc_config: GrpcConfig,
}

impl WyrdClient {
    /// Build the ambient client: the one resolution every no-argument
    /// constructor uses.
    ///
    /// [`Self::from_environment`] over [`Environment::Process`]: endpoints
    /// from the global config file, then environment variables, then
    /// defaults, then the effective credential via
    /// [`ClientConfig::resolve_credential`], assembled into the auth and HTTP
    /// layers. No network call is made here.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::Config`] when the global config file exists
    /// but cannot be read or parsed, [`WyrdClientError::NoCredentials`] when no
    /// credential source is configured, or [`WyrdClientError::TransportDown`]
    /// when the HTTP client cannot be built.
    pub fn from_global() -> Result<Self, WyrdClientError> {
        Self::from_environment(Environment::Process)
    }

    /// Resolve the ambient client from `environment`: the one resolver every
    /// no-client handle `connect` reaches through [`Self::from_global`] with
    /// [`Environment::Process`].
    ///
    /// Loads `config.toml` from the configuration directory `environment`
    /// names, overlays `environment`'s endpoint variables and defaults, and
    /// resolves the credential chain from the same variables. No network call
    /// is made here.
    ///
    /// # Arguments
    /// * `environment` - Source of every ambient variable, such as a fixed set
    ///   in a test or a saved local configuration home.
    ///
    /// # Errors
    /// As [`Self::from_global`].
    pub fn from_environment(environment: Environment) -> Result<Self, WyrdClientError> {
        let global = GlobalConfig::load_in(&environment)?;
        Self::with_config(ClientConfig::from_environment(
            environment,
            &global,
            None,
            None,
        ))
    }

    /// Resolve the ambient client and complete any credential exchange it
    /// requires: the one path every no-client handle `connect` takes.
    ///
    /// # Errors
    /// Returns the configuration or credential-resolution error
    /// [`Self::from_global`] reports, or the stable error of a refused or
    /// unreachable exchange, as [`Self::connected`].
    pub(crate) async fn connect_ambient() -> Result<Self, WyrdError> {
        Self::from_global()
            .map_err(WyrdError::from)?
            .connected()
            .await
    }

    /// Return a clone of this client after completing any credential exchange
    /// its credential path requires, for a handle to retain.
    ///
    /// Reads through [`Self::access_token`], so a presented-as-is bearer or a
    /// fresh cached token returns without a request, while an API key,
    /// workload JWT, renewable saved login, or delegated credential exchanges
    /// once and fills the shared cache every clone reads. Later renewal and
    /// refusal-triggered re-exchange are unchanged.
    ///
    /// # Errors
    /// Returns the server's stable error when it refuses the exchange, or a
    /// transport error when `/auth/token` cannot be reached.
    pub(crate) async fn connected(&self) -> Result<Self, WyrdError> {
        // A browser session is presented as itself; it has no exchange to complete.
        if !self.is_browser_session() {
            self.access_token().await?;
        }
        Ok(self.clone())
    }

    /// Whether this client acts as a forwarded browser session.
    ///
    /// Such a client reaches the tenant `/v1` HTTP surface only, so handles
    /// that would otherwise open a bearer-only channel, such as Bifrost's gRPC
    /// ingest, skip it.
    pub(crate) fn is_browser_session(&self) -> bool {
        matches!(
            self.auth.credential(),
            ResolvedCredential::BrowserSession(_)
        )
    }

    /// Build a client that acts as one forwarded browser session.
    ///
    /// For a server-side web host, such as the Wyrd UI's server, that serves a
    /// browser signed in through Rust-owned browser sessions. Every request
    /// presents the session's cookie and trusted public origin; nothing is
    /// minted, cached or renewed, and no ambient credential is consulted. The
    /// client reaches the tenant `/v1` HTTP surface only: gRPC, MCP and
    /// [`Self::access_token`] refuse it because a session is not a bearer.
    /// No network call is made here.
    ///
    /// # Arguments
    /// * `server_url` - The Wyrd server's HTTP base URL.
    /// * `session` - The validated session cookie and its public origin.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::Config`] for an invalid `server_url`, or
    /// [`WyrdClientError::TransportDown`] when the HTTP client cannot be built.
    pub fn for_browser_session(
        server_url: &str,
        session: BrowserSession,
    ) -> Result<Self, WyrdClientError> {
        let config = ClientConfig {
            http: HttpConfig {
                base_url: server_url.trim_end_matches('/').to_owned(),
                ..HttpConfig::default()
            },
            ..ClientConfig::default()
        };
        let auth = AuthMiddleware::new(&config, ResolvedCredential::BrowserSession(session))?;
        let http = HttpTransport::new(&config.http, Arc::clone(&auth))?;
        Ok(Self {
            auth,
            http,
            grpc_config: config.grpc,
        })
    }

    /// Build a client from an explicit [`ClientConfig`].
    ///
    /// Use this when endpoints or the API key are set programmatically rather
    /// than from the environment. Resolution and assembly are identical to
    /// [`WyrdClient::from_global`].
    ///
    /// # Arguments
    /// * `config` - Endpoints, credential sources, and transport settings for the client.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::NoCredentials`] when the config resolves no
    /// credential, or [`WyrdClientError::TransportDown`] when the HTTP client
    /// cannot be built.
    pub fn with_config(config: ClientConfig) -> Result<Self, WyrdClientError> {
        let credential = config.resolve_credential()?;
        let auth = AuthMiddleware::new(&config, credential)?;
        let http = HttpTransport::new(&config.http, Arc::clone(&auth))?;
        Ok(Self {
            auth,
            http,
            grpc_config: config.grpc,
        })
    }

    /// Assemble a client from pre-built layers.
    ///
    /// Use this when a caller has already constructed an [`AuthMiddleware`] and
    /// an [`HttpTransport`] against them (for example, a test harness dialing
    /// a mock server, or an embedder that shares an auth stack across several
    /// service clients). The regular [`Self::from_global`] and
    /// [`Self::with_config`] paths remain the recommended constructors for
    /// production callers.
    ///
    /// # Arguments
    /// * `auth` - Shared authentication middleware the client presents bearers from.
    /// * `http` - HTTP transport already bound to `auth`.
    /// * `grpc_config` - gRPC settings used when a capability dials the gRPC transport.
    #[must_use]
    #[cfg(feature = "internal")]
    pub fn from_parts(
        auth: Arc<AuthMiddleware>,
        http: HttpTransport,
        grpc_config: GrpcConfig,
    ) -> Self {
        Self {
            auth,
            http,
            grpc_config,
        }
    }

    /// Copy this client with an HTTP per-attempt deadline of at least `floor`.
    ///
    /// Shares the auth path, connection pool, and gRPC configuration; only a
    /// capability whose server-side deadline exceeds the configured default
    /// uses it.
    #[must_use]
    pub(crate) fn with_min_request_timeout(&self, floor: std::time::Duration) -> Self {
        Self {
            auth: Arc::clone(&self.auth),
            http: self.http.with_min_request_timeout(floor),
            grpc_config: self.grpc_config.clone(),
        }
    }

    /// Send a JSON request and decode the JSON response body.
    ///
    /// Delegates to [`HttpTransport::request_json`].
    ///
    /// # Arguments
    /// * `method` - HTTP method of the request.
    /// * `path` - Server-relative request path (leading `/`), joined to the validated
    ///   deployment origin.
    /// * `body` - Request body serialized as JSON; `None` sends no body.
    ///
    /// # Errors
    /// Non-`2xx` server responses map to [`WyrdError`]; transport failures
    /// become [`WyrdError::Internal`].
    #[cfg(feature = "internal")]
    pub async fn request_json<S, D>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&S>,
    ) -> Result<D, WyrdError>
    where
        S: Serialize,
        D: DeserializeOwned,
    {
        self.http.request_json(method, path, body).await
    }

    /// Send a request and return raw Arrow IPC bytes plus metadata headers.
    ///
    /// Delegates to [`HttpTransport::request_arrow`].
    ///
    /// # Arguments
    /// * `method` - HTTP method of the request.
    /// * `path` - Server-relative request path (leading `/`), joined to the validated
    ///   deployment origin.
    /// * `body` - Request body serialized as JSON; `None` sends no body.
    ///
    /// # Errors
    /// Non-`2xx` server responses map to [`WyrdError`]; transport failures
    /// become [`WyrdError::Internal`].
    #[cfg(feature = "internal")]
    pub async fn request_arrow<S>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&S>,
    ) -> Result<ArrowResponse, WyrdError>
    where
        S: Serialize,
    {
        self.http.request_arrow(method, path, body).await
    }

    /// Send a request with a stable `Idempotency-Key` minted once and replayed
    /// across retries.
    ///
    /// Delegates to [`HttpTransport::submit_idempotent`].
    ///
    /// # Arguments
    /// * `method` - HTTP method of the request.
    /// * `path` - Server-relative request path (leading `/`), joined to the validated
    ///   deployment origin.
    /// * `body` - Request body, serialized as JSON.
    ///
    /// # Errors
    /// Non-`2xx` server responses map to [`WyrdError`]; transport failures
    /// become [`WyrdError::Internal`].
    #[cfg(feature = "internal")]
    pub async fn submit_idempotent<S, D>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: &S,
    ) -> Result<D, WyrdError>
    where
        S: Serialize,
        D: DeserializeOwned,
    {
        self.http.submit_idempotent(method, path, body).await
    }

    /// Send a JSON mutation with a caller-supplied idempotency key.
    ///
    /// The key is passed verbatim through the transport retry loop, allowing a
    /// deterministic client saga to safely replay a lost response.
    ///
    /// # Arguments
    /// * `method` - HTTP method of the request.
    /// * `path` - Server-relative request path (leading `/`), joined to the validated
    ///   deployment origin.
    /// * `body` - Request body, serialized as JSON.
    /// * `key` - Idempotency key sent verbatim as `Idempotency-Key` on every attempt.
    ///
    /// # Errors
    /// Non-`2xx` server responses map to [`WyrdError`]; serialization and transport failures
    /// become [`WyrdError::Internal`].
    #[cfg(feature = "internal")]
    pub async fn submit_with_idempotency_key<S, D>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: &S,
        key: &str,
    ) -> Result<D, WyrdError>
    where
        S: Serialize,
        D: DeserializeOwned,
    {
        self.http
            .submit_with_idempotency_key(method, path, body, key)
            .await
    }

    /// Send an authenticated one-shot streaming request.
    ///
    /// # Arguments
    /// * `method` - HTTP method of the request.
    /// * `path` - Server-relative request path (leading `/`), joined to the validated
    ///   deployment origin.
    /// * `body` - One-shot streaming body; it is never retried.
    ///
    /// # Errors
    /// Returns a stable Wyrd error for URL, authentication, transport, or HTTP problem
    /// responses.
    #[cfg(feature = "internal")]
    pub async fn request_stream(
        &self,
        method: reqwest::Method,
        path: &str,
        body: reqwest::Body,
    ) -> Result<reqwest::Response, WyrdError> {
        self.http.request_stream(method, path, body).await
    }

    /// Send an authenticated raw request and return its response stream.
    ///
    /// # Arguments
    /// * `method` - HTTP method of the request.
    /// * `path` - Server-relative request path (leading `/`), joined to the validated
    ///   deployment origin.
    ///
    /// # Errors
    /// Returns a stable Wyrd error for URL, authentication, transport, or HTTP problem
    /// responses.
    #[cfg(feature = "internal")]
    pub async fn request_raw(
        &self,
        method: reqwest::Method,
        path: &str,
    ) -> Result<reqwest::Response, WyrdError> {
        self.http.request_raw(method, path).await
    }

    /// Send a cross-origin streaming request through the shared pool without
    /// Wyrd credentials.
    ///
    /// Delegates to [`HttpTransport::request_external_stream`]. Used by
    /// storage-client dispatch modules for presigned/SAS backend PUTs and
    /// GETs.
    ///
    /// # Arguments
    /// * `method` - HTTP method of the request.
    /// * `url` - Absolute cross-origin URL, such as a presigned backend URL.
    /// * `body` - Optional streaming body; `None` sends no body.
    /// * `headers` - Name/value pairs applied verbatim to the request.
    ///
    /// # Errors
    /// Transport failures become [`WyrdError::Internal`].
    #[cfg(feature = "internal")]
    pub async fn request_external_stream(
        &self,
        method: reqwest::Method,
        url: &str,
        body: Option<reqwest::Body>,
        headers: &[(&str, &str)],
    ) -> Result<reqwest::Response, WyrdError> {
        self.http
            .request_external_stream(method, url, body, headers)
            .await
    }

    /// Send an authenticated JSON request and preserve the response as a
    /// streaming body for incremental protocol decoding.
    ///
    /// # Arguments
    /// * `method` - HTTP method of the request.
    /// * `path` - Server-relative request path (leading `/`), joined to the validated
    ///   deployment origin.
    /// * `body` - Request body, serialized as JSON.
    ///
    /// # Errors
    ///
    /// Returns a stable Wyrd error for serialization, authentication,
    /// transport, HTTP problem, or response media-type failures.
    #[cfg(feature = "internal")]
    pub async fn request_json_stream<S>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: &S,
    ) -> Result<reqwest::Response, WyrdError>
    where
        S: Serialize,
    {
        self.http.request_json_stream(method, path, body).await
    }

    /// Sends a streaming JSON request with a caller-owned request ID and verifies its echo.
    ///
    /// Cancelling this future before return abandons connection setup. After
    /// return, dropping the response stops unbuffered body consumption.
    ///
    /// # Arguments
    /// * `method` - HTTP method of the request.
    /// * `path` - Server-relative request path (leading `/`), joined to the validated
    ///   deployment origin.
    /// * `body` - Request body, serialized as JSON.
    /// * `request_id` - Caller-owned request id sent as `wyrd-request-id`; the response must
    ///   echo it.
    ///
    /// # Errors
    ///
    /// Returns a stable request, authentication, transport, response, or ID-mismatch error.
    #[cfg(feature = "internal")]
    pub async fn request_json_stream_with_id<S>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: &S,
        request_id: &RequestId,
    ) -> Result<reqwest::Response, WyrdError>
    where
        S: Serialize,
    {
        self.http
            .request_json_stream_with_id(method, path, body, request_id)
            .await
    }

    /// Return a client in which this client acts for the holder of
    /// `subject_token`, bound to `audience` (RFC 8693 token exchange).
    ///
    /// This client's own credential is the actor: its current bearer is
    /// presented as `actor_token` alongside the inbound `subject_token`. The
    /// server verifies both and issues a short-lived token whose subject is
    /// the inbound principal, whose outer `act` is this client's principal, and
    /// whose permissions are the intersection of both. The first exchange runs
    /// here so a refusal surfaces at the call site; the returned client caches
    /// the delegated token in memory only and re-exchanges before expiry or
    /// after one authentication refusal. It shares this client's connection
    /// pools.
    ///
    /// # Arguments
    /// * `subject_token` - Inbound principal's token presented as the RFC 8693 subject token.
    /// * `audience` - Surface the delegated token is bound to.
    ///
    /// # Errors
    /// Returns the server's stable error when the exchange is refused — an
    /// invalid subject or actor token, a policy denial, a missing actor — or a
    /// transport error when `/auth/token` cannot be reached.
    pub async fn on_behalf_of(
        &self,
        subject_token: SecretString,
        audience: TokenAudience,
    ) -> Result<Self, WyrdError> {
        let auth = self.auth.on_behalf_of(subject_token, audience);
        auth.bearer().await.map_err(AuthError::into_wyrd)?;
        Ok(Self {
            http: self.http.with_auth(Arc::clone(&auth)),
            auth,
            grpc_config: self.grpc_config.clone(),
        })
    }

    /// Return a client whose requests run as the exact Service root
    /// `uid`, with this client's credential and connection pools.
    ///
    /// The returned client keeps its own token cache, so rootless and rooted
    /// clients over one credential never share a token: an API key or
    /// workload token requests `uid` in its exchange and every renewal, and
    /// every request carries it on
    /// [`WORKLOAD_UID_HEADER`](wyrd_spec::auth::WORKLOAD_UID_HEADER), which
    /// selects context for a user bearer. No network call is made here; the
    /// server validates the root on first use and refuses one the account may
    /// not select.
    ///
    /// # Arguments
    /// * `uid` - Exact Service Card UID the returned client runs as.
    #[must_use]
    pub fn for_workload(&self, uid: CardUid) -> Self {
        let auth = self.auth.for_workload(uid);
        Self {
            http: self.http.with_auth(Arc::clone(&auth)),
            auth,
            grpc_config: self.grpc_config.clone(),
        }
    }

    /// Return a client whose requests run as the exact Service root `uid` in
    /// `deployment_environment`, as [`Self::for_workload`] does.
    ///
    /// Native Bifrost writes and gateway calls also carry the environment on
    /// [`DEPLOYMENT_ENVIRONMENT_HEADER`](wyrd_spec::auth::DEPLOYMENT_ENVIRONMENT_HEADER)
    /// as descriptive context; it never changes the grant.
    ///
    /// # Arguments
    /// * `uid` - Exact Service Card UID the returned client runs as.
    /// * `deployment_environment` - The non-empty configured environment, or
    ///   `None` when unknown.
    #[must_use]
    pub fn for_service(&self, uid: CardUid, deployment_environment: Option<String>) -> Self {
        let auth = self.auth.for_service(uid, deployment_environment);
        Self {
            http: self.http.with_auth(Arc::clone(&auth)),
            auth,
            grpc_config: self.grpc_config.clone(),
        }
    }

    /// The exact root this client's requests run as, or `None` for rootless
    /// work: the one [`Self::for_workload`] selected, else `WYRD_WORKLOAD_UID`.
    #[must_use]
    pub fn workload_uid(&self) -> Option<&CardUid> {
        self.auth.workload_uid()
    }

    /// The deployment environment this client's requests declare, or `None`
    /// when none is configured: the one [`Self::for_service`] selected.
    #[must_use]
    pub fn deployment_environment(&self) -> Option<&str> {
        self.auth.deployment_environment()
    }

    /// Whether `other` shares this client's credential path: true for this
    /// client, its clones, and a [`WyrdState`](crate::state::WyrdState)'s pinned client
    /// on every clone of that state.
    #[must_use]
    pub fn shares_auth(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.auth, &other.auth)
    }

    /// Return the shared [`AuthMiddleware`] handle.
    ///
    /// Use this to obtain the current bearer for a transport the façade does
    /// not wrap, or to share one token path across hand-built clients.
    #[must_use]
    #[cfg(feature = "internal")]
    pub fn auth(&self) -> Arc<AuthMiddleware> {
        Arc::clone(&self.auth)
    }

    /// Return a current bearer for this client's credential.
    ///
    /// Hands the token to a third-party client, such as an OpenAI SDK pointed
    /// at the Gateway. The value comes from the shared [`AuthMiddleware`]: a
    /// fresh cached token is returned as-is, otherwise the middleware
    /// exchanges or renews it first. Nothing is cached here, so a caller that
    /// holds the token past its expiry calls this again for a fresh one. The
    /// returned [`SecretBearer`] redacts itself in `Debug`.
    ///
    /// # Errors
    /// Returns the server's stable error when it refuses the credential, or a
    /// transport error when `/auth/token` cannot be reached.
    pub async fn access_token(&self) -> Result<SecretBearer, WyrdError> {
        self.auth.bearer().await.map_err(AuthError::into_wyrd)
    }

    /// The effective HTTP server URL this client sends requests to.
    #[must_use]
    pub fn server_url(&self) -> &str {
        self.http.base_url()
    }

    /// The effective gRPC endpoint the gRPC plane dials: the explicit
    /// override when one was configured, else the server URL's host on the
    /// public gRPC port.
    #[must_use]
    pub fn grpc_url(&self) -> &str {
        &self.grpc_config.endpoint
    }

    /// Borrow the underlying [`HttpTransport`].
    #[must_use]
    #[cfg(feature = "internal")]
    pub fn http(&self) -> &HttpTransport {
        &self.http
    }

    /// Dial the gRPC endpoint, sharing this client's [`AuthMiddleware`].
    ///
    /// The connection is established on demand — the façade holds only the
    /// [`GrpcConfig`] until this is called. The returned
    /// [`GrpcConnection`][crate::transport::grpc::GrpcConnection] carries the
    /// same `Arc<AuthMiddleware>` as the HTTP plane, so both authenticate
    /// through one token cache.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::TransportDown`] when the endpoint URI is
    /// invalid or the dial fails on all attempts.
    #[cfg(feature = "internal")]
    pub async fn connect_grpc(
        &self,
    ) -> Result<crate::transport::grpc::GrpcConnection, WyrdClientError> {
        crate::transport::grpc::GrpcConnection::connect(&self.grpc_config, Arc::clone(&self.auth))
            .await
    }
}

/// Unit tests for the assembled client's token surface.
#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::WyrdClient;
    use crate::auth::AuthMiddleware;
    use crate::cards::Cards;
    use crate::config::{ClientConfig, TokenCacheMode};
    use crate::gateway_admin::Gateway;
    use crate::operator_connections::OperatorConnections;
    use crate::platform::Platform;
    use crate::principals::Principals;
    use crate::transport::credential::{BrowserSession, ResolvedCredential};
    use crate::transport::http::HttpTransport;

    /// Build a client against `server` presenting `credential`, with an
    /// in-memory token cache so no test touches a token file.
    ///
    /// # Panics
    /// Panics when the auth middleware or HTTP transport cannot be built for
    /// the mock server's URL.
    fn client_for(server: &MockServer, credential: ResolvedCredential) -> WyrdClient {
        let mut config = ClientConfig::default();
        config.http.base_url = server.uri();
        config.token_cache = TokenCacheMode::InMemory;
        let auth = AuthMiddleware::new(&config, credential).expect("middleware builds");
        let http = HttpTransport::new(&config.http, Arc::clone(&auth)).expect("transport builds");
        WyrdClient::from_parts(auth, http, config.grpc)
    }

    /// Every HTTP-only handle's `connect_with` completes the client's required
    /// exchange before returning and shares its one token cache: four handles
    /// over one API-key client cause exactly one `/auth/token` exchange. A
    /// presented-as-is bearer connects every handle with no request at all, a
    /// refused exchange fails `connect_with` with the server's stable error,
    /// and `Platform` keeps its own platform-session exchange.
    ///
    /// # Panics
    /// Panics when a handle fails to connect, a refused exchange connects, or
    /// the mock server sees a number of exchanges other than its expectation.
    #[tokio::test]
    async fn server_handles_connect_eagerly_and_share_the_client_auth_path() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/auth/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tenant-bearer",
                "token_type": "Bearer",
                "expires_in": 3600
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/auth/platform/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "platform-session",
                "token_type": "Bearer",
                "expires_in": 3600
            })))
            .expect(1)
            .mount(&server)
            .await;
        let api_key = client_for(
            &server,
            ResolvedCredential::ApiKey("wyrd_sk_t_v_s".to_owned().into()),
        );
        Cards::connect_with(&api_key).await.expect("cards connect");
        Gateway::connect_with(&api_key)
            .await
            .expect("gateway connects");
        Principals::connect_with(&api_key)
            .await
            .expect("principals connect");
        OperatorConnections::connect_with(&api_key)
            .await
            .expect("operator connections connect");

        let bearer = client_for(
            &server,
            ResolvedCredential::BearerToken("presented-as-is".to_owned().into()),
        );
        Cards::connect_with(&bearer).await.expect("cards connect");
        Gateway::connect_with(&bearer)
            .await
            .expect("gateway connects");
        Principals::connect_with(&bearer)
            .await
            .expect("principals connect");
        OperatorConnections::connect_with(&bearer)
            .await
            .expect("operator connections connect");
        Platform::connect_with(&bearer)
            .await
            .expect("platform exchanges its session");

        let refusing = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/auth/token"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "type": "about:blank",
                "title": "Unauthenticated",
                "status": 401,
                "code": "WYRD_AUTH_401_UNAUTHENTICATED",
                "detail": "credential refused"
            })))
            .mount(&refusing)
            .await;
        let refused = client_for(
            &refusing,
            ResolvedCredential::ApiKey("wyrd_sk_t_v_s".to_owned().into()),
        );
        let error = Principals::connect_with(&refused)
            .await
            .expect_err("a refused exchange fails at connect");
        assert_eq!(error.status(), 401, "{error:?}");
        assert!(
            !format!("{error:?}").contains("wyrd_sk_t_v_s"),
            "errors never carry the credential"
        );
    }

    /// `access_token` reads through the shared [`AuthMiddleware`]: an API key
    /// is exchanged at `/auth/token`, a token inside the refresh skew is
    /// re-exchanged on the next call rather than served from any client-side
    /// copy (the mock expects exactly two exchanges), and the bearer's `Debug`
    /// stays redacted.
    #[tokio::test]
    async fn access_token_uses_the_shared_refreshing_auth_path() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/auth/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "gateway-bearer",
                "token_type": "Bearer",
                "expires_in": 1
            })))
            .expect(2)
            .mount(&server)
            .await;
        let mut config = ClientConfig::default();
        config.http.base_url = server.uri();
        config.token_cache = TokenCacheMode::InMemory;
        let auth = AuthMiddleware::new(
            &config,
            ResolvedCredential::ApiKey("api-key-value".to_owned().into()),
        )
        .expect("middleware builds");
        let http = HttpTransport::new(&config.http, Arc::clone(&auth)).expect("transport builds");
        let client = WyrdClient::from_parts(auth, http, config.grpc);

        let first = client.access_token().await.expect("first exchange");
        let second = client.access_token().await.expect("stale token refreshes");

        assert_eq!(first.expose(), "gateway-bearer");
        assert_eq!(second.expose(), "gateway-bearer");
        assert!(
            !format!("{first:?}").contains("gateway-bearer"),
            "Debug must redact the bearer"
        );
    }

    /// A browser session's Bifrost handle is query-only: connecting dials no
    /// gRPC ingest channel, which would refuse a session that is not a
    /// bearer. Nothing listens on the derived gRPC port, so a dial would fail.
    ///
    /// # Panics
    /// Panics when the session or client is refused, or when connecting the
    /// Bifrost handle fails because it dialled the ingest channel.
    #[tokio::test]
    async fn browser_session_bifrost_connects_without_an_ingest_channel() {
        let session = BrowserSession::new(
            "wyrd_session_acme=opaque.1".to_owned().into(),
            "https://ui.example.com",
        )
        .expect("a well-formed session");
        let client =
            WyrdClient::for_browser_session("http://127.0.0.1:1", session).expect("client builds");
        crate::bifrost::Bifrost::connect_with(&client, crate::bifrost::BifrostOptions::default())
            .await
            .expect("a browser session connects without dialling ingest");
    }

    /// A browser-session client presents exactly the forwarded session cookie
    /// and its trusted public origin (never a bearer), connects a handle
    /// without any exchange, refuses to produce a bearer, and redacts the
    /// cookie in `Debug`. Malformed cookies and origins are refused before
    /// any request.
    ///
    /// # Panics
    /// Panics when the client sends a bearer, omits a session header, or
    /// accepts a malformed session.
    #[tokio::test]
    async fn browser_session_client_presents_only_the_forwarded_session() {
        use wiremock::matchers::header;

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/cards"))
            .and(header("cookie", "wyrd_session_acme=opaque.1"))
            .and(header("origin", "https://ui.example.com:8443"))
            .and(header("x-forwarded-host", "ui.example.com:8443"))
            .and(header("x-forwarded-proto", "https"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .expect(1)
            .mount(&server)
            .await;
        let session = BrowserSession::new(
            "wyrd_session_acme=opaque.1".to_owned().into(),
            "https://ui.example.com:8443",
        )
        .expect("a well-formed session");
        let client =
            WyrdClient::for_browser_session(&server.uri(), session).expect("client builds");
        Cards::connect_with(&client)
            .await
            .expect("a browser session connects without an exchange");
        let _: serde_json::Value = client
            .http
            .request_json::<(), _>(reqwest::Method::GET, "/v1/cards", None)
            .await
            .expect("the session reaches /v1");
        let requests = server.received_requests().await.expect("recorded requests");
        assert!(
            requests
                .iter()
                .all(|request| !request.headers.contains_key("x-wyrd-access-token")),
            "a browser session never sends a bearer"
        );
        assert!(
            client.access_token().await.is_err(),
            "a browser session has no bearer to hand out"
        );
        assert!(
            !format!("{client:?}").contains("opaque.1"),
            "Debug redacts the cookie"
        );

        for (cookie, origin) in [
            (
                "wyrd_session_acme=a; wyrd_session_beta=b",
                "https://ui.example.com",
            ),
            ("theme=dark", "https://ui.example.com"),
            ("wyrd_session_=a", "https://ui.example.com"),
            ("wyrd_session_acme=", "https://ui.example.com"),
            ("wyrd_session_acme=a", "https://ui.example.com/path"),
            ("wyrd_session_acme=a", "https://user@ui.example.com"),
            ("wyrd_session_acme=a", "ftp://ui.example.com"),
        ] {
            assert!(
                BrowserSession::new(cookie.to_owned().into(), origin).is_err(),
                "{cookie} at {origin} must be refused"
            );
        }
    }
}
