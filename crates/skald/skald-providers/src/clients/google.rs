//! Google AI Studio GenerateContent and embeddings client.

use async_trait::async_trait;
use serde_json::value::RawValue;
use skald_spec::{ProviderRequest, ProviderResponse};

use crate::auth::GoogleApiKeyAuth;
use crate::error::{ProviderError, ProviderResult};
use crate::raw;
use crate::retry::RetryPolicy;
use crate::stream::decode_jsonl_events;
use crate::trait_::{ProviderClient, ProviderStream};
use crate::transport::{HttpTransport, TransportConfig};

/// Google AI Studio provider client.
#[derive(Debug, Clone)]
pub struct GoogleClient {
    auth: GoogleApiKeyAuth,
    transport: HttpTransport,
    retry: RetryPolicy,
    model: String,
}

impl GoogleClient {
    /// Creates a Google client from explicit auth and model path component.
    pub fn new(auth: GoogleApiKeyAuth, model: impl Into<String>) -> ProviderResult<Self> {
        Ok(Self::with_transport(
            auth,
            model,
            HttpTransport::new(TransportConfig::default())?,
            RetryPolicy::default(),
        ))
    }

    /// Creates a Google client from environment variables.
    pub fn from_env(model: impl Into<String>) -> ProviderResult<Self> {
        Self::new(GoogleApiKeyAuth::from_env()?, model)
    }

    /// Creates a Google client over a shared transport and retry policy.
    pub fn with_transport(
        auth: GoogleApiKeyAuth,
        model: impl Into<String>,
        transport: HttpTransport,
        retry: RetryPolicy,
    ) -> Self {
        Self {
            auth,
            transport,
            retry,
            model: model.into(),
        }
    }

    /// URL of `method` on the client's model; every model request builds its
    /// URL here.
    fn model_url(&self, method: &str) -> String {
        self.auth.url(&format!(
            "/v1beta/models/{}:{method}",
            urlencoding::encode(&self.model)
        ))
    }

    /// Posts a caller-built `GenerateContent` `body` for the client's model and
    /// returns the provider's answer bytes unchanged.
    ///
    /// Uses the same URL, auth headers, transport, retry policy, bounded read,
    /// and status errors as typed requests, but skips (de)serialization, so
    /// provider members the Skald wire types do not model survive unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Status`] for a non-success answer,
    /// [`ProviderError::Decode`] when the answer is not JSON or the auth
    /// headers are invalid, and [`ProviderError::Connect`],
    /// [`ProviderError::Timeout`], or [`ProviderError::Upstream`] when the
    /// exchange fails.
    pub async fn send_raw(&self, body: &RawValue) -> ProviderResult<Box<RawValue>> {
        raw::send_raw(
            &self.transport,
            "google",
            &self.model_url("generateContent"),
            self.auth.headers()?,
            body,
            &self.retry,
        )
        .await
    }

    /// Posts a caller-built `GenerateContent` `body` to the client's model as
    /// a server-sent event stream and returns the answer for incremental
    /// reading, its bytes unchanged.
    ///
    /// Uses `streamGenerateContent?alt=sse` with the same auth headers as
    /// [`Self::send_raw`] but never retries.
    ///
    /// # Errors
    ///
    /// Returns the errors of [`super::open_stream`], or
    /// [`ProviderError::Decode`] when the auth headers are invalid.
    pub async fn stream_raw(&self, body: &RawValue) -> ProviderResult<super::ProviderByteStream> {
        super::open_stream(
            &self.transport,
            "google",
            &self.model_url("streamGenerateContent?alt=sse"),
            self.auth.headers()?,
            body,
        )
        .await
    }

