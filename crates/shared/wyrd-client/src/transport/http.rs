//! Async `reqwest`-based HTTP transport.
//!
//! [`HttpTransport`] is the async HTTP client for read and admin paths. It
//! injects bearer auth, `wyrd-request-id`, and a retry policy with
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
//! All authenticated helpers route through [`HttpTransport::authenticated_url`],
//! which rejects absolute URLs that do not match the configured Wyrd origin.
//! This prevents `x-wyrd-access-token` and `wyrd-request-id` from being
//! attached to a third-party host. Cross-origin traffic (S3/GCS/Azure
//! presigned PUT/GET) must go through
//! [`HttpTransport::request_external_stream`], which never sends Wyrd auth
//! headers.

use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use serde::de::DeserializeOwned;
use uuid::Uuid;
use wyrd_spec::error::WyrdError;

use crate::auth::{AuthError, AuthMiddleware};
use crate::error::{WyrdClientError, from_problem_json};
use crate::transport::config::HttpConfig;

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

/// Async `reqwest` HTTP transport for Wyrd read and admin paths.
///
/// Holds one [`reqwest::Client`] built from [`HttpConfig`] (timeout, optional
/// gzip) and a shared [`AuthMiddleware`] (D3: same `Arc` as gRPC). Each
/// request helper resolves the bearer via [`AuthMiddleware::bearer`], attaches
/// `wyrd-request-id`, and applies the retry policy before returning.
#[derive(Clone)]
pub struct HttpTransport {
    client: reqwest::Client,
    auth: Arc<AuthMiddleware>,
    base_url: String,
}

impl std::fmt::Debug for HttpTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpTransport")
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

impl HttpTransport {
    /// Clone the shared reqwest client for a capability that must reuse this
    /// transport's connection and TLS pools.
    #[must_use]
    pub fn client(&self) -> reqwest::Client {
        self.client.clone()
    }
    /// Build a transport from config and a shared auth middleware.
    ///
    /// Sets the per-request timeout from `config.timeout_ms` and enables gzip
    /// decompression when `config.compression` is `true`.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::TransportDown`] when the underlying
    /// `reqwest::Client` cannot be constructed.
    pub fn new(config: &HttpConfig, auth: Arc<AuthMiddleware>) -> Result<Self, WyrdClientError> {
        let client = build_http_client(config)?;
        Ok(Self {
            client,
            auth,
            base_url: config.base_url.trim_end_matches('/').to_owned(),
        })
    }

    /// Send a JSON request and decode the JSON response body.
    ///
    /// Injects `x-wyrd-access-token: Bearer <token>` and a minted `wyrd-request-id`
    /// on every attempt. Retries on `408`, `429`, `5xx`, and connect/timeout
    /// errors (up to 3 attempts with exponential backoff 100 ms → 1 s). A
    /// single `401` triggers exactly one [`AuthMiddleware::force_refresh`] and
    /// one additional retry without consuming the normal retry budget.
    ///
    /// # Errors
    /// Non-`2xx` server responses are mapped to [`WyrdError`] via
    /// `application/problem+json`. Transport failures become
    /// [`WyrdError::Internal`].
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
        let url = self.authenticated_url(path)?;
        let request_id = self.auth.request_id(None);
        let body_bytes = serialize_body(body)?;

        let resp = self
            .send_with_retry(&method, &url, &request_id, body_bytes.as_deref(), None)
            .await?;

