//! Async `reqwest`-based HTTP transport.
//!
//! [`HttpTransport`] is the async HTTP client for read and admin paths. It
//! injects bearer auth, `wyrd-request-id`, the caller's W3C trace context
//! ([`crate::transport::trace`]), and a retry policy with
//! exponential backoff. Three response-shaped helpers cover the three distinct
//! wire shapes:
//!
//! - [`HttpTransport::request_json`] — JSON in, JSON out.
//! - [`HttpTransport::request_arrow`] — JSON in, raw Arrow IPC bytes + metadata headers out.
//! - [`HttpTransport::request_json_stream`] — JSON in, terminal-framed streaming bytes out.
//! - [`HttpTransport::submit_idempotent`] — JSON in, JSON out, one stable `Idempotency-Key`.
//!
//! Each helper mints a fresh origin `wyrd-request-id` per request: in v1 the
//! client is the request origin, so there is no inbound id to forward. The
//! forwarding seam already exists ([`AuthMiddleware::request_id`] accepts an
//! inbound id) for a future relay caller; the helpers pass `None` today.
//!
//! All authenticated helpers route through `HttpTransport::authenticated_url`,
//! which rejects absolute URLs that do not match the configured Wyrd origin.
//! This prevents `x-wyrd-access-token` and `wyrd-request-id` from being
//! attached to a third-party host. Cross-origin traffic (S3/GCS/Azure
//! presigned PUT/GET) must go through
//! [`HttpTransport::request_external_stream`], which never sends Wyrd auth
//! headers.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use reqwest::header::{HeaderName, HeaderValue};
use serde::Serialize;
use serde::de::DeserializeOwned;
use uuid::Uuid;
use wyrd_benchmark::Observation;
use wyrd_spec::auth::{SecretBearer, WORKLOAD_UID_HEADER};
use wyrd_spec::error::WyrdError;
use wyrd_spec::operator_connection::HttpsOrigin;
use wyrd_spec::request_id::RequestId;

use crate::auth::{AuthError, AuthMiddleware};
use crate::error::{WyrdClientError, from_problem_json};
use crate::transport::config::HttpConfig;
use crate::transport::credential::{BrowserSession, ResolvedCredential};
use crate::transport::trace;
use reqwest::Method;
use reqwest::StatusCode;
use reqwest::header::InvalidHeaderValue;

/// Response from a raw Arrow IPC request.
pub struct ArrowResponse {
    /// Raw Arrow IPC stream bytes.
    pub frames: Vec<u8>,
    /// Value of the `X-Wyrd-Schema-Fingerprint` response header, when present.
    pub schema_fingerprint: Option<String>,
    /// Value of the `X-Wyrd-Row-Count` response header, when present.
    pub row_count: Option<u64>,
}

const HEADER_REQUEST_ID: &str = "wyrd-request-id";
const HEADER_IDEMPOTENCY_KEY: &str = "Idempotency-Key";
const HEADER_SCHEMA_FINGERPRINT: &str = "X-Wyrd-Schema-Fingerprint";
const HEADER_ROW_COUNT: &str = "X-Wyrd-Row-Count";
const QUERY_STREAM_CONTENT_TYPE: &str = "application/vnd.wyrd.bifrost-query-stream";
/// Wyrd access-token header. The server authenticates data-plane requests from
/// this header only; the application's own `Authorization` header is reserved
/// for the embedding app and is never read or written by Wyrd.
const HEADER_WYRD_ACCESS_TOKEN: &str = "x-wyrd-access-token";

/// The benchmark root of one logical buffered call, from its body's
/// serialization through its answer's decoding, across every attempt and
/// credential refresh.
const HTTP_ROOT: &str = "client.http";

/// The benchmark root of one send attempt of a logical buffered call, from
/// the send to its response headers or failure.
const ATTEMPT_ROOT: &str = "client.http.attempt";

/// The [`HTTP_ROOT`] components: serializing the body, producing the
/// credential (refreshes included), sending to response headers, waiting
/// out retry backoff, receiving the body, and decoding it.
const SERIALIZE: &str = "serialize";
/// See [`SERIALIZE`].
const AUTH: &str = "auth";
/// See [`SERIALIZE`].
const SEND: &str = "send";
/// See [`SERIALIZE`].
const BACKOFF: &str = "backoff";
/// See [`SERIALIZE`].
const BODY: &str = "body";
/// See [`SERIALIZE`].
const DECODE: &str = "decode";

/// The [`HTTP_ROOT`] body whose size a received answer records.
const RESPONSE: &str = "response";

/// [`HEADER_WYRD_ACCESS_TOKEN`] as a typed header name.
///
/// The externally-framed path in [`HttpTransport::authenticated_replay`] hands
/// its caller a typed header map rather than a `reqwest::RequestBuilder`, so
/// the one header vocabulary has to be reachable in both forms.
///
/// # Panics
/// Never: the constant is a valid lowercase header name.
fn access_token_header() -> HeaderName {
    HeaderName::from_static(HEADER_WYRD_ACCESS_TOKEN)
}

/// One buffered request that every attempt of
/// [`HttpTransport::send_with_retry`] repeats unchanged.
struct Buffered<'a> {
    /// HTTP method; with the idempotency key, it decides replay safety.
    method: Method,
    /// Same-origin URL resolved by [`HttpTransport::authenticated_url`].
    url: &'a str,
    /// `wyrd-request-id` every attempt carries.
    request_id: &'a str,
    /// Serialized JSON body, owned once and shared by every attempt.
    body: Option<Bytes>,
    /// `Idempotency-Key` every attempt carries, when the request has one.
    idempotency_key: Option<&'a str>,
    /// Request-scoped headers applied verbatim on every attempt.
    extra_headers: &'a [(&'a str, &'a str)],
    /// Total deadline of each attempt.
    timeout: Duration,
}

/// What authenticates one HTTP request: a Wyrd bearer, or a browser session
/// presented by its cookie and trusted public origin.
enum RequestCredential {
    /// The middleware's current access token.
    Bearer(SecretBearer),
    /// A forwarded Rust-owned browser session.
    BrowserSession(BrowserSession),
}

/// [`HEADER_REQUEST_ID`] as a typed header name.
///
/// # Panics
/// Never: the constant is a valid lowercase header name.
fn request_id_header() -> HeaderName {
    HeaderName::from_static(HEADER_REQUEST_ID)
}

