//! Wyrd credential decoration for the official `rmcp` Streamable HTTP client.
//!
//! Wyrd owns exactly one thing on the MCP client path: reading a status out of
//! an `rmcp` transport error, which is framing and therefore belongs here.
//! Everything else — the Wyrd headers, the bearer, and the bounded replay a
//! refusal buys — is decided by the shared client transport, so the MCP surface
//! cannot drift from the rest of Wyrd. Protocol framing, MCP headers, session
//! handling, SSE parsing, and request cancellation belong to `rmcp` and its
//! reqwest transport, which this type decorates rather than replaces. There is
//! no Wyrd MCP client facade, handler, or lifecycle wrapper.

use reqwest::Error as ReqwestError;
use std::collections::HashMap;
use std::future::Future;
use std::io;
use std::sync::Arc;

use futures_util::stream::BoxStream;
use http::{HeaderName, HeaderValue, StatusCode};
use rmcp::model::ClientJsonRpcMessage;
use rmcp::transport::streamable_http_client::{
    SseError, StreamableHttpClient, StreamableHttpError, StreamableHttpPostResponse,
};
use sse_stream::Sse;
use wyrd_client::WyrdClient;
use wyrd_client::transport::{AuthenticatedReplayError, HttpTransport};

/// Read the HTTP status out of a delegated `rmcp` transport failure.
///
/// This is framing, not policy: `rmcp` reports the same refusal in two shapes
/// depending on which delegated operation met it, and only this crate knows
/// that. What a given status means is the shared transport's decision.
///
/// - the SSE and session paths call `error_for_status`, so the typed status
///   survives on the transport's own error;
/// - the POST path re-renders any non-success response it cannot read as
///   JSON-RPC into `HTTP {status}: {body}` — Wyrd answers an unusable token
///   with an `application/problem+json` body, which is not `application/json`,
///   so it always takes that branch and the rendered status line is the only
///   signal left.
fn delegated_status(error: &StreamableHttpError<ReqwestError>) -> Option<StatusCode> {
    match error {
        StreamableHttpError::Client(client) => client.status(),
        // The rendering is `HTTP {status}: {body}`, and `StatusCode`'s own
        // Display writes both the code and its reason phrase, so the code is
        // the first whitespace-delimited token after the prefix.
        StreamableHttpError::UnexpectedServerResponse(message) => message
            .strip_prefix("HTTP ")
            .and_then(|rest| rest.split_whitespace().next())
            .and_then(|code| code.parse::<u16>().ok())
            .and_then(|code| StatusCode::from_u16(code).ok()),
        _ => None,
    }
}

/// An `rmcp` Streamable HTTP client that presents Wyrd's credential.
///
/// One instance is handed to
/// [`StreamableHttpClientTransport::with_client`](rmcp::transport::StreamableHttpClientTransport::with_client);
/// the transport then drives it for every POST, SSE stream, and session delete.
#[derive(Clone)]
pub struct WyrdMcpHttpClient {
    /// The configured Wyrd HTTP pool this type decorates, shared with every
    /// other capability of the originating [`WyrdClient`]. `rmcp` composes its
    /// own requests on this pool.
    inner: reqwest::Client,
    /// Sole owner of Wyrd's header vocabulary, its credentials, and the
    /// bounded replay an authentication refusal buys. Cloned from the
    /// originating client, so it shares that client's token cache.
    transport: HttpTransport,
}

impl WyrdMcpHttpClient {
    /// Decorate `client`'s HTTP pool with the credential policy `client` owns.
    ///
    /// Both halves come from the one configured capability so the MCP surface
    /// cannot drift from the rest of the client: no separate pool, TLS
    /// provider, header vocabulary, token cache, or renewal rule exists in this
    /// crate.
    #[must_use]
    pub fn new(client: &WyrdClient) -> Self {
        Self {
            inner: client.http().client(),
            transport: client.http().clone(),
        }
    }

