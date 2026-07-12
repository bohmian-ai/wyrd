//! Anthropic Messages client.

use async_trait::async_trait;
use skald_spec::{ProviderRequest, ProviderResponse};

use crate::auth::AnthropicAuth;
use crate::error::{ProviderError, ProviderResult};
use crate::raw;
use crate::retry::RetryPolicy;
use crate::stream::decode_sse_events;
use crate::trait_::{ProviderClient, ProviderStream};
use crate::transport::{HttpTransport, TransportConfig};

/// Anthropic provider client.
#[derive(Debug, Clone)]
pub struct AnthropicClient {
    auth: AnthropicAuth,
    transport: HttpTransport,
    retry: RetryPolicy,
}

impl AnthropicClient {
    /// Creates an Anthropic client from explicit auth.
    pub fn new(auth: AnthropicAuth) -> ProviderResult<Self> {
        Ok(Self {
            auth,
            transport: HttpTransport::new(TransportConfig::default())?,
            retry: RetryPolicy::default(),
        })
    }

    /// Creates an Anthropic client from environment variables.
    pub fn from_env() -> ProviderResult<Self> {
        Self::new(AnthropicAuth::from_env()?)
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

    /// Returns the required unsupported-embeddings error.
    pub fn unsupported_embeddings(&self) -> ProviderError {
        ProviderError::bad_request("anthropic", "anthropic does not support embeddings")
    }

    /// Sends a native Anthropic request.
    pub async fn send_native(&self, request: ProviderRequest) -> ProviderResult<ProviderResponse> {
        match request {
            ProviderRequest::AnthropicMessage(request) => {
                let url = format!("{}/v1/messages", self.auth.base_url());
                let response = super::send_json_with_retry(
                    &self.transport,
                    "anthropic",
                    &url,
                    self.auth.headers()?,
                    &request,
                    &self.retry,
                )
                .await?;
                Ok(ProviderResponse::AnthropicMessage(response))
            }
            ProviderRequest::RawV1 { body, .. } => {
                let url = format!("{}/raw", self.auth.base_url());
                let response = raw::send_raw(
                    &self.transport,
                    "anthropic",
                    &url,
                    self.auth.headers()?,
                    &body,
                    &self.retry,
                )
                .await?;
                Ok(ProviderResponse::RawV1(response))
            }
            other => Err(ProviderError::variant_mismatch(
                "anthropic",
                super::request_variant_label(&other),
            )),
        }
    }
}

#[async_trait]
impl ProviderClient for AnthropicClient {
    async fn send(&self, request: ProviderRequest) -> ProviderResult<ProviderResponse> {
        self.send_native(request).await
    }

    async fn stream(&self, request: ProviderRequest) -> ProviderResult<ProviderStream> {
        match request {
            ProviderRequest::AnthropicMessage(request) => {
                let url = format!("{}/v1/messages", self.auth.base_url());
                let body = serde_json::to_vec(&request)
                    .map_err(|error| ProviderError::decode("anthropic", error))?;
                let text = super::send_bytes_with_retry(
                    &self.transport,
                    "anthropic",
                    reqwest::Method::POST,
                    &url,
                    self.auth.headers()?,
                    body,
                    &self.retry,
                )
                .await?;
                let events = decode_sse_events("anthropic", text.as_bytes())?;
                Ok(ProviderStream::Anthropic(events))
            }
            other => Err(ProviderError::variant_mismatch(
                "anthropic",
                super::request_variant_label(&other),
            )),
        }
    }
}

#[cfg(test)]
mod anthropic_messages {
    use crate::ProviderClient;
    use crate::common;
    use skald_spec::{AnthropicMessagesRequest, ProviderRequest, ProviderResponse};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn messages_request_body_matches_fixture_and_response_parses() {
        let server = MockServer::start().await;
        let request_body = common::fixture("anthropic/messages_request.json");
        let response_body = common::fixture("anthropic/messages_response.json");
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
            .mount(&server)
            .await;

        let request: AnthropicMessagesRequest =
            serde_json::from_str(&request_body).expect("fixture parses");
        let response = common::anthropic_client(&server.uri())
            .send(ProviderRequest::AnthropicMessage(request))
            .await
            .expect("request succeeds");

        assert!(matches!(response, ProviderResponse::AnthropicMessage(_)));
        assert_eq!(response.adapter().tool_calls().len(), 1);
        common::assert_received_body(&server, &request_body).await;
    }

    #[tokio::test]
    async fn messages_with_cache_control_body_matches_fixture() {
        let server = MockServer::start().await;
        let request_body = common::fixture("anthropic/with_cache_control.json");
        let response_body = common::fixture("anthropic/messages_response.json");
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
            .mount(&server)
            .await;

        let request: AnthropicMessagesRequest =
            serde_json::from_str(&request_body).expect("fixture parses");
        common::anthropic_client(&server.uri())
            .send(ProviderRequest::AnthropicMessage(request))
            .await
            .expect("request succeeds");
        common::assert_received_body(&server, &request_body).await;
    }

    #[test]
    fn anthropic_embed_returns_unsupported() {
        let client = common::anthropic_client("http://localhost");

        assert_eq!(
            client.unsupported_embeddings().code(),
            "SKALD_PROVIDERS_400_BAD_REQUEST"
        );
    }
}