/// Why an externally-framed authenticated operation did not produce a value.
///
/// Splitting the credential half from the operation half is what lets a caller
/// that owns its own protocol — MCP over `rmcp`, for instance — map each back
/// into its own error vocabulary without inspecting Wyrd's.
#[derive(Debug, thiserror::Error)]
pub enum AuthenticatedReplayError<E> {
    /// A Wyrd credential could not be produced or rendered as a header.
    #[error("wyrd credential unavailable: {0}")]
    Credential(WyrdError),
    /// The operation itself failed, on its last attempt.
    #[error("authenticated operation failed")]
    Operation(E),
}

impl<E> AuthenticatedReplayError<E> {
    /// Report a value that cannot travel as an HTTP header.
    fn header(error: &InvalidHeaderValue) -> Self {
        Self::Credential(WyrdError::Internal {
            message: format!("wyrd header value is not valid for transport: {error}"),
            details: serde_json::json!({}),
        })
    }
}

/// Async `reqwest` HTTP transport for Wyrd read and admin paths.
///
/// Holds one connection-bounded [`reqwest::Client`] built from [`HttpConfig`]
/// plus a shared [`AuthMiddleware`] (D3: same `Arc` as gRPC). Retried JSON and
/// control requests add the configured total deadline per attempt. Streaming
/// transfers and terminal query streams bound only connection establishment,
/// leaving body lifetime to the server deadline and caller cancellation.
#[derive(Clone)]
pub struct HttpTransport {
    /// Shared pool whose builder bounds only connection establishment.
    client: reqwest::Client,
    /// Total request/response deadline applied to each `send_with_retry` attempt.
    request_timeout: Duration,
    auth: Arc<AuthMiddleware>,
    /// Deployment origin from [`HttpConfig::validate`]; every request path is
    /// joined to it, and only absolute URLs of this origin carry credentials.
    origin: HttpsOrigin,
}

impl std::fmt::Debug for HttpTransport {
    /// Prints only the normalized, userinfo-free deployment origin. The pool,
    /// the auth middleware, and the configured URL spelling are omitted, so no
    /// credential or credential-bearing URL reaches a log line through this
    /// value.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpTransport")
            .field("origin", &self.origin)
            .finish_non_exhaustive()
    }
}

impl HttpTransport {
    /// The deployment origin every request path is joined to,
    /// `scheme://host[:port]` without a trailing slash.
    #[must_use]
    pub fn base_url(&self) -> &str {
        self.origin.as_str()
    }

    /// Clone the shared reqwest client for a capability that must reuse this
    /// transport's connection and TLS pools.
    #[must_use]
    pub fn client(&self) -> reqwest::Client {
        self.client.clone()
    }
    /// Build a transport from config and a shared auth middleware.
    ///
    /// Validates `config` first: its [`HttpConfig::validate`] origin, the same
    /// one the token exchange uses, is the root every request path joins.
    /// Then builds one client whose connection establishment is bounded by
    /// `config.timeout_ms`; the same duration becomes the per-attempt total
    /// deadline of retried JSON and control requests.
    ///
    /// # Arguments
    /// * `config` - HTTP settings; `base_url` sets the request origin and `timeout_ms` the
    ///   connect and per-attempt deadline.
    /// * `auth` - Shared authentication middleware that supplies bearers and request ids.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::Config`] when `config` fails validation, and
    /// [`WyrdClientError::TransportDown`] when the underlying
    /// `reqwest::Client` cannot be constructed.
    pub fn new(config: &HttpConfig, auth: Arc<AuthMiddleware>) -> Result<Self, WyrdClientError> {
        let origin = config.validate()?;
        Ok(Self {
            client: build_http_client(config)?,
            request_timeout: Duration::from_millis(config.timeout_ms),
            auth,
            origin,
        })
    }

    /// The configured transport timeout, `HttpConfig::timeout_ms`.
    ///
    /// Retried requests use it as each attempt's total deadline. A terminal
    /// query stream uses it to bound only the connection close that must
    /// follow its terminal frame; the body before the terminal stays bounded
    /// by the server-pinned query deadline, never by this value.
    #[must_use]
    pub(crate) const fn request_timeout(&self) -> Duration {
        self.request_timeout
    }

    /// Rebind this transport to another auth path, sharing its connection pool.
    ///
    /// Used by [`WyrdClient::on_behalf_of`](crate::WyrdClient::on_behalf_of),
    /// whose delegated client reaches the same deployment under a different
    /// bearer.
    #[must_use]
    pub(crate) fn with_auth(&self, auth: Arc<AuthMiddleware>) -> Self {
        Self {
            client: self.client.clone(),
            request_timeout: self.request_timeout,
            auth,
            origin: self.origin.clone(),
        }
    }

