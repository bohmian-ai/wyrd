mod common;

use skald_providers::ProviderClient;
use skald_spec::{GoogleGenerateContentRequest, ProviderRequest, ProviderResponse};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn generate_request_body_matches_fixture_and_response_parses() {
    let server = MockServer::start().await;
    let request_body = common::fixture("google/generate_content_request.json");
    let response_body = common::fixture("google/generate_content_response.json");
    Mock::given(method("POST"))
        .and(path("/v1beta/models/gemini-2.5-flash:generateContent"))
        .and(query_param("key", "google-test"))
        .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
        .mount(&server)
        .await;

    let request: GoogleGenerateContentRequest =
        serde_json::from_str(&request_body).expect("fixture parses");
    let response = common::google_client(&server.uri(), "gemini-2.5-flash")
        .send(ProviderRequest::GeminiGenerateContent(request))
        .await
        .expect("request succeeds");

    assert!(matches!(
        response,
        ProviderResponse::GeminiGenerateContent(_)
    ));
    assert_eq!(response.adapter().tool_calls().len(), 1);
    common::assert_received_body(&server, &request_body).await;
}
