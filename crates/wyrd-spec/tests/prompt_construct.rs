mod prompt_support;

use prompt_support::{
    anthropic_request, google_request, openai_chat_request, openai_responses_request, prompt_spec,
    raw_request, vertex_request,
};
use skald_spec::ProviderName;

#[test]
fn new_provider_variants_build() {
    for request in [
        openai_chat_request("hello"),
        openai_responses_request("hello"),
        anthropic_request("hello"),
        google_request("hello"),
        vertex_request("hello"),
    ] {
        let spec = prompt_spec(request, Vec::new());
        assert!(spec.parameters().is_empty());
    }
}

#[test]
fn new_raw_v1_builds_for_every_provider_name() {
    for provider in [
        ProviderName::OpenAi,
        ProviderName::Anthropic,
        ProviderName::Google,
        ProviderName::Vertex,
        ProviderName::Custom("acme".to_owned()),
    ] {
        let spec = prompt_spec(raw_request(provider), Vec::new());
        assert!(spec.is_fully_bound());
    }
}

#[test]
fn parameters_typed_view_returns_validated_names() {
    let spec = prompt_spec(
        openai_chat_request("hello {{name}} from {{city}}"),
        vec!["name", "city"],
    );
    let parameters: Vec<_> = spec
        .parameters()
        .into_iter()
        .map(|name| name.to_string())
        .collect();

    assert_eq!(parameters, vec!["name", "city"]);
}