    /// Run one externally-framed operation under Wyrd's credential policy.
    ///
    /// Some first-party surfaces do not compose their request through this
    /// transport — the MCP client hands its HTTP call to `rmcp`, which owns the
    /// framing, session, and SSE semantics — but they still speak to the same
    /// Wyrd edge and must present the same credential under the same rules.
    /// This is that policy, and the only copy of it: the Wyrd bearer and
    /// correlation headers are decorated onto `custom_headers`, the operation
    /// runs, and a refusal buys exactly one [`AuthMiddleware::force_refresh`]
    /// and exactly one replay before it is reported. Replaying is safe because
    /// Wyrd's edge rejects an unusable credential before any service acts on
    /// the request, so a refused first attempt applied nothing.
    ///
    /// The caller's W3C trace context is decorated alongside them, as on
    /// every authenticated request. The replay reuses the first attempt's
    /// header map — correlation id and trace context included — and swaps
    /// only the bearer, so both HTTP requests join to one
    /// logical message in the server's audit and traces.
    ///
    /// `status_of` is the caller's only contribution: reading a status out of
    /// its own error type is framing, which stays with the framing owner. What
    /// a `401` *means* is decided here.
    ///
    /// A caller-supplied `wyrd-request-id` is preserved; otherwise one is
    /// minted through the same path every other request uses. A
    /// caller-supplied `traceparent` likewise wins over the ambient trace
    /// context, with whatever `tracestate` the caller sent beside it. Headers the
    /// caller's own protocol needs are passed through untouched.
    ///
    /// # Arguments
    /// * `custom_headers` - Caller protocol headers; the bearer and a missing `wyrd-request-id`
    ///   are added.
    /// * `status_of` - Reads the HTTP status from the operation's error; `401` triggers the one
    ///   replay.
    /// * `operation` - Runs one attempt with the decorated headers; called at most twice.
    ///
    /// # Errors
    /// Returns [`AuthenticatedReplayError::Credential`] when the middleware
    /// cannot produce a bearer or either header value is not valid for HTTP
    /// transport, and [`AuthenticatedReplayError::Operation`] carrying whatever
    /// the operation returned on its last attempt.
    pub async fn authenticated_replay<T, E, F, O>(
        &self,
        mut custom_headers: HashMap<HeaderName, HeaderValue>,
        status_of: impl Fn(&E) -> Option<StatusCode>,
        operation: F,
    ) -> Result<T, AuthenticatedReplayError<E>>
    where
        F: Fn(HashMap<HeaderName, HeaderValue>) -> O,
        O: Future<Output = Result<T, E>>,
    {
        custom_headers.insert(access_token_header(), self.bearer_header(false).await?);
        if let Some(uid) = self.auth.workload_uid() {
            custom_headers.insert(
                HeaderName::from_static(WORKLOAD_UID_HEADER),
                HeaderValue::from_str(uid.as_str())
                    .map_err(|error| AuthenticatedReplayError::header(&error))?,
            );
        }
        if let Entry::Vacant(slot) = custom_headers.entry(request_id_header()) {
            slot.insert(
                HeaderValue::from_str(&self.auth.request_id(None))
                    .map_err(|error| AuthenticatedReplayError::header(&error))?,
            );
        }
        if !custom_headers.contains_key(&HeaderName::from_static(trace::TRACEPARENT)) {
            for (name, value) in trace::headers() {
                custom_headers.insert(
                    HeaderName::from_static(name),
                    HeaderValue::from_str(&value)
                        .map_err(|error| AuthenticatedReplayError::header(&error))?,
                );
            }
        }

        let first = operation(custom_headers.clone()).await;
        let refused = match &first {
            Err(error) => status_of(error) == Some(reqwest::StatusCode::UNAUTHORIZED),
            Ok(_) => false,
        };
        if !refused {
            return first.map_err(AuthenticatedReplayError::Operation);
        }

        custom_headers.insert(access_token_header(), self.bearer_header(true).await?);
        operation(custom_headers)
            .await
            .map_err(AuthenticatedReplayError::Operation)
    }

    /// Render the current Wyrd bearer as a header value.
    ///
    /// `force` selects the reactive path: [`AuthMiddleware::force_refresh`]
    /// re-exchanges a durable credential unconditionally, where
    /// [`AuthMiddleware::bearer`] serves the cached token until its own
    /// proactive skew expires it.
    ///
    /// # Errors
    /// Returns [`AuthenticatedReplayError::Credential`] when the middleware
    /// cannot produce a bearer or the rendered value is not valid for HTTP
    /// transport.
    async fn bearer_header<E>(
        &self,
        force: bool,
    ) -> Result<HeaderValue, AuthenticatedReplayError<E>> {
        let bearer = if force {
            self.auth.force_refresh().await
        } else {
            self.auth.bearer().await
        }
        .map_err(|error| AuthenticatedReplayError::Credential(AuthError::into_wyrd(error)))?;
        HeaderValue::from_str(&format!("Bearer {}", bearer.expose()))
            .map_err(|error| AuthenticatedReplayError::header(&error))
    }

    /// Send a JSON request and decode the JSON response body.
    ///
    /// Injects `x-wyrd-access-token: Bearer <token>` and a minted `wyrd-request-id`
    /// on every attempt. Retries on `408`, `429`, `5xx`, and connect/timeout
    /// errors (up to 3 attempts with exponential backoff 100 ms → 1 s). A
    /// single `401` triggers exactly one [`AuthMiddleware::force_refresh`] and
    /// one additional retry without consuming the normal retry budget.
    ///
    /// # Arguments
    /// * `method` - HTTP method of the request.
    /// * `path` - Server-relative request path (leading `/`), joined to the validated
    ///   deployment origin.
    /// * `body` - Request body serialized as JSON; `None` sends no body.
    ///
    /// # Errors
    /// Non-`2xx` server responses are mapped to [`WyrdError`] via
    /// `application/problem+json`. Transport failures become
    /// [`WyrdError::Internal`].
    pub async fn request_json<S, D>(
        &self,
        method: Method,
        path: &str,
        body: Option<&S>,
    ) -> Result<D, WyrdError>
    where
        S: Serialize,
        D: DeserializeOwned,
    {
        self.request_json_with_headers(method, path, body, &[])
            .await
    }

    /// Send a JSON request carrying additional request-scoped headers.
    ///
    /// Behaves exactly like [`Self::request_json`] — same authentication,
    /// request id, retry, and `application/problem+json` mapping — and applies
    /// `headers` verbatim on every attempt. It exists for a route whose
    /// authorization needs a second credential beside the caller's Wyrd token,
    /// such as a per-run lease.
    ///
    /// Deliberately crate-visible: header assembly is the transport's job, so a
    /// capability handle inside this crate owns the header name and value while
    /// no consumer outside it can start composing headers of its own.
    ///
    /// # Errors
    /// Non-`2xx` server responses are mapped to [`WyrdError`] via
    /// `application/problem+json`. Transport failures become
    /// [`WyrdError::Internal`].
    pub(crate) async fn request_json_with_headers<S, D>(
        &self,
        method: Method,
        path: &str,
        body: Option<&S>,
        headers: &[(&str, &str)],
    ) -> Result<D, WyrdError>
    where
        S: Serialize,
        D: DeserializeOwned,
    {
        self.request_json_within(method, path, body, headers, self.request_timeout)
            .await
    }

    /// Send a JSON request like [`Self::request_json`] whose attempts each
    /// wait at least `floor`.
    ///
    /// Used by a request whose server-side deadline exceeds the configured
    /// default, so the server's timeout answer reaches the caller. Only this
    /// request's deadline changes; the transport, its pool, and every other
    /// request keep the configured one.
    ///
    /// # Errors
    /// As [`Self::request_json`].
    pub(crate) async fn request_json_with_min_timeout<S, D>(
        &self,
        method: Method,
        path: &str,
        body: Option<&S>,
        floor: Duration,
    ) -> Result<D, WyrdError>
    where
        S: Serialize,
        D: DeserializeOwned,
    {
        self.request_json_within(method, path, body, &[], self.request_timeout.max(floor))
            .await
    }

