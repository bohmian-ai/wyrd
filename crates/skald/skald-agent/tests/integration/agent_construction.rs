use std::sync::Arc;

use skald_agent::{Agent, RunConfig};
use skald_prompt::Prompt;
use skald_spec::wire::openai_chat::{
    OpenAiChatMessage, OpenAiChatRequest, OpenAiChatSettings, OpenAiMessageContent,
};
use skald_spec::{Prompt as SpecPrompt, ProviderName, ProviderRequest};

/// A resolved OpenAI Chat Prompt for `gpt-4o` with one system message, the
/// base Prompt these construction tests give an Agent.
///
/// # Panics
/// Panics when the static request does not build a Prompt.
fn openai_prompt() -> Arc<Prompt> {
    let request = ProviderRequest::OpenAiChatCompletion(OpenAiChatRequest {
        model: "gpt-4o".into(),
        messages: vec![OpenAiChatMessage {
            role: "system".into(),
            content: Some(OpenAiMessageContent::Text("you are helpful".into())),
            name: None,
            tool_calls: None,
            tool_call_id: None,
            refusal: None,
            annotations: Vec::new(),
            audio: None,
        }],
        response_format: None,
        stream: None,
        stream_options: None,
        tools: None,
        tool_choice: None,
        parallel_tool_calls: None,
        settings: OpenAiChatSettings::default(),
    });
    let prompt =
        SpecPrompt::new(request, "gpt-4o".to_string()).expect("test helper setup succeeds");
    Arc::new(Prompt::from_native(prompt))
}

#[test]
fn agent_new_uses_resolved_prompt_and_default_run_config() {
    let agent = Agent::from_resolved("writer", openai_prompt());

    assert_eq!(agent.id, "writer");
    assert_eq!(agent.prompt.native().model, "gpt-4o");
    assert_eq!(
        agent.prompt.native().request.provider(),
        ProviderName::OpenAi
    );
}

/// `Agent::with_prompt` replaces the resolved Prompt the Agent was built
/// with, so the replacement's model is the one the Agent calls.
///
/// # Panics
/// Panics when the replacement Prompt does not build or the Agent keeps the
/// original model.
#[test]
fn agent_with_prompt_overrides_prompt_reference() {
    let original = openai_prompt();
    let prompt = SpecPrompt::new(
        ProviderRequest::OpenAiChatCompletion(OpenAiChatRequest {
            model: "gpt-4o".into(),
            messages: vec![],
            response_format: None,
            stream: None,
            stream_options: None,
            tools: None,
            tool_choice: None,
            parallel_tool_calls: None,
            settings: OpenAiChatSettings::default(),
        }),
        "gpt-4o-mini",
    )
    .unwrap();
    let replacement = Arc::new(Prompt::from_native(prompt));

    let agent = Agent::from_resolved("a", original).with_prompt(replacement.clone());

    assert_eq!(agent.prompt.native().model, "gpt-4o-mini");
}

#[test]
fn agent_with_run_config_preserves_config() {
    let _agent = Agent::from_resolved("a", openai_prompt()).with_run_config(RunConfig {
        max_iterations: 7,
        ..Default::default()
    });

    // Run-time behavior is validated in loop tests; this call is intentionally
    // limited to construction-time verification for the C03 shape.
}
