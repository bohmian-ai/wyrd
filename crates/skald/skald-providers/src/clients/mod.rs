//! Concrete provider clients.

pub mod anthropic;
pub mod embeddings;
pub mod google;
pub mod jev;
pub mod openai;
pub mod vertex;

pub use anthropic::AnthropicClient;
pub use google::GoogleClient;
pub use jev::JevClient;
pub use openai::{
    MediaAnswer, OpenAiBatchRoute, OpenAiClient, OpenAiMediaRoute, OpenAiRoute, UploadContent,
    UploadFile,
};
pub use vertex::VertexClient;

use std::time::SystemTime;

use reqwest::Method;
use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE, HeaderMap, HeaderValue, RETRY_AFTER};

use crate::error::{ProviderError, ProviderResult};
use crate::exchange::ExchangeTimer;
use crate::retry::{RetryPolicy, parse_retry_after};
use crate::transport::HttpTransport;

/// Sends a JSON request body with retry and returns the response body text.
///
/// Every answer body is read through the transport's `max_response_bytes`
/// bound before its status is inspected, so neither a success nor a refusal
/// can buffer an unbounded body.
///
/// `body` is owned by the request once: every exchange, retries included,
/// sends the same immutable backing rather than a copy of it. A valid UTF-8
/// answer becomes the returned text without copying.
///
/// Each HTTP exchange, retries included, is logged in the caller's
/// [`crate::exchange::scope`] as ended once its body is read, before decode.
///
/// # Errors
///
/// Returns the mapped transport error, [`ProviderError::Upstream`] when an
/// answer exceeds the body bound, or the provider status error of the final
/// non-success answer after `retry` is exhausted.
pub async fn send_bytes_with_retry(
    transport: &HttpTransport,
    provider: &str,
    method: Method,
    url: &str,
    mut headers: HeaderMap,
    body: Vec<u8>,
    retry: &RetryPolicy,
) -> ProviderResult<String> {
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    let request = transport
        .client()
        .request(method, url)
        .headers(headers)
        .body(body)
        .build()
        .map_err(|error| map_reqwest_error(provider, error))?;
    let mut attempt = 0;
    loop {
        let mut exchange = ExchangeTimer::start();
        let response = transport
            .client()
            .execute(attempt_request(&request))
            .await
            .map_err(|error| map_reqwest_error(provider, error))?;
        exchange.headers();

        let status = response.status();
        let retry_after = retry_after_ms(&response);
        let bytes =
            read_bounded_bytes(provider, response, transport.config().max_response_bytes).await?;
        drop(exchange);
        let text = answer_text(bytes);

        if status.is_success() {
            return Ok(text);
        }
        if !retry.should_retry(attempt, status) {
            return Err(ProviderError::from_status(
                provider,
                status,
                text,
                retry_after,
            ));
        }

        attempt += 1;
        let header_value = retry_after_header_value(retry_after);
        let wait = retry.wait_for_attempt(attempt, header_value.as_deref(), SystemTime::now());
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
    }
}

/// Successful answer of a streaming provider request, read as it arrives.
///
/// Dropping it closes the connection, which aborts the upstream response.
#[derive(Debug)]
pub struct ProviderByteStream {
    /// Provider named in read errors.
    provider: String,
    /// Successful answer whose body has not been read yet.
    response: reqwest::Response,
}

impl ProviderByteStream {
    /// The answer's `content-type`, or `application/octet-stream`.
    #[must_use]
    pub fn content_type(&self) -> &str {
        answer_type(&self.response)
    }

    /// Waits for the next body chunk and returns its bytes unchanged, or
    /// `None` once the body ends.
    ///
    /// # Errors
    ///
    /// Returns the mapped transport error when the exchange breaks or times
    /// out mid-body.
    pub async fn chunk(&mut self) -> ProviderResult<Option<Vec<u8>>> {
        self.response
            .chunk()
            .await
            .map(|chunk| chunk.map(|bytes| bytes.to_vec()))
            .map_err(|error| map_reqwest_error(&self.provider, error))
    }
}

