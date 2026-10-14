//! TypeSafe Jev `systemone` client.
//!
//! Jev answers keyed questions over one non-streaming route,
//! `POST {base}/v1/systemone`, authenticated with a bearer API key. Callers
//! submit a validated JSON body and receive the answer bytes unchanged, so the
//! gateway can relay native answers intact or translate them itself.

use serde_json::value::RawValue;

use crate::auth::OpenAiAuth;
use crate::error::ProviderResult;
use crate::raw;
use crate::retry::RetryPolicy;
use crate::transport::HttpTransport;

/// Production TypeSafe API base URL.
pub const JEV_BASE_URL: &str = "https://api.typesafe.ai";

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

    /// Posts a `systemone` `body` and returns the answer bytes unchanged.
    ///
    /// The body is sent as given; callers validate it first. Retries follow
    /// the client's policy.
    ///
    /// # Errors
    ///
    /// Returns [`crate::ProviderError::Status`] for a non-success answer,
    /// [`crate::ProviderError::Decode`] when the answer is not JSON or the
    /// credential is not a valid header, and [`crate::ProviderError::Connect`],
    /// [`crate::ProviderError::Timeout`], or [`crate::ProviderError::Upstream`]
    /// when the exchange fails.
    pub async fn send_raw(&self, body: &RawValue) -> ProviderResult<Box<RawValue>> {
        raw::send_raw(
            &self.transport,
            "jev",
            &self.systemone_url(),
            self.auth.headers()?,
            body,
            &self.retry,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use serde_json::value::to_raw_value;
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::{ProviderError, common};

    /// Builds a client of `server` with the test key.
    fn client(server: &MockServer) -> JevClient {
        JevClient::with_transport(
            JevClient::auth("jev-test", Some(&server.uri())),
            common::transport(),
            common::retry_policy(),
        )
    }

    /// The body reaches `/v1/systemone` with bearer auth and the answer
    /// returns byte for byte.
    #[tokio::test]
    async fn systemone_posts_bearer_body_and_returns_answer_bytes() {
        let server = MockServer::start().await;
        let request = json!({"model": "jev-latest", "state": "s",
                             "questions": {"q": {"type": "noul", "instructions": "x"}}});
        let answer = r#"{"model":"jev-1.13.0","answers":{"q":{"type":"noul","noul":0.9}},"usage":{"input_tokens":3,"output_tokens":1}}"#;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .and(header("authorization", "Bearer jev-test"))
            .and(body_json(&request))
            .respond_with(ResponseTemplate::new(200).set_body_string(answer))
            .expect(1)
            .mount(&server)
            .await;

        let body = to_raw_value(&request).expect("raw body");
        let response = client(&server).send_raw(&body).await.expect("answers");
        assert_eq!(response.get(), answer);
    }

    /// A validation refusal is a status error, not retried.
    #[tokio::test]
    async fn refusal_is_a_status_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(422).set_body_string(r#"{"detail":"bad"}"#))
            .expect(1)
            .mount(&server)
            .await;

        let body = to_raw_value(&json!({})).expect("raw body");
        let error = client(&server).send_raw(&body).await.expect_err("refused");
        assert!(matches!(error, ProviderError::Status { status: 422, .. }));
    }
}
