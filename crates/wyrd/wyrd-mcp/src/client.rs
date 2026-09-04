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
