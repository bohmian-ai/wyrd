//! TypeSafe Jev `systemone` client.
//!
//! Jev answers keyed questions over one non-streaming route,
//! `POST {base}/v1/systemone`, authenticated with a bearer API key. The client
//! sends a typed [`skald_spec::JevRequest`] and decodes the typed
//! [`skald_spec::JevResponse`], exactly as the other provider clients carry
//! their native wire structs.

use async_trait::async_trait;
use skald_spec::{ProviderRequest, ProviderResponse};

use crate::auth::OpenAiAuth;
use crate::error::{ProviderError, ProviderResult};
use crate::retry::RetryPolicy;
use crate::trait_::{ProviderClient, ProviderStream};
use crate::transport::{HttpTransport, TransportConfig};

/// Production TypeSafe API base URL.
pub const JEV_BASE_URL: &str = "https://api.typesafe.ai";

/// Environment variable holding the TypeSafe API key [`JevClient::from_env`]
/// reads.
pub const JEV_API_KEY_VARIABLE: &str = "TYPESAFE_API_KEY";

/// Provider label on every error this client raises.
const PROVIDER: &str = "jev";

/// Jev provider client: bearer auth, a shared transport, and a retry policy.
#[derive(Debug, Clone)]
pub struct JevClient {
    /// Bearer credential and base URL.
    auth: OpenAiAuth,
    /// Policy-enforcing HTTP transport.
    transport: HttpTransport,
    /// Retry policy of each exchange.
    retry: RetryPolicy,
}

impl JevClient {
    /// Bearer auth presenting `api_key` to `base_url`, or to
    /// [`JEV_BASE_URL`] when `base_url` is `None`.
    #[must_use]
    pub fn auth(api_key: impl Into<String>, base_url: Option<&str>) -> OpenAiAuth {
        OpenAiAuth::new(api_key).with_base_url(base_url.unwrap_or(JEV_BASE_URL))
    }

    /// Creates a client from explicit auth with the default transport and
    /// retry policy.
    ///
    /// # Errors
    ///
    /// Returns the transport construction error.
    pub fn new(auth: OpenAiAuth) -> ProviderResult<Self> {
        Ok(Self::with_transport(
            auth,
            HttpTransport::new(TransportConfig::default())?,
            RetryPolicy::default(),
        ))
    }

    /// Creates a production client keyed by [`JEV_API_KEY_VARIABLE`].
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Auth`] when the variable is unset or empty,
    /// and the transport construction error.
    pub fn from_env() -> ProviderResult<Self> {
        let key = std::env::var(JEV_API_KEY_VARIABLE)
            .ok()
            .filter(|key| !key.is_empty())
            .ok_or_else(|| {
                ProviderError::auth(PROVIDER, format!("{JEV_API_KEY_VARIABLE} is not set"))
            })?;
        Self::new(Self::auth(key, None))
    }

    /// Creates a client over a shared transport and retry policy; `auth`
    /// carries the bearer key and base URL, usually built by [`Self::auth`].
    #[must_use]
    pub const fn with_transport(
        auth: OpenAiAuth,
        transport: HttpTransport,
        retry: RetryPolicy,
    ) -> Self {
        Self {
            auth,
            transport,
            retry,
        }
    }

    /// URL of the `systemone` route; every request builds its URL here.
    fn systemone_url(&self) -> String {
        format!(
            "{}/v1/systemone",
            self.auth.base_url().trim_end_matches('/')
        )
    }

    /// Sends a typed Jev request and decodes the typed answer.
    ///
    /// The request is sent as given and is not validated here: a native
    /// `systemone` caller checks it with [`skald_spec::JevRequest::validate`]
    /// first, while the unified chat pass-through forwards it unvalidated.
    /// Retries follow the client's policy.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::VariantMismatch`] for any request other than
    /// [`ProviderRequest::JevSystemOne`], [`ProviderError::Status`] for a
    /// non-success answer, [`ProviderError::Decode`] when the answer is not a
    /// Jev response or the credential is not a valid header, and
    /// [`ProviderError::Connect`], [`ProviderError::Timeout`], or
    /// [`ProviderError::Upstream`] when the exchange fails.
    pub async fn send_native(&self, request: ProviderRequest) -> ProviderResult<ProviderResponse> {
        let ProviderRequest::JevSystemOne(request) = request else {
            return Err(ProviderError::variant_mismatch(
                PROVIDER,
                super::request_variant_label(&request),
            ));
        };
        let response = super::send_json_with_retry(
            &self.transport,
            PROVIDER,
            &self.systemone_url(),
            self.auth.headers()?,
            &request,
            &self.retry,
        )
        .await?;
        Ok(ProviderResponse::JevSystemOne(response))
    }
}

