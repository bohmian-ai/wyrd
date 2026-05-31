//! Vertex GenerateContent and Predict client.

use async_trait::async_trait;
use skald_spec::{ProviderRequest, ProviderResponse};

use crate::auth::VertexAuth;
use crate::error::{ProviderError, ProviderResult};
use crate::raw;
use crate::retry::RetryPolicy;
use crate::stream::decode_jsonl_events;
use crate::trait_::{ProviderClient, ProviderStream};
use crate::transport::{HttpTransport, TransportConfig};

/// Vertex provider client.
#[derive(Debug)]
pub struct VertexClient {
    auth: VertexAuth,
    transport: HttpTransport,
    retry: RetryPolicy,
    model: String,
}

impl VertexClient {
    /// Creates a Vertex client from explicit auth and model path component.
    pub fn new(auth: VertexAuth, model: impl Into<String>) -> ProviderResult<Self> {
        Ok(Self {
            auth,
            transport: HttpTransport::new(TransportConfig::default())?,
            retry: RetryPolicy::default(),
            model: model.into(),
        })
    }

    /// Creates a Vertex client from environment variables.
    pub fn from_env(model: impl Into<String>) -> ProviderResult<Self> {
        Self::new(VertexAuth::from_env()?, model)
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

    /// Sends a native Vertex request.
    pub async fn send_native(&self, request: ProviderRequest) -> ProviderResult<ProviderResponse> {
        match request {
            ProviderRequest::Vertex(request) => {
                let url = self.auth.model_url(&self.model, "generateContent");
                let response = super::send_json_with_retry(
                    &self.transport,
                    "vertex",
                    &url,
                    self.auth.headers().await?,
                    &request,
                    &self.retry,
                )
                .await?;
                Ok(ProviderResponse::VertexGenerateContent(response))
            }
            ProviderRequest::VertexPredict(request) => {
                let url = self.auth.model_url(&self.model, "predict");
                let response = super::send_json_with_retry(
                    &self.transport,
                    "vertex",
                    &url,
                    self.auth.headers().await?,
                    &request,
                    &self.retry,
                )
                .await?;
                Ok(ProviderResponse::VertexPredict(response))
            }
            ProviderRequest::RawV1 { body, .. } => {
                let url = format!("{}/raw", self.auth.model_url(&self.model, "predict"));
                let response = raw::send_raw(
                    &self.transport,
                    "vertex",
                    &url,
                    self.auth.headers().await?,
                    &body,
                    &self.retry,
                )
                .await?;
                Ok(ProviderResponse::RawV1(response))
            }
            other => Err(ProviderError::variant_mismatch(
                "vertex",
                super::request_variant_label(&other),
            )),
        }
    }
}

#[async_trait]
impl ProviderClient for VertexClient {
    async fn send(&self, request: ProviderRequest) -> ProviderResult<ProviderResponse> {
        self.send_native(request).await
    }

    async fn stream(&self, request: ProviderRequest) -> ProviderResult<ProviderStream> {
        match request {
            ProviderRequest::Vertex(request) => {
                let url = self.auth.model_url(&self.model, "streamGenerateContent");
                let body = serde_json::to_vec(&request)
                    .map_err(|error| ProviderError::decode("vertex", error))?;
                let text = super::send_bytes_with_retry(
                    &self.transport,
                    "vertex",
                    reqwest::Method::POST,
                    &url,
                    self.auth.headers().await?,
                    body,
                    &self.retry,
                )
                .await?;
                let events = decode_jsonl_events("vertex", text.as_bytes())?;
                Ok(ProviderStream::Google(events))
            }
            other => Err(ProviderError::variant_mismatch(
                "vertex",
                super::request_variant_label(&other),
            )),
        }
    }
}
