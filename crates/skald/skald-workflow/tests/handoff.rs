use std::sync::Arc;

use skald_agent::RunConfig;
use skald_runtime::{MockProvider, ProviderRegistry};
use skald_spec::wire::anthropic_messages::{
    AnthropicContentBlock, AnthropicMessage, AnthropicMessagesRequest, AnthropicMessagesResponse,
    AnthropicMessagesSettings, AnthropicStopReason, AnthropicUsage,
};
use skald_spec::wire::openai_chat::{
    OpenAiChatChoice, OpenAiChatMessage, OpenAiChatRequest, OpenAiChatResponse, OpenAiChatSettings,
    OpenAiMessageContent,
};
use skald_spec::{
    MessageNum, Prompt, ProviderName, ProviderRequest, ProviderResponse, ResponseType,
};
use skald_workflow::WorkflowAgent;
use skald_workflow::{Context, DagExecutor, TaskDef, WorkflowDef, extract_messages_for_handoff};

fn openai_prompt() -> Prompt {
    Prompt {
        request: ProviderRequest::OpenAiChatCompletion(OpenAiChatRequest {
            model: "gpt-4o".to_owned(),
            messages: vec![OpenAiChatMessage {
                role: "user".to_owned(),
                content: Some(OpenAiMessageContent::Text("hello".to_owned())),
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
        }),
        model: "gpt-4o".to_owned(),
        version: None,
        variables: Vec::new(),
        media_variables: Vec::new(),
        response_type: ResponseType::Text,
    }
}

fn anthropic_prompt() -> Prompt {
    Prompt {
        request: ProviderRequest::AnthropicMessage(AnthropicMessagesRequest {
            model: "claude-3-5-sonnet-latest".to_owned(),
            messages: Vec::new(),
            system: None,
            stream: None,
            tools: None,
            tool_choice: None,
            output_config: None,
            settings: AnthropicMessagesSettings::default(),
        }),
        model: "claude-3-5-sonnet-latest".to_owned(),
        version: None,
        variables: Vec::new(),
        media_variables: Vec::new(),
        response_type: ResponseType::Text,
    }
}

fn openai_text(text: &str) -> ProviderResponse {
    ProviderResponse::OpenAiChatCompletion(OpenAiChatResponse {
        id: "r".to_owned(),
        object: "chat.completion".to_owned(),
        created: 0,
        model: "gpt-4o".to_owned(),
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

fn anthropic_text(text: &str) -> ProviderResponse {
    ProviderResponse::AnthropicMessage(AnthropicMessagesResponse {
        id: "m".to_owned(),
        r#type: "message".to_owned(),
        role: "assistant".to_owned(),
        model: "claude".to_owned(),
        content: vec![AnthropicContentBlock::Text {
            text: text.to_owned(),
            cache_control: None,
            citations: None,
        }],
        stop_reason: Some(AnthropicStopReason::EndTurn),
        stop_sequence: None,
        usage: AnthropicUsage {
            input_tokens: 1,
            output_tokens: 1,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        },
    })
}

fn agent(id: &str, provider: ProviderName) -> WorkflowAgent {
    let prompt = match provider {
        ProviderName::OpenAi => openai_prompt(),
        ProviderName::Anthropic => anthropic_prompt(),
        other => panic!("unsupported test provider: {other:?}"),
    };
    WorkflowAgent {
        id: id.to_owned(),
        prompt,
        run_config: RunConfig::default(),
    }
}

#[tokio::test]
async fn same_provider_handoff_is_passthrough() {
    let openai = MockProvider::new(ProviderName::OpenAi);
    openai.push_response(openai_text("first"));
    openai.push_response(openai_text("second"));

    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(openai));

    let def = WorkflowDef {
        id: "wf".to_owned(),
        name: "same".to_owned(),
        agents: vec![agent("a", ProviderName::OpenAi)],
        tasks: vec![
            TaskDef {
                id: "t1".to_owned(),
                agent_id: "a".to_owned(),
                prompt: openai_prompt(),
                dependencies: Vec::new(),
                max_retries: 0,
            },
            TaskDef {
                id: "t2".to_owned(),
                agent_id: "a".to_owned(),
                prompt: openai_prompt(),
                dependencies: vec!["t1".to_owned()],
                max_retries: 0,
            },
        ],
    };

    let workflow = Arc::new(
        DagExecutor::build(def, &providers)
            .await
            .expect("workflow builds"),
    );
    let run = workflow.run(Context::new()).await.expect("workflow runs");

    assert_eq!(run.tasks.len(), 2);
}

#[tokio::test]
async fn cross_provider_handoff_uses_message_conversion() {
    let openai = MockProvider::new(ProviderName::OpenAi);
    openai.push_response(openai_text("first answer"));

    let expected_anthropic_request = ProviderRequest::AnthropicMessage(AnthropicMessagesRequest {
        model: "claude-3-5-sonnet-latest".to_owned(),
        messages: vec![AnthropicMessage {
            role: "assistant".to_owned(),
            content: vec![AnthropicContentBlock::Text {
                text: "first answer".to_owned(),
                cache_control: None,
                citations: None,
            }],
        }],
        system: None,
        stream: None,
        tools: None,
        tool_choice: None,
        output_config: None,
        settings: AnthropicMessagesSettings::default(),
    });
    let anthropic = MockProvider::new(ProviderName::Anthropic)
        .expect_request(expected_anthropic_request)
        .respond_with(anthropic_text("second"));

    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(openai));
    providers.register(Arc::new(anthropic));

    let def = WorkflowDef {
        id: "wf".to_owned(),
        name: "cross".to_owned(),
        agents: vec![
            agent("writer_openai", ProviderName::OpenAi),
            agent("reviewer_anthropic", ProviderName::Anthropic),
        ],
        tasks: vec![
            TaskDef {
                id: "draft".to_owned(),
                agent_id: "writer_openai".to_owned(),
                prompt: openai_prompt(),
                dependencies: Vec::new(),
                max_retries: 0,
            },
            TaskDef {
                id: "review".to_owned(),
                agent_id: "reviewer_anthropic".to_owned(),
                prompt: anthropic_prompt(),
                dependencies: vec!["draft".to_owned()],
                max_retries: 0,
            },
        ],
    };

    let workflow = Arc::new(
        DagExecutor::build(def, &providers)
            .await
            .expect("workflow builds"),
    );

    workflow.run(Context::new()).await.expect("workflow runs");
}

#[test]
fn handoff_messages_rejects_raw_v1() {
    let raw =
        serde_json::value::RawValue::from_string("{\"x\":1}".to_owned()).expect("raw value builds");
    let messages = vec![MessageNum::RawV1(raw)];

    let err = skald_workflow::handoff_messages(
        &ProviderName::OpenAi,
        &ProviderName::Anthropic,
        &messages,
    )
    .expect_err("raw messages cannot hand off");

    assert_eq!(err.code(), "SKALD_WORKFLOW_501_UNSUPPORTED_HANDOFF");
}

#[test]
fn handoff_messages_same_provider_is_clone_passthrough() {
    let messages = vec![MessageNum::OpenAi(OpenAiChatMessage {
        role: "user".to_owned(),
        content: Some(OpenAiMessageContent::Text("hi".to_owned())),
        name: None,
        tool_calls: None,
        tool_call_id: None,
        refusal: None,
        annotations: Vec::new(),
        audio: None,
    })];

    let out =
        skald_workflow::handoff_messages(&ProviderName::OpenAi, &ProviderName::OpenAi, &messages)
            .expect("same provider handoff succeeds");

    assert_eq!(out, messages);
}

#[test]
fn handoff_messages_rejects_missing_conversion() {
    let messages = vec![MessageNum::OpenAi(OpenAiChatMessage {
        role: "assistant".to_owned(),
        content: Some(OpenAiMessageContent::Text("hi".to_owned())),
        name: None,
        tool_calls: None,
        tool_call_id: None,
        refusal: None,
        annotations: Vec::new(),
        audio: None,
    })];

    let err = skald_workflow::handoff_messages(
        &ProviderName::OpenAi,
        &ProviderName::Custom("other".to_owned()),
        &messages,
    )
    .expect_err("custom conversion is unsupported");

    assert_eq!(err.code(), "SKALD_WORKFLOW_501_UNSUPPORTED_HANDOFF");
}

#[test]
fn extract_messages_for_handoff_rejects_mismatched_response_provider() {
    let response = openai_text("hello");
    let err = extract_messages_for_handoff(&response, &ProviderName::Anthropic)
        .expect_err("OpenAI response with Anthropic provider label must fail");
    assert_eq!(err.code(), "SKALD_WORKFLOW_501_UNSUPPORTED_HANDOFF");
}

#[test]
fn extract_messages_for_handoff_returns_assistant_message_for_openai() {
    let response = openai_text("result");
    let msgs = extract_messages_for_handoff(&response, &ProviderName::OpenAi)
        .expect("matching OpenAI provider must succeed");
    assert_eq!(msgs.len(), 1);
    assert!(matches!(msgs[0], MessageNum::OpenAi(_)));
}

#[test]
fn extract_messages_for_handoff_returns_assistant_message_for_anthropic() {
    let response = anthropic_text("result");
    let msgs = extract_messages_for_handoff(&response, &ProviderName::Anthropic)
        .expect("matching Anthropic provider must succeed");
    assert_eq!(msgs.len(), 1);
    assert!(matches!(msgs[0], MessageNum::Anthropic(_)));
}
