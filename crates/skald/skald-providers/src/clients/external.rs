//! Direct client for a user-provided external LLM gateway.
//!
//! An external gateway speaks one provider-native protocol at a declared base
//! URL. This client posts the typed native request body unchanged to the
//! dialect's conventional path below that base URL and decodes the matching
//! native response. Egress runs under an [`EndpointPolicy`] transport, so the
//! endpoint is screened, resolved addresses are pinned, redirects are refused,
//! no proxy is consulted, and answers are read through the transport's body
//! bound. Secret headers are marked sensitive so they never appear in debug
//! output.

use reqwest::header::HeaderMap;
use skald_spec::{ProviderRequest, ProviderResponse};
use url::Url;

use crate::endpoint::EndpointPolicy;
use crate::error::{ProviderError, ProviderResult};
use crate::retry::RetryPolicy;
use crate::transport::HttpTransport;

/// Provider label used in errors raised by external-gateway calls.
const PROVIDER: &str = "ext_gateway";

/// Client for one external gateway endpoint and header set.
///
/// The client is immutable after construction: its base URL, headers, retry
/// policy, and screened transport are fixed, so one client can be shared by
/// every attempt of the step that owns it.
#[derive(Clone)]
pub struct ExternalGatewayClient {
    /// Screened, pinned, redirect-refusing transport.
    transport: HttpTransport,
    /// Declared base URL; dialect paths are appended to it.
    base_url: Url,
    /// Authored non-secret headers merged with bound secret headers.
    headers: HeaderMap,
    /// Provider-internal retry policy applied inside one Workflow attempt.
    retry: RetryPolicy,
}

impl std::fmt::Debug for ExternalGatewayClient {
    /// Formats only the base URL and header names, never header values.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ExternalGatewayClient")
            .field("base_url", &self.base_url.as_str())
            .field("headers", &self.headers.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

impl ExternalGatewayClient {
    /// Builds a client for `base_url` under `policy` with the merged `headers`.
    ///
    /// The base URL must be admitted by the policy (scheme, port, and literal
    /// address); hostnames are screened by the policy's resolver when they
    /// resolve.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::BadRequest`] when the policy refuses the base
    /// URL, or the transport construction error when the client cannot be
    /// built.
    pub fn new(policy: EndpointPolicy, base_url: Url, headers: HeaderMap) -> ProviderResult<Self> {
        if !policy.admits(&base_url) {
            return Err(ProviderError::bad_request(
                PROVIDER,
                "external gateway endpoint is not permitted by the endpoint policy",
            ));
        }
        Ok(Self {
            transport: policy.transport()?,
            base_url,
            headers,
            retry: RetryPolicy::default(),
        })
    }

    /// Posts `request` to the dialect path below the base URL and decodes the
    /// native response of the same dialect.
    ///
    /// `model` names the model for the Google dialects, whose model lives in
    /// the request path rather than the body. Provider-internal retries apply
    /// within this call.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::VariantMismatch`] for a request that is not
    /// one of the five supported generation dialects, the transport or status
    /// error of the exchange, or a decode error when the answer is not the
    /// dialect's native response.
    pub async fn send(&self, model: &str, request: ProviderRequest) -> ProviderResult<ProviderResponse> {
        match request {
            ProviderRequest::OpenAiChatCompletion(body) => self
                .post(&self.url("chat/completions"), &body)
                .await
                .map(ProviderResponse::OpenAiChatCompletion),
            ProviderRequest::OpenAiResponses(body) => self
                .post(&self.url("responses"), &body)
                .await
                .map(ProviderResponse::OpenAiResponses),
            ProviderRequest::AnthropicMessage(body) => self
                .post(&self.url("messages"), &body)
                .await
                .map(ProviderResponse::AnthropicMessage),
            ProviderRequest::GeminiGenerateContent(body) => self
                .post(&self.url(&format!("models/{model}:generateContent")), &body)
                .await
                .map(ProviderResponse::GeminiGenerateContent),
            ProviderRequest::Vertex(body) => self
                .post(&self.url(&format!("models/{model}:generateContent")), &body)
                .await
                .map(ProviderResponse::VertexGenerateContent),
            other => Err(ProviderError::variant_mismatch(
                PROVIDER,
                super::request_variant_label(&other),
            )),
        }
    }

    /// Joins `path` below the base URL's path, keeping any declared prefix.
    fn url(&self, path: &str) -> String {
        format!("{}/{path}", self.base_url.as_str().trim_end_matches('/'))
    }

    /// Posts one JSON body with the configured headers and retry policy.
    ///
    /// # Errors
    ///
    /// Returns the errors of [`super::send_json_with_retry`].
    async fn post<T, R>(&self, url: &str, body: &T) -> ProviderResult<R>
    where
        T: serde::Serialize + ?Sized,
        R: serde::de::DeserializeOwned,
    {
        super::send_json_with_retry(
            &self.transport,
            PROVIDER,
            url,
            self.headers.clone(),
            body,
            &self.retry,
        )
        .await
    }
}
