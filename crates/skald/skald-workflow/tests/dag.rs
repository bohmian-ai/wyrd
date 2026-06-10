use std::collections::HashSet;
use std::sync::Arc;

use skald_agent::RunConfig;
use skald_runtime::{MockProvider, ProviderRegistry};
use skald_spec::wire::openai_chat::{
    OpenAiChatChoice, OpenAiChatMessage, OpenAiChatRequest, OpenAiChatResponse,
    OpenAiMessageContent,
};
use skald_spec::{Prompt, ProviderName, ProviderRequest, ProviderResponse, ResponseType};
use skald_workflow::WorkflowAgent;
use skald_workflow::{
    Context, DagExecutor, TaskDef, TaskList, TaskStatus, WorkflowDef, WorkflowError,
    default_max_retries,
};

fn openai_prompt(text: &str) -> Prompt {
    Prompt {
        request: ProviderRequest::OpenAiChatCompletion(OpenAiChatRequest {
            model: "gpt-4o".to_owned(),
            messages: vec![OpenAiChatMessage {
                role: "user".to_owned(),
                content: Some(OpenAiMessageContent::Text(text.to_owned())),
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
        }),
        model: "gpt-4o".to_owned(),
        version: None,
        variables: Vec::new(),
        media_variables: Vec::new(),
        response_type: ResponseType::Text,
    }
}

fn openai_response(content: &str) -> ProviderResponse {
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

fn agent_def() -> WorkflowAgent {
    WorkflowAgent {
        id: "agent".to_owned(),
        prompt: openai_prompt("agent"),
        run_config: RunConfig::default(),
    }
}

fn task_def(id: &str, dependencies: Vec<&str>) -> TaskDef {
    TaskDef {
        id: id.to_owned(),
        agent_id: "agent".to_owned(),
        prompt: openai_prompt(id),
        dependencies: dependencies.into_iter().map(str::to_owned).collect(),
        max_retries: default_max_retries(),
    }
}

fn workflow_def(tasks: Vec<TaskDef>) -> WorkflowDef {
    WorkflowDef {
        id: "workflow".to_owned(),
        name: "DAG workflow".to_owned(),
        agents: vec![agent_def()],
        tasks,
    }
}

async fn build_workflow(def: WorkflowDef, responses: usize) -> Arc<DagExecutor> {
    let mock = MockProvider::new(ProviderName::OpenAi);
    for index in 0..responses {
        mock.push_response(openai_response(&format!("ok-{index}")));
    }
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(mock));
    Arc::new(
        DagExecutor::build(def, &providers)
            .await
            .expect("workflow builds"),
    )
}

async fn build_error(def: WorkflowDef) -> WorkflowError {
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(MockProvider::new(ProviderName::OpenAi)));
    match DagExecutor::build(def, &providers).await {
        Ok(_) => panic!("workflow unexpectedly built"),
        Err(err) => err,
    }
}

#[tokio::test]
async fn linear_dag_executes_in_topological_order() {
    let workflow = build_workflow(
        workflow_def(vec![
            task_def("a", vec![]),
            task_def("b", vec!["a"]),
            task_def("c", vec!["b"]),
        ]),
        3,
    )
    .await;

    workflow.run(Context::new()).await.expect("linear DAG runs");

    let plan = workflow.execution_plan().expect("plan builds");
    assert_eq!(plan.len(), 3);
    assert_eq!(plan.get(&0), Some(&HashSet::from(["a".to_owned()])));
    assert_eq!(plan.get(&1), Some(&HashSet::from(["b".to_owned()])));
    assert_eq!(plan.get(&2), Some(&HashSet::from(["c".to_owned()])));
    assert!(workflow.task_list().is_complete().expect("status reads"));
}

#[tokio::test]
async fn diamond_dag_places_siblings_in_one_level() {
    let workflow = build_workflow(
        workflow_def(vec![
            task_def("a", vec![]),
            task_def("b", vec!["a"]),
            task_def("c", vec!["a"]),
            task_def("d", vec!["b", "c"]),
        ]),
        4,
    )
    .await;

    let plan = workflow.execution_plan().expect("plan builds");
    assert_eq!(plan.get(&0), Some(&HashSet::from(["a".to_owned()])));
    assert_eq!(
        plan.get(&1),
        Some(&HashSet::from(["b".to_owned(), "c".to_owned()]))
    );
    assert_eq!(plan.get(&2), Some(&HashSet::from(["d".to_owned()])));

    workflow
        .run(Context::new())
        .await
        .expect("diamond DAG runs");
    assert!(workflow.task_list().is_complete().expect("status reads"));
}

#[tokio::test]
async fn workflow_build_accepts_dependencies_declared_later() {
    let workflow = build_workflow(
        workflow_def(vec![
            task_def("c", vec!["b"]),
            task_def("a", vec![]),
            task_def("b", vec!["a"]),
        ]),
        3,
    )
    .await;

    assert_eq!(
        workflow.task_list().execution_order(),
        &["a".to_owned(), "b".to_owned(), "c".to_owned()]
    );
    workflow
        .run(Context::new())
        .await
        .expect("dependency-ordered insertion runs");
}

#[tokio::test]
async fn workflow_build_rejects_duplicate_id() {
    let err = build_error(workflow_def(vec![
        task_def("a", vec![]),
        task_def("a", vec![]),
    ]))
    .await;
    assert_eq!(err.code(), "WYRD_WORKFLOW_422_DUPLICATE_STEP_ID");
}

#[tokio::test]
async fn workflow_build_rejects_self_dependency() {
    let err = build_error(workflow_def(vec![task_def("a", vec!["a"])])).await;
    assert_eq!(err.code(), "WYRD_WORKFLOW_422_MISSING_DEPENDENCY");
}

#[tokio::test]
async fn workflow_build_rejects_missing_dependency() {
    let err = build_error(workflow_def(vec![task_def("a", vec!["missing"])])).await;
    assert_eq!(err.code(), "WYRD_WORKFLOW_422_MISSING_DEPENDENCY");
}

#[test]
fn workflow_def_validate_graph_rejects_cycle() {
    let err = workflow_def(vec![
        task_def("a", vec!["c"]),
        task_def("b", vec!["a"]),
        task_def("c", vec!["b"]),
    ])
    .validate_graph()
    .expect_err("cycle fails");
    assert_eq!(err.code(), "WYRD_WORKFLOW_422_CYCLE");
}

#[test]
fn tasklist_add_task_rejects_duplicate_id() {
    let mut tasks = TaskList::new();
    tasks
        .add_task(task_def("a", vec![]))
        .expect("first add works");
    let err = tasks
        .add_task(task_def("a", vec![]))
        .expect_err("duplicate task fails");
    assert_eq!(err.code(), "WYRD_WORKFLOW_422_DUPLICATE_STEP_ID");
}

#[test]
fn tasklist_add_task_rejects_self_dependency() {
    let mut tasks = TaskList::new();
    let err = tasks
        .add_task(task_def("a", vec!["a"]))
        .expect_err("self dependency fails");
    assert_eq!(err.code(), "WYRD_WORKFLOW_422_MISSING_DEPENDENCY");
}

#[test]
fn tasklist_add_task_rejects_missing_dependency() {
    let mut tasks = TaskList::new();
    let err = tasks
        .add_task(task_def("a", vec!["missing"]))
        .expect_err("missing dependency fails");
    assert_eq!(err.code(), "WYRD_WORKFLOW_422_MISSING_DEPENDENCY");
}

#[tokio::test]
async fn execution_order_is_deterministic() {
    let workflow = build_workflow(
        workflow_def(vec![
            task_def("c", vec![]),
            task_def("a", vec![]),
            task_def("b", vec![]),
        ]),
        3,
    )
    .await;

    assert_eq!(
        workflow.task_list().execution_order(),
        &["a".to_owned(), "b".to_owned(), "c".to_owned()]
    );
}

#[tokio::test]
async fn runtime_stall_reports_pending_ids() {
    let workflow = build_workflow(
        workflow_def(vec![task_def("a", vec![]), task_def("b", vec!["a"])]),
        0,
    )
    .await;
    let task = workflow.task_list().get_task("a").expect("task exists");
    task.write().expect("task lock opens").status = TaskStatus::Failed;

    let err = workflow
        .run(Context::new())
        .await
        .expect_err("failed dependency stalls");
    assert_eq!(err.code(), "SKALD_WORKFLOW_500_STALLED");
    match err {
        WorkflowError::Stalled(ids) => assert_eq!(ids, vec!["a".to_owned(), "b".to_owned()]),
        other => panic!("expected stalled error, got {other:?}"),
    }
}
