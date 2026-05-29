mod common;

use skald_providers::ProviderClient;
use skald_spec::{OpenAiChatRequest, ProviderRequest, ProviderResponse};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn chat_request_body_matches_fixture_and_response_parses() {
    let server = MockServer::start().await;
    let request_body = common::fixture("openai/chat_completion_request.json");
    let response_body = common::fixture("openai/chat_completion_response.json");
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
        .mount(&server)
        .await;

    let request: OpenAiChatRequest = serde_json::from_str(&request_body).expect("fixture parses");
    let response = common::openai_client(&server.uri())
        .send(ProviderRequest::OpenAiChatCompletion(request))
        .await
        .expect("request succeeds");

    assert!(matches!(
        response,
        ProviderResponse::OpenAiChatCompletion(_)
    ));
    common::assert_received_body(&server, &request_body).await;
}

#[tokio::test]
async fn chat_tool_calls_response_parses() {
    let server = MockServer::start().await;
    let request_body = common::fixture("openai/chat_completion_request.json");
    let response_body = common::fixture("openai/chat_completion_tool_call_response.json");
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
        .mount(&server)
        .await;

    let request: OpenAiChatRequest = serde_json::from_str(&request_body).expect("fixture parses");
    let response = common::openai_client(&server.uri())
        .send(ProviderRequest::OpenAiChatCompletion(request))
        .await
        .expect("request succeeds");

    assert_eq!(response.adapter().tool_calls().len(), 1);
}

#[tokio::test]
async fn chat_429_retries_then_succeeds() {
    let server = MockServer::start().await;
    let request_body = common::fixture("openai/chat_completion_request.json");
    let response_body = common::fixture("openai/chat_completion_response.json");
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "0"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
        .mount(&server)
        .await;

    let request: OpenAiChatRequest = serde_json::from_str(&request_body).expect("fixture parses");
    let response = common::openai_client(&server.uri())
        .send(ProviderRequest::OpenAiChatCompletion(request))
        .await
        .expect("retry succeeds");

    assert!(matches!(
        response,
        ProviderResponse::OpenAiChatCompletion(_)
    ));
}
