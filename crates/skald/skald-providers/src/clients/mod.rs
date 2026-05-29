//! Concrete provider clients.

pub mod anthropic;
pub mod embeddings;
pub mod google;
pub mod openai;
pub mod vertex;

pub use anthropic::AnthropicClient;
pub use google::GoogleClient;
pub use openai::OpenAiClient;
pub use vertex::VertexClient;

use std::time::SystemTime;

use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderValue, RETRY_AFTER};
use reqwest::{Method, StatusCode};

use crate::error::{ProviderError, ProviderResult};
use crate::retry::{RetryPolicy, parse_retry_after};
use crate::transport::HttpTransport;

/// Sends a JSON request body with retry and returns the response body text.
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
        let text = response
            .text()
            .await
            .map_err(|error| map_reqwest_error(provider, error))?;

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

fn map_reqwest_error(provider: &str, error: reqwest::Error) -> ProviderError {
    if error.is_timeout() {
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
