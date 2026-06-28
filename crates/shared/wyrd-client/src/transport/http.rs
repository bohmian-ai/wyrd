//! Async `reqwest`-based HTTP transport.
//!
//! [`HttpTransport`] is the async HTTP client for read and admin paths. It
//! injects bearer auth, `wyrd-request-id`, and a retry policy with
//! exponential backoff. Three response-shaped helpers cover the three distinct
//! wire shapes:
//!
//! - [`HttpTransport::request_json`] — JSON in, JSON out.
//! - [`HttpTransport::request_arrow`] — JSON in, raw Arrow IPC bytes + metadata headers out.
//! - [`HttpTransport::submit_idempotent`] — JSON in, JSON out, one stable `Idempotency-Key`.
//!
//! Each helper mints a fresh origin `wyrd-request-id` per request: in v1 the
//! client is the request origin, so there is no inbound id to forward. The
//! forwarding seam already exists ([`AuthMiddleware::request_id`] accepts an
//! inbound id) for a future relay caller; the helpers pass `None` today.

#[cfg(feature = "transport-http")]
use std::sync::Arc;
#[cfg(feature = "transport-http")]
use std::time::Duration;

#[cfg(feature = "transport-http")]
use serde::Serialize;
#[cfg(feature = "transport-http")]
use serde::de::DeserializeOwned;
#[cfg(feature = "transport-http")]
use uuid::Uuid;
#[cfg(feature = "transport-http")]
use wyrd_spec::error::WyrdError;

#[cfg(feature = "transport-http")]
use crate::auth::{AuthError, AuthMiddleware};
#[cfg(feature = "transport-http")]
use crate::error::{WyrdClientError, from_problem_json};
#[cfg(feature = "transport-http")]
use crate::transport::config::HttpConfig;

/// Response from a raw Arrow IPC request.
#[cfg(feature = "transport-http")]
pub struct ArrowResponse {
    /// Raw Arrow IPC stream bytes.
    pub frames: Vec<u8>,
    /// Value of the `X-Wyrd-Schema-Fingerprint` response header, when present.
    pub schema_fingerprint: Option<String>,
    /// Value of the `X-Wyrd-Row-Count` response header, when present.
    pub row_count: Option<u64>,
}

#[cfg(feature = "transport-http")]
const HEADER_REQUEST_ID: &str = "wyrd-request-id";
#[cfg(feature = "transport-http")]
const HEADER_IDEMPOTENCY_KEY: &str = "Idempotency-Key";
#[cfg(feature = "transport-http")]
const HEADER_SCHEMA_FINGERPRINT: &str = "X-Wyrd-Schema-Fingerprint";
#[cfg(feature = "transport-http")]
const HEADER_ROW_COUNT: &str = "X-Wyrd-Row-Count";
/// Wyrd access-token header. The server authenticates data-plane requests from
/// this header only; the application's own `Authorization` header is reserved
/// for the embedding app and is never read or written by Wyrd.
#[cfg(feature = "transport-http")]
const HEADER_WYRD_ACCESS_TOKEN: &str = "x-wyrd-access-token";

/// Async `reqwest` HTTP transport for Wyrd read and admin paths.
///
/// Holds one [`reqwest::Client`] built from [`HttpConfig`] (timeout, optional
/// gzip) and a shared [`AuthMiddleware`] (D3: same `Arc` as gRPC). Each
/// request helper resolves the bearer via [`AuthMiddleware::bearer`], attaches
/// `wyrd-request-id`, and applies the retry policy before returning.
#[cfg(feature = "transport-http")]
#[derive(Clone)]
pub struct HttpTransport {
    client: reqwest::Client,
    auth: Arc<AuthMiddleware>,
    base_url: String,
}

#[cfg(feature = "transport-http")]
impl std::fmt::Debug for HttpTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpTransport")
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "transport-http")]
impl HttpTransport {
    /// Build a transport from config and a shared auth middleware.
    ///
    /// Sets the per-request timeout from `config.timeout_ms` and enables gzip
    /// decompression when `config.compression` is `true`.
    ///
    /// # Errors
    /// Returns [`WyrdClientError::TransportDown`] when the underlying
    /// `reqwest::Client` cannot be constructed.
    pub fn new(config: &HttpConfig, auth: Arc<AuthMiddleware>) -> Result<Self, WyrdClientError> {
        let mut builder =
            reqwest::Client::builder().timeout(Duration::from_millis(config.timeout_ms));
        if config.compression {
            builder = builder.gzip(true);
        }
        let client = builder
            .build()
            .map_err(|err| WyrdClientError::TransportDown {
                transport: "http".to_owned(),
                message: format!("failed to build HTTP client: {err}"),
            })?;
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
        let url = self.url(path);
        let request_id = self.auth.request_id(None);
        let body_bytes = serialize_body(body)?;

        let resp = self
            .send_with_retry(&method, &url, &request_id, body_bytes.as_deref(), None)
            .await?;

        let bytes = resp.bytes().await.map_err(body_read_err)?;
        serde_json::from_slice(&bytes).map_err(|err| WyrdError::Internal {
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
        let url = self.url(path);
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
        let url = self.url(path);
        let request_id = self.auth.request_id(None);
        let idempotency_key = Uuid::now_v7().to_string();
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
                Some(&idempotency_key),
            )
            .await?;

        let bytes = resp.bytes().await.map_err(body_read_err)?;
        serde_json::from_slice(&bytes).map_err(|err| WyrdError::Internal {
            message: format!("response deserialization failed: {err}"),
            details: serde_json::json!({}),
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.base_url, path.trim_start_matches('/'))
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

/// Serialize an optional body to JSON bytes.
#[cfg(feature = "transport-http")]
fn serialize_body<S: Serialize>(body: Option<&S>) -> Result<Option<Vec<u8>>, WyrdError> {
    body.map(serde_json::to_vec)
        .transpose()
        .map_err(|err| WyrdError::Internal {
            message: format!("request serialization failed: {err}"),
            details: serde_json::json!({}),
        })
}

/// Map a body-read error to [`WyrdError::Internal`].
#[cfg(feature = "transport-http")]
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
#[cfg(feature = "transport-http")]
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
#[cfg(feature = "transport-http")]
fn header_str<'a>(resp: &'a reqwest::Response, name: &str) -> Option<&'a str> {
    resp.headers().get(name)?.to_str().ok()
}