/// Posts a JSON `body` once and returns the successful answer for incremental
/// reading.
///
/// Streaming requests never retry, because output may already have been
/// billed. A non-success answer is read through the transport's body bound and
/// returned as its status error, like a buffered request. A successful
/// exchange is logged in the caller's [`crate::exchange::scope`] as open,
/// since its body is read incrementally; a refusal is logged as ended.
///
/// # Errors
///
/// Returns the mapped transport error, [`ProviderError::Upstream`] when a
/// refusal body exceeds the bound, or [`ProviderError::Status`] for a
/// non-success answer.
pub async fn open_stream(
    transport: &HttpTransport,
    provider: &str,
    url: &str,
    mut headers: HeaderMap,
    body: Vec<u8>,
) -> ProviderResult<ProviderByteStream> {
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    let mut exchange = ExchangeTimer::start();
    let response = transport
        .client()
        .post(url)
        .headers(headers)
        .body(body)
        .send()
        .await
        .map_err(|error| map_reqwest_error(provider, error))?;
    exchange.headers();
    let status = response.status();
    if !status.is_success() {
        let retry_after = retry_after_ms(&response);
        let text = read_bounded(provider, response, transport.config().max_response_bytes).await?;
        return Err(ProviderError::from_status(
            provider,
            status,
            text,
            retry_after,
        ));
    }
    exchange.open();
    Ok(ProviderByteStream {
        provider: provider.to_owned(),
        response,
    })
}

/// Serializes a native request body, sends it, and decodes the native response.
pub async fn send_json_with_retry<T, R>(
    transport: &HttpTransport,
    provider: &str,
    url: &str,
    headers: HeaderMap,
    body: &T,
    retry: &RetryPolicy,
) -> ProviderResult<R>
where
    T: serde::Serialize + ?Sized,
    R: serde::de::DeserializeOwned,
{
    let body = serde_json::to_vec(body).map_err(|error| ProviderError::decode(provider, error))?;
    let text =
        send_bytes_with_retry(transport, provider, Method::POST, url, headers, body, retry).await?;
    serde_json::from_str(&text).map_err(|error| ProviderError::decode(provider, error))
}

/// Returns a stable provider request variant label for mismatch errors.
pub fn request_variant_label(request: &skald_spec::ProviderRequest) -> &'static str {
    match request {
        skald_spec::ProviderRequest::OpenAiChatCompletion(_) => "openai_chat_completion",
        skald_spec::ProviderRequest::OpenAiResponses(_) => "openai_responses",
        skald_spec::ProviderRequest::OpenAiEmbeddings(_) => "openai_embeddings",
        skald_spec::ProviderRequest::AnthropicMessage(_) => "anthropic_message",
        skald_spec::ProviderRequest::GeminiGenerateContent(_) => "gemini_generate_content",
        skald_spec::ProviderRequest::GoogleBatchEmbed(_) => "google_batch_embed",
        skald_spec::ProviderRequest::VertexPredict(_) => "vertex_predict",
        skald_spec::ProviderRequest::JevSystemOne(_) => "jev_system_one",
        skald_spec::ProviderRequest::RawV1 { .. } => "raw_v1",
        _ => "unknown",
    }
}

/// Reads an answer body as text, failing once it exceeds `limit` bytes.
///
/// Invalid UTF-8 is replaced, as `reqwest::Response::text` does.
///
/// # Errors
///
/// Returns the errors of [`read_bounded_bytes`].
async fn read_bounded(
    provider: &str,
    response: reqwest::Response,
    limit: usize,
) -> ProviderResult<String> {
    let bytes = read_bounded_bytes(provider, response, limit).await?;
    Ok(answer_text(bytes))
}

/// Another handle on a buffered `request` for one exchange, sharing its
/// immutable body backing.
///
/// # Panics
///
/// Panics when the body is streamed, which a request built from in-memory
/// bytes by [`send_bytes_with_retry`] never is.
fn attempt_request(request: &reqwest::Request) -> reqwest::Request {
    request
        .try_clone()
        .expect("a request with an in-memory body is reusable")
}

/// Answer `bytes` as text: valid UTF-8 keeps its buffer, and invalid UTF-8 is
/// replaced, as `reqwest::Response::text` does.
fn answer_text(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes)
        .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned())
}

