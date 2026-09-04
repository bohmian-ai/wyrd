//! Wyrd credential decoration for the official `rmcp` Streamable HTTP client.
//!
//! Wyrd owns exactly one thing on the MCP client path: the per-request Wyrd
//! headers. Everything else — protocol framing, MCP headers, session handling,
//! SSE parsing, request cancellation — belongs to `rmcp` and its reqwest
//! transport, which this type decorates rather than replaces. There is no Wyrd
//! MCP client facade, handler, or lifecycle wrapper.

use std::collections::HashMap;
use std::io;
use std::sync::Arc;

use futures_util::stream::BoxStream;
use http::{HeaderName, HeaderValue};
use rmcp::model::ClientJsonRpcMessage;
use rmcp::transport::streamable_http_client::{
    SseError, StreamableHttpClient, StreamableHttpError, StreamableHttpPostResponse,
};
use sse_stream::Sse;
use wyrd_client::auth::AuthMiddleware;

/// Wyrd's bearer credential header. The application-owned `Authorization`
/// header is never written by this decorator.
const HEADER_WYRD_ACCESS_TOKEN: HeaderName = HeaderName::from_static("x-wyrd-access-token");

/// Wyrd's request-correlation header, propagated end to end.
const HEADER_WYRD_REQUEST_ID: HeaderName = HeaderName::from_static("wyrd-request-id");

/// An `rmcp` Streamable HTTP client that adds Wyrd's per-request headers.
///
/// One instance is handed to
/// [`StreamableHttpClientTransport::with_client`](rmcp::transport::StreamableHttpClientTransport::with_client);
/// the transport then drives it for every POST, SSE stream, and session delete.
#[derive(Clone)]
pub struct WyrdMcpHttpClient {
    /// The `rmcp`-implemented transport this type decorates.
    inner: reqwest::Client,
    /// Sole owner of Wyrd credentials and their proactive refresh.
    auth: Arc<AuthMiddleware>,
}

impl WyrdMcpHttpClient {
    /// Decorate `inner` with the credentials `auth` owns.
    #[must_use]
    pub fn new(inner: reqwest::Client, auth: Arc<AuthMiddleware>) -> Self {
        Self { inner, auth }
    }

    /// Add Wyrd's headers to the transport-supplied custom headers.
    ///
    /// The bearer comes from [`AuthMiddleware::bearer`] on every request, which
    /// already owns proactive refresh — so there is deliberately no reactive
    /// 401 parsing or retry here. `rmcp`'s reqwest transport erases the typed
    /// status and the Wyrd problem body for a pre-protocol 401, leaving nothing
    /// a retry could key on that `bearer()` has not already handled.
    ///
    /// A caller-supplied `wyrd-request-id` is preserved; otherwise one is
    /// minted through the same [`AuthMiddleware::request_id`] path the HTTP
    /// client uses. MCP's own headers are passed through untouched.
    ///
    /// # Errors
    ///
    /// Returns [`StreamableHttpError::Io`] wrapping the credential error when
    /// the middleware cannot produce a current bearer, and when either header
    /// value is not valid for HTTP transport.
    async fn wyrd_headers(
        &self,
        mut custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<HashMap<HeaderName, HeaderValue>, StreamableHttpError<reqwest::Error>> {
        let bearer = self
            .auth
            .bearer()
            .await
            .map_err(|error| StreamableHttpError::Io(io::Error::other(error)))?;
        let token = HeaderValue::from_str(&format!("Bearer {}", bearer.expose()))
            .map_err(|error| StreamableHttpError::Io(io::Error::other(error)))?;
        custom_headers.insert(HEADER_WYRD_ACCESS_TOKEN, token);

        if !custom_headers.contains_key(&HEADER_WYRD_REQUEST_ID) {
            let request_id = HeaderValue::from_str(&self.auth.request_id(None))
                .map_err(|error| StreamableHttpError::Io(io::Error::other(error)))?;
            custom_headers.insert(HEADER_WYRD_REQUEST_ID, request_id);
        }
        Ok(custom_headers)
    }
}

impl StreamableHttpClient for WyrdMcpHttpClient {
    type Error = reqwest::Error;