#[async_trait]
impl ProviderClient for JevClient {
    /// Sends one Jev request through [`JevClient::send_native`], the single
    /// owner of the Jev exchange.
    ///
    /// # Errors
    ///
    /// Forwards every [`JevClient::send_native`] failure unchanged:
    /// [`ProviderError::VariantMismatch`] for a non-Jev request,
    /// [`ProviderError::Status`] for a non-success answer,
    /// [`ProviderError::Decode`] for an undecodable answer or invalid
    /// credential header, and [`ProviderError::Connect`],
    /// [`ProviderError::Timeout`], or [`ProviderError::Upstream`] when the
    /// exchange fails.
    async fn send(&self, request: ProviderRequest) -> ProviderResult<ProviderResponse> {
        self.send_native(request).await
    }

    /// Jev never streams.
    ///
    /// # Errors
    ///
    /// Always returns [`ProviderError::BadRequest`].
    async fn stream(&self, _request: ProviderRequest) -> ProviderResult<ProviderStream> {
        Err(ProviderError::bad_request(PROVIDER, "jev does not stream"))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use skald_spec::{JevAnswer, JevRequest};
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::common;

    /// Builds a client of `server` with the test key.
    fn client(server: &MockServer) -> JevClient {
        JevClient::with_transport(
            JevClient::auth("jev-test", Some(&server.uri())),
            common::transport(),
            common::retry_policy(),
        )
    }

    /// The documented request body.
    fn request() -> JevRequest {
        serde_json::from_value(json!({"model": "jev-latest", "state": "s",
            "questions": {"q": {"type": "noul", "instructions": "x"}}}))
        .expect("request decodes")
    }

    /// The typed body reaches `/v1/systemone` with bearer auth and the
    /// answer decodes typed.
    #[tokio::test]
    async fn systemone_posts_bearer_body_and_decodes_the_answer() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .and(header("authorization", "Bearer jev-test"))
            .and(body_json(request()))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "model": "jev-1.13.0",
                "answers": {"q": {"type": "noul", "noul": 0.9}},
                "usage": {"input_tokens": 3, "output_tokens": 1}
            })))
            .expect(1)
            .mount(&server)
            .await;

        let response = client(&server)
            .send(ProviderRequest::JevSystemOne(request()))
            .await
            .expect("answers");
        let ProviderResponse::JevSystemOne(answer) = response else {
            panic!("a Jev request answers a Jev response");
        };
        assert_eq!(answer.answers["q"], JevAnswer::Noul { noul: 0.9 });
    }

    /// A validation refusal is a status error, and another dialect never
    /// reaches the provider.
    #[tokio::test]
    async fn refusal_is_a_status_error_and_other_dialects_mismatch() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(400).set_body_string(
                r#"{"detail":{"error_type":"api_usage_error","message":"Invalid request."}}"#,
            ))
            .expect(1)
            .mount(&server)
            .await;

        let error = client(&server)
            .send(ProviderRequest::JevSystemOne(request()))
            .await
            .expect_err("refused");
        assert!(matches!(error, ProviderError::Status { status: 400, .. }));
        let error = client(&server)
            .send(ProviderRequest::RawV1 {
                provider: skald_spec::ProviderName::Jev,
                body: json!({}),
            })
            .await
            .expect_err("mismatch");
        assert!(matches!(error, ProviderError::VariantMismatch { .. }));
    }
}