    /// Run one delegated `rmcp` operation under the shared credential policy.
    ///
    /// The shared transport decorates the headers, decides what a refusal
    /// means, and owns the single replay; this only translates its error back
    /// into `rmcp`'s vocabulary.
    ///
    /// # Errors
    ///
    /// Returns [`StreamableHttpError::Io`] wrapping a credential or header
    /// failure, and otherwise whatever the delegated operation returned on its
    /// last attempt.
    async fn authenticated<T, F, O>(
        &self,
        custom_headers: HashMap<HeaderName, HeaderValue>,
        operation: F,
    ) -> Result<T, StreamableHttpError<reqwest::Error>>
    where
        F: Fn(HashMap<HeaderName, HeaderValue>) -> O,
        O: Future<Output = Result<T, StreamableHttpError<reqwest::Error>>>,
    {
        self.transport
            .authenticated_replay(custom_headers, delegated_status, operation)
            .await
            .map_err(|error| match error {
                AuthenticatedReplayError::Credential(error) => {
                    StreamableHttpError::Io(io::Error::other(error))
                }
                AuthenticatedReplayError::Operation(error) => error,
            })
    }
}

impl StreamableHttpClient for WyrdMcpHttpClient {
    /// Underlying HTTP transport failure; credential/header failures use the
    /// enclosing `StreamableHttpError::Io` variant.
    type Error = reqwest::Error;