/// Reads an answer body unchanged, failing once it exceeds `limit` bytes. An
/// advertised length, capped at `limit`, sizes the buffer up front.
///
/// # Errors
///
/// Returns [`ProviderError::Upstream`] past the bound, or the mapped transport
/// error when reading fails.
async fn read_bounded_bytes(
    provider: &str,
    mut response: reqwest::Response,
    limit: usize,
) -> ProviderResult<Vec<u8>> {
    let status = response.status().as_u16();
    // An advertised length sizes the answer, but never past the bound.
    let hint = response
        .content_length()
        .and_then(|len| usize::try_from(len).ok())
        .map_or(0, |len| len.min(limit));
    let mut bytes = Vec::with_capacity(hint);
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| map_reqwest_error(provider, error))?
    {
        if bytes.len() + chunk.len() > limit {
            return Err(ProviderError::upstream(
                provider,
                status,
                format!("response body exceeded {limit} bytes"),
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

/// Encoded request body with its content type and exact byte length.
pub(crate) struct Payload {
    /// `content-type` of the body.
    content_type: HeaderValue,
    /// Exact body length, sent as `content-length` so a streamed body is not
    /// chunked.
    len: u64,
    /// Body bytes, in memory or streamed from spooled files.
    body: reqwest::Body,
}

impl Payload {
    /// Wraps in-memory `bytes` of `content_type`.
    pub(crate) fn bytes(content_type: HeaderValue, bytes: Vec<u8>) -> Self {
        Self {
            content_type,
            len: bytes.len() as u64,
            body: bytes.into(),
        }
    }

    /// Wraps a streamed `body` of exactly `len` bytes of `content_type`.
    pub(crate) const fn streamed(content_type: HeaderValue, len: u64, body: reqwest::Body) -> Self {
        Self {
            content_type,
            len,
            body,
        }
    }
}

impl HttpTransport {
    /// Sends one `method` request, with `payload` when present, and returns
    /// the successful answer before its body is read.
    ///
    /// Never retries, because the request may start non-idempotent provider
    /// work. A refusal body is read through this transport's
    /// `max_response_bytes` bound. A successful exchange is logged in the
    /// caller's [`crate::exchange::scope`] as open; a refusal as ended.
    ///
    /// # Errors
    ///
    /// Returns the mapped transport error, [`ProviderError::Upstream`] when a
    /// refusal exceeds the bound, or [`ProviderError::Status`] for a
    /// non-success answer.
    pub(crate) async fn open_once(
        &self,
        provider: &str,
        method: Method,
        url: &str,
        headers: HeaderMap,
        payload: Option<Payload>,
    ) -> ProviderResult<reqwest::Response> {
        let (response, exchange) = self
            .open_timed(provider, method, url, headers, payload)
            .await?;
        exchange.open();
        Ok(response)
    }

    /// Sends one request exactly as [`Self::open_once`] describes and returns
    /// the successful answer with the timer of its still-unread exchange.
    ///
    /// # Errors
    ///
    /// Returns the errors of [`Self::open_once`]; a failed exchange is logged
    /// as ended when it fails.
    async fn open_timed(
        &self,
        provider: &str,
        method: Method,
        url: &str,
        mut headers: HeaderMap,
        payload: Option<Payload>,
    ) -> ProviderResult<(reqwest::Response, ExchangeTimer)> {
        let mut request = self.client().request(method, url);
        if let Some(payload) = payload {
            headers.insert(CONTENT_TYPE, payload.content_type);
            headers.insert(CONTENT_LENGTH, HeaderValue::from(payload.len));
            request = request.body(payload.body);
        }
        let mut exchange = ExchangeTimer::start();
        let response = request
            .headers(headers)
            .send()
            .await
            .map_err(|error| map_reqwest_error(provider, error))?;
        exchange.headers();
        let status = response.status();
        if !status.is_success() {
            let retry_after = retry_after_ms(&response);
            let text = read_bounded(provider, response, self.config().max_response_bytes).await?;
            return Err(ProviderError::from_status(
                provider,
                status,
                text,
                retry_after,
            ));
        }
        Ok((response, exchange))
    }

    /// Sends one request like [`Self::open_once`] and returns the successful
    /// answer's content type and bytes, read through `max_response_bytes`;
    /// the exchange is logged in the caller's [`crate::exchange::scope`] as
    /// ended once the body is read.
    ///
    /// # Errors
    ///
    /// Returns the errors of [`Self::open_once`], and
    /// [`ProviderError::Upstream`] when the answer exceeds the bound.
    pub(crate) async fn send_once(
        &self,
        provider: &str,
        method: Method,
        url: &str,
        headers: HeaderMap,
        payload: Option<Payload>,
    ) -> ProviderResult<(String, Vec<u8>)> {
        let (response, exchange) = self
            .open_timed(provider, method, url, headers, payload)
            .await?;
        let answer_type = answer_type(&response).to_owned();
        let bytes =
            read_bounded_bytes(provider, response, self.config().max_response_bytes).await?;
        drop(exchange);
        Ok((answer_type, bytes))
    }
}

/// The answer's `content-type`, or `application/octet-stream`.
fn answer_type(response: &reqwest::Response) -> &str {
    response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/octet-stream")
}

/// Maps a reqwest failure: connection failures never reached the provider,
/// timeouts and other failures may have.
pub(crate) fn map_reqwest_error(provider: &str, error: reqwest::Error) -> ProviderError {
    if error.is_connect() {
        ProviderError::Connect {
            provider: provider.to_owned(),
            detail: error.to_string(),
        }
    } else if error.is_timeout() {
        ProviderError::timeout(provider)
    } else {
        ProviderError::upstream(provider, 0, error.to_string())
    }
}

fn retry_after_ms(response: &reqwest::Response) -> Option<u64> {
    let value = response.headers().get(RETRY_AFTER)?.to_str().ok()?;
    let duration = parse_retry_after(value, SystemTime::now())?;
    u64::try_from(duration.as_millis()).ok()
}

fn retry_after_header_value(retry_after_ms: Option<u64>) -> Option<String> {
    retry_after_ms.map(|millis| (millis / 1000).to_string())
}

#[cfg(test)]
mod bounded_answers {
    use crate::common;
    use crate::{HttpTransport, ProviderClient, ProviderError, TransportConfig};
    use skald_spec::{OpenAiChatRequest, ProviderRequest};
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// An answer larger than the transport bound fails instead of buffering,
    /// and a refused answer keeps its status and body. A known length sizes the
    /// collected answer exactly; a chunked answer still collects; an advertised
    /// length past the bound reserves at most the bound; truncation still fails.
    #[tokio::test]
    async fn oversized_answers_fail_and_refusals_keep_status_and_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string("x".repeat(2048)))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(418).set_body_string("{\"error\":\"teapot\"}"))
            .mount(&server)
            .await;
        let request: OpenAiChatRequest =
            serde_json::from_str(&common::fixture("openai/chat_completion_request.json"))
                .expect("fixture parses");
        let transport = HttpTransport::new(TransportConfig {
            max_response_bytes: 1024,
            ..TransportConfig::default()
        })
        .expect("transport builds");
        let client = crate::OpenAiClient::with_transport(
            crate::auth::OpenAiAuth::new("sk-test").with_base_url(server.uri()),
            transport,
            common::retry_policy(),
        );

        let oversized = client
            .send(ProviderRequest::OpenAiChatCompletion(request.clone()))
            .await
            .expect_err("oversized answer fails");
        assert!(
            matches!(oversized, ProviderError::Upstream { status: 200, .. }),
            "{oversized:?}"
        );

        let refused = client
            .send(ProviderRequest::OpenAiChatCompletion(request))
            .await
            .expect_err("refusal fails");
        assert_eq!(
            refused,
            ProviderError::Status {
                provider: "openai".to_owned(),
                status: 418,
                body: "{\"error\":\"teapot\"}".to_owned(),
                retry_after_ms: None,
            }
        );

        let read = |head: &'static str, body: Vec<u8>, limit| async move {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("listener binds");
            let url = format!(
                "http://{}/",
                listener.local_addr().expect("listener address")
            );
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let (mut socket, _) = listener.accept().await.expect("client connects");
                let mut request = [0_u8; 1024];
                let _ = socket.read(&mut request).await;
                let _ = socket.write_all(head.as_bytes()).await;
                let _ = socket.write_all(&body).await;
            });
            let response = reqwest::get(url).await.expect("answer starts");
            super::read_bounded_bytes("openai", response, limit).await
        };
        let head = "HTTP/1.1 200 OK\r\ncontent-length: 100000\r\n\r\n";
        let known = read(head, vec![b'k'; 100_000], 1 << 20)
            .await
            .expect("known length collects");
        assert_eq!((known.len(), known.capacity()), (100_000, 100_000));
        let chunked = read(
            "HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n",
            b"3\r\nabc\r\n2\r\nde\r\n0\r\n\r\n".to_vec(),
            1024,
        )
        .await
        .expect("chunked answer collects");
        assert_eq!(chunked, b"abcde");
        let advertised = "HTTP/1.1 200 OK\r\ncontent-length: 18446744073709551613\r\n\r\n";
        assert!(matches!(
            read(advertised, vec![b'h'; 2048], 1024).await,
            Err(ProviderError::Upstream { status: 200, .. })
        ));
        assert!(matches!(
            read(
                "HTTP/1.1 200 OK\r\ncontent-length: 2048\r\n\r\n",
                vec![b'o'; 2048],
                1024
            )
            .await,
            Err(ProviderError::Upstream { status: 200, .. })
        ));
        let truncated = read(
            "HTTP/1.1 200 OK\r\ncontent-length: 512\r\n\r\n",
            vec![b't'; 100],
            1024,
        )
        .await
        .expect_err("truncated answer fails");
        assert!(
            matches!(truncated, ProviderError::Upstream { status: 0, .. }),
            "{truncated:?}"
        );
    }

    /// A native body is serialized once and its allocation, spare capacity
    /// included, backs the first exchange and every retry, and a valid UTF-8 answer keeps its buffer as
    /// text; invalid UTF-8 is replaced, refusals keep status and body, and an
    /// oversized answer fails before its status is inspected.
    #[tokio::test]
    async fn native_transport_reuses_owned_buffers() {
        let request = r#"{"model":"m","messages":[],"vendor":"é"}"#;
        let mut bytes = Vec::with_capacity(request.len() * 2);
        bytes.extend_from_slice(request.as_bytes());
        let backing = bytes.as_ptr();
        let built = reqwest::Client::new()
            .post("http://localhost/")
            .body(bytes)
            .build()
            .expect("request builds");
        for _ in 0..2 {
            let attempt = super::attempt_request(&built);
            let sent = attempt.body().and_then(reqwest::Body::as_bytes);
            assert_eq!(sent.map(<[u8]>::as_ptr), Some(backing), "no body copy");
        }
        let valid = "{\"x\":\"é\"}".as_bytes().to_vec();
        let answer = valid.as_ptr();
        let text = super::answer_text(valid);
        assert_eq!(text.as_ptr(), answer, "no text copy");
        assert_eq!(super::answer_text(vec![b'a', 0xff]), "a\u{fffd}");

        let ok = || ResponseTemplate::new(200);
        let cases: [(_, Vec<_>, Result<Vec<u8>, _>); 4] = [
            (
                "retried",
                vec![
                    ResponseTemplate::new(500),
                    ok().set_body_string("{\"id\":\"é\"}"),
                ],
                Ok("{\"id\":\"é\"}".into()),
            ),
            (
                "invalid utf-8",
                vec![ok().set_body_bytes(vec![b'"', 0xff, b'"'])],
                Ok("\"\u{fffd}\"".into()),
            ),
            (
                "refused",
                vec![ResponseTemplate::new(418).set_body_string("{\"error\":\"teapot\"}")],
                Err(ProviderError::Status {
                    provider: "openai".to_owned(),
                    status: 418,
                    body: "{\"error\":\"teapot\"}".to_owned(),
                    retry_after_ms: None,
                }),
            ),
            (
                "oversized refusal",
                vec![ResponseTemplate::new(418).set_body_string("x".repeat(2048))],
                Err(ProviderError::upstream(
                    "openai",
                    418,
                    "response body exceeded 1024 bytes",
                )),
            ),
        ];
        for (name, answers, expected) in cases {
            let server = MockServer::start().await;
            let attempts = answers.len();
            for answer in answers {
                Mock::given(method("POST"))
                    .respond_with(answer)
                    .up_to_n_times(1)
                    .mount(&server)
                    .await;
            }
            let transport = HttpTransport::new(TransportConfig {
                max_response_bytes: 1024,
                ..TransportConfig::default()
            })
            .expect("transport builds");
            let answer = super::send_bytes_with_retry(
                &transport,
                "openai",
                reqwest::Method::POST,
                &server.uri(),
                reqwest::header::HeaderMap::new(),
                request.as_bytes().to_vec(),
                &common::retry_policy(),
            )
            .await
            .map(String::into_bytes);
            assert_eq!(answer, expected, "{name}");
            let received = server.received_requests().await.expect("recorded");
            assert_eq!(received.len(), attempts, "{name}");
            for exchange in received {
                assert_eq!(exchange.body, request.as_bytes(), "{name}: same body");
            }
        }
    }
}
