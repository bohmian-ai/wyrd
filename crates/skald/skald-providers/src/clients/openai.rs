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