    /// The tail [`Self::request_json_with_headers`] and
    /// [`Self::request_json_with_min_timeout`] share: send `body` with
    /// `headers` on every attempt, each bounded by `timeout`, and decode the
    /// answer.
    ///
    /// # Errors
    /// As [`Self::request_json`].
    async fn request_json_within<S, D>(
        &self,
        method: Method,
        path: &str,
        body: Option<&S>,
        headers: &[(&str, &str)],
        timeout: Duration,
    ) -> Result<D, WyrdError>
    where
        S: Serialize,
        D: DeserializeOwned,
    {
        let mut observation = Observation::start(HTTP_ROOT, trace::benchmark_cohort);
        let result = async {
            let url = self.authenticated_url(path)?;
            let request_id = self.auth.request_id(None);
            let started = body.and_then(|_| observation.now());
            let body = serialize_body(body)?;
            observation.since(SERIALIZE, started);
            let resp = self
                .send_with_retry(
                    Buffered {
                        method,
                        url: &url,
                        request_id: &request_id,
                        body,
                        idempotency_key: None,
                        extra_headers: headers,
                        timeout,
                    },
                    &mut observation,
                )
                .await?;

            let bytes = received(resp, &mut observation).await?;
            // A `2xx` with an empty body (e.g. `204 No Content` from an admin
            // DELETE) decodes as JSON `null`, so callers can request `D = ()` or
            // `D = serde_json::Value` for no-content routes without a second helper.
            let bytes: &[u8] = if bytes.is_empty() { b"null" } else { &bytes };
            decoded(bytes, &mut observation)
        }
        .await;
        observation.settle(&result);
        result
    }

    /// Send a request and return raw Arrow IPC bytes plus metadata headers.
    ///
    /// The optional request body is serialized as JSON. On `2xx` the raw
    /// response body is returned in [`ArrowResponse::frames`] without any JSON
    /// wrapper. On non-`2xx` the body is parsed as `application/problem+json`
    /// via the commit-03 mapper.
    ///
    /// # Arguments
    /// * `method` - HTTP method of the request.
    /// * `path` - Server-relative request path (leading `/`), joined to the validated
    ///   deployment origin.
    /// * `body` - Request body serialized as JSON; `None` sends no body.
    ///
    /// # Errors
    /// Non-`2xx` server responses are mapped to [`WyrdError`] via
    /// `application/problem+json`. Transport failures become
    /// [`WyrdError::Internal`].
    pub async fn request_arrow<S>(
        &self,
        method: Method,
        path: &str,
        body: Option<&S>,
    ) -> Result<ArrowResponse, WyrdError>
    where
        S: Serialize,
    {
        let mut observation = Observation::start(HTTP_ROOT, trace::benchmark_cohort);
        let result = async {
            let url = self.authenticated_url(path)?;
            let request_id = self.auth.request_id(None);
            let started = body.and_then(|_| observation.now());
            let body = serialize_body(body)?;
            observation.since(SERIALIZE, started);
            let resp = self
                .send_with_retry(
                    Buffered {
                        method,
                        url: &url,
                        request_id: &request_id,
                        body,
                        idempotency_key: None,
                        extra_headers: &[],
                        timeout: self.request_timeout,
                    },
                    &mut observation,
                )
                .await?;

            let schema_fingerprint =
                header_str(&resp, HEADER_SCHEMA_FINGERPRINT).map(str::to_owned);
            let row_count = header_str(&resp, HEADER_ROW_COUNT).and_then(|s| s.parse::<u64>().ok());

            let frames = received(resp, &mut observation).await?.to_vec();
            Ok(ArrowResponse {
                frames,
                schema_fingerprint,
                row_count,
            })
        }
        .await;
        observation.settle(&result);
        result
    }

    /// Send an authenticated request whose body is a one-shot stream.
    ///
    /// This capability is used by the storage client for LocalFs routes. The
    /// body is deliberately not retried because a streaming body cannot be
    /// replayed without re-opening its source. Only connection establishment
    /// is bounded by `HttpConfig::timeout_ms`; a healthy slow transfer is not
    /// cut off by a total deadline. Dropping the future abandons the transfer.
    ///
    /// # Arguments
    /// * `method` - HTTP method of the request.
    /// * `path` - Server-relative request path (leading `/`), joined to the validated
    ///   deployment origin.
    /// * `body` - One-shot streaming body; it is never retried.
    ///
    /// # Errors
    /// Returns a stable Wyrd error for URL, authentication, transport, or HTTP
    /// problem responses.
    pub async fn request_stream(
        &self,
        method: Method,
        path: &str,
        body: reqwest::Body,
    ) -> Result<reqwest::Response, WyrdError> {
        let url = self.authenticated_url(path)?;
        let credential = self.request_credential().await?;
        let request = self
            .authenticated(method, url, &credential, &self.auth.request_id(None))
            .body(body);
        let response = request.send().await.map_err(|err| WyrdError::Internal {
            message: format!("transport error: {err}"),
            details: serde_json::json!({"transport": "http"}),
        })?;
        if response.status().is_success() {
            Ok(response)
        } else {
            let body = response
                .json::<serde_json::Value>()
                .await
                .unwrap_or_else(|_| serde_json::json!({}));
            Err(from_problem_json(&body))
        }
    }

    /// Send a **cross-origin** streaming request through the shared client
    /// pool without Wyrd credentials.
    ///
    /// This is the storage-client seam: presigned/SAS backend PUTs and GETs
    /// (S3, GCS, Azure) must never carry `x-wyrd-access-token`, but they
    /// should reuse the same `reqwest::Client` connection pool as the
    /// authenticated Wyrd traffic. This helper serves those cross-origin
    /// calls: no `x-wyrd-access-token`, no `wyrd-request-id`, no retry
    /// (streaming bodies cannot be replayed), and caller-supplied headers
    /// (`Content-Range`, `Content-Type`, ETag validators, …) applied verbatim.
    /// Only connection establishment is bounded by `HttpConfig::timeout_ms`.
    ///
    /// # Arguments
    /// * `method` - HTTP method of the request.
    /// * `url` - Absolute cross-origin URL, such as a presigned backend URL.
    /// * `body` - Optional streaming body; `None` sends no body.
    /// * `headers` - Name/value pairs applied verbatim to the request.
    ///
    /// # Errors
    /// Transport failures become [`WyrdError::Internal`]. Non-`2xx`
    /// responses are returned untouched so callers can inspect response
    /// headers (S3 `ETag`, Azure block ack) before mapping to a
    /// `StorageClientError`.
    pub async fn request_external_stream(
        &self,
        method: Method,
        url: &str,
        body: Option<reqwest::Body>,
        headers: &[(&str, &str)],
    ) -> Result<reqwest::Response, WyrdError> {
        let mut request = self.client.request(method, url);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        if let Some(body) = body {
            request = request.body(body);
        }
        request.send().await.map_err(|err| WyrdError::Internal {
            message: format!("external transport error: {err}"),
            details: serde_json::json!({"transport": "http-external"}),
        })
    }

