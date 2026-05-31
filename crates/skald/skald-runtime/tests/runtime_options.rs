mod support;

use skald_runtime::{MockProvider, ProviderRegistry, RuntimeConfig};
use skald_spec::{ProviderName, ProviderRequest};
use support::{
    google_request_with_options, openai_request_with_options, openai_response, runtime_with,
};

#[tokio::test]
async fn request_options_round_trip_through_mock_dispatch() {
    let request = openai_request_with_options();
    let mock = MockProvider::new(ProviderName::OpenAi)
        .expect_request(request.clone())
        .respond_with(openai_response("ok"));
    let runtime = runtime_with(&mock);

    runtime
        .send(request)
        .await
        .expect("options request dispatches");
}

#[test]
fn runtime_config_defaults_are_explicit() {
    let config = RuntimeConfig::default();

    assert_eq!(config.cache_capacity, 1024);
    assert_eq!(config.max_retries, 2);
    assert!(!config.default_google_model.trim().is_empty());
}

#[test]
fn registry_reports_registered_provider_names() {
    let mut registry = ProviderRegistry::new();
    registry.register(std::sync::Arc::new(MockProvider::new(ProviderName::OpenAi)));

    let names: Vec<_> = registry.names().cloned().collect();

    assert_eq!(names, vec![ProviderName::OpenAi]);
}

#[test]
fn google_cached_content_option_stays_native() {
    let request = google_request_with_options();

    assert!(matches!(
        request,
        ProviderRequest::GeminiGenerateContent(inner)
            if inner.settings.cached_content.as_deref() == Some("cachedContents/abc")
    ));
}
