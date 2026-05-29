mod common;

use skald_providers::ProviderClient;
use skald_spec::{ProviderRequest, ProviderResponse, VertexGenerateContentRequest};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn vertex_generate_request_body_matches_fixture_and_path_has_projectlocation() {
    let server = MockServer::start().await;
    let request_body = common::fixture("vertex/generate_request.json");
    let response_body = common::fixture("vertex/generate_response.json");
    Mock::given(method("POST"))
        .and(path("/v1/projects/project-a/locations/us-central1/publishers/google/models/gemini-2.5-flash:generateContent"))
        .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
        .mount(&server)
        .await;

    let request: VertexGenerateContentRequest =
        serde_json::from_str(&request_body).expect("fixture parses");
    let response = common::vertex_client(&server.uri(), "gemini-2.5-flash")
        .send(ProviderRequest::Vertex(request))
        .await
        .expect("request succeeds");

    assert!(matches!(
        response,
        ProviderResponse::VertexGenerateContent(_)
    ));
    common::assert_received_body(&server, &request_body).await;
}