    /// Send an authenticated request and return its streaming response.
    ///
    /// Only connection establishment is bounded by `HttpConfig::timeout_ms`;
    /// the caller owns the unbuffered body's lifetime and may drop it to stop.
    ///
    /// # Arguments
    /// * `method` - HTTP method of the request.
    /// * `path` - Server-relative request path (leading `/`), joined to the validated
    ///   deployment origin.
    ///
    /// # Errors
    /// Returns a stable Wyrd error for URL, authentication, transport, or HTTP
    /// problem responses.
    pub async fn request_raw(
        &self,
        method: Method,
        path: &str,
    ) -> Result<reqwest::Response, WyrdError> {
        let url = self.authenticated_url(path)?;
        let credential = self.request_credential().await?;
        let response = self
            .authenticated(method, url, &credential, &self.auth.request_id(None))
            .send()
            .await
            .map_err(|err| WyrdError::Internal {
                message: format!("transport error: {err}"),
                details: serde_json::json!({"transport": "http"}),
            })?;
        if response.status().is_success() {
            Ok(response)
        } else {
            let body = response
                .json::<serde_json::Value>()
                .await
                .unwrap_or_else(|_| serde_json::json!({}));
            Err(from_problem_json(&body))
        }
    }

    /// Send an authenticated JSON request and return its response body as a
    /// stream. This is used by terminal-safe query clients so response bytes
    /// are decoded incrementally without buffering the result. Connection
    /// establishment is bounded by `HttpConfig::timeout_ms`; body lifetime is
    /// intentionally not subject to the ordinary total-request deadline.
    /// Cancelling this future abandons connection setup; dropping the returned
    /// response stops unbuffered body consumption.
    ///
    /// # Arguments
    /// * `method` - HTTP method of the request.
    /// * `path` - Server-relative request path (leading `/`), joined to the validated
    ///   deployment origin.
    /// * `body` - Request body, serialized as JSON.
    ///
    /// # Errors
    ///
    /// Returns a stable Wyrd error for request serialization, authentication,
    /// transport, HTTP problem responses, or an unexpected success media type.
    pub async fn request_json_stream<S>(
        &self,
        method: Method,
        path: &str,
        body: &S,
    ) -> Result<reqwest::Response, WyrdError>
    where
        S: Serialize,
    {
        let request_id = self.auth.request_id(None);
        self.request_json_stream_inner(method, path, body, &request_id, false)
            .await
    }

    /// Sends a streaming JSON request with one caller-owned request identity.
    ///
    /// The response remains unbuffered. Success is returned only when the
    /// server echoes the exact UUIDv7 request ID in its response headers.
    /// Cancelling this future abandons connection setup; dropping the returned
    /// response stops unbuffered body consumption.
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
    /// Returns a stable transport or protocol error, including a missing or
    /// mismatched response request ID.
    pub async fn request_json_stream_with_id<S>(
        &self,
        method: Method,
        path: &str,
        body: &S,
        request_id: &RequestId,
    ) -> Result<reqwest::Response, WyrdError>
    where
        S: Serialize,
    {
        self.request_json_stream_inner(method, path, body, request_id.as_str(), true)
            .await
    }