        let bytes = resp.bytes().await.map_err(body_read_err)?;
        // A `2xx` with an empty body (e.g. `204 No Content` from an admin
        // DELETE) decodes as JSON `null`, so callers can request `D = ()` or
        // `D = serde_json::Value` for no-content routes without a second helper.
        let bytes: &[u8] = if bytes.is_empty() { b"null" } else { &bytes };
        serde_json::from_slice(bytes).map_err(|err| WyrdError::Internal {
            message: format!("response deserialization failed: {err}"),
            details: serde_json::json!({}),
        })
    }

    /// Send a request and return raw Arrow IPC bytes plus metadata headers.
    ///
    /// The optional request body is serialized as JSON. On `2xx` the raw
    /// response body is returned in [`ArrowResponse::frames`] without any JSON
    /// wrapper. On non-`2xx` the body is parsed as `application/problem+json`
    /// via the commit-03 mapper.
    ///
    /// # Errors
    /// Non-`2xx` server responses are mapped to [`WyrdError`] via
    /// `application/problem+json`. Transport failures become
    /// [`WyrdError::Internal`].
    pub async fn request_arrow<S>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&S>,
    ) -> Result<ArrowResponse, WyrdError>
    where
        S: Serialize,
    {
        let url = self.authenticated_url(path)?;
        let request_id = self.auth.request_id(None);
        let body_bytes = serialize_body(body)?;

        let resp = self
            .send_with_retry(&method, &url, &request_id, body_bytes.as_deref(), None)
            .await?;

        let schema_fingerprint = header_str(&resp, HEADER_SCHEMA_FINGERPRINT).map(str::to_owned);
        let row_count = header_str(&resp, HEADER_ROW_COUNT).and_then(|s| s.parse::<u64>().ok());

        let frames = resp.bytes().await.map_err(body_read_err)?.to_vec();
        Ok(ArrowResponse {
            frames,
            schema_fingerprint,
            row_count,
        })
    }

    /// Send an authenticated request whose body is a one-shot stream.
    ///
    /// This capability is used by the storage client for LocalFs routes. The
    /// body is deliberately not retried because a streaming body cannot be
    /// replayed without re-opening its source.
    pub async fn request_stream(
        &self,
        method: reqwest::Method,
        path: &str,
        body: reqwest::Body,
    ) -> Result<reqwest::Response, WyrdError> {
        let url = self.authenticated_url(path)?;
        let bearer = self.auth.bearer().await.map_err(auth_to_wyrd)?;
        let request = self
            .client
            .request(method, url)
            .header(
                HEADER_WYRD_ACCESS_TOKEN,
                format!("Bearer {}", bearer.expose()),
            )
            .header(HEADER_REQUEST_ID, self.auth.request_id(None))
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
    ///
    /// # Errors
    /// Transport failures become [`WyrdError::Internal`]. Non-`2xx`
    /// responses are returned untouched so callers can inspect response
    /// headers (S3 `ETag`, Azure block ack) before mapping to a
    /// `StorageClientError`.
    pub async fn request_external_stream(
        &self,
        method: reqwest::Method,
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
    pub async fn request_raw(
        &self,
        method: reqwest::Method,
        path: &str,
    ) -> Result<reqwest::Response, WyrdError> {
        let url = self.authenticated_url(path)?;
        let bearer = self.auth.bearer().await.map_err(auth_to_wyrd)?;
        let response = self
            .client
            .request(method, url)
            .header(
                HEADER_WYRD_ACCESS_TOKEN,
                format!("Bearer {}", bearer.expose()),
            )
            .header(HEADER_REQUEST_ID, self.auth.request_id(None))
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
    /// are decoded incrementally without buffering the result.
    ///
    /// # Errors
    ///
    /// Returns a stable Wyrd error for request serialization, authentication,
    /// transport, HTTP problem responses, or an unexpected success media type.
    pub async fn request_json_stream<S>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: &S,
    ) -> Result<reqwest::Response, WyrdError>
    where
        S: Serialize,
    {
        let url = self.authenticated_url(path)?;
        let bearer = self.auth.bearer().await.map_err(auth_to_wyrd)?;
        let payload = serde_json::to_vec(body).map_err(|error| WyrdError::Internal {
            message: format!("request serialization failed: {error}"),
            details: serde_json::json!({}),
        })?;
        let response = self
            .client
            .request(method, url)
            .header(
                HEADER_WYRD_ACCESS_TOKEN,
                format!("Bearer {}", bearer.expose()),
            )
            .header(HEADER_REQUEST_ID, self.auth.request_id(None))
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
                Ok(response)
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
    /// # Errors
    /// Non-`2xx` server responses are mapped to [`WyrdError`] via
    /// `application/problem+json`. Transport failures become
    /// [`WyrdError::Internal`].
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
        self.submit_with_optional_idempotency_key(method, path, body, None)
            .await
    }

    /// Send a JSON mutation with a caller-supplied stable `Idempotency-Key`.
    ///
    /// The key is passed unchanged to every retry attempt. This is intended
    /// for deterministic saga keys; callers that do not need deterministic
    /// replay should use [`Self::submit_idempotent`].
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
        self.submit_with_optional_idempotency_key(method, path, body, Some(key))
            .await
    }

    async fn submit_with_optional_idempotency_key<S, D>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: &S,
        key: Option<&str>,
    ) -> Result<D, WyrdError>
    where
        S: Serialize,
        D: DeserializeOwned,
    {
        let url = self.authenticated_url(path)?;
        let request_id = self.auth.request_id(None);
        let generated_key;
        let idempotency_key = match key {
            Some(key) => key,
            None => {
                generated_key = Uuid::now_v7().to_string();
                &generated_key
            }
        };
        let body_bytes = serde_json::to_vec(body).map_err(|err| WyrdError::Internal {
            message: format!("request serialization failed: {err}"),
            details: serde_json::json!({}),
        })?;

        let resp = self
            .send_with_retry(
                &method,
                &url,
                &request_id,
                Some(&body_bytes),
                Some(idempotency_key),
            )
            .await?;

        let bytes = resp.bytes().await.map_err(body_read_err)?;
        serde_json::from_slice(&bytes).map_err(|err| WyrdError::Internal {
            message: format!("response deserialization failed: {err}"),
            details: serde_json::json!({}),
        })
    }

    /// Resolve a path or absolute URL to a same-origin URL suitable for
    /// authenticated requests.
    ///
    /// A relative path is joined to the configured `base_url`. An absolute URL
    /// is accepted only when it targets the exact configured Wyrd origin;
    /// cross-origin URLs are rejected so that `x-wyrd-access-token` and
    /// `wyrd-request-id` never travel to a third-party host (V-001).
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when the input is an absolute URL
    /// whose scheme+authority does not match the configured base URL.
    fn authenticated_url(&self, path: &str) -> Result<String, WyrdError> {
        if path.starts_with("http://") || path.starts_with("https://") {
            if same_origin(&self.base_url, path) {
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
            Ok(format!(
                "{}/{}",
                self.base_url,
                path.trim_start_matches('/')
            ))
        }
    }

    /// Core send-with-retry loop shared by all three helpers.
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
    async fn send_with_retry(
        &self,
        method: &reqwest::Method,
        url: &str,
        request_id: &str,
        body: Option<&[u8]>,
        idempotency_key: Option<&str>,
    ) -> Result<reqwest::Response, WyrdError> {
        // A request is replay-safe when re-sending it cannot double-apply a
        // server-side effect: idempotent HTTP methods, or any request carrying
        // a stable Idempotency-Key the server dedupes on.
        let replay_safe = method.is_idempotent() || idempotency_key.is_some();
        let mut attempt = 0u32;
        let mut auth_retried = false;

        loop {
            let bearer = self.auth.bearer().await.map_err(auth_to_wyrd)?;

            let mut req = self
                .client
                .request(method.clone(), url)
                .header(
                    HEADER_WYRD_ACCESS_TOKEN,
                    format!("Bearer {}", bearer.expose()),
                )
                .header(HEADER_REQUEST_ID, request_id);

            if let Some(key) = idempotency_key {
                req = req.header(HEADER_IDEMPOTENCY_KEY, key);
            }

            if let Some(bytes) = body {
                req = req
                    .header("content-type", "application/json")
                    .body(bytes.to_vec());
            }

            let result = req.send().await;

            match result {
                // A connect error means the request never reached the server, so
                // replaying it is always safe regardless of idempotency.
                Err(err) if err.is_connect() && attempt < 2 => {
                    tokio::time::sleep(Duration::from_millis(crate::transport::backoff_ms(
                        attempt,
                    )))
                    .await;
                    attempt += 1;
                    continue;
                }
                // A timeout is ambiguous: the server may have processed the
                // request. Only retry when replay-safe.
                Err(err) if err.is_timeout() && replay_safe && attempt < 2 => {
                    tokio::time::sleep(Duration::from_millis(crate::transport::backoff_ms(
                        attempt,
                    )))
                    .await;
                    attempt += 1;
                    continue;
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
                        let _ = self.auth.force_refresh().await;
                        continue;
                    }

                    if (status == 408 || status == 429 || status >= 500)
                        && attempt < 2
                        && replay_safe
                    {
                        tokio::time::sleep(Duration::from_millis(crate::transport::backoff_ms(
                            attempt,
                        )))
                        .await;
                        attempt += 1;
                        continue;
                    }

                    if !resp.status().is_success() {
                        let body_val = resp
                            .json::<serde_json::Value>()
                            .await
                            .unwrap_or(serde_json::json!({}));
                        return Err(from_problem_json(&body_val));
                    }

                    return Ok(resp);
                }
            }
        }
    }
}

