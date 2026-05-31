mod support;

use skald_runtime::MockProvider;
use skald_spec::ProviderName;
use support::{openai_request, openai_response, runtime_with};
use tracing::Instrument;

#[tokio::test]
async fn send_records_usage_accessible_from_native_response() {
    let request = openai_request("hello");
    let mock = MockProvider::new(ProviderName::OpenAi)
        .expect_request(request.clone())
        .respond_with(openai_response("world"));
    let runtime = runtime_with(&mock);
    let span = tracing::info_span!("test_runtime_send");

    let response = async { runtime.send(request).await }
        .instrument(span)
        .await
        .expect("runtime succeeds");
    let usage = response.adapter().usage().expect("usage is present");

    assert_eq!(usage.usage.input_tokens, 3);
    assert_eq!(usage.usage.output_tokens, 5);
}