    /// Refresh Wyrd headers before delegating MCP message framing to `rmcp`,
    /// re-exchanging and replaying once if the edge refuses the credential.
    ///
    /// # Errors
    /// Returns credential, invalid-header, or delegated HTTP/protocol errors.
    async fn post_message(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session_id: Option<Arc<str>>,
        auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<StreamableHttpPostResponse, StreamableHttpError<Self::Error>> {
        self.authenticated(custom_headers, |headers| {
            self.inner.post_message(
                Arc::clone(&uri),
                message.clone(),
                session_id.clone(),
                auth_header.clone(),
                headers,
            )
        })
        .await
    }

    /// Send a decorated message while preserving the transport's SSE size bound.
    ///
    /// # Errors
    /// Returns credential, invalid-header, HTTP/protocol, or SSE-limit errors.
    async fn post_message_with_max_sse_event_size(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session_id: Option<Arc<str>>,
        auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
        max_sse_event_size: usize,
    ) -> Result<StreamableHttpPostResponse, StreamableHttpError<Self::Error>> {
        self.authenticated(custom_headers, |headers| {
            self.inner.post_message_with_max_sse_event_size(
                Arc::clone(&uri),
                message.clone(),
                session_id.clone(),
                auth_header.clone(),
                headers,
                max_sse_event_size,
            )
        })
        .await
    }

    /// Authenticate transport-requested session deletion with current credentials.
    ///
    /// # Errors
    /// Returns credential, invalid-header, or delegated session-delete errors.
    async fn delete_session(
        &self,
        uri: Arc<str>,
        session_id: Arc<str>,
        auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<(), StreamableHttpError<Self::Error>> {
        self.authenticated(custom_headers, |headers| {
            self.inner.delete_session(
                Arc::clone(&uri),
                Arc::clone(&session_id),
                auth_header.clone(),
                headers,
            )
        })
        .await
    }

    /// Open the transport's resumable SSE stream with refreshed Wyrd headers.
    ///
    /// # Errors
    /// Returns credential, invalid-header, or stream-opening errors; later SSE
    /// errors remain items of the returned stream.
    async fn get_stream(
        &self,
        uri: Arc<str>,
        session_id: Option<Arc<str>>,
        last_event_id: Option<String>,
        auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<BoxStream<'static, Result<Sse, SseError>>, StreamableHttpError<Self::Error>> {
        self.authenticated(custom_headers, |headers| {
            self.inner.get_stream(
                Arc::clone(&uri),
                session_id.clone(),
                last_event_id.clone(),
                auth_header.clone(),
                headers,
            )
        })
        .await
    }

    /// Open a decorated SSE stream under the caller's event-size ceiling.
    ///
    /// # Errors
    /// Returns credential, invalid-header, or stream-opening errors. SSE decode
    /// and size-limit failures are yielded by the returned stream.
    async fn get_stream_with_max_sse_event_size(
        &self,
        uri: Arc<str>,
        session_id: Option<Arc<str>>,
        last_event_id: Option<String>,
        auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
        max_sse_event_size: usize,
    ) -> Result<BoxStream<'static, Result<Sse, SseError>>, StreamableHttpError<Self::Error>> {
        self.authenticated(custom_headers, |headers| {
            self.inner.get_stream_with_max_sse_event_size(
                Arc::clone(&uri),
                session_id.clone(),
                last_event_id.clone(),
                auth_header.clone(),
                headers,
                max_sse_event_size,
            )
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use http::{HeaderName, HeaderValue};
    use rmcp::model::ClientJsonRpcMessage;
    use rmcp::transport::streamable_http_client::{
        StreamableHttpClient, StreamableHttpPostResponse,
    };
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    use tokio::net::TcpListener;
    use wyrd_client::WyrdClient;
    use wyrd_client::auth::AuthMiddleware;
    use wyrd_client::config::{ClientConfig, TokenCacheMode};
    use wyrd_client::transport::credential::ResolvedCredential;
    use wyrd_client::transport::http::HttpTransport;

    use super::WyrdMcpHttpClient;

    /// A bounded local HTTP server that mints rotating access tokens and
    /// records every header of every MCP request it receives.
    ///
    /// Two knobs shape what the recorder proves: how long a minted token stays
    /// fresh — short enough and every `bearer()` re-exchanges, long enough and
    /// only a forced refresh does — and how many leading MCP requests are
    /// refused with `401`, which is the only way to drive the decorator's
    /// reactive path from in-process.
    struct Recorder {
        /// Base URL both the auth middleware and the MCP client target.
        base_url: String,
        /// Headers of each recorded MCP request, in arrival order.
        mcp_requests: Arc<Mutex<Vec<HashMap<String, String>>>>,
        /// Number of `/auth/token` exchanges served.
        token_exchanges: Arc<AtomicUsize>,
        /// Accept loop; aborted when the recorder drops.
        handle: tokio::task::JoinHandle<()>,
    }

    impl Drop for Recorder {
        fn drop(&mut self) {
            self.handle.abort();
        }
    }

    impl Recorder {
        /// Headers of the MCP request at `index`.
        ///
        /// # Panics
        ///
        /// Panics if fewer than `index + 1` MCP requests were recorded.
        fn mcp_request(&self, index: usize) -> HashMap<String, String> {
            self.mcp_requests
                .lock()
                .expect("recorder lock is healthy")
                .get(index)
                .cloned()
                .unwrap_or_else(|| panic!("MCP request {index} was recorded"))
        }

        /// Number of MCP requests recorded so far.
        ///
        /// # Panics
        ///
        /// Panics if the recorder mutex was poisoned.
        fn mcp_request_count(&self) -> usize {
            self.mcp_requests
                .lock()
                .expect("recorder lock is healthy")
                .len()
        }
    }

    /// Read one HTTP request head plus its declared body, returning the head.
    async fn read_request(stream: &mut tokio::net::TcpStream) -> String {
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 1024];
        loop {
            let head_end = buffer
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .map(|at| at + 4);
            if let Some(head_end) = head_end {
                let head = String::from_utf8_lossy(&buffer[..head_end]).into_owned();
                let length: usize = head
                    .lines()
                    .filter_map(|line| line.split_once(':'))
                    .find(|(name, _)| name.trim().eq_ignore_ascii_case("content-length"))
                    .and_then(|(_, value)| value.trim().parse().ok())
                    .unwrap_or(0);
                // Drain the body so the peer never sees a response before its
                // request was fully accepted.
                while buffer.len() < head_end + length {
                    match stream.read(&mut chunk).await {
                        Ok(0) | Err(_) => break,
                        Ok(read) => buffer.extend_from_slice(&chunk[..read]),
                    }
                }
                return head;
            }
            match stream.read(&mut chunk).await {
                Ok(0) | Err(_) => return String::from_utf8_lossy(&buffer).into_owned(),
                Ok(read) => buffer.extend_from_slice(&chunk[..read]),
            }
        }
    }

    /// Start a [`Recorder`] on an ephemeral loopback port.
    ///
    /// `/auth/token` mints `access-1`, `access-2`, ..., each valid for
    /// `token_ttl_seconds`: five seconds lands inside the middleware's
    /// proactive-refresh skew so every `bearer()` re-exchanges, while an hour
    /// leaves the cached token fresh so an exchange can only come from a
    /// forced refresh.
    ///
    /// Every other path is the MCP endpoint. The first `refusals` requests to
    /// it answer `401` with no `WWW-Authenticate` challenge — exactly the shape
    /// Wyrd's edge produces — and the rest answer `202 Accepted`, which is what
    /// `rmcp`'s reqwest transport expects for a notification. Every MCP
    /// request is recorded either way, so a refused attempt is still evidence.
    async fn spawn_recorder(token_ttl_seconds: i64, refusals: usize) -> Recorder {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("recorder binds");
        let addr = listener.local_addr().expect("recorder address");
        let mcp_requests: Arc<Mutex<Vec<HashMap<String, String>>>> =
            Arc::new(Mutex::new(Vec::new()));
        let token_exchanges = Arc::new(AtomicUsize::new(0));
        let requests = Arc::clone(&mcp_requests);
        let exchanges = Arc::clone(&token_exchanges);
        let refusals = Arc::new(AtomicUsize::new(refusals));
        let handle = tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let requests = Arc::clone(&requests);
                let exchanges = Arc::clone(&exchanges);
                let refusals = Arc::clone(&refusals);
                tokio::spawn(async move {
                    let head = read_request(&mut stream).await;
                    let mut lines = head.lines();
                    let request_line = lines.next().unwrap_or_default().to_owned();
                    let headers: HashMap<String, String> = lines
                        .filter_map(|line| line.split_once(':'))
                        .map(|(name, value)| {
                            (name.trim().to_ascii_lowercase(), value.trim().to_owned())
                        })
                        .collect();
                    let response = if request_line.contains("/auth/token") {
                        let nth = exchanges.fetch_add(1, Ordering::SeqCst) + 1;
                        let body = serde_json::json!({
                            "access_token": format!("access-{nth}"),
                            "token_type": "Bearer",
                            "expires_at": chrono::Utc::now()
                                + chrono::Duration::seconds(token_ttl_seconds),
                        })
                        .to_string();
                        format!(
                            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                            body.len(),
                            body,
                        )
                    } else {
                        requests
                            .lock()
                            .expect("recorder lock is healthy")
                            .push(headers);
                        let refuse = refusals
                            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
                                left.checked_sub(1)
                            })
                            .is_ok();
                        if refuse {
                            "HTTP/1.1 401 Unauthorized\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                                .to_owned()
                        } else {
                            "HTTP/1.1 202 Accepted\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                                .to_owned()
                        }
                    };
                    let _ = stream.write_all(response.as_bytes()).await;
                    let _ = stream.flush().await;
                });
            }
        });
        Recorder {
            base_url: format!("http://{addr}"),
            mcp_requests,
            token_exchanges,
            handle,
        }
    }

    /// Build the decorator over a [`WyrdClient`] configured against `recorder`.
    ///
    /// This is the production assembly path in miniature: one configured
    /// client owns the pool and the credential, and the MCP decorator is
    /// derived from it rather than built beside it.
    ///
    /// # Panics
    ///
    /// Panics when the auth middleware or HTTP transport cannot be built for
    /// the recorder's URL, which is a fault in the test itself.
    fn client_for(recorder: &Recorder) -> WyrdMcpHttpClient {
        let mut config = ClientConfig::default();
        config.http.base_url = recorder.base_url.clone();
        config.token_cache = TokenCacheMode::InMemory;
        let auth = AuthMiddleware::new(
            &config,
            ResolvedCredential::ApiKey("api-key-value".to_owned().into()),
        )
        .expect("auth middleware builds");
        let http =
            HttpTransport::new(&config.http, Arc::clone(&auth)).expect("HTTP transport builds");
        WyrdMcpHttpClient::new(&WyrdClient::from_parts(auth, http, config.grpc))
    }

    /// A minimal client-to-server MCP message that needs no session.
    fn notification() -> ClientJsonRpcMessage {
        serde_json::from_value(serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
        }))
        .expect("initialized notification parses")
    }

    /// Every MCP request carries the current Wyrd bearer and a request id, and
    /// reaches the underlying transport exactly once.
    ///
    /// The decorator's whole contract is per-request: it may not pin the first
    /// bearer it saw, may not clobber a caller-supplied correlation id, may not
    /// touch the application-owned `Authorization` header or any MCP header,
    /// and may not turn one MCP message into more than one HTTP request.
    #[tokio::test]
    async fn decorator_adds_current_wyrd_headers_once_per_request() {
        let recorder = spawn_recorder(5, 0).await;
        let client = client_for(&recorder);
        let uri: Arc<str> = Arc::from(format!("{}/mcp", recorder.base_url).as_str());

        let supplied_id = "11111111-2222-3333-4444-555555555555";
        let mut supplied = HashMap::new();
        supplied.insert(
            HeaderName::from_static("wyrd-request-id"),
            HeaderValue::from_static("11111111-2222-3333-4444-555555555555"),
        );
        let first = client
            .post_message(Arc::clone(&uri), notification(), None, None, supplied)
            .await
            .expect("first MCP request is accepted");
        let second = client
            .post_message(uri, notification(), None, None, HashMap::new())
            .await
            .expect("second MCP request is accepted");

        assert!(matches!(first, StreamableHttpPostResponse::Accepted));
        assert!(matches!(second, StreamableHttpPostResponse::Accepted));
        assert_eq!(
            recorder.mcp_request_count(),
            2,
            "each message is delegated to the transport exactly once"
        );
        assert_eq!(
            recorder.token_exchanges.load(Ordering::SeqCst),
            2,
            "the current bearer is obtained for every request"
        );

        let first = recorder.mcp_request(0);
        let second = recorder.mcp_request(1);
        assert_eq!(
            first.get("x-wyrd-access-token").map(String::as_str),
            Some("Bearer access-1")
        );
        assert_eq!(
            second.get("x-wyrd-access-token").map(String::as_str),
            Some("Bearer access-2"),
            "a rotated token must not be served from the first request's value"
        );
        assert!(
            !first.contains_key("authorization") && !second.contains_key("authorization"),
            "the application-owned Authorization header is never written"
        );
        assert_eq!(
            first.get("wyrd-request-id").map(String::as_str),
            Some(supplied_id),
            "a caller-supplied request id is preserved verbatim"
        );
        let minted = second
            .get("wyrd-request-id")
            .expect("a request id is minted when the caller supplied none");
        assert!(
            uuid::Uuid::parse_str(minted).is_ok() && minted != supplied_id,
            "a fresh request id is minted per request, got {minted}"
        );
        for request in [&first, &second] {
            assert_eq!(
                request.get("accept").map(String::as_str),
                Some("text/event-stream, application/json"),
                "rmcp's own MCP headers pass through untouched"
            );
            assert_eq!(
                request.get("content-type").map(String::as_str),
                Some("application/json")
            );
        }
    }

    /// One `401` costs exactly one re-exchange and exactly one replay.
    ///
    /// The minted token stays fresh for an hour, so the cache would serve the
    /// first bearer indefinitely: a second exchange can only come from the
    /// forced refresh the refusal triggers. The replayed request carries the
    /// newly exchanged token and the first attempt's correlation id, because
    /// both HTTP requests are one logical MCP message.
    #[tokio::test]
    async fn one_refusal_costs_one_re_exchange_and_one_replay() {
        let recorder = spawn_recorder(3600, 1).await;
        let client = client_for(&recorder);
        let uri: Arc<str> = Arc::from(format!("{}/mcp", recorder.base_url).as_str());

        let response = client
            .post_message(uri, notification(), None, None, HashMap::new())
            .await
            .expect("the replay after the re-exchange is accepted");

        assert!(matches!(response, StreamableHttpPostResponse::Accepted));
        assert_eq!(
            recorder.mcp_request_count(),
            2,
            "the refused attempt is replayed exactly once"
        );
        assert_eq!(
            recorder.token_exchanges.load(Ordering::SeqCst),
            2,
            "a fresh cached token is re-exchanged only because the server refused it"
        );
        let refused = recorder.mcp_request(0);
        let replayed = recorder.mcp_request(1);
        assert_eq!(
            refused.get("x-wyrd-access-token").map(String::as_str),
            Some("Bearer access-1")
        );
        assert_eq!(
            replayed.get("x-wyrd-access-token").map(String::as_str),
            Some("Bearer access-2"),
            "the replay carries the re-exchanged credential, not the refused one"
        );
        assert_eq!(
            refused.get("wyrd-request-id"),
            replayed.get("wyrd-request-id"),
            "both attempts join to the one logical MCP message"
        );
    }

    /// No Wyrd credential policy is written in this crate.
    ///
    /// The behavioral tests above pass whether the policy lives here or in the
    /// shared transport, which is exactly how a second copy came to exist. This
    /// is the part they cannot see: the header vocabulary, the bearer
    /// rendering, the credential handle, and the "what does a 401 buy" decision
    /// must have exactly one owner, and it is not this one. Reading a status
    /// out of an `rmcp` error stays here because it is framing.
    ///
    /// # Panics
    /// Panics when production code in this module names any of them again.
    #[test]
    fn no_wyrd_credential_policy_is_written_here() {
        let source = include_str!("client.rs");
        let production = source
            .split_once("#[cfg(test)]")
            .map_or(source, |(before, _)| before);

        for banned in [
            "x-wyrd-access-token",
            "wyrd-request-id",
            "Bearer",
            "AuthMiddleware",
            "force_refresh",
        ] {
            assert!(
                !production.contains(banned),
                "`{banned}` is the shared transport's to own, not this crate's"
            );
        }
    }

    /// A second refusal is terminal: no third attempt, no third exchange.
    ///
    /// This is the bound the whole policy rests on. A server that refuses
    /// every credential must produce one failure, not a re-exchange loop
    /// against its own token endpoint.
    #[tokio::test]
    async fn a_second_refusal_is_terminal() {
        let recorder = spawn_recorder(3600, usize::MAX).await;
        let client = client_for(&recorder);
        let uri: Arc<str> = Arc::from(format!("{}/mcp", recorder.base_url).as_str());

        let error = client
            .post_message(uri, notification(), None, None, HashMap::new())
            .await
            .expect_err("a server that refuses every credential fails the request");

        assert!(
            super::delegated_status(&error) == Some(http::StatusCode::UNAUTHORIZED),
            "the second refusal reaches the caller as the server's own rejection: {error}"
        );
        assert_eq!(
            recorder.mcp_request_count(),
            2,
            "the decorator attempts the request twice and then stops"
        );
        assert_eq!(
            recorder.token_exchanges.load(Ordering::SeqCst),
            2,
            "exactly one forced re-exchange follows the first refusal"
        );
    }
}
