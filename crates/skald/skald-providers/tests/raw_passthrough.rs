mod common;

use serde_json::value::RawValue;
use skald_providers::ProviderClient;
use skald_spec::{ProviderName, ProviderRequest, ProviderResponse};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn raw_v1_writes_body_bytes_directly_and_returns_raw_response() {
    let server = MockServer::start().await;
    let request_body = common::fixture("raw_v1/unknown_provider_request.json");
    let response_body = common::fixture("raw_v1/unknown_provider_response.json");
    Mock::given(method("POST"))
        .and(path("/raw"))
        .respond_with(ResponseTemplate::new(200).set_body_string(response_body.clone()))
        .mount(&server)
        .await;
    let body = RawValue::from_string(request_body.clone()).expect("raw request parses");

    let response = common::openai_client(&server.uri())
        .send(ProviderRequest::RawV1 {
            provider: ProviderName::OpenAi,
            body,
        })
        .await
        .expect("request succeeds");

    let ProviderResponse::RawV1(body) = response else {
        panic!("expected raw response");
    };
    assert_eq!(body.get(), response_body);
    common::assert_received_body(&server, &request_body).await;
}
