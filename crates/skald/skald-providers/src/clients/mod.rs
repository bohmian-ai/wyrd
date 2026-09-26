//! Concrete provider clients.

pub mod anthropic;
pub mod embeddings;
pub mod google;
pub mod openai;
pub mod vertex;

pub use anthropic::AnthropicClient;
pub use google::GoogleClient;
pub use openai::{
    MediaAnswer, OpenAiBatchRoute, OpenAiClient, OpenAiMediaRoute, OpenAiRoute, UploadContent,
    UploadFile,
};
pub use vertex::VertexClient;

use std::time::SystemTime;

use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE, HeaderMap, HeaderValue, RETRY_AFTER};
use reqwest::{Method, StatusCode};
use serde_json::value::RawValue;

use crate::error::{ProviderError, ProviderResult};
use crate::retry::{RetryPolicy, parse_retry_after};
use crate::transport::HttpTransport;

/// Sends a JSON request body with retry and returns the response body text.
///
/// Every answer body is read through the transport's `max_response_bytes`
/// bound before its status is inspected, so neither a success nor a refusal
/// can buffer an unbounded body.
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
    let mut attempt = 0;
    loop {
        let response = transport
            .client()
            .request(method.clone(), url)
            .headers(headers.clone())
            .body(body.clone())
            .send()
            .await
            .map_err(|error| map_reqwest_error(provider, error))?;

        let status = response.status();
        let retry_after = retry_after_ms(&response);
        let text = read_bounded(provider, response, transport.config().max_response_bytes).await?;

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
/// returned as its status error, like a buffered request.
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
    body: &RawValue,
) -> ProviderResult<ProviderByteStream> {
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    let response = transport
        .client()
        .post(url)
        .headers(headers)
        .body(body.get().as_bytes().to_vec())
        .send()
        .await
        .map_err(|error| map_reqwest_error(provider, error))?;
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
        skald_spec::ProviderRequest::Vertex(_) => "vertex_generate_content",
        skald_spec::ProviderRequest::VertexPredict(_) => "vertex_predict",
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
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Reads an answer body unchanged, failing once it exceeds `limit` bytes.
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
    let mut bytes = Vec::new();
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
    /// `max_response_bytes` bound.
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
        mut headers: HeaderMap,
        payload: Option<Payload>,
    ) -> ProviderResult<reqwest::Response> {
        let mut request = self.client().request(method, url);
        if let Some(payload) = payload {
            headers.insert(CONTENT_TYPE, payload.content_type);
            headers.insert(CONTENT_LENGTH, HeaderValue::from(payload.len));
            request = request.body(payload.body);
        }
        let response = request
            .headers(headers)
            .send()
            .await
            .map_err(|error| map_reqwest_error(provider, error))?;
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
        Ok(response)
    }

    /// Sends one request like [`Self::open_once`] and returns the successful
    /// answer's content type and bytes, read through `max_response_bytes`.
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
        let response = self
            .open_once(provider, method, url, headers, payload)
            .await?;
        let answer_type = answer_type(&response).to_owned();
        Ok((
            answer_type,
            read_bounded_bytes(provider, response, self.config().max_response_bytes).await?,
        ))
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

#[allow(dead_code)]
fn status_is_success(status: StatusCode) -> bool {
    status.is_success()
}

#[cfg(test)]
mod bounded_answers {
    use crate::common;
    use crate::{HttpTransport, ProviderClient, ProviderError, TransportConfig};
    use skald_spec::{OpenAiChatRequest, ProviderRequest};
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// An answer larger than the transport bound fails instead of buffering,
    /// and a refused answer keeps its status and body.
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
    }
}
