#![allow(dead_code)]

use std::time::Duration;

use skald_providers::auth::{AnthropicAuth, GoogleApiKeyAuth, GoogleOAuth, OpenAiAuth, VertexAuth};
use skald_providers::{
    AnthropicClient, GoogleClient, HttpTransport, OpenAiClient, RetryPolicy, TransportConfig,
    VertexClient,
};
use wiremock::MockServer;

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
    })
    .expect("test transport builds")
}

pub fn openai_client(base_url: &str) -> OpenAiClient {
    OpenAiClient::new(OpenAiAuth::new("sk-test").with_base_url(base_url))
        .expect("client builds")
        .with_transport_and_retry(transport(), retry_policy())
}

pub fn anthropic_client(base_url: &str) -> AnthropicClient {
    AnthropicClient::new(AnthropicAuth::new("sk-ant-test").with_base_url(base_url))
        .expect("client builds")
        .with_transport_and_retry(transport(), retry_policy())
}

pub fn google_client(base_url: &str, model: &str) -> GoogleClient {
    GoogleClient::new(
        GoogleApiKeyAuth::new("google-test").with_base_url(base_url),
        model,
    )
    .expect("client builds")
    .with_transport_and_retry(transport(), retry_policy())
}

pub fn vertex_client(base_url: &str, model: &str) -> VertexClient {
    let oauth =
        GoogleOAuth::from_account_json(r#"{"access_token":"vertex-test-token","expires_in":3600}"#)
            .expect("oauth builds");
    VertexClient::new(
        VertexAuth::new("project-a", "us-central1", oauth).with_base_url(base_url),
        model,
    )
    .expect("client builds")
    .with_transport_and_retry(transport(), retry_policy())
}

pub async fn assert_received_body(server: &MockServer, expected: &str) {
    let requests = server
        .received_requests()
        .await
        .expect("wiremock records received requests");
    let actual = String::from_utf8(requests[0].body.clone()).expect("body is UTF-8");
    assert_eq!(actual, expected);
}
