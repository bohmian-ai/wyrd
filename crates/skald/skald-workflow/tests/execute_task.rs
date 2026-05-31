use std::sync::Arc;

use serde_json::json;
use skald_agent::{AgentDef, NoopObserver, Observer, RunConfig, ToolRegistry};
use skald_providers::ProviderError;
use skald_runtime::{MockProvider, ProviderRegistry};
use skald_spec::wire::openai_chat::{
    OpenAiChatChoice, OpenAiChatMessage, OpenAiChatRequest, OpenAiChatResponse,
    OpenAiMessageContent,
};
use skald_spec::{Prompt, ProviderName, ProviderRequest, ProviderResponse, ResponseType};
use skald_workflow::{Context, TaskDef, TaskStatus, Workflow, WorkflowDef};

fn text_prompt() -> Prompt {
    Prompt {
        request: ProviderRequest::OpenAiChatCompletion(OpenAiChatRequest {
            model: "gpt-4o".to_owned(),
            messages: vec![OpenAiChatMessage {
                role: "user".to_owned(),
                content: Some(OpenAiMessageContent::Text("x".to_owned())),
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
            settings: Default::default(),
        }),
        model: "gpt-4o".to_owned(),
        version: None,
        variables: Vec::new(),
        media_variables: Vec::new(),
        response_type: ResponseType::Text,
    }
}

fn schema_prompt() -> Prompt {
    let mut prompt = text_prompt();
    prompt.response_type = ResponseType::JsonSchema {
        name: "echo".to_owned(),
        schema: json!({
            "type": "object",
            "properties": { "ok": { "type": "boolean" } },
            "required": ["ok"]
        }),
    };
    prompt
}

fn ok_text(content: &str) -> ProviderResponse {
    ProviderResponse::OpenAiChatCompletion(OpenAiChatResponse {
        id: "r".to_owned(),
        object: "chat.completion".to_owned(),
        created: 0,
        model: "gpt-4o".to_owned(),
        choices: vec![OpenAiChatChoice {
            index: 0,
            message: OpenAiChatMessage {
                role: "assistant".to_owned(),
                content: Some(OpenAiMessageContent::Text(content.to_owned())),
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

async fn build_workflow(prompt: Prompt, mock: MockProvider, max_retries: u32) -> Workflow {
    let def = WorkflowDef {
        id: "wf".to_owned(),
        name: "test".to_owned(),
        agents: vec![AgentDef {
            id: "a".to_owned(),
            provider: ProviderName::OpenAi,
            system_prompt: None,
            model: None,
            tool_names: Vec::new(),
            run_config: RunConfig::default(),
        }],
        tasks: vec![TaskDef {
            id: "t1".to_owned(),
            agent_id: "a".to_owned(),
            prompt,
            dependencies: Vec::new(),
            max_retries,
        }],
    };
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(mock));
    Workflow::from_def(
        def,
        &providers,
        &ToolRegistry::new(),
        Arc::new(NoopObserver) as Arc<dyn Observer>,
    )
    .await
    .expect("workflow builds")
}

#[tokio::test]
async fn execute_task_runs_one_node_without_mutating_status() {
    let mock = MockProvider::new(ProviderName::OpenAi);
    mock.push_response(ok_text("hi"));
    let workflow = build_workflow(text_prompt(), mock, 3).await;

    let response = workflow
        .execute_task("t1", &Context::new())
        .await
        .expect("single task runs");

    assert_eq!(response.adapter().text().as_deref(), Some("hi"));
    let task = workflow.task_list().get_task("t1").expect("task exists");
    assert_eq!(task.read().expect("task lock").status, TaskStatus::Pending);
}

#[tokio::test]
async fn execute_task_unknown_task_returns_404_task() {
    let mock = MockProvider::new(ProviderName::OpenAi);
    let workflow = build_workflow(text_prompt(), mock, 3).await;

    let err = workflow
        .execute_task("nope", &Context::new())
        .await
        .expect_err("unknown task");

    assert_eq!(err.code(), "SKALD_WORKFLOW_404_TASK");
}

#[tokio::test]
async fn execute_task_retries_until_success() {
    let mock = MockProvider::new(ProviderName::OpenAi);
    mock.push_error(ProviderError::bad_request("openai", "one"));
    mock.push_error(ProviderError::bad_request("openai", "two"));
    mock.push_response(ok_text("ok"));
    let workflow = build_workflow(text_prompt(), mock, 3).await;

    workflow
        .execute_task("t1", &Context::new())
        .await
        .expect("third attempt succeeds");
}

#[tokio::test]
async fn execute_task_retries_exhausted_yields_max_retries() {
    let mock = MockProvider::new(ProviderName::OpenAi);
    for _ in 0..3 {
        mock.push_error(ProviderError::bad_request("openai", "boom"));
    }
    let workflow = build_workflow(text_prompt(), mock, 2).await;

    let err = workflow
        .execute_task("t1", &Context::new())
        .await
        .expect_err("attempts exhaust");

    assert_eq!(err.code(), "SKALD_WORKFLOW_500_MAX_RETRIES");
}

#[tokio::test]
async fn execute_task_schema_violation_retries_until_valid() {
    let mock = MockProvider::new(ProviderName::OpenAi);
    mock.push_response(ok_text(r#"{"ok":"no"}"#));
    mock.push_response(ok_text(r#"{"ok":"still no"}"#));
    mock.push_response(ok_text(r#"{"ok":true}"#));
    let workflow = build_workflow(schema_prompt(), mock, 3).await;

    workflow
        .execute_task("t1", &Context::new())
        .await
        .expect("schema-valid retry succeeds");
}

#[tokio::test]
async fn execute_task_all_schema_attempts_yield_output_schema() {
    let mock = MockProvider::new(ProviderName::OpenAi);
    for _ in 0..3 {
        mock.push_response(ok_text(r#"{"ok":"nope"}"#));
    }
    let workflow = build_workflow(schema_prompt(), mock, 2).await;

    let err = workflow
        .execute_task("t1", &Context::new())
        .await
        .expect_err("schema attempts exhaust");

    assert_eq!(err.code(), "SKALD_WORKFLOW_422_OUTPUT_SCHEMA");
}
