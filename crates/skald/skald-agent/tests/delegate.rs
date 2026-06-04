use std::cell::RefCell;
use std::sync::Arc;

use serde_json::{Value, json};
use skald_agent::{
    Agent, AgentDelegateTool, AgentError, RunConfig,
    delegation::{DELEGATION_DEPTH_CAP, WYRD_AGENT_DELEGATION_CHAIN, WYRD_AGENT_DELEGATION_DEPTH},
};
use skald_prompt::Prompt;
use skald_providers::ProviderError;
use skald_runtime::{MockProvider, ProviderRegistry};
use skald_spec::{
    Prompt as SpecPrompt, ProviderName, ProviderRequest, ProviderResponse, ResponseType,
    wire::openai_chat::{
        OpenAiChatChoice, OpenAiChatMessage, OpenAiChatRequest, OpenAiChatResponse,
        OpenAiChatSettings, OpenAiMessageContent,
    },
};
use skald_tool::{AgentTool, ToolError};

#[tokio::test]
async fn agent_delegate_tool_invokes_sub_agent_run() {
    let mock = MockProvider::new(ProviderName::OpenAi);
    mock.push_response(openai_text_response("researched: foo"));
    let providers = providers(mock.clone());
    let agent = Arc::new(test_agent("researcher"));
    let tool = AgentDelegateTool::from_agent(agent, Arc::clone(&providers));

    let result = tool
        .invoke(json!({"input": "hi"}))
        .await
        .expect("delegate invoke succeeds");

    assert_eq!(result, Value::String("researched: foo".to_owned()));
    assert_eq!(mock.remaining(), 0);
}

#[tokio::test]
async fn agent_delegate_tool_schema_matches_input_envelope() {
    let tool = AgentDelegateTool::from_agent(Arc::new(test_agent("researcher")), empty_providers());

    assert_eq!(
        tool.input_schema(),
        json!({
            "type": "object",
            "properties": {
                "input": {
                    "type": "string"
                }
            },
            "required": ["input"]
        })
    );
}

#[tokio::test]
async fn agent_delegate_tool_default_output_schema_is_string() {
    let tool = AgentDelegateTool::from_agent(Arc::new(test_agent("researcher")), empty_providers());

    assert_eq!(tool.output_schema(), json!({"type": "string"}));
}

#[tokio::test]
async fn agent_delegate_tool_with_description_override() {
    let agent = Arc::new(test_agent("researcher"));
    let typed = AgentDelegateTool::new(Arc::clone(&agent), empty_providers())
        .with_description("Research the request and return a concise answer");

    assert_eq!(
        typed.description(),
        "Research the request and return a concise answer"
    );
    assert_eq!(typed.name(), "researcher");
    assert_eq!(
        typed.input_schema(),
        json!({
            "type": "object",
            "properties": {
                "input": {
                    "type": "string"
                }
            },
            "required": ["input"]
        })
    );
    assert_eq!(typed.output_schema(), json!({"type": "string"}));
}

#[tokio::test]
async fn agent_delegate_tool_depth_cap_at_3() {
    let tool = AgentDelegateTool::from_agent(Arc::new(test_agent("D")), empty_providers());

    let err = WYRD_AGENT_DELEGATION_DEPTH
        .scope(DELEGATION_DEPTH_CAP, async {
            WYRD_AGENT_DELEGATION_CHAIN
                .scope(
                    RefCell::new(vec!["A".to_owned(), "B".to_owned(), "C".to_owned()]),
                    async { tool.invoke(json!({"input": "x"})).await },
                )
                .await
        })
        .await
        .expect_err("depth cap should reject the attempted delegate");

    match err {
        ToolError::Invocation {
            detail: _,
            cause: Some(cause),
        } => {
            let agent_err = cause
                .downcast_ref::<AgentError>()
                .expect("cause should preserve AgentError");
            assert_eq!(agent_err.code(), "SKALD_AGENT_412_DELEGATION_DEPTH");
            match agent_err {
                AgentError::DelegationDepthExceeded { chain } => {
                    assert_eq!(chain, &vec!["A", "B", "C", "D"]);
                }
                other => panic!("expected DelegationDepthExceeded, got {other:?}"),
            }
        }
        other => panic!("expected ToolError::Invocation with cause, got {other:?}"),
    }
}

#[tokio::test]
async fn agent_delegate_tool_depth_resets_outside_scope() {
    let mock = MockProvider::new(ProviderName::OpenAi);
    mock.push_response(openai_text_response("first"));
    mock.push_response(openai_text_response("second"));
    mock.push_response(openai_text_response("third"));
    mock.push_response(openai_text_response("fourth"));
    let tool = AgentDelegateTool::from_agent(Arc::new(test_agent("researcher")), providers(mock));

    for expected in ["first", "second", "third", "fourth"] {
        let result = tool
            .invoke(json!({"input": "hi"}))
            .await
            .expect("top-level delegate call should enter with fresh depth");
        assert_eq!(result, Value::String(expected.to_owned()));
    }
}

#[tokio::test]
async fn agent_delegate_tool_propagates_sub_agent_error() {
    let mock = MockProvider::new(ProviderName::OpenAi);
    mock.push_error(ProviderError::bad_request("mock", "forced failure"));
    let tool = AgentDelegateTool::from_agent(Arc::new(test_agent("researcher")), providers(mock));

    let err = tool
        .invoke(json!({"input": "hi"}))
        .await
        .expect_err("provider failure should map to tool invocation failure");

    match err {
        ToolError::Invocation {
            detail,
            cause: Some(cause),
        } => {
            assert!(detail.contains("provider call failed"));
            match cause
                .downcast_ref::<AgentError>()
                .expect("cause should preserve AgentError")
            {
                AgentError::Provider(_) => {}
                other => panic!("expected AgentError::Provider, got {other:?}"),
            }
        }
        other => panic!("expected ToolError::Invocation with AgentError cause, got {other:?}"),
    }
}

#[tokio::test]
async fn agent_delegate_tool_input_missing_returns_invalid_input() {
    let tool = AgentDelegateTool::from_agent(Arc::new(test_agent("researcher")), empty_providers());

    let err = tool
        .invoke(json!({"wrong_key": "x"}))
        .await
        .expect_err("missing input should be invalid");

    match err {
        ToolError::InvalidInput(detail) => {
            assert!(detail.contains("missing 'input' field"));
        }
        other => panic!("expected ToolError::InvalidInput, got {other:?}"),
    }
}

fn test_agent(id: &str) -> Agent {
    Agent::from_resolved(id, test_prompt()).with_run_config(RunConfig {
        max_iterations: 3,
        ..Default::default()
    })
}

fn test_prompt() -> Arc<Prompt> {
    Arc::new(Prompt::from_native(
        SpecPrompt::new(openai_request(), "gpt-4o", None, ResponseType::Text)
            .expect("test prompt should build"),
    ))
}

fn providers(mock: MockProvider) -> Arc<ProviderRegistry> {
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(mock));
    Arc::new(providers)
}

fn empty_providers() -> Arc<ProviderRegistry> {
    Arc::new(ProviderRegistry::new())
}

fn openai_request() -> ProviderRequest {
    ProviderRequest::OpenAiChatCompletion(OpenAiChatRequest {
        model: "gpt-4o".to_owned(),
        messages: vec![],
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
            },
            finish_reason: Some("stop".to_owned()),
            logprobs: None,
        }],
        usage: None,
        system_fingerprint: None,
        service_tier: None,
    })
}