/// Builds the shared Reqwest client after installing Wyrd's process TLS provider.
///
/// # Errors
///
/// Returns [`WyrdClientError::TransportDown`] when another Rustls provider
/// already owns the process or Reqwest rejects the client configuration.
///
fn build_http_client(config: &HttpConfig) -> Result<reqwest::Client, WyrdClientError> {
    wyrd_tls::install_crypto_provider().map_err(|error| WyrdClientError::TransportDown {
        transport: "http".to_owned(),
        message: error.to_string(),
    })?;
    let mut builder = reqwest::Client::builder().timeout(Duration::from_millis(config.timeout_ms));
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

/// Serialize an optional body to JSON bytes.
fn serialize_body<S: Serialize>(body: Option<&S>) -> Result<Option<Vec<u8>>, WyrdError> {
    body.map(serde_json::to_vec)
        .transpose()
        .map_err(|err| WyrdError::Internal {
            message: format!("request serialization failed: {err}"),
            details: serde_json::json!({}),
        })
}

/// Map a body-read error to [`WyrdError::Internal`].
fn body_read_err(err: reqwest::Error) -> WyrdError {
    WyrdError::Internal {
        message: format!("transport error reading response body: {err}"),
        details: serde_json::json!({"transport": "http"}),
    }
}

/// Convert an [`AuthError`] to a [`WyrdError`].
///
/// Server-reported auth failures pass through; client-local auth failures
/// (transport down during token exchange) become [`WyrdError::Internal`].
fn auth_to_wyrd(err: AuthError) -> WyrdError {
    match err {
        AuthError::Server(wyrd) => wyrd,
        AuthError::Client(client_err) => WyrdError::Internal {
            message: format!("auth error: {client_err}"),
            details: serde_json::json!({}),
        },
    }
}

/// Extract a response header value as a `&str`.
fn header_str<'a>(resp: &'a reqwest::Response, name: &str) -> Option<&'a str> {
    resp.headers().get(name)?.to_str().ok()
}

/// Return `true` when `candidate` targets the same scheme+authority as
/// `configured_origin`.
///
/// Compares the scheme (`http`/`https`) and authority (host and optional
/// port) portions and requires them to be byte-for-byte identical. Paths are
/// intentionally ignored — the caller may reach any route under the origin,
/// but never a different host.
fn same_origin(configured_origin: &str, candidate: &str) -> bool {
    match (split_origin(configured_origin), split_origin(candidate)) {
        (Some(base), Some(cand)) => base == cand,
        _ => false,
    }
}

/// Extract the `(scheme, authority)` prefix of an absolute URL, or `None` if
/// the input is not a recognizable absolute URL.
fn split_origin(url: &str) -> Option<(&str, &str)> {
    let (scheme, rest) = if let Some(rest) = url.strip_prefix("https://") {
        ("https", rest)
    } else if let Some(rest) = url.strip_prefix("http://") {
        ("http", rest)
    } else {
        return None;
    };
    let authority = rest.split('/').next().unwrap_or("");
    if authority.is_empty() {
        return None;
    }
    Some((scheme, authority))
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
