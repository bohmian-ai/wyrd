mod support;

use skald_providers::ProviderStream;
use skald_runtime::{MockProvider, Provider};
use skald_spec::{AnthropicStreamEvent, ProviderName};
use support::{anthropic_request, openai_request, openai_response};

#[tokio::test]
async fn mock_queues_responses_in_order() {
    let mock = MockProvider::new(ProviderName::OpenAi);
    mock.push_response(openai_response("first"));
    mock.push_response(openai_response("second"));

    let first = mock
        .send(openai_request("one"))
        .await
        .expect("first response");
    let second = mock
        .send(openai_request("two"))
        .await
        .expect("second response");

    assert_eq!(first.adapter().text().as_deref(), Some("first"));
    assert_eq!(second.adapter().text().as_deref(), Some("second"));
    assert_eq!(mock.remaining(), 0);
}

#[tokio::test]
async fn mock_unexpected_request_returns_provider_error() {
    let expected = openai_request("expected");
    let actual = openai_request("actual");
    let mock = MockProvider::new(ProviderName::OpenAi)
        .expect_request(expected)
        .respond_with(openai_response("unused"));

    let err = mock.send(actual).await.expect_err("mismatch fails");

    assert_eq!(err.code(), "SKALD_PROVIDERS_400_BAD_REQUEST");
    assert!(err.to_string().contains("mock request mismatch"));
}

#[tokio::test]
async fn mock_supports_stream_chunks() {
    let mock = MockProvider::new(ProviderName::Anthropic);
    mock.push_stream(ProviderStream::Anthropic(vec![
        AnthropicStreamEvent::Ping {},
    ]));

    let stream = mock
        .stream(anthropic_request("stream"))
        .await
        .expect("stream response");

    assert!(matches!(
        stream,
        ProviderStream::Anthropic(events) if matches!(events.as_slice(), [AnthropicStreamEvent::Ping {}])
    ));
}
