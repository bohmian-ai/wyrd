mod common;

use skald_providers::ProviderClient;
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
