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
#[cfg(feature = "internal")]
use wyrd_spec::request_id::RequestId;

use crate::auth::{AuthError, AuthMiddleware};
use crate::config::ClientConfig;
use crate::error::WyrdClientError;
use crate::transport::config::GrpcConfig;
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
    /// Resolves [`ClientConfig::from_global`] for endpoints (the global config
    /// file, then environment variables, then defaults), then the effective
    /// credential via [`ClientConfig::resolve_credential`], and assembles the
    /// auth and HTTP layers. No network call is made here; the first token
    /// exchange happens lazily on the first request.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::Config`] when the global config file exists
    /// but cannot be read or parsed, [`WyrdClientError::NoCredentials`] when no
    /// credential source is configured, or [`WyrdClientError::TransportDown`]
    /// when the HTTP client cannot be built.
    pub fn from_global() -> Result<Self, WyrdClientError> {
        Self::with_config(ClientConfig::from_global()?)
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
    use crate::config::{ClientConfig, TokenCacheMode};
    use crate::transport::credential::ResolvedCredential;
    use crate::transport::http::HttpTransport;

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
}
