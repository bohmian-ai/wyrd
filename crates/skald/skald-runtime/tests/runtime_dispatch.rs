mod support;

use serde_json::value::to_raw_value;
use skald_runtime::{MockProvider, ProviderRegistry, SkaldRuntimeError, dispatch};
use skald_spec::{ProviderName, ProviderRequest, ProviderResponse};
use support::{openai_request, openai_response, runtime_with};

#[tokio::test]
async fn routes_typed_request_by_provider_name() {
    let request = openai_request("hello");
    let response = openai_response("world");
    let mock = MockProvider::new(ProviderName::OpenAi)
        .expect_request(request.clone())
        .respond_with(response.clone());
    let runtime = runtime_with(&mock);

    assert_eq!(
        runtime.send(request).await.expect("dispatch succeeds"),
        response
    );
    assert_eq!(mock.remaining(), 0);
}

#[tokio::test]
async fn raw_v1_uses_embedded_provider_for_dispatch() {
    let body = to_raw_value(&serde_json::json!({"custom": true})).expect("raw value builds");
    let request = ProviderRequest::RawV1 {
        provider: ProviderName::Custom("acme".to_owned()),
        body,
    };
    let response =
        ProviderResponse::RawV1(to_raw_value(&serde_json::json!({"ok": true})).expect("raw"));
    let mock = MockProvider::new(ProviderName::Custom("acme".to_owned()))
        .expect_request(request.clone())
        .respond_with(response.clone());
    let runtime = runtime_with(&mock);

    assert_eq!(
        runtime.send(request).await.expect("raw dispatch succeeds"),
        response
    );
}

#[tokio::test]
async fn unregistered_provider_returns_runtime_error() {
    let err = dispatch(&ProviderRegistry::new(), openai_request("hello"))
        .await
        .expect_err("provider is missing");

    assert!(matches!(
        err,
        SkaldRuntimeError::ProviderNotRegistered {
            provider: ProviderName::OpenAi
        }
    ));
    assert_eq!(err.code(), "SKALD_RUNTIME_404_PROVIDER");
}
