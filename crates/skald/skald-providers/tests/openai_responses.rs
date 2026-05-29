mod common;

use skald_providers::ProviderClient;
use skald_spec::{OpenAiResponsesRequest, ProviderRequest, ProviderResponse};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn responses_request_body_matches_fixture_and_response_parses() {
    let server = MockServer::start().await;
    let request_body = common::fixture("openai/responses_request.json");
    let response_body = common::fixture("openai/responses_response.json");
    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
        .mount(&server)
        .await;

    let request: OpenAiResponsesRequest =
        serde_json::from_str(&request_body).expect("fixture parses");
    let response = common::openai_client(&server.uri())
        .send(ProviderRequest::OpenAiResponses(request))
        .await
        .expect("request succeeds");

    assert!(matches!(response, ProviderResponse::OpenAiResponses(_)));
    common::assert_received_body(&server, &request_body).await;
}
