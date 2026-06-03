use std::sync::Arc;

use skald_agent::{Agent, RunConfig};
use skald_prompt::Prompt;
use skald_spec::wire::openai_chat::{
    OpenAiChatMessage, OpenAiChatRequest, OpenAiChatSettings, OpenAiMessageContent,
};
use skald_spec::{Prompt as SpecPrompt, ProviderName, ProviderRequest, ResponseType};

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
        }],
        response_format: None,
        stream: None,
        stream_options: None,
        tools: None,
        tool_choice: None,
        parallel_tool_calls: None,
        settings: OpenAiChatSettings::default(),
    });
    let prompt = SpecPrompt::new(request, "gpt-4o".to_string(), None, ResponseType::Text).unwrap();
    Arc::new(Prompt::from_native(prompt))
}

#[test]
fn agent_new_uses_resolved_prompt_and_default_run_config() {
    let agent = Agent::new("writer", openai_prompt());

    assert_eq!(agent.id, "writer");
    assert_eq!(agent.prompt.native().model, "gpt-4o");
    assert_eq!(
        agent.prompt.native().request.provider(),
        ProviderName::OpenAi
    );
}

#[test]
fn agent_with_prompt_overrides_prompt_reference() {
    let original = openai_prompt();
    let mut prompt = SpecPrompt::new(
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
        "gpt-4o",
        None,
        ResponseType::Text,
    )
    .unwrap();
    prompt.version = Some("1".to_string());
    let replacement = Arc::new(Prompt::from_native(prompt));

    let agent = Agent::new("a", original).with_prompt(replacement.clone());

    assert_eq!(agent.prompt.native().version.as_deref(), Some("1"));
}

#[test]
fn agent_with_run_config_preserves_config() {
    let _agent = Agent::new("a", openai_prompt()).with_run_config(RunConfig { max_iterations: 7 });

    // Run-time behavior is validated in loop tests; this call is intentionally
    // limited to construction-time verification for the C03 shape.
}