    /// Sends one streaming request and optionally enforces the caller-owned ID echo.
    ///
    /// Cancelling this future abandons connection setup; a successful response
    /// remains unbuffered and is owned by the caller.
    ///
    /// # Errors
    ///
    /// Returns stable URL, authentication, serialization, transport, HTTP,
    /// media-type, or request-identity errors.
    async fn request_json_stream_inner<S>(
        &self,
        method: Method,
        path: &str,
        body: &S,
        request_id: &str,
        verify_request_id: bool,
    ) -> Result<reqwest::Response, WyrdError>
    where
        S: Serialize,
    {
        let url = self.authenticated_url(path)?;
        let credential = self.request_credential().await?;
        let payload = serde_json::to_vec(body).map_err(|error| WyrdError::Internal {
            message: format!("request serialization failed: {error}"),
            details: serde_json::json!({}),
        })?;
        let response = self
            .authenticated(method, url, &credential, request_id)
            .header("content-type", "application/json")
            .header("accept", QUERY_STREAM_CONTENT_TYPE)
            .body(payload)
            .send()
            .await
            .map_err(|error| WyrdError::Internal {
                message: format!("transport error: {error}"),
                details: serde_json::json!({"transport": "http"}),
            })?;
        if response.status().is_success() {
            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.split(';').next())
                .map(str::trim);
            if content_type == Some(QUERY_STREAM_CONTENT_TYPE) {
                let echoed = header_str(&response, HEADER_REQUEST_ID);
                if !verify_request_id || echoed == Some(request_id) {
                    Ok(response)
                } else {
                    Err(WyrdError::UpstreamFailure {
                        message: "query response request identity did not match".to_owned(),
                        details: serde_json::json!({"reason": "request_id_mismatch"}),
                    })
                }
            } else {
                Err(WyrdError::UpstreamFailure {
                    message: "query response used an unsupported content type".to_owned(),
                    details: serde_json::json!({
                        "expected": QUERY_STREAM_CONTENT_TYPE,
                        "actual": content_type
                    }),
                })
            }
        } else {
            let body = response
                .json::<serde_json::Value>()
                .await
                .unwrap_or_else(|_| serde_json::json!({}));
            Err(from_problem_json(&body))
        }
    }

    /// Send a request with a stable UUIDv7 `Idempotency-Key`.
    ///
    /// The key is minted **once, outside** the retry loop and replayed on
    /// every attempt so a retry-after-enqueue returns the existing server-side
    /// job via `ON CONFLICT (idempotency_key)`, never a duplicate. (Minting
    /// inside the loop would defeat the conflict key.)
    ///
    /// # Arguments
    /// * `method` - HTTP method of the request.
    /// * `path` - Server-relative request path (leading `/`), joined to the validated
    ///   deployment origin.
    /// * `body` - Request body, serialized as JSON.
    ///
    /// # Errors
    /// Non-`2xx` server responses are mapped to [`WyrdError`] via
    /// `application/problem+json`. Transport failures become
    /// [`WyrdError::Internal`].
    pub async fn submit_idempotent<S, D>(
        &self,
        method: Method,
        path: &str,
        body: &S,
    ) -> Result<D, WyrdError>
    where
        S: Serialize,
        D: DeserializeOwned,
    {
        self.submit_with_optional_idempotency_key(method, path, body, None)
            .await
    }

    /// Send a JSON mutation with a caller-supplied stable `Idempotency-Key`.
    ///
    /// The key is passed unchanged to every retry attempt. This is intended
    /// for deterministic saga keys; callers that do not need deterministic
    /// replay should use [`Self::submit_idempotent`].
    ///
    /// # Arguments
    /// * `method` - HTTP method of the request.
    /// * `path` - Server-relative request path (leading `/`), joined to the validated
    ///   deployment origin.
    /// * `body` - Request body, serialized as JSON.
    /// * `key` - Idempotency key sent verbatim as `Idempotency-Key` on every attempt.
    ///
    /// # Errors
    /// Non-`2xx` server responses are mapped to [`WyrdError`] via `application/problem+json`.
    /// Serialization and transport failures become [`WyrdError::Internal`].
    pub async fn submit_with_idempotency_key<S, D>(
        &self,
        method: Method,
        path: &str,
        body: &S,
        key: &str,
    ) -> Result<D, WyrdError>
    where
        S: Serialize,
        D: DeserializeOwned,
    {
        self.submit_with_optional_idempotency_key(method, path, body, Some(key))
            .await
    }

    /// Send a JSON body to `path` and decode the response, attaching an
    /// idempotency key when one is supplied or generated.
    ///
    /// The single tail every write method shares, so authentication, request
    /// id, idempotency, and problem-json mapping are decided once.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when `path` is not a server-relative
    /// path, [`WyrdError::Internal`] when the body cannot be serialized or the
    /// response cannot be decoded, and the server's own mapped error when the
    /// request fails or answers non-2xx.
    async fn submit_with_optional_idempotency_key<S, D>(
        &self,
        method: Method,
        path: &str,
        body: &S,
        key: Option<&str>,
    ) -> Result<D, WyrdError>
    where
        S: Serialize,
        D: DeserializeOwned,
    {
        let mut observation = Observation::start(HTTP_ROOT, trace::benchmark_cohort);
        let result = async {
            let url = self.authenticated_url(path)?;
            let request_id = self.auth.request_id(None);
            let generated_key;
            let idempotency_key = if let Some(key) = key {
                key
            } else {
                generated_key = Uuid::now_v7().to_string();
                &generated_key
            };
            let started = observation.now();
            let body = serialize_body(Some(body))?;
            observation.since(SERIALIZE, started);

            let resp = self
                .send_with_retry(
                    Buffered {
                        method,
                        url: &url,
                        request_id: &request_id,
                        body,
                        idempotency_key: Some(idempotency_key),
                        extra_headers: &[],
                        timeout: self.request_timeout,
                    },
                    &mut observation,
                )
                .await?;

            let bytes = received(resp, &mut observation).await?;
            decoded(&bytes, &mut observation)
        }
        .await;
        observation.settle(&result);
        result
    }

    /// The credential that authenticates this transport's next request.
    ///
    /// A browser session is presented as itself; every other credential as
    /// the middleware's current bearer, exchanged or renewed as needed.
    ///
    /// # Errors
    /// Returns the middleware's stable error when no bearer can be produced.
    async fn request_credential(&self) -> Result<RequestCredential, WyrdError> {
        match self.auth.credential() {
            ResolvedCredential::BrowserSession(session) => {
                Ok(RequestCredential::BrowserSession(session.clone()))
            }
            _ => Ok(RequestCredential::Bearer(
                self.auth.bearer().await.map_err(AuthError::into_wyrd)?,
            )),
        }
    }

    /// Start a request to `url`, already resolved by
    /// [`Self::authenticated_url`], carrying `credential` (a bearer as
    /// `x-wyrd-access-token`, a browser session as its cookie and trusted
    /// public origin), `request_id` as `wyrd-request-id`, the
    /// middleware's workload root, when it has one, as [`WORKLOAD_UID_HEADER`],
    /// and the caller's trace context from [`trace::outbound`] as `traceparent` and,
    /// when non-empty, `tracestate`.
    ///
    /// Every authenticated helper starts its requests here, so each request,
    /// including each retry attempt, carries the same headers.
    fn authenticated(
        &self,
        method: Method,
        url: impl reqwest::IntoUrl,
        credential: &RequestCredential,
        request_id: &str,
    ) -> reqwest::RequestBuilder {
        let request = self
            .client
            .request(method, url)
            .header(HEADER_REQUEST_ID, request_id);
        let request = match self.auth.workload_uid() {
            Some(uid) => request.header(WORKLOAD_UID_HEADER, uid.as_str()),
            None => request,
        };
        let request = match credential {
            RequestCredential::Bearer(bearer) => request.header(
                HEADER_WYRD_ACCESS_TOKEN,
                format!("Bearer {}", bearer.expose()),
            ),
            RequestCredential::BrowserSession(session) => session
                .headers()
                .into_iter()
                .fold(request, |request, (name, value)| {
                    request.header(name, value)
                }),
        };
        trace::headers()
            .into_iter()
            .fold(request, |request, (name, value)| {
                request.header(name, value)
            })
    }

    /// Resolve a path or absolute URL to a same-origin URL suitable for
    /// authenticated requests.
    ///
    /// A relative path is joined to the configured origin. An absolute URL
    /// is accepted only when [`HttpsOrigin::of_url`] gives it the exact
    /// configured Wyrd origin; cross-origin and userinfo-carrying URLs are
    /// rejected so that `x-wyrd-access-token` and `wyrd-request-id` never
    /// travel to a third-party host (V-001).
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when the input is an absolute URL
    /// whose origin does not match the configured one.
    fn authenticated_url(&self, path: &str) -> Result<String, WyrdError> {
        if path.starts_with("http://") || path.starts_with("https://") {
            let origin = reqwest::Url::parse(path)
                .ok()
                .and_then(|url| HttpsOrigin::of_url(&url).ok());
            if origin.as_ref() == Some(&self.origin) {
                Ok(path.to_owned())
            } else {
                Err(WyrdError::Validation {
                    message: "authenticated request URL must match the configured Wyrd origin"
                        .to_owned(),
                    details: serde_json::json!({
                        "reason": "cross_origin_url_rejected",
                        "transport": "http",
                    }),
                })
            }
        } else {
            Ok(format!("{}/{}", self.origin, path.trim_start_matches('/')))
        }
    }

    /// One attempt of `request`: [`Self::authenticated`] headers carrying
    /// `credential`, the request's deadline, idempotency key and extra
    /// headers, and its JSON body.
    ///
    /// Every attempt sends the same serialized body backing; only the
    /// credential each attempt resolved differs.
    fn attempt(
        &self,
        request: &Buffered<'_>,
        credential: &RequestCredential,
    ) -> reqwest::RequestBuilder {
        let mut req = self
            .authenticated(
                request.method.clone(),
                request.url,
                credential,
                request.request_id,
            )
            .timeout(request.timeout);
        if let Some(key) = request.idempotency_key {
            req = req.header(HEADER_IDEMPOTENCY_KEY, key);
        }
        // Re-applied on every attempt, like the bearer: a retry is a new
        // request and must carry the same secondary credential.
        for (name, value) in request.extra_headers {
            req = req.header(*name, *value);
        }
        if let Some(bytes) = &request.body {
            req = req
                .header("content-type", "application/json")
                .body(bytes.clone());
        }
        req
    }

    /// Core send-with-retry loop shared by all three helpers.
    ///
    /// Each attempt carries the request's deadline, `HttpConfig::timeout_ms`
    /// unless a longer server-side deadline raised it, as its total deadline,
    /// covering connection, headers, and the caller's later body read, so JSON
    /// and control responses always finish or fail within a finite wait.
    ///
    /// The request's extra headers are request-scoped headers applied verbatim
    /// on every attempt, used for a secondary credential such as a per-run lease.
    ///
    /// Policy:
    /// - Fetches a fresh bearer before each attempt.
    /// - **Connect** errors (the request never reached the server) always
    ///   retry, up to 3 total attempts with exponential backoff (100 ms, 1 s).
    /// - **Timeout** and `408`/`429`/`5xx` retry only when the request is
    ///   *replay-safe* — an idempotent method (GET/PUT/DELETE/…) or one carrying
    ///   an `Idempotency-Key`. A non-idempotent `request_json` POST that timed
    ///   out or 5xx'd may already have been processed server-side, so replaying
    ///   it could double-execute the mutation; it surfaces the error instead.
    /// - On `401`: calls `force_refresh()` exactly once and retries once more,
    ///   not counted against the normal retry budget. Safe regardless of method
    ///   because a `401` is rejected before the server acts on the request.
    /// - On non-retryable non-`2xx`: reads the problem+json body and maps it
    ///   to a [`WyrdError`].
    ///
    /// Note on status divergence from the gRPC transport: an HTTP transport that cannot reach
    /// the server surfaces per-request as [`WyrdError::Internal`] (500), whereas
    /// the gRPC transport surfaces an unreachable server at connection-establish
    /// time as [`WyrdClientError::TransportDown`] (503). The two live on
    /// different API surfaces (per-request send vs. one-time `connect`); the
    /// divergence is intentional and documented in both modules.
    ///
    /// The logical call's `observation` gains each reached [`AUTH`],
    /// [`SEND`], [`BACKOFF`], and refused-[`BODY`] interval, and every send
    /// is observed as its own [`ATTEMPT_ROOT`]: successful when it answered
    /// `2xx`, an error when it failed or answered otherwise, so a refreshed
    /// `401` replay is two attempts of one call.
    ///
    /// # Errors
    /// Returns the credential failure, the last transport failure, or the
    /// server's mapped problem once the retry policy gives up.
    async fn send_with_retry(
        &self,
        request: Buffered<'_>,
        observation: &mut Observation,
    ) -> Result<reqwest::Response, WyrdError> {
        // A request is replay-safe when re-sending it cannot double-apply a
        // server-side effect: idempotent HTTP methods, or any request carrying
        // a stable Idempotency-Key the server dedupes on.
        let replay_safe = request.method.is_idempotent() || request.idempotency_key.is_some();
        let mut attempt = 0u32;
        let mut auth_retried = false;

        loop {
            let started = observation.now();
            let credential = self.request_credential().await?;
            observation.since(AUTH, started);
            let req = self.attempt(&request, &credential);

            let sent = observation.child(ATTEMPT_ROOT);
            let started = observation.now();
            let result = req.send().await;
            observation.since(SEND, started);
            sent.finish(match &result {
                Ok(resp) if resp.status().is_success() => wyrd_benchmark::SUCCESS,
                _ => wyrd_benchmark::ERROR,
            });

            match result {
                // A connect error means the request never reached the server, so
                // replaying it is always safe regardless of idempotency.
                Err(err) if err.is_connect() && attempt < 2 => {
                    backoff(attempt, observation).await;
                    attempt += 1;
                }
                // A timeout is ambiguous: the server may have processed the
                // request. Only retry when replay-safe.
                Err(err) if err.is_timeout() && replay_safe && attempt < 2 => {
                    backoff(attempt, observation).await;
                    attempt += 1;
                }
                Err(err) => {
                    return Err(WyrdError::Internal {
                        message: format!("transport error: {err}"),
                        details: serde_json::json!({"transport": "http"}),
                    });
                }
                Ok(resp) => {
                    let status = resp.status().as_u16();

                    if status == 401 && !auth_retried {
                        auth_retried = true;
                        let started = observation.now();
                        let _ = self.auth.force_refresh().await;
                        observation.since(AUTH, started);
                        continue;
                    }

                    if (status == 408 || status == 429 || status >= 500)
                        && attempt < 2
                        && replay_safe
                    {
                        backoff(attempt, observation).await;
                        attempt += 1;
                        continue;
                    }

                    if !resp.status().is_success() {
                        let started = observation.now();
                        let body_val = resp
                            .json::<serde_json::Value>()
                            .await
                            .unwrap_or(serde_json::json!({}));
                        observation.since(BODY, started);
                        return Err(from_problem_json(&body_val));
                    }

                    return Ok(resp);
                }
            }
        }
    }
}

