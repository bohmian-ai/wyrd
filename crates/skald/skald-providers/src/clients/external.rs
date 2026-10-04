//! Direct client for a user-provided external LLM gateway.
//!
//! An external gateway speaks one provider-native protocol at a declared base
//! URL. This client posts the typed native request body unchanged to the
//! dialect's conventional path below that base URL and decodes the matching
//! native response. Egress runs under an [`EndpointPolicy`] transport, so the
//! endpoint is screened, resolved addresses are pinned, redirects are refused,
//! no proxy is consulted, and answers are read through the transport's body
//! bound. Secret headers are marked sensitive so they never appear in debug
//! output, and no answer, refusal, or decode diagnostic that could carry a
//! sensitive header value is returned.

use reqwest::header::HeaderMap;
use serde_json::Value;
use skald_spec::{ProviderRequest, ProviderResponse};
use url::Url;

use crate::endpoint::EndpointPolicy;
use crate::error::{ProviderError, ProviderResult};
use crate::retry::RetryPolicy;
use crate::transport::HttpTransport;

/// Provider label used in errors raised by external-gateway calls.
const PROVIDER: &str = "ext_gateway";

/// Fixed diagnostic replacing an external gateway's refusal body, which may
/// reflect the bound credential the gateway received.
const WITHHELD_REFUSAL_BODY: &str = "external gateway refusal body withheld";

/// Fixed diagnostic replacing a decode failure's detail, which may quote a
/// response value reflecting the bound credential.
const WITHHELD_DECODE_DETAIL: &str = "external gateway response decode detail withheld";

/// Fixed, non-retryable refusal of a successful answer that reflects a bound
/// credential; it names neither the answer nor the matched value.
const REFLECTED_CREDENTIAL: &str = "external gateway response reflected a bound credential";

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
    /// Authored non-secret headers merged with bound secret headers; the
    /// secret values are the ones marked sensitive, and successful answers
    /// are checked against them.
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
    /// A custom OpenAI-compatible request posts its inner OpenAI Chat body to
    /// the same `chat/completions` path; its provider name only selected the
    /// registry adapter. `model` names the model for the Google dialects, whose
    /// model lives in the request path rather than the body. Provider-internal retries apply
    /// within this call.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::VariantMismatch`] for a request that is not
    /// one of the five supported generation dialects, the transport or status
    /// error of the exchange, or a decode error when the answer is not the
    /// dialect's native response.
    pub async fn send(
        &self,
        model: &str,
        request: ProviderRequest,
    ) -> ProviderResult<ProviderResponse> {
        match request {
            ProviderRequest::OpenAiChatCompletion(body)
            | ProviderRequest::OpenAiChatCompatible { request: body, .. } => self
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
    /// A final non-success answer keeps its status and `Retry-After` hint, so
    /// status-based retry classification is unchanged, but its body is
    /// replaced by [`WITHHELD_REFUSAL_BODY`]: a gateway can echo the bound
    /// secret headers it received, and the error is publicly inspectable. A
    /// decode failure keeps its variant and retryability but its detail is
    /// replaced by [`WITHHELD_DECODE_DETAIL`], because serde quotes offending
    /// values. A decoded success that [reflects a credential](Self::reflects_credential)
    /// is refused after the provider-internal retries, so the refusal itself
    /// is never retried.
    ///
    /// # Errors
    ///
    /// Returns the errors of [`super::send_json_with_retry`], with any
    /// [`ProviderError::Status`] body and [`ProviderError::Decode`] detail
    /// withheld, or the non-retryable [`ProviderError::BadRequest`]
    /// [`REFLECTED_CREDENTIAL`] for a reflecting success.
    async fn post<T, R>(&self, url: &str, body: &T) -> ProviderResult<R>
    where
        T: serde::Serialize + ?Sized,
        R: serde::de::DeserializeOwned + serde::Serialize,
    {
        let response: R = super::send_json_with_retry(
            &self.transport,
            PROVIDER,
            url,
            self.headers.clone(),
            body,
            &self.retry,
        )
        .await
        .map_err(|error| match error {
            ProviderError::Status {
                provider,
                status,
                retry_after_ms,
                ..
            } => ProviderError::Status {
                provider,
                status,
                body: WITHHELD_REFUSAL_BODY.to_owned(),
                retry_after_ms,
            },
            ProviderError::Decode { provider, .. } => ProviderError::Decode {
                provider,
                message: WITHHELD_DECODE_DETAIL.to_owned(),
            },
            other => other,
        })?;
        if self.reflects_credential(&response) {
            return Err(ProviderError::bad_request(PROVIDER, REFLECTED_CREDENTIAL));
        }
        Ok(response)
    }

    /// Returns whether the typed `response` contains a nonempty sensitive
    /// header value in any string or object member name.
    ///
    /// The response is re-encoded from its typed form, so only members the
    /// typed response retained are inspected, and JSON escapes in the raw
    /// answer are already decoded. A response that cannot be re-encoded is
    /// treated as reflecting, failing closed.
    fn reflects_credential(&self, response: &impl serde::Serialize) -> bool {
        let secrets: Vec<&str> = self
            .headers
            .values()
            .filter(|value| value.is_sensitive())
            .filter_map(|value| value.to_str().ok())
            .filter(|value| !value.is_empty())
            .collect();
        if secrets.is_empty() {
            return false;
        }
        serde_json::to_value(response).map_or(true, |value| contains_any(&value, &secrets))
    }
}

/// Returns whether any string or member name within `value` contains one of
/// `needles`.
fn contains_any(value: &Value, needles: &[&str]) -> bool {
    let hit = |text: &str| needles.iter().any(|needle| text.contains(needle));
    match value {
        Value::String(text) => hit(text),
        Value::Array(items) => items.iter().any(|item| contains_any(item, needles)),
        Value::Object(members) => members
            .iter()
            .any(|(name, member)| hit(name) || contains_any(member, needles)),
        Value::Null | Value::Bool(_) | Value::Number(_) => false,
    }
}
