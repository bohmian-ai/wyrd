mod common;

use skald_providers::ProviderClient;
use skald_spec::{ProviderRequest, ProviderResponse, VertexPredictRequest};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn vertex_predict_embedding_request_body_matches_fixture_and_response_parses() {
    let server = MockServer::start().await;
    let request_body = common::fixture("vertex/predict_request.json");
    let response_body = common::fixture("vertex/predict_response.json");
    Mock::given(method("POST"))
        .and(path("/v1/projects/project-a/locations/us-central1/publishers/google/models/text-embedding-004:predict"))
        .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
        .mount(&server)
        .await;

    let request: VertexPredictRequest =
        serde_json::from_str(&request_body).expect("fixture parses");
    let response = common::vertex_client(&server.uri(), "text-embedding-004")
        .send(ProviderRequest::VertexPredict(request))
        .await
        .expect("request succeeds");

    assert!(matches!(response, ProviderResponse::VertexPredict(_)));
    common::assert_received_body(&server, &request_body).await;
}
