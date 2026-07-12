//! OpenAI Chat, Responses, and Embeddings client.

use async_trait::async_trait;
use skald_spec::{ProviderRequest, ProviderResponse};

use crate::auth::OpenAiAuth;
use crate::error::{ProviderError, ProviderResult};
use crate::raw;
use crate::retry::RetryPolicy;
use crate::stream::decode_sse_events;
use crate::trait_::{ProviderClient, ProviderStream};
use crate::transport::{HttpTransport, TransportConfig};

/// OpenAI provider client.
#[derive(Debug, Clone)]
pub struct OpenAiClient {
    auth: OpenAiAuth,
    transport: HttpTransport,
    retry: RetryPolicy,
}

impl OpenAiClient {
    /// Creates an OpenAI client from explicit auth.
    pub fn new(auth: OpenAiAuth) -> ProviderResult<Self> {
        Ok(Self {
            auth,
            transport: HttpTransport::new(TransportConfig::default())?,
            retry: RetryPolicy::default(),
        })
    }

    /// Creates an OpenAI client from environment variables.
    pub fn from_env() -> ProviderResult<Self> {
        Self::new(OpenAiAuth::from_env()?)
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

    /// Sends a native OpenAI request.
    pub async fn send_native(&self, request: ProviderRequest) -> ProviderResult<ProviderResponse> {
        match request {
            ProviderRequest::OpenAiChatCompletion(request) => {
                let url = format!("{}/chat/completions", self.auth.base_url());
                let response = super::send_json_with_retry(
                    &self.transport,
                    "openai",
                    &url,
                    self.auth.headers()?,
                    &request,
                    &self.retry,
                )
                .await?;
                Ok(ProviderResponse::OpenAiChatCompletion(response))
            }
            ProviderRequest::OpenAiResponses(request) => {
                let url = format!("{}/responses", self.auth.base_url());
                let response = super::send_json_with_retry(
                    &self.transport,
                    "openai",
                    &url,
                    self.auth.headers()?,
                    &request,
                    &self.retry,
                )
                .await?;
                Ok(ProviderResponse::OpenAiResponses(response))
            }
            ProviderRequest::OpenAiEmbeddings(request) => {
                let url = format!("{}/embeddings", self.auth.base_url());
                let response = super::send_json_with_retry(
                    &self.transport,
                    "openai",
                    &url,
                    self.auth.headers()?,
                    &request,
                    &self.retry,
                )
                .await?;
                Ok(ProviderResponse::OpenAiEmbeddings(response))
            }
            ProviderRequest::RawV1 { body, .. } => {
                let url = format!("{}/raw", self.auth.base_url());
                let response = raw::send_raw(
                    &self.transport,
                    "openai",
                    &url,
                    self.auth.headers()?,
                    &body,
                    &self.retry,
                )
                .await?;
                Ok(ProviderResponse::RawV1(response))
            }
            other => Err(ProviderError::variant_mismatch(
                "openai",
                super::request_variant_label(&other),
            )),
        }
    }
}

#[async_trait]
impl ProviderClient for OpenAiClient {
    async fn send(&self, request: ProviderRequest) -> ProviderResult<ProviderResponse> {
        self.send_native(request).await
    }

    async fn stream(&self, request: ProviderRequest) -> ProviderResult<ProviderStream> {
        match request {
            ProviderRequest::OpenAiChatCompletion(request) => {
                let url = format!("{}/chat/completions", self.auth.base_url());
                let body = serde_json::to_vec(&request)
                    .map_err(|error| ProviderError::decode("openai", error))?;
                let text = super::send_bytes_with_retry(
                    &self.transport,
                    "openai",
                    reqwest::Method::POST,
                    &url,
                    self.auth.headers()?,
                    body,
                    &self.retry,
                )
                .await?;
                let chunks = decode_sse_events("openai", text.as_bytes())?;
                Ok(ProviderStream::OpenAiChat(chunks))
            }
            ProviderRequest::OpenAiResponses(request) => {
                let url = format!("{}/responses", self.auth.base_url());
                let body = serde_json::to_vec(&request)
                    .map_err(|error| ProviderError::decode("openai", error))?;
                let text = super::send_bytes_with_retry(
                    &self.transport,
                    "openai",
                    reqwest::Method::POST,
                    &url,
                    self.auth.headers()?,
                    body,
                    &self.retry,
                )
                .await?;
                let events = decode_sse_events("openai", text.as_bytes())?;
                Ok(ProviderStream::OpenAiResponses(events))
            }
            other => Err(ProviderError::variant_mismatch(
                "openai",
                super::request_variant_label(&other),
            )),
        }
    }
}

#[cfg(test)]
mod openai_chat {
    use crate::ProviderClient;
    use crate::common;
    use skald_spec::{OpenAiChatRequest, ProviderRequest, ProviderResponse};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn chat_request_body_matches_fixture_and_response_parses() {
        let server = MockServer::start().await;
        let request_body = common::fixture("openai/chat_completion_request.json");
        let response_body = common::fixture("openai/chat_completion_response.json");
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
            .mount(&server)
            .await;

        let request: OpenAiChatRequest =
            serde_json::from_str(&request_body).expect("fixture parses");
        let response = common::openai_client(&server.uri())
            .send(ProviderRequest::OpenAiChatCompletion(request))
            .await
            .expect("request succeeds");

        assert!(matches!(
            response,
            ProviderResponse::OpenAiChatCompletion(_)
        ));
        common::assert_received_body(&server, &request_body).await;
    }

