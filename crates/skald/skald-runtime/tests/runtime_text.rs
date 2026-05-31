mod support;

use skald_runtime::MockProvider;
use skald_spec::ProviderName;
use support::{
    anthropic_request, anthropic_response, google_request, google_response, openai_request,
    openai_response, runtime_with,
};

#[tokio::test]
async fn mock_openai_response_text_is_readable_through_adapter() {
    let request = openai_request("hello");
    let mock = MockProvider::new(ProviderName::OpenAi)
        .expect_request(request.clone())
        .respond_with(openai_response("openai text"));
    let runtime = runtime_with(&mock);

    let response = runtime.send(request).await.expect("runtime succeeds");

    assert_eq!(response.adapter().text().as_deref(), Some("openai text"));
}

#[tokio::test]
async fn mock_anthropic_response_text_is_readable_through_adapter() {
    let request = anthropic_request("hello");
    let mock = MockProvider::new(ProviderName::Anthropic)
        .expect_request(request.clone())
        .respond_with(anthropic_response("anthropic text"));
    let runtime = runtime_with(&mock);

    let response = runtime.send(request).await.expect("runtime succeeds");

    assert_eq!(response.adapter().text().as_deref(), Some("anthropic text"));
}

#[tokio::test]
async fn mock_google_response_text_is_readable_through_adapter() {
    let request = google_request("hello");
    let mock = MockProvider::new(ProviderName::Google)
        .expect_request(request.clone())
        .respond_with(google_response("google text"));
    let runtime = runtime_with(&mock);

    let response = runtime.send(request).await.expect("runtime succeeds");

    assert_eq!(response.adapter().text().as_deref(), Some("google text"));
}
