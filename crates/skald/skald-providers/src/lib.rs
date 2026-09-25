//! Native HTTP provider clients for Skald.
//!
//! This crate owns provider HTTP transport, auth, retry, streaming decode, and
//! concrete provider clients. Clients serialize native `skald-spec` request
//! variants directly and decode native response variants.

pub mod auth;
pub mod clients;
pub mod error;
pub mod raw;
pub mod retry;
pub mod stream;
pub mod trait_;
pub mod transport;

pub use clients::{
    AnthropicClient, GoogleClient, MediaAnswer, OpenAiBatchRoute, OpenAiClient, OpenAiMediaRoute,
    OpenAiRoute, ProviderByteStream, UploadContent, UploadFile, VertexClient,
};
pub use error::{ProviderError, ProviderResult};
pub use retry::RetryPolicy;
pub use trait_::{ProviderClient, ProviderStream};
pub use transport::{HttpTransport, TransportConfig};

#[cfg(test)]
#[allow(dead_code)]
pub(crate) mod common {
    use std::time::Duration;

    use wiremock::MockServer;

    use crate::auth::{AnthropicAuth, GoogleApiKeyAuth, GoogleOAuth, OpenAiAuth, VertexAuth};
    use crate::{
        AnthropicClient, GoogleClient, HttpTransport, OpenAiClient, RetryPolicy, TransportConfig,
        VertexClient,
    };

    pub fn fixture(path: &str) -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join(path);
        std::fs::read_to_string(path)
            .expect("fixture exists")
            .trim_end_matches('\n')
            .to_owned()
    }

    pub fn retry_policy() -> RetryPolicy {
        RetryPolicy {
            max_attempts: 2,
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(1),
            jitter: false,
        }
    }

    pub fn transport() -> HttpTransport {
        HttpTransport::new(TransportConfig {
            timeout: Duration::from_secs(5),
            connect_timeout: Duration::from_secs(1),
            pool_max_idle_per_host: 2,
            user_agent: "skald-providers-test".to_owned(),
            ..TransportConfig::default()
        })
        .expect("test transport builds")
    }

    pub fn openai_client(base_url: &str) -> OpenAiClient {
        OpenAiClient::with_transport(
            OpenAiAuth::new("sk-test").with_base_url(base_url),
            transport(),
            retry_policy(),
        )
    }

    pub fn anthropic_client(base_url: &str) -> AnthropicClient {
        AnthropicClient::with_transport(
            AnthropicAuth::new("sk-ant-test").with_base_url(base_url),
            transport(),
            retry_policy(),
        )
    }

    pub fn google_client(base_url: &str, model: &str) -> GoogleClient {
        GoogleClient::with_transport(
            GoogleApiKeyAuth::new("google-test").with_base_url(base_url),
            model,
            transport(),
            retry_policy(),
        )
    }

    pub fn vertex_client(base_url: &str, model: &str) -> VertexClient {
        let oauth = GoogleOAuth::from_account_json(
            r#"{"access_token":"vertex-test-token","expires_in":3600}"#,
        )
        .expect("oauth builds");
        VertexClient::with_transport(
            VertexAuth::new("project-a", "us-central1", oauth).with_base_url(base_url),
            model,
            transport(),
            retry_policy(),
        )
    }

    pub async fn assert_received_body(server: &MockServer, expected: &str) {
        let requests = server
            .received_requests()
            .await
            .expect("wiremock records received requests");
        let actual = String::from_utf8(requests[0].body.clone()).expect("body is UTF-8");
        let actual_json: serde_json::Value =
            serde_json::from_str(&actual).expect("actual body is JSON");
        let expected_json: serde_json::Value =
            serde_json::from_str(expected).expect("expected body is JSON");
        assert_eq!(actual_json, expected_json);
    }
}
