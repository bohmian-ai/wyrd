//! Google AI Studio GenerateContent and embeddings client.

use async_trait::async_trait;
use reqwest::header::HeaderMap;
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
        Ok(Self {
            auth,
            transport: HttpTransport::new(TransportConfig::default())?,
            retry: RetryPolicy::default(),
            model: model.into(),
        })
    }

    /// Creates a Google client from environment variables.
    pub fn from_env(model: impl Into<String>) -> ProviderResult<Self> {
        Self::new(GoogleApiKeyAuth::from_env()?, model)
    }

    /// Overrides transport and retry policy, primarily for tests.
    pub fn with_transport_and_retry(
        mut self,
        transport: HttpTransport,
        retry: RetryPolicy,
    ) -> Self {
        self.transport = transport;
        self.retry = retry;
        self
    }

    /// Sends a native Google request.
    pub async fn send_native(&self, request: ProviderRequest) -> ProviderResult<ProviderResponse> {
        match request {
            ProviderRequest::GeminiGenerateContent(request) => {
                let path = format!(
                    "/v1beta/models/{}:generateContent",
                    urlencoding::encode(&self.model)
                );
                let url = self.auth.url_with_key(&path)?;
                let response = super::send_json_with_retry(
                    &self.transport,
                    "google",
                    &url,
                    HeaderMap::new(),
                    &request,
                    &self.retry,
                )
                .await?;
                Ok(ProviderResponse::GeminiGenerateContent(response))
            }
            ProviderRequest::GoogleBatchEmbed(request) => {
                let path = format!(
                    "/v1beta/models/{}:batchEmbedContents",
                    urlencoding::encode(&self.model)
                );
                let url = self.auth.url_with_key(&path)?;
                let response = super::send_json_with_retry(
                    &self.transport,
                    "google",
                    &url,
                    HeaderMap::new(),
                    &request,
                    &self.retry,
                )
                .await?;
                Ok(ProviderResponse::GoogleBatchEmbed(response))
            }
            ProviderRequest::RawV1 { body, .. } => {
                let url = self.auth.url_with_key("/raw")?;
                let response = raw::send_raw(
                    &self.transport,
                    "google",
                    &url,
                    HeaderMap::new(),
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
                let path = format!(
                    "/v1beta/models/{}:streamGenerateContent",
                    urlencoding::encode(&self.model)
                );
                let url = self.auth.url_with_key(&path)?;
                let body = serde_json::to_vec(&request)
                    .map_err(|error| ProviderError::decode("google", error))?;
                let text = super::send_bytes_with_retry(
                    &self.transport,
                    "google",
                    reqwest::Method::POST,
                    &url,
                    HeaderMap::new(),
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
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn google_embeddings_request_body_matches_fixture_and_response_parses() {
        let server = MockServer::start().await;
        let request_body = common::fixture("google/batch_embed_request.json");
        let response_body = common::fixture("google/batch_embed_response.json");
        Mock::given(method("POST"))
            .and(path("/v1beta/models/text-embedding-004:batchEmbedContents"))
            .and(query_param("key", "google-test"))
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
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn generate_request_body_matches_fixture_and_response_parses() {
        let server = MockServer::start().await;
        let request_body = common::fixture("google/generate_content_request.json");
        let response_body = common::fixture("google/generate_content_response.json");
        Mock::given(method("POST"))
            .and(path("/v1beta/models/gemini-2.5-flash:generateContent"))
            .and(query_param("key", "google-test"))
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
    }
}
