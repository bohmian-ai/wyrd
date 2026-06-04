use std::sync::Arc;

use skald_agent::RunConfig;
use skald_runtime::{MockProvider, ProviderRegistry};
use skald_spec::wire::openai_chat::{
    OpenAiChatChoice, OpenAiChatMessage, OpenAiChatRequest, OpenAiChatResponse,
    OpenAiMessageContent,
};
use skald_spec::{Prompt, ProviderName, ProviderRequest, ProviderResponse, ResponseType};
use skald_workflow::WorkflowAgent;
use skald_workflow::{Context, TaskDef, TaskStatus, Workflow, WorkflowDef};

fn prompt() -> Prompt {
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

fn ok() -> ProviderResponse {
    ProviderResponse::OpenAiChatCompletion(OpenAiChatResponse {
        id: "r".to_owned(),
        object: "chat.completion".to_owned(),
        created: 0,
        model: "gpt-4o".to_owned(),
        choices: vec![OpenAiChatChoice {
            index: 0,
            message: OpenAiChatMessage {
                role: "assistant".to_owned(),
                content: Some(OpenAiMessageContent::Text("hi".to_owned())),
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

#[tokio::test]
async fn workflow_run_carries_outcomes_events_last_task() {
    let def = WorkflowDef {
        id: "wf".to_owned(),
        name: "n".to_owned(),
        agents: vec![WorkflowAgent {
            id: "a".to_owned(),
            prompt: prompt(),
            run_config: RunConfig::default(),
        }],
        tasks: vec![
            TaskDef {
                id: "first".to_owned(),
                agent_id: "a".to_owned(),
                prompt: prompt(),
                dependencies: Vec::new(),
                max_retries: 0,
            },
            TaskDef {
                id: "last".to_owned(),
                agent_id: "a".to_owned(),
                prompt: prompt(),
                dependencies: vec!["first".to_owned()],
                max_retries: 0,
            },
        ],
    };
    let mock = MockProvider::new(ProviderName::OpenAi);
    mock.push_response(ok());
    mock.push_response(ok());
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(mock));
    let workflow = Arc::new(
        Workflow::build(def, &providers)
            .await
            .expect("workflow builds"),
    );

    let run = workflow.run(Context::new()).await.expect("run succeeds");

    assert_eq!(run.tasks.len(), 2);
    assert_eq!(
        run.tasks.get("first").expect("first outcome").status,
        TaskStatus::Completed
    );
    assert_eq!(run.last_task_id.as_deref(), Some("last"));
    assert!(run.result().is_some());
    assert_eq!(run.events.len(), 2);
    for event in &run.events {
        assert_eq!(event.status, TaskStatus::Completed);
        assert_eq!(event.attempt, 1);
        assert!(event.error.is_none());
    }
}
