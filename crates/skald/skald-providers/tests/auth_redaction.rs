use skald_providers::auth::{AnthropicAuth, GoogleApiKeyAuth, GoogleOAuth, OpenAiAuth, VertexAuth};

#[test]
fn auth_debug_redacts_secrets() {
    let openai = format!("{:?}", OpenAiAuth::new("openai-secret"));
    let anthropic = format!("{:?}", AnthropicAuth::new("anthropic-secret"));
    let google = format!("{:?}", GoogleApiKeyAuth::new("google-secret"));
    let oauth =
        GoogleOAuth::from_account_json(r#"{"access_token":"oauth-secret","expires_in":3600}"#)
            .expect("oauth builds");
    let vertex = format!("{:?}", VertexAuth::new("project", "location", oauth));

    assert!(openai.contains("<redacted>"));
    assert!(!openai.contains("openai-secret"));
    assert!(!anthropic.contains("anthropic-secret"));
    assert!(!google.contains("google-secret"));
    assert!(!vertex.contains("oauth-secret"));
}