    #[tokio::test]
    async fn chat_tool_calls_response_parses() {
        let server = MockServer::start().await;
        let request_body = common::fixture("openai/chat_completion_request.json");
        let response_body = common::fixture("openai/chat_completion_tool_call_response.json");
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
            .mount(&server)
            .await;

        let request: OpenAiChatRequest =
            serde_json::from_str(&request_body).expect("fixture parses");
        let response = common::openai_client(&server.uri())
            .send(ProviderRequest::OpenAiChatCompletion(request))
            .await
            .expect("request succeeds");

        assert_eq!(response.adapter().tool_calls().len(), 1);
    }

    #[tokio::test]
    async fn chat_429_retries_then_succeeds() {
        let server = MockServer::start().await;
        let request_body = common::fixture("openai/chat_completion_request.json");
        let response_body = common::fixture("openai/chat_completion_response.json");
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "0"))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
            .mount(&server)
            .await;

        let request: OpenAiChatRequest =
            serde_json::from_str(&request_body).expect("fixture parses");
        let response = common::openai_client(&server.uri())
            .send(ProviderRequest::OpenAiChatCompletion(request))
            .await
            .expect("retry succeeds");

        assert!(matches!(
            response,
            ProviderResponse::OpenAiChatCompletion(_)
        ));
    }
}

#[cfg(test)]
mod openai_embed {
    use crate::ProviderClient;
    use crate::common;
    use skald_spec::{OpenAiEmbeddingsRequest, ProviderRequest, ProviderResponse};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn openai_embeddings_request_body_matches_fixture_and_response_parses() {
        let server = MockServer::start().await;
        let request_body = common::fixture("openai/embeddings_request.json");
        let response_body = common::fixture("openai/embeddings_response.json");
        Mock::given(method("POST"))
            .and(path("/embeddings"))
            .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
            .mount(&server)
            .await;

        let request: OpenAiEmbeddingsRequest =
            serde_json::from_str(&request_body).expect("fixture parses");
        let response = common::openai_client(&server.uri())
            .send(ProviderRequest::OpenAiEmbeddings(request))
            .await
            .expect("request succeeds");

        assert!(matches!(response, ProviderResponse::OpenAiEmbeddings(_)));
        common::assert_received_body(&server, &request_body).await;
    }
}

#[cfg(test)]
mod openai_responses {
    use crate::ProviderClient;
    use crate::common;
    use skald_spec::{OpenAiResponsesRequest, ProviderRequest, ProviderResponse};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn responses_request_body_matches_fixture_and_response_parses() {
        let server = MockServer::start().await;
        let request_body = common::fixture("openai/responses_request.json");
        let response_body = common::fixture("openai/responses_response.json");
        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
            .mount(&server)
            .await;

        let request: OpenAiResponsesRequest =
            serde_json::from_str(&request_body).expect("fixture parses");
        let response = common::openai_client(&server.uri())
            .send(ProviderRequest::OpenAiResponses(request))
            .await
            .expect("request succeeds");

        assert!(matches!(response, ProviderResponse::OpenAiResponses(_)));
        common::assert_received_body(&server, &request_body).await;
    }
}

#[cfg(test)]
mod variant_mismatch {
    use crate::ProviderClient;
    use crate::common;
    use skald_spec::{AnthropicMessagesRequest, ProviderRequest};

    #[tokio::test]
    async fn wrong_provider_request_variant_returns_variant_mismatch() {
        let request_body = common::fixture("anthropic/messages_request.json");
        let request: AnthropicMessagesRequest =
            serde_json::from_str(&request_body).expect("fixture parses");

        let error = common::openai_client("http://localhost")
            .send(ProviderRequest::AnthropicMessage(request))
            .await
            .expect_err("variant mismatch");

        assert_eq!(error.code(), "SKALD_PROVIDERS_400_VARIANT_MISMATCH");
    }
}
