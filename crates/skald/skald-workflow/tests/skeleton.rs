use serde_json::json;
use skald_agent::RunConfig;
use skald_spec::wire::google_embeddings::{
    GoogleBatchEmbedRequest, GoogleEmbedContent, GoogleEmbedPart, GoogleEmbedRequest,
};
use skald_spec::wire::openai_chat::{
    OpenAiChatChoice, OpenAiChatMessage, OpenAiChatRequest, OpenAiChatResponse,
    OpenAiMessageContent,
};
use skald_spec::wire::openai_embeddings::{OpenAiEmbeddingsInput, OpenAiEmbeddingsRequest};
use skald_spec::wire::openai_responses::OpenAiResponsesRequest;
use skald_spec::wire::vertex_predict::{VertexEmbedInstance, VertexPredictRequest};
use skald_spec::{Prompt, ProviderName, ProviderRequest, ProviderResponse, ResponseType};
use skald_workflow::WorkflowAgent;
use skald_workflow::{
    Context, ContextSnapshot, Task, TaskDef, TaskStatus, WorkflowDef, WorkflowError,
    default_max_retries,
};

fn fixture_prompt(json_schema: bool) -> Prompt {
    Prompt {
        request: ProviderRequest::OpenAiChatCompletion(openai_chat_request()),
        model: "gpt-4o".to_owned(),
        version: None,
        variables: Vec::new(),
        media_variables: Vec::new(),
        response_type: if json_schema {
            ResponseType::JsonSchema {
                name: "echo".to_owned(),
                schema: json!({
                    "type": "object",
                    "properties": { "ok": { "type": "boolean" } },
                    "required": ["ok"]
                }),
            }
        } else {
            ResponseType::Text
        },
    }
}