    async fn post_message(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session_id: Option<Arc<str>>,
        auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<StreamableHttpPostResponse, StreamableHttpError<Self::Error>> {
        let headers = self.wyrd_headers(custom_headers).await?;
        self.inner
            .post_message(uri, message, session_id, auth_header, headers)
            .await
    }

    async fn post_message_with_max_sse_event_size(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session_id: Option<Arc<str>>,
        auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
        max_sse_event_size: usize,
    ) -> Result<StreamableHttpPostResponse, StreamableHttpError<Self::Error>> {
        let headers = self.wyrd_headers(custom_headers).await?;
        self.inner
            .post_message_with_max_sse_event_size(
                uri,
                message,
                session_id,
                auth_header,
                headers,
                max_sse_event_size,
            )
            .await
    }

    async fn delete_session(
        &self,
        uri: Arc<str>,
        session_id: Arc<str>,
        auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<(), StreamableHttpError<Self::Error>> {
        let headers = self.wyrd_headers(custom_headers).await?;
        self.inner
            .delete_session(uri, session_id, auth_header, headers)
            .await
    }

    async fn get_stream(
        &self,
        uri: Arc<str>,
        session_id: Option<Arc<str>>,
        last_event_id: Option<String>,
        auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<BoxStream<'static, Result<Sse, SseError>>, StreamableHttpError<Self::Error>> {
        let headers = self.wyrd_headers(custom_headers).await?;
        self.inner
            .get_stream(uri, session_id, last_event_id, auth_header, headers)
            .await
    }

    async fn get_stream_with_max_sse_event_size(
        &self,
        uri: Arc<str>,
        session_id: Option<Arc<str>>,
        last_event_id: Option<String>,
        auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
        max_sse_event_size: usize,
    ) -> Result<BoxStream<'static, Result<Sse, SseError>>, StreamableHttpError<Self::Error>> {
        let headers = self.wyrd_headers(custom_headers).await?;
        self.inner
            .get_stream_with_max_sse_event_size(
                uri,
                session_id,
                last_event_id,
                auth_header,
                headers,
                max_sse_event_size,
            )
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
    use wyrd_client::auth::AuthMiddleware;
    use wyrd_client::config::{ClientConfig, TokenCacheMode};
    use wyrd_client::transport::credential::ResolvedCredential;

    use super::WyrdMcpHttpClient;

    /// A bounded local HTTP server that mints rotating access tokens and
    /// records every header of every MCP request it receives.
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
    /// `/auth/token` mints `access-1`, `access-2`, ... and always expires
    /// inside the middleware's proactive-refresh skew, so every `bearer()`
    /// re-exchanges and each request must observe a different current token.
    /// Every other path is treated as the MCP endpoint and answered `202
    /// Accepted`, which is what `rmcp`'s reqwest transport expects for a
    /// notification.
    async fn spawn_recorder() -> Recorder {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("recorder binds");
        let addr = listener.local_addr().expect("recorder address");
        let mcp_requests: Arc<Mutex<Vec<HashMap<String, String>>>> =
            Arc::new(Mutex::new(Vec::new()));
        let token_exchanges = Arc::new(AtomicUsize::new(0));
        let requests = Arc::clone(&mcp_requests);
        let exchanges = Arc::clone(&token_exchanges);
        let handle = tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let requests = Arc::clone(&requests);
                let exchanges = Arc::clone(&exchanges);
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
                            "expires_at": chrono::Utc::now() + chrono::Duration::seconds(5),
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
                        "HTTP/1.1 202 Accepted\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                            .to_owned()
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
        let recorder = spawn_recorder().await;
        let mut config = ClientConfig::default();
        config.http.base_url = recorder.base_url.clone();
        config.token_cache = TokenCacheMode::InMemory;
        let auth = AuthMiddleware::new(
            &config,
            ResolvedCredential::ApiKey("api-key-value".to_owned().into()),
        )
        .expect("auth middleware builds");
        let client = WyrdMcpHttpClient::new(reqwest::Client::new(), auth);
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
}
