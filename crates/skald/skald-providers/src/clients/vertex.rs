//! Vertex GenerateContent and Predict client.

use async_trait::async_trait;
use serde_json::value::RawValue;
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
        Ok(Self::with_transport(
            auth,
            model,
            HttpTransport::new(TransportConfig::default())?,
            RetryPolicy::default(),
        ))
    }

    /// Creates a Vertex client from environment variables.
    pub fn from_env(model: impl Into<String>) -> ProviderResult<Self> {
        Self::new(VertexAuth::from_env()?, model)
    }

    /// Creates a Vertex client over a shared transport and retry policy.
    pub fn with_transport(
        auth: VertexAuth,
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
            "vertex",
            &self.auth.model_url(&self.model, "generateContent"),
            self.auth.headers().await?,
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
            "vertex",
            &self
                .auth
                .model_url(&self.model, "streamGenerateContent?alt=sse"),
            self.auth.headers().await?,
            body,
        )
        .await
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

#[cfg(test)]
mod vertex_embed {
    use crate::ProviderClient;
    use crate::common;
    use skald_spec::{ProviderRequest, ProviderResponse, VertexPredictRequest};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn vertex_predict_embedding_request_body_matches_fixture_and_response_parses() {
        let server = MockServer::start().await;
        let request_body = common::fixture("vertex/predict_request.json");
        let response_body = common::fixture("vertex/predict_response.json");
        Mock::given(method("POST"))
            .and(path("/v1/projects/project-a/locations/us-central1/publishers/google/models/text-embedding-004:predict"))
            .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
            .mount(&server)
            .await;

        let request: VertexPredictRequest =
            serde_json::from_str(&request_body).expect("fixture parses");
        let response = common::vertex_client(&server.uri(), "text-embedding-004")
            .send(ProviderRequest::VertexPredict(request))
            .await
            .expect("request succeeds");

        assert!(matches!(response, ProviderResponse::VertexPredict(_)));
        common::assert_received_body(&server, &request_body).await;
    }
}

#[cfg(test)]
mod vertex_generate {
    use crate::ProviderClient;
    use crate::common;
    use skald_spec::{ProviderRequest, ProviderResponse, VertexGenerateContentRequest};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn vertex_generate_request_body_matches_fixture_and_path_has_projectlocation() {
        let server = MockServer::start().await;
        let request_body = common::fixture("vertex/generate_request.json");
        let response_body = common::fixture("vertex/generate_response.json");
        Mock::given(method("POST"))
            .and(path("/v1/projects/project-a/locations/us-central1/publishers/google/models/gemini-2.5-flash:generateContent"))
            .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
            .mount(&server)
            .await;

        let request: VertexGenerateContentRequest =
            serde_json::from_str(&request_body).expect("fixture parses");
        let response = common::vertex_client(&server.uri(), "gemini-2.5-flash")
            .send(ProviderRequest::Vertex(request))
            .await
            .expect("request succeeds");

        assert!(matches!(
            response,
            ProviderResponse::VertexGenerateContent(_)
        ));
        common::assert_received_body(&server, &request_body).await;
    }
}