fn openai_chat_request() -> OpenAiChatRequest {
    OpenAiChatRequest {
        model: "gpt-4o".to_owned(),
        messages: vec![OpenAiChatMessage {
            role: "user".to_owned(),
            content: Some(OpenAiMessageContent::Text("hi".to_owned())),
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
        settings: Default::default(),
    }
}

fn agent_def() -> WorkflowAgent {
    WorkflowAgent {
        id: "a".to_owned(),
        prompt: fixture_prompt(false),
        run_config: RunConfig::default(),
    }
}

fn task_def(id: &str, dependencies: Vec<String>) -> TaskDef {
    TaskDef {
        id: id.to_owned(),
        agent_id: "a".to_owned(),
        prompt: fixture_prompt(false),
        dependencies,
        max_retries: default_max_retries(),
    }
}

fn workflow_def(tasks: Vec<TaskDef>) -> WorkflowDef {
    WorkflowDef {
        id: "wf".to_owned(),
        name: "Test workflow".to_owned(),
        agents: vec![agent_def()],
        tasks,
    }
}

fn openai_response(content: Option<&str>) -> ProviderResponse {
    ProviderResponse::OpenAiChatCompletion(OpenAiChatResponse {
        id: "r".to_owned(),
        object: "chat.completion".to_owned(),
        created: 0,
        model: "gpt-4o".to_owned(),
        choices: vec![OpenAiChatChoice {
            index: 0,
            message: OpenAiChatMessage {
                role: "assistant".to_owned(),
                content: content.map(|text| OpenAiMessageContent::Text(text.to_owned())),
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

fn assert_task_prompt_rejected(prompt: Prompt) {
    let err = Task::build(TaskDef {
        id: "t".to_owned(),
        agent_id: "a".to_owned(),
        prompt,
        dependencies: Vec::new(),
        max_retries: default_max_retries(),
    })
    .expect_err("unsupported prompt must reject");
    assert_eq!(err.code(), "SKALD_AGENT_422_PROMPT");
}

fn assert_workflow_prompt_rejected(prompt: Prompt) {
    let err = workflow_def(vec![TaskDef {
        id: "bad".to_owned(),
        agent_id: "a".to_owned(),
        prompt,
        dependencies: Vec::new(),
        max_retries: default_max_retries(),
    }])
    .validate_graph()
    .expect_err("unsupported prompt must reject");
    assert_eq!(err.code(), "SKALD_AGENT_422_PROMPT");
}

#[test]
fn workflow_def_round_trips() {
    let def = workflow_def(vec![
        task_def("t1", Vec::new()),
        TaskDef {
            prompt: fixture_prompt(true),
            max_retries: 5,
            ..task_def("t2", vec!["t1".to_owned()])
        },
    ]);
    let text = serde_json::to_string(&def).expect("workflow def serializes");
    let parsed: WorkflowDef = serde_json::from_str(&text).expect("workflow def parses");
    assert_eq!(parsed, def);
}

#[test]
fn task_def_default_max_retries_is_three() {
    let json_text = r#"{
        "id": "t",
        "agent_id": "a",
        "prompt": {
            "request": {
                "model": "gpt-4o",
                "messages": [{"role": "user", "content": "hi"}]
            },
            "model": "gpt-4o",
            "response_type": "text"
        }
    }"#;
    let parsed: TaskDef = serde_json::from_str(json_text).expect("task def parses");
    assert_eq!(parsed.max_retries, 3);
}

#[test]
fn task_build_text_response_has_no_validator() {
    let task = Task::build(task_def("t", Vec::new())).expect("text task compiles");
    assert!(task.output_validator.is_none());
    assert_eq!(task.status, TaskStatus::Pending);
    assert_eq!(task.retry_count, 0);
    assert!(task.dependencies().is_empty());
}

#[test]
fn task_build_compiles_jsonschema_validator() {
    let task = Task::build(TaskDef {
        prompt: fixture_prompt(true),
        ..task_def("t", Vec::new())
    })
    .expect("schema compiles");
    assert!(task.output_validator.is_some());
}

#[test]
fn task_validate_response_passes_for_text_type() {
    let task = Task::build(task_def("t", Vec::new())).expect("text task compiles");
    task.validate_response(&openai_response(Some("ok")))
        .expect("text response passes");
}

#[test]
fn task_validate_response_passes_for_matching_structured_output() {
    let task = Task::build(TaskDef {
        prompt: fixture_prompt(true),
        ..task_def("t", Vec::new())
    })
    .expect("schema compiles");
    task.validate_response(&openai_response(Some(r#"{"ok": true}"#)))
        .expect("schema-valid response passes");
}

#[test]
fn task_validate_response_rejects_invalid_schema_output() {
    let task = Task::build(TaskDef {
        prompt: fixture_prompt(true),
        ..task_def("t", Vec::new())
    })
    .expect("schema compiles");
    let err = task
        .validate_response(&openai_response(Some(r#"{"ok": "no"}"#)))
        .expect_err("schema-invalid response fails");
    assert_eq!(err.code(), "SKALD_WORKFLOW_422_OUTPUT_SCHEMA");
}

#[test]
fn task_validate_response_rejects_missing_structured_output() {
    let task = Task::build(TaskDef {
        prompt: fixture_prompt(true),
        ..task_def("t", Vec::new())
    })
    .expect("schema compiles");
    let err = task
        .validate_response(&openai_response(Some("plain text")))
        .expect_err("missing structured output fails");
    assert_eq!(err.code(), "SKALD_WORKFLOW_422_OUTPUT_SCHEMA");
}

#[test]
fn task_build_rejects_openai_responses_prompt_upfront() {
    let mut prompt = fixture_prompt(false);
    prompt.request = ProviderRequest::OpenAiResponses(OpenAiResponsesRequest {
        model: "gpt-4o".to_owned(),
        input: Vec::new().into(),
        instructions: None,
        text: None,
        tools: None,
        tool_choice: None,
        parallel_tool_calls: None,
        previous_response_id: None,
        stream: None,
        settings: Default::default(),
    });
    assert_task_prompt_rejected(prompt);
}

#[test]
fn task_build_rejects_openai_embedding_prompt_upfront() {
    let mut prompt = fixture_prompt(false);
    prompt.request = ProviderRequest::OpenAiEmbeddings(OpenAiEmbeddingsRequest {
        model: "text-embedding-3-small".to_owned(),
        input: OpenAiEmbeddingsInput::One("x".to_owned()),
        encoding_format: None,
        dimensions: None,
        user: None,
    });
    assert_task_prompt_rejected(prompt);
}

#[test]
fn task_build_rejects_google_batch_embed_prompt_upfront() {
    let mut prompt = fixture_prompt(false);
    prompt.request = ProviderRequest::GoogleBatchEmbed(GoogleBatchEmbedRequest {
        requests: vec![GoogleEmbedRequest {
            model: "models/text-embedding-004".to_owned(),
            content: GoogleEmbedContent {
                parts: vec![GoogleEmbedPart {
                    text: "x".to_owned(),
                }],
            },
            task_type: None,
            title: None,
            output_dimensionality: None,
        }],
    });
    assert_task_prompt_rejected(prompt);
}

#[test]
fn task_build_rejects_vertex_predict_prompt_upfront() {
    let mut prompt = fixture_prompt(false);
    prompt.request = ProviderRequest::VertexPredict(VertexPredictRequest {
        instances: vec![VertexEmbedInstance {
            content: "x".to_owned(),
            task_type: None,
            title: None,
        }],
        parameters: None,
    });
    assert_task_prompt_rejected(prompt);
}

#[test]
fn task_build_rejects_raw_v1_prompt_upfront() {
    let mut prompt = fixture_prompt(false);
    prompt.request = ProviderRequest::RawV1 {
        provider: ProviderName::Custom("preview".to_owned()),
        body: serde_json::value::RawValue::from_string("{}".to_owned())
            .expect("static JSON body is valid"),
    };
    assert_task_prompt_rejected(prompt);
}

#[test]
fn workflow_def_validate_graph_accepts_valid_dag() {
    workflow_def(vec![
        task_def("a", Vec::new()),
        task_def("b", vec!["a".to_owned()]),
        task_def("c", vec!["a".to_owned()]),
        task_def("d", vec!["b".to_owned(), "c".to_owned()]),
    ])
    .validate_graph()
    .expect("valid graph passes");
}

#[test]
fn workflow_def_validate_graph_rejects_duplicate_task() {
    let err = workflow_def(vec![task_def("a", Vec::new()), task_def("a", Vec::new())])
        .validate_graph()
        .expect_err("duplicate task fails");
    assert_eq!(err.code(), "WYRD_WORKFLOW_422_DUPLICATE_STEP_ID");
}

#[test]
fn workflow_def_validate_graph_rejects_self_dependency() {
    let err = workflow_def(vec![task_def("a", vec!["a".to_owned()])])
        .validate_graph()
        .expect_err("self dependency fails");
    assert_eq!(err.code(), "WYRD_WORKFLOW_422_MISSING_DEPENDENCY");
}

#[test]
fn workflow_def_validate_graph_rejects_missing_dependency() {
    let err = workflow_def(vec![task_def("a", vec!["missing".to_owned()])])
        .validate_graph()
        .expect_err("missing dependency fails");
    assert_eq!(err.code(), "WYRD_WORKFLOW_422_MISSING_DEPENDENCY");
}

#[test]
fn workflow_def_validate_graph_rejects_cycle() {
    let err = workflow_def(vec![
        task_def("a", vec!["c".to_owned()]),
        task_def("b", vec!["a".to_owned()]),
        task_def("c", vec!["b".to_owned()]),
    ])
    .validate_graph()
    .expect_err("cycle fails");
    assert_eq!(err.code(), "WYRD_WORKFLOW_422_CYCLE");
}

#[test]
fn workflow_def_validate_graph_rejects_unsupported_prompt_shapes() {
    let mut responses_prompt = fixture_prompt(false);
    responses_prompt.request = ProviderRequest::OpenAiResponses(OpenAiResponsesRequest {
        model: "gpt-4o".to_owned(),
        input: Vec::new().into(),
        instructions: None,
        text: None,
        tools: None,
        tool_choice: None,
        parallel_tool_calls: None,
        previous_response_id: None,
        stream: None,
        settings: Default::default(),
    });
    assert_workflow_prompt_rejected(responses_prompt);

    let mut embeddings_prompt = fixture_prompt(false);
    embeddings_prompt.request = ProviderRequest::OpenAiEmbeddings(OpenAiEmbeddingsRequest {
        model: "text-embedding-3-small".to_owned(),
        input: OpenAiEmbeddingsInput::One("x".to_owned()),
        encoding_format: None,
        dimensions: None,
        user: None,
    });
    assert_workflow_prompt_rejected(embeddings_prompt);

    let mut google_batch_prompt = fixture_prompt(false);
    google_batch_prompt.request = ProviderRequest::GoogleBatchEmbed(GoogleBatchEmbedRequest {
        requests: vec![GoogleEmbedRequest {
            model: "models/text-embedding-004".to_owned(),
            content: GoogleEmbedContent {
                parts: vec![GoogleEmbedPart {
                    text: "x".to_owned(),
                }],
            },
            task_type: None,
            title: None,
            output_dimensionality: None,
        }],
    });
    assert_workflow_prompt_rejected(google_batch_prompt);

    let mut vertex_predict_prompt = fixture_prompt(false);
    vertex_predict_prompt.request = ProviderRequest::VertexPredict(VertexPredictRequest {
        instances: vec![VertexEmbedInstance {
            content: "x".to_owned(),
            task_type: None,
            title: None,
        }],
        parameters: None,
    });
    assert_workflow_prompt_rejected(vertex_predict_prompt);

    let mut raw_prompt = fixture_prompt(false);
    raw_prompt.request = ProviderRequest::RawV1 {
        provider: ProviderName::Custom("preview".to_owned()),
        body: serde_json::value::RawValue::from_string("{}".to_owned())
            .expect("static JSON body is valid"),
    };
    assert_workflow_prompt_rejected(raw_prompt);
}

#[test]
fn context_snapshot_round_trips() {
    let mut ctx = Context::with_global(json!({"workflow": "wf"}));
    ctx.state = json!({"counter": 1});
    let snapshot = ContextSnapshot::from(&ctx);
    let json_text = serde_json::to_string(&snapshot).expect("snapshot serializes");
    let parsed: ContextSnapshot = serde_json::from_str(&json_text).expect("snapshot parses");
    let restored: Context = parsed.into();
    assert_eq!(restored.state, ctx.state);
    assert_eq!(restored.global.as_deref(), ctx.global.as_deref());
}

#[test]
fn workflow_error_codes_match_catalog() {
    assert_eq!(
        WorkflowError::TaskNotFound("t".to_owned()).code(),
        "SKALD_WORKFLOW_404_TASK"
    );
    assert_eq!(
        WorkflowError::AgentNotFound("a".to_owned()).code(),
        "SKALD_WORKFLOW_404_AGENT"
    );
    assert_eq!(
        WorkflowError::DependencyNotFound("d".to_owned()).code(),
        "WYRD_WORKFLOW_422_MISSING_DEPENDENCY"
    );
    assert_eq!(
        WorkflowError::TaskAlreadyExists("t".to_owned()).code(),
        "WYRD_WORKFLOW_422_DUPLICATE_STEP_ID"
    );
    assert_eq!(
        WorkflowError::TaskDependsOnItself("t".to_owned()).code(),
        "WYRD_WORKFLOW_422_MISSING_DEPENDENCY"
    );
    assert_eq!(
        WorkflowError::MaxRetriesExceeded("t".to_owned()).code(),
        "SKALD_WORKFLOW_500_MAX_RETRIES"
    );
    assert_eq!(
        WorkflowError::ResponseValidationFailed {
            task_id: "t".to_owned(),
            expected_schema: "echo".to_owned(),
            received: "bad".to_owned(),
        }
        .code(),
        "SKALD_WORKFLOW_422_OUTPUT_SCHEMA"
    );
    assert_eq!(
        WorkflowError::Stalled(vec!["t".to_owned()]).code(),
        "SKALD_WORKFLOW_500_STALLED"
    );
    assert_eq!(
        WorkflowError::UnsupportedHandoff {
            src: ProviderName::OpenAi,
            dst: ProviderName::Anthropic,
        }
        .code(),
        "SKALD_WORKFLOW_501_UNSUPPORTED_HANDOFF"
    );
    assert_eq!(
        WorkflowError::Cycle("t".to_owned()).code(),
        "WYRD_WORKFLOW_422_CYCLE"
    );
    assert_eq!(WorkflowError::Lock.code(), "SKALD_WORKFLOW_500_LOCK");
}
