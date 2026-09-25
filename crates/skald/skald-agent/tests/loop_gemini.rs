use std::sync::Arc;

use skald_agent::{Agent, FinishReason, RunConfig};
use skald_prompt::Prompt;
use skald_runtime::{MockProvider, ProviderRegistry};
use skald_spec::wire::google_generate::{
    GoogleAnswerContent, GoogleCandidate, GoogleContent, GoogleFinishReason,
    GoogleGenerateContentRequest, GoogleGenerateContentResponse, GoogleGenerateSettings,
    GooglePart,
};
use skald_spec::{
    Prompt as SpecPrompt, ProviderName, ProviderRequest, ProviderResponse, ResponseType,
};

fn google_answer(text: &str) -> GoogleAnswerContent {
    GoogleAnswerContent {
        role: Some("model".to_owned()),
        parts: vec![GooglePart::Text {
            text: text.to_owned(),
        }],
    }
}

fn google_content(role: &str, text: &str) -> GoogleContent {
    GoogleContent {
        role: role.to_owned(),
        parts: vec![GooglePart::Text {
            text: text.to_owned(),
        }],
    }
}

fn gemini_request(contents: Vec<GoogleContent>) -> ProviderRequest {
    ProviderRequest::GeminiGenerateContent(GoogleGenerateContentRequest {
        contents,
        system_instruction: Some(google_content("system", "be helpful")),
        tools: None,
        tool_config: None,
        settings: GoogleGenerateSettings::default(),
    })
}

fn gemini_text_response(text: &str) -> ProviderResponse {
    ProviderResponse::GeminiGenerateContent(GoogleGenerateContentResponse {
        candidates: vec![GoogleCandidate {
            content: google_answer(text),
            finish_reason: Some(GoogleFinishReason::Stop),
            index: Some(0),
            safety_ratings: Vec::new(),
            citation_metadata: None,
            grounding_metadata: None,
            avg_logprobs: None,
        }],
        usage_metadata: None,
        model_version: Some("gemini-1.5-pro".to_owned()),
        prompt_feedback: None,
        response_id: None,
        create_time: None,
    })
}

#[tokio::test]
async fn agent_run_executes_gemini_loop_against_mock_provider() {
    let expected_request = gemini_request(vec![google_content("user", "hello")]);
    let mock = MockProvider::new(ProviderName::Google)
        .expect_request(expected_request)
        .respond_with(gemini_text_response("hi"));
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(mock));

    let prompt = Prompt::from_native(
        SpecPrompt::new(
            gemini_request(Vec::new()),
            "gemini-1.5-pro",
            None,
            ResponseType::Text,
        )
        .expect("Gemini prompt should build"),
    );
    let agent = Agent::new(prompt).with_id("g").with_run_config(RunConfig {
        max_iterations: 3,
        ..Default::default()
    });

    let run = agent
        .run_with(&providers, None, "hello")
        .await
        .expect("Gemini loop must run");

    assert_eq!(run.finish_reason, FinishReason::ModelStopped);
    assert_eq!(run.iterations, 1);
    assert_eq!(run.output, "hi");
    assert!(run.final_response.is_some());
}