/// Builds the shared connection-bounded Reqwest client after installing Wyrd's
/// process TLS provider.
///
/// The client carries no total deadline; `send_with_retry` applies one per
/// attempt so streaming helpers keep unbounded body lifetimes.
///
/// # Errors
///
/// Returns [`WyrdClientError::TransportDown`] when another Rustls provider
/// already owns the process or Reqwest rejects the client configuration.
fn build_http_client(config: &HttpConfig) -> Result<reqwest::Client, WyrdClientError> {
    wyrd_tls::install_crypto_provider().map_err(|error| WyrdClientError::TransportDown {
        transport: "http".to_owned(),
        message: error.to_string(),
    })?;
    let mut builder =
        reqwest::Client::builder().connect_timeout(Duration::from_millis(config.timeout_ms));
    if config.compression {
        builder = builder.gzip(true);
    }
    builder
        .build()
        .map_err(|err| WyrdClientError::TransportDown {
            transport: "http".to_owned(),
            message: format!("failed to build HTTP client: {err}"),
        })
}

/// Serialize an optional body to JSON bytes, owned once and shared by every
/// attempt.
fn serialize_body<S: Serialize>(body: Option<&S>) -> Result<Option<Bytes>, WyrdError> {
    body.map(|body| serde_json::to_vec(body).map(Bytes::from))
        .transpose()
        .map_err(|err| WyrdError::Internal {
            message: format!("request serialization failed: {err}"),
            details: serde_json::json!({}),
        })
}

