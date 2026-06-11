#![allow(dead_code)]

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::Utc;
use serde_json::{Value, json};
use skald_prompt::{OpenAiChatOptions, Prompt, ResponseFormat, openai_chat};
use skald_runtime::{MockProvider, ProviderRegistry};
use skald_spec::wire::openai_chat::{
    OpenAiChatChoice, OpenAiChatMessage, OpenAiChatResponse, OpenAiMessageContent,
};
use skald_spec::{ProviderName, ProviderResponse};
use uuid::Uuid;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardName, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::eval::record::EvalRecordObservation;
use wyrd_spec::vala::eval::{
    AssertionTask, ComparisonOperator, EvalScenario, EvalSpec, EvalTask, JsonPath, LlmJudgeTask,
    RecordId, ScenarioId, ScenarioTask, TaskId,
};
use wyrd_spec::vala::ids::RunId;
use wyrd_spec::version::VersionBlock;

pub fn tid(value: &str) -> TaskId {
    TaskId::new(value).expect("static task id is valid")
}

pub fn sid(value: &str) -> ScenarioId {
    ScenarioId::new(value).expect("static scenario id is valid")
}

pub fn card_ref(kind: CardKind, name: &str) -> CardRef {
    CardRef {
        kind,
        name: CardName::new(name).expect("static card name is valid"),
        version: VersionBlock::parse("1.0.0").expect("static version is valid"),
        space: Some(SpaceName::new("tests").expect("static space is valid")),
        uid: None,
    }
}

pub fn eval_ref() -> CardRef {
    card_ref(CardKind::Eval, "orchestrator-rubric")
}

pub fn subject_ref() -> CardRef {
    card_ref(CardKind::Agent, "agent-under-test")
}

pub fn judge_ref() -> CardRef {
    card_ref(CardKind::Prompt, "judge")
}

pub fn scenario(
    id: &str,
    predefined_turns: Vec<&str>,
    termination_signal: Option<&str>,
    max_turns: u32,
) -> EvalScenario {
    EvalScenario {
        id: sid(id),
        initial_query: "Start".to_owned(),
        expected_outcome: Some("The agent says DONE.".to_owned()),
        predefined_turns: predefined_turns.into_iter().map(str::to_owned).collect(),
        simulated_user_persona: Some("A concise product user.".to_owned()),
        termination_signal: termination_signal.map(str::to_owned),
        max_turns,
        tasks: vec![ScenarioTask {
            id: tid("final_contains_done"),
            operator: ComparisonOperator::Contains,
            expected: json!("DONE"),
            condition: None,
        }],
    }
}

pub fn assertion_task(id: &str) -> EvalTask {
    EvalTask::Assertion(AssertionTask {
        id: tid(id),
        context_path: Some(JsonPath::new("$.ok").expect("static JSONPath is valid")),
        item_context_path: None,
        operator: ComparisonOperator::Equals,
        expected: json!(true),
        depends_on: Vec::new(),
        condition: None,
    })
}

pub fn judge_task(id: &str) -> EvalTask {
    EvalTask::LlmJudge(LlmJudgeTask {
        id: tid(id),
        judge_ref: judge_ref(),
        context_path: None,
        expected: json!({"passed": true}),
        operator: ComparisonOperator::Equals,
        depends_on: Vec::new(),
        max_retries: 0,
        condition: None,
    })
}

pub fn spec(tasks: Vec<EvalTask>) -> EvalSpec {
    let tasks = tasks
        .into_iter()
        .map(|task| (task.id().clone(), task))
        .collect::<BTreeMap<_, _>>();
    EvalSpec {
        subject_ref: Some(subject_ref()),
        dataset: None,
        tasks,
        workflow: None,
        sampling: None,
        pass_gate: None,
        context_capture: None,
    }
}

pub fn record(run_id: &RunId, ok: bool) -> EvalRecordObservation {
    static NEXT_RECORD: AtomicU64 = AtomicU64::new(1);
    let record_id = u128::from(NEXT_RECORD.fetch_add(1, Ordering::Relaxed));
    EvalRecordObservation {
        record_id: RecordId(Uuid::from_u128(record_id)),
        run_id: run_id.clone(),
        session_id: None,
        eval_ref: eval_ref(),
        context: json!({ "ok": ok }),
        trace_id: None,
        span_id: None,
        created_at: Utc::now(),
    }
}

pub fn structured_prompt() -> Prompt {
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["passed"],
        "properties": { "passed": { "type": "boolean" } }
    });
    openai_chat(
        "gpt-test",
        OpenAiChatOptions {
            messages: vec!["Judge: {{response}}".to_owned()],
            variables: vec!["response".to_owned()],
            output: Some(ResponseFormat::json_schema("judge_result", schema).expect("schema")),
            ..OpenAiChatOptions::default()
        },
    )
    .expect("static prompt is valid")
}

pub fn registry_returning_text(text: &str) -> Arc<ProviderRegistry> {
    let mock = MockProvider::new(ProviderName::OpenAi);
    mock.push_response(openai_text_response(text));
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(mock));
    Arc::new(providers)
}

pub fn registry_returning_texts(texts: &[&str]) -> Arc<ProviderRegistry> {
    let mock = MockProvider::new(ProviderName::OpenAi);
    for text in texts {
        mock.push_response(openai_text_response(text));
    }
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(mock));
    Arc::new(providers)
}

pub fn openai_text_response(text: &str) -> ProviderResponse {
    ProviderResponse::OpenAiChatCompletion(OpenAiChatResponse {
        id: "resp_1".to_owned(),
        object: "chat.completion".to_owned(),
        created: 0,
        model: "gpt-test".to_owned(),
        choices: vec![OpenAiChatChoice {
            index: 0,
            message: OpenAiChatMessage {
                role: "assistant".to_owned(),
                content: Some(OpenAiMessageContent::Text(text.to_owned())),
                ..Default::default()
            },
            finish_reason: Some("stop".to_owned()),
            logprobs: None,
        }],
        usage: None,
        system_fingerprint: None,
        service_tier: None,
    })
}

pub fn fixture_value(raw: &str) -> Value {
    serde_json::from_str(raw).expect("fixture JSON is valid")
}
