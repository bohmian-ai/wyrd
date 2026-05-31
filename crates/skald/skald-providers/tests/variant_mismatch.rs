mod common;

use skald_providers::ProviderClient;
use skald_spec::{AnthropicMessagesRequest, ProviderRequest};

#[tokio::test]
async fn wrong_provider_request_variant_returns_variant_mismatch() {
    let request_body = common::fixture("anthropic/messages_request.json");
    let request: AnthropicMessagesRequest =
        serde_json::from_str(&request_body).expect("fixture parses");

    let error = common::openai_client("http://localhost")
        .send(ProviderRequest::AnthropicMessage(request))
        .await
        .expect_err("variant mismatch");

    assert_eq!(error.code(), "SKALD_PROVIDERS_400_VARIANT_MISMATCH");
}
