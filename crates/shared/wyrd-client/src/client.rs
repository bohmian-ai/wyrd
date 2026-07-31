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
//! [`WyrdClient::connect_grpc`], sharing the same [`AuthMiddleware`] so the HTTP
//! and gRPC planes authenticate through one token path.

use std::sync::Arc;

use serde::Serialize;
use serde::de::DeserializeOwned;
use wyrd_spec::error::WyrdError;

use crate::auth::AuthMiddleware;
use crate::config::ClientConfig;
use crate::error::WyrdClientError;
use crate::transport::config::GrpcConfig;
use crate::transport::http::{ArrowResponse, HttpTransport};

/// Assembled Wyrd client: a single handle over the resolved transport + auth
/// stack.
///
/// Construct once with [`WyrdClient::from_env`] (or
/// [`WyrdClient::with_config`]) and reuse it for every request. Cloning is
/// cheap — the underlying `reqwest::Client` and [`AuthMiddleware`] are shared
/// handles, so a clone reuses the same connection pool and token cache rather
/// than re-resolving credentials or redialing.
#[derive(Debug, Clone)]
pub struct WyrdClient {
    auth: Arc<AuthMiddleware>,
    http: HttpTransport,
    /// Held for lazy [`WyrdClient::connect_grpc`]; only the gRPC plane reads it.
    grpc_config: GrpcConfig,
}

impl WyrdClient {
    /// Build a client from environment variables.
    ///
    /// Resolves [`ClientConfig::from_env`] for endpoints, then the effective
    /// credential via [`ClientConfig::resolve_credential`], and assembles the
    /// auth and HTTP layers. No network call is made here; the first token
    /// exchange happens lazily on the first request.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::NoCredentials`] when no credential source is
    /// configured, or [`WyrdClientError::TransportDown`] when the HTTP client
    /// cannot be built.
    pub fn from_env() -> Result<Self, WyrdClientError> {
        Self::with_config(ClientConfig::from_env())
    }

    /// Build a client from an explicit [`ClientConfig`].
    ///
    /// Use this when endpoints or the API key are set programmatically rather
    /// than from the environment. Resolution and assembly are identical to
    /// [`WyrdClient::from_env`].
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
    /// service clients). The regular [`Self::from_env`] and
    /// [`Self::with_config`] paths remain the recommended constructors for
    /// production callers.
    #[must_use]
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

    /// Send a JSON request and decode the JSON response body.
    ///
    /// Delegates to [`HttpTransport::request_json`].
    ///
    /// # Errors
    /// Non-`2xx` server responses map to [`WyrdError`]; transport failures
    /// become [`WyrdError::Internal`].
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
    /// # Errors
    /// Non-`2xx` server responses map to [`WyrdError`]; transport failures
    /// become [`WyrdError::Internal`].
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
    /// # Errors
    /// Non-`2xx` server responses map to [`WyrdError`]; transport failures
    /// become [`WyrdError::Internal`].
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

    /// Send a cross-origin streaming request through the shared pool without
    /// Wyrd credentials.
    ///
    /// Delegates to [`HttpTransport::request_external_stream`]. Used by
    /// storage-client dispatch modules for presigned/SAS backend PUTs and
    /// GETs.
    ///
    /// # Errors
    /// Transport failures become [`WyrdError::Internal`].
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
    /// # Errors
    ///
    /// Returns a stable Wyrd error for serialization, authentication,
    /// transport, HTTP problem, or response media-type failures.
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

    /// Return the shared [`AuthMiddleware`] handle.
    ///
    /// Use this to obtain the current bearer for a transport the façade does
    /// not wrap, or to share one token path across hand-built clients.
    #[must_use]
    pub fn auth(&self) -> Arc<AuthMiddleware> {
        Arc::clone(&self.auth)
    }

    /// Borrow the underlying [`HttpTransport`].
    #[must_use]
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
    pub async fn connect_grpc(
        &self,
    ) -> Result<crate::transport::grpc::GrpcConnection, WyrdClientError> {
        crate::transport::grpc::GrpcConnection::connect(&self.grpc_config, Arc::clone(&self.auth))
            .await
    }
}
