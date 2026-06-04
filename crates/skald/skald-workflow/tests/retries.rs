use std::sync::Arc;

use serde_json::json;
use skald_agent::RunConfig;
use skald_providers::ProviderError;
use skald_runtime::{MockProvider, ProviderRegistry};
use skald_spec::wire::openai_chat::{
    OpenAiChatChoice, OpenAiChatMessage, OpenAiChatRequest, OpenAiChatResponse,
    OpenAiMessageContent,
};
use skald_spec::{Prompt, ProviderName, ProviderRequest, ProviderResponse, ResponseType};
use skald_workflow::WorkflowAgent;
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

fn ok_response() -> ProviderResponse {
    ProviderResponse::OpenAiChatCompletion(OpenAiChatResponse {
        id: "r".to_owned(),
        object: "chat.completion".to_owned(),
        created: 0,
        model: "gpt-4o".to_owned(),
        choices: vec![OpenAiChatChoice {
            index: 0,
            message: OpenAiChatMessage {
                role: "assistant".to_owned(),
                content: Some(OpenAiMessageContent::Text("ok".to_owned())),
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

async fn workflow_with(mock: MockProvider, max_retries: u32) -> Arc<Workflow> {
    workflow_with_prompt(mock, text_prompt(), max_retries).await
}

async fn workflow_with_prompt(
    mock: MockProvider,
    prompt: Prompt,
    max_retries: u32,
) -> Arc<Workflow> {
    let def = WorkflowDef {
        id: "wf".to_owned(),
        name: "test".to_owned(),
        agents: vec![WorkflowAgent {
            id: "a".to_owned(),
            prompt: prompt.clone(),
            run_config: RunConfig::default(),
        }],
        tasks: vec![TaskDef {
            id: "t".to_owned(),
            agent_id: "a".to_owned(),
            prompt,
            dependencies: Vec::new(),
            max_retries,
        }],
    };
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(mock));
    Arc::new(
        Workflow::build(def, &providers)
            .await
            .expect("workflow builds"),
    )
}

#[tokio::test]
async fn dag_runner_retries_then_succeeds() {
    let mock = MockProvider::new(ProviderName::OpenAi);
    mock.push_error(ProviderError::bad_request("openai", "one"));
    mock.push_response(ok_response());
    let workflow = workflow_with(mock, 2).await;

    let run = workflow.run(Context::new()).await.expect("run succeeds");
    let outcome = run.tasks.get("t").expect("task outcome");

    assert_eq!(outcome.status, TaskStatus::Completed);
    assert!(outcome.result.is_some());
    assert_eq!(outcome.retries, 1);
    assert_eq!(run.events.len(), 2);
    assert_eq!(run.events[0].status, TaskStatus::Failed);
    assert_eq!(run.events[0].attempt, 1);
    assert_eq!(run.events[1].status, TaskStatus::Completed);
    assert_eq!(run.events[1].attempt, 2);
}

#[tokio::test]
async fn dag_runner_propagates_terminal_provider_failure() {
    let mock = MockProvider::new(ProviderName::OpenAi);
    for _ in 0..3 {
        mock.push_error(ProviderError::bad_request("openai", "boom"));
    }
    let workflow = workflow_with(mock, 2).await;

    let err = workflow
        .run(Context::new())
        .await
        .expect_err("attempts exhaust");

    assert_eq!(err.code(), "SKALD_WORKFLOW_500_MAX_RETRIES");
    let task = workflow.task_list().get_task("t").expect("task exists");
    assert_eq!(task.read().expect("task lock").status, TaskStatus::Failed);
}

#[tokio::test]
async fn dag_runner_propagates_terminal_validation_failure() {
    let mock = MockProvider::new(ProviderName::OpenAi);
    for _ in 0..3 {
        mock.push_response(ok_response());
    }
    let workflow = workflow_with_prompt(mock, schema_prompt(), 2).await;

    let err = workflow
        .run(Context::new())
        .await
        .expect_err("schema attempts exhaust");

    assert_eq!(err.code(), "SKALD_WORKFLOW_422_OUTPUT_SCHEMA");
    let task = workflow.task_list().get_task("t").expect("task exists");
    let task = task.read().expect("task lock");
    assert_eq!(task.status, TaskStatus::Failed);
    assert_eq!(task.retry_count, 2);
}
