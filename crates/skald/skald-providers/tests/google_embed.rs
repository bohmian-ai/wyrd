mod common;

use skald_providers::ProviderClient;
use skald_spec::{GoogleBatchEmbedRequest, ProviderRequest, ProviderResponse};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn google_embeddings_request_body_matches_fixture_and_response_parses() {
    let server = MockServer::start().await;
    let request_body = common::fixture("google/batch_embed_request.json");
    let response_body = common::fixture("google/batch_embed_response.json");
    Mock::given(method("POST"))
        .and(path("/v1beta/models/text-embedding-004:batchEmbedContents"))
        .and(query_param("key", "google-test"))
        .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
        .mount(&server)
        .await;

    let request: GoogleBatchEmbedRequest =
        serde_json::from_str(&request_body).expect("fixture parses");
    let response = common::google_client(&server.uri(), "text-embedding-004")
        .send(ProviderRequest::GoogleBatchEmbed(request))
        .await
        .expect("request succeeds");

    assert!(matches!(response, ProviderResponse::GoogleBatchEmbed(_)));
    common::assert_received_body(&server, &request_body).await;
}