/// Waits out retry `attempt`'s backoff, adding it to `observation`'s
/// [`BACKOFF`].
async fn backoff(attempt: u32, observation: &mut Observation) {
    let started = observation.now();
    tokio::time::sleep(Duration::from_millis(crate::transport::backoff_ms(attempt))).await;
    observation.since(BACKOFF, started);
}

/// Receives `resp`'s whole body, adding the wait to `observation`'s
/// [`BODY`] whether or not it completes and recording the size of a body
/// it read as the call's [`RESPONSE`] body.
///
/// # Errors
/// Returns the body-read failure as [`WyrdError::Internal`].
async fn received(
    resp: reqwest::Response,
    observation: &mut Observation,
) -> Result<Bytes, WyrdError> {
    let started = observation.now();
    let bytes = resp.bytes().await.map_err(body_read_err);
    observation.since(BODY, started);
    if let Ok(bytes) = &bytes {
        observation.bytes(RESPONSE, bytes.len());
    }
    bytes
}

/// Decodes a JSON answer from `bytes`, adding the work to `observation`'s
/// [`DECODE`] whether or not it succeeds.
///
/// # Errors
/// Returns the decoding failure as [`WyrdError::Internal`].
fn decoded<D: DeserializeOwned>(
    bytes: &[u8],
    observation: &mut Observation,
) -> Result<D, WyrdError> {
    let started = observation.now();
    let decoded = serde_json::from_slice(bytes).map_err(|err| WyrdError::Internal {
        message: format!("response deserialization failed: {err}"),
        details: serde_json::json!({}),
    });
    observation.since(DECODE, started);
    decoded
}

/// Map a body-read error to [`WyrdError::Internal`].
fn body_read_err(err: reqwest::Error) -> WyrdError {
    WyrdError::Internal {
        message: format!("transport error reading response body: {err}"),
        details: serde_json::json!({"transport": "http"}),
    }
}

/// Extract a response header value as a `&str`.
fn header_str<'a>(resp: &'a reqwest::Response, name: &str) -> Option<&'a str> {
    resp.headers().get(name)?.to_str().ok()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bytes::Bytes;
    use reqwest::Method;
    use wyrd_spec::auth::SecretBearer;

    use super::{Buffered, HttpTransport, RequestCredential};
    use crate::auth::AuthMiddleware;
    use crate::config::ClientConfig;
    use crate::transport::config::HttpConfig;
    use crate::transport::credential::ResolvedCredential;

    /// Every attempt of a buffered request sends the serialized body's own
    /// allocation, spare capacity included, with the same bytes and length:
    /// the initial attempt and each replay share it instead of copying it.
    ///
    /// # Panics
    /// Panics when an attempt's body is not the serialized backing.
    #[test]
    fn serialized_body_backing_is_shared_across_attempts() {
        let auth = AuthMiddleware::new(
            &ClientConfig::default(),
            ResolvedCredential::BearerToken("token".to_owned().into()),
        )
        .expect("auth builds");
        let transport = HttpTransport::new(
            &HttpConfig {
                base_url: "http://127.0.0.1:9".to_owned(),
                ..HttpConfig::default()
            },
            auth,
        )
        .expect("transport builds");
        let mut serialized = Vec::with_capacity(64);
        serialized.extend_from_slice(br#"{"key":"value"}"#);
        let backing = (serialized.as_ptr(), serialized.len());
        let request = Buffered {
            method: Method::POST,
            url: "http://127.0.0.1:9/v1/test",
            request_id: "request",
            body: Some(Bytes::from(serialized)),
            idempotency_key: Some("key"),
            extra_headers: &[],
            timeout: Duration::from_secs(70),
        };
        let credential = RequestCredential::Bearer(SecretBearer::new("token".to_owned()));

        for _ in 0..2 {
            let request = transport
                .attempt(&request, &credential)
                .build()
                .expect("attempt builds");
            let sent = request
                .body()
                .and_then(reqwest::Body::as_bytes)
                .expect("an in-memory body");
            assert_eq!((sent.as_ptr(), sent.len()), backing);
            assert_eq!(sent, br#"{"key":"value"}"#);
            assert_eq!(request.timeout(), Some(&Duration::from_secs(70)));
        }
    }
}

#[cfg(test)]
mod tls_tests {
    /// Reqwest builds an HTTPS request before any tonic/server initialization.
    #[test]
    fn https_client_initializes_provider_standalone() {
        let client = super::build_http_client(&crate::transport::config::HttpConfig::default())
            .expect("standalone HTTPS client builds");
        client
            .get("https://localhost/health")
            .build()
            .expect("HTTPS request builds");
        wyrd_tls::install_crypto_provider().expect("HTTP boundary retained AWS-LC ownership");
    }
}