    /// Sends a native Google request.
    pub async fn send_native(&self, request: ProviderRequest) -> ProviderResult<ProviderResponse> {
        match request {
            ProviderRequest::GeminiGenerateContent(request) => {
                let response = super::send_json_with_retry(
                    &self.transport,
                    "google",
                    &self.model_url("generateContent"),
                    self.auth.headers()?,
                    &request,
                    &self.retry,
                )
                .await?;
                Ok(ProviderResponse::GeminiGenerateContent(response))
            }
            ProviderRequest::GoogleBatchEmbed(request) => {
                let response = super::send_json_with_retry(
                    &self.transport,
                    "google",
                    &self.model_url("batchEmbedContents"),
                    self.auth.headers()?,
                    &request,
                    &self.retry,
                )
                .await?;
                Ok(ProviderResponse::GoogleBatchEmbed(response))
            }
            ProviderRequest::RawV1 { body, .. } => {
                let response = raw::send_raw(
                    &self.transport,
                    "google",
                    &self.auth.url("/raw"),
                    self.auth.headers()?,
                    &body,
                    &self.retry,
                )
                .await?;
                Ok(ProviderResponse::RawV1(response))
            }
            other => Err(ProviderError::variant_mismatch(
                "google",
                super::request_variant_label(&other),
            )),
        }
    }
}

#[async_trait]
impl ProviderClient for GoogleClient {
    async fn send(&self, request: ProviderRequest) -> ProviderResult<ProviderResponse> {
        self.send_native(request).await
    }

    async fn stream(&self, request: ProviderRequest) -> ProviderResult<ProviderStream> {
        match request {
            ProviderRequest::GeminiGenerateContent(request) => {
                let body = serde_json::to_vec(&request)
                    .map_err(|error| ProviderError::decode("google", error))?;
                let text = super::send_bytes_with_retry(
                    &self.transport,
                    "google",
                    reqwest::Method::POST,
                    &self.model_url("streamGenerateContent"),
                    self.auth.headers()?,
                    body,
                    &self.retry,
                )
                .await?;
                let events = decode_jsonl_events("google", text.as_bytes())?;
                Ok(ProviderStream::Google(events))
            }
            other => Err(ProviderError::variant_mismatch(
                "google",
                super::request_variant_label(&other),
            )),
        }
    }
}

#[cfg(test)]
mod google_embed {
    use crate::ProviderClient;
    use crate::common;
    use skald_spec::{GoogleBatchEmbedRequest, ProviderRequest, ProviderResponse};
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn google_embeddings_request_body_matches_fixture_and_response_parses() {
        let server = MockServer::start().await;
        let request_body = common::fixture("google/batch_embed_request.json");
        let response_body = common::fixture("google/batch_embed_response.json");
        Mock::given(method("POST"))
            .and(path("/v1beta/models/text-embedding-004:batchEmbedContents"))
            .and(header("x-goog-api-key", "google-test"))
            .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
            .mount(&server)
            .await;

        let request: GoogleBatchEmbedRequest =
            serde_json::from_str(&request_body).expect("fixture parses");
        let response = common::google_client(&server.uri(), "text-embedding-004")
            .send(ProviderRequest::GoogleBatchEmbed(request))
            .await
            .expect("request succeeds");

        assert!(matches!(response, ProviderResponse::GoogleBatchEmbed(_)));
        common::assert_received_body(&server, &request_body).await;
    }
}

#[cfg(test)]
mod google_generate {
    use crate::ProviderClient;
    use crate::common;
    use skald_spec::{GoogleGenerateContentRequest, ProviderRequest, ProviderResponse};
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// The API key travels only in the `x-goog-api-key` header, never in the
    /// URL where transport errors and access logs would expose it.
    #[tokio::test]
    async fn generate_request_body_matches_fixture_and_response_parses() {
        let server = MockServer::start().await;
        let request_body = common::fixture("google/generate_content_request.json");
        let response_body = common::fixture("google/generate_content_response.json");
        Mock::given(method("POST"))
            .and(path("/v1beta/models/gemini-2.5-flash:generateContent"))
            .and(header("x-goog-api-key", "google-test"))
            .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
            .mount(&server)
            .await;

        let request: GoogleGenerateContentRequest =
            serde_json::from_str(&request_body).expect("fixture parses");
        let response = common::google_client(&server.uri(), "gemini-2.5-flash")
            .send(ProviderRequest::GeminiGenerateContent(request))
            .await
            .expect("request succeeds");

        assert!(matches!(
            response,
            ProviderResponse::GeminiGenerateContent(_)
        ));
        assert_eq!(response.adapter().tool_calls().len(), 1);
        common::assert_received_body(&server, &request_body).await;
        let requests = server.received_requests().await.expect("recording");
        assert_eq!(requests[0].url.query(), None);
    }
}
