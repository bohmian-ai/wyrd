use std::sync::Arc;

use skald_runtime::{MockProvider, ProviderRegistry};
use skald_spec::wire::openai_chat::{
    OpenAiChatChoice, OpenAiChatMessage, OpenAiChatRequest, OpenAiChatResponse, OpenAiChatSettings,
    OpenAiMessageContent,
};
use skald_spec::{ProviderName, ProviderRequest, ProviderResponse, ResponseType};
use wyrd::agent::*;

#[tokio::test]
async fn agent_run_via_umbrella_imports() {
    let mock = MockProvider::new(ProviderName::OpenAi);
    mock.push_response(openai_text_response("done"));
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(mock));
    let agent = Agent::new(Prompt::from_native(prompt()))
        .with_id("planner-agent")
        .name("planner-agent")
        .version("0.3.0");

    let run = agent
        .run_with(&providers, None, "make a plan")
        .await
        .expect("agent run succeeds");

    assert_eq!(run.finish_reason, FinishReason::ModelStopped);
    assert_eq!(run.output, "done");
    assert_eq!(run.conversation.len(), 2);
    assert!(matches!(
        run.conversation.turns().first(),
        Some(ConversationTurn::User { content }) if content == "make a plan"
    ));
    assert!(matches!(
        run.conversation.turns().last(),
        Some(ConversationTurn::Assistant { .. })
    ));
}

fn prompt() -> skald_spec::Prompt {
    skald_spec::Prompt::new(openai_request(), "gpt-4o-mini", None, ResponseType::Text)
        .expect("static prompt is valid")
}

fn openai_request() -> ProviderRequest {
    ProviderRequest::OpenAiChatCompletion(OpenAiChatRequest {
        model: "gpt-4o-mini".to_owned(),
        messages: vec![OpenAiChatMessage {
            role: "user".to_owned(),
            content: Some(OpenAiMessageContent::Text("plan".to_owned())),
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
    })
}

fn openai_text_response(text: &str) -> ProviderResponse {
    ProviderResponse::OpenAiChatCompletion(OpenAiChatResponse {
        id: "resp_1".to_owned(),
        object: "chat.completion".to_owned(),
        created: 0,
        model: "gpt-4o-mini".to_owned(),
        choices: vec![OpenAiChatChoice {
            index: 0,
            message: OpenAiChatMessage {
                role: "assistant".to_owned(),
                content: Some(OpenAiMessageContent::Text(text.to_owned())),
                name: None,
                tool_calls: None,
                tool_call_id: None,
                refusal: None,
                annotations: Vec::new(),
                audio: None,
            },
            finish_reason: Some("stop".to_owned()),
            logprobs: None,
        }],
        usage: None,
        system_fingerprint: None,
        service_tier: None,
    })
}
