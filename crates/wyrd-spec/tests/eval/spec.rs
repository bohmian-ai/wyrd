use std::collections::BTreeMap;

use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::CardName;
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::eval::assertion::AssertionTask;
use wyrd_spec::vala::eval::condition::{ConditionCombinator, EvalCondition};
use wyrd_spec::vala::eval::ids::{JsonPath, TaskId};
use wyrd_spec::vala::eval::llm_judge::LlmJudgeTask;
use wyrd_spec::vala::eval::operator::ComparisonOperator;
use wyrd_spec::vala::eval::result::{AssertionResult, EvalContextCapture, EvalPassGate};
use wyrd_spec::vala::eval::spec::{DatasetRef, EvalSampling, EvalSpec, MAX_EVAL_TASKS};
use wyrd_spec::vala::eval::task::EvalTask;
use wyrd_spec::version::VersionBlock;

fn data_ref(name: &str) -> CardRef {
    CardRef {
        kind: CardKind::Data,
        name: CardName::new(name).unwrap(),
        version: VersionBlock::parse("1.0.0").unwrap(),
        space: None,
        uid: None,
    }
}

fn prompt_ref(name: &str) -> CardRef {
    CardRef {
        kind: CardKind::Prompt,
        name: CardName::new(name).unwrap(),
        version: VersionBlock::parse("1.0.0").unwrap(),
        space: None,
        uid: None,
    }
}

fn one_task_map() -> BTreeMap<TaskId, EvalTask> {
    let mut tasks = BTreeMap::new();
    let id = TaskId::new("a").unwrap();
    tasks.insert(
        id.clone(),
        EvalTask::Assertion(AssertionTask {
            id,
            context_path: Some(JsonPath::new("$.x").unwrap()),
            item_context_path: None,
            operator: ComparisonOperator::IsNotNull,
            expected: serde_json::Value::Null,
            depends_on: vec![],
            condition: None,
        }),
    );
    tasks
}

#[test]
fn spec_new_validates_dag() {
    EvalSpec::new(one_task_map()).expect("valid");
}

#[test]
fn spec_minimal_round_trip() {
    let spec = EvalSpec::new(one_task_map()).unwrap();
    let serialized = serde_json::to_string(&spec).unwrap();
    let back: EvalSpec = serde_json::from_str(&serialized).unwrap();
    assert_eq!(spec, back);
}

#[test]
fn spec_with_full_fields_round_trip() {
    let mut spec = EvalSpec::new(one_task_map()).unwrap();
    spec.target_ref = Some(prompt_ref("retriever-quality"));
    spec.dataset = Some(DatasetRef::new(data_ref("eval-set")).unwrap());
    spec.sampling = Some(EvalSampling::Ratio { ratio: 0.1 });
    spec.pass_gate = Some(EvalPassGate::OverallPassRate { threshold: 0.9 });

    let serialized = serde_json::to_string(&spec).unwrap();
    let back: EvalSpec = serde_json::from_str(&serialized).unwrap();
    assert_eq!(spec, back);
}

#[test]
fn dataset_ref_rejects_non_data_kind() {
    let err = DatasetRef::new(prompt_ref("prompt")).unwrap_err();
    assert_eq!(err.code(), "WYRD_VALA_400_EVAL_REF_KIND_MISMATCH");
}

#[test]
fn validate_catches_deserialized_task_key_id_mismatch() {
    let spec = EvalSpec::new(one_task_map()).unwrap();
    let mut value = serde_json::to_value(&spec).unwrap();
    let tasks = value["tasks"].as_object_mut().unwrap();
    let task = tasks.remove("a").unwrap();
    tasks.insert("map_key_only".to_string(), task);

    let bad: EvalSpec = serde_json::from_value(value).unwrap();
    let err = bad.validate().unwrap_err();
    let public: WyrdError = err.into();

    assert_eq!(public.code(), "WYRD_SPEC_400_VALIDATION");
    assert!(public.to_string().contains("must match inner task id"));
}

#[test]
fn new_catches_task_key_id_mismatch() {
    let mut tasks = one_task_map();
    let task = tasks.remove(&TaskId::new("a").unwrap()).unwrap();
    tasks.insert(TaskId::new("map_key_only").unwrap(), task);

    let err = EvalSpec::new(tasks).unwrap_err();
    let public: WyrdError = err.into();

    assert_eq!(public.code(), "WYRD_SPEC_400_VALIDATION");
    assert!(public.to_string().contains("must match inner task id"));
}

#[test]
fn validate_catches_cycle_after_mutation() {
    let mut spec = EvalSpec::new(one_task_map()).unwrap();
    let id = TaskId::new("b").unwrap();
    spec.tasks.insert(
        id.clone(),
        EvalTask::Assertion(AssertionTask {
            id: id.clone(),
            context_path: Some(JsonPath::new("$.y").unwrap()),
            item_context_path: None,
            operator: ComparisonOperator::IsNotNull,
            expected: serde_json::Value::Null,
            depends_on: vec![TaskId::new("a").unwrap()],
            condition: None,
        }),
    );

    if let EvalTask::Assertion(assertion) = spec.tasks.get_mut(&TaskId::new("a").unwrap()).unwrap()
    {
        assertion.depends_on.push(id);
    }

    let err = spec.validate().unwrap_err();
    let public: WyrdError = err.into();
    assert_eq!(public.code(), "WYRD_VALA_400_TASK_DAG_INVALID");
}

#[test]
fn validate_catches_bad_sampling() {
    let mut spec = EvalSpec::new(one_task_map()).unwrap();
    spec.sampling = Some(EvalSampling::Ratio { ratio: 2.0 });
    assert!(spec.validate().is_err());
}

#[test]
fn validate_catches_bad_pass_gate() {
    let mut spec = EvalSpec::new(one_task_map()).unwrap();
    spec.pass_gate = Some(EvalPassGate::PerJudgePassRate { threshold: -0.1 });
    assert!(spec.validate().is_err());
}

#[test]
fn spec_with_workflow_round_trip() {
    use wyrd_spec::vala::eval::workflow::{Workflow, WorkflowFieldType};

    let mut spec = EvalSpec::new(one_task_map()).unwrap();
    let mut fields = BTreeMap::new();
    fields.insert("response".to_string(), WorkflowFieldType::String);
    fields.insert("score".to_string(), WorkflowFieldType::Float);
    fields.insert("tool_calls".to_string(), WorkflowFieldType::Array);
    spec.workflow = Some(Workflow { fields });

    let serialized = serde_json::to_string(&spec).unwrap();
    let back: EvalSpec = serde_json::from_str(&serialized).unwrap();
    assert_eq!(spec, back);
}

#[test]
fn sampling_deterministic_validates_bucket_lt_modulus() {
    let bad = EvalSampling::DeterministicByHash {
        key_path: JsonPath::new("$.user_id").unwrap(),
        modulus: 10,
        bucket: 10,
    };
    assert!(bad.validate().is_err());
}

#[test]
fn sampling_every_nth_validates_nonzero() {
    assert!(EvalSampling::EveryNth { n: 0 }.validate().is_err());
}

#[test]
fn sampling_deterministic_rejects_zero_modulus() {
    let bad = EvalSampling::DeterministicByHash {
        key_path: JsonPath::new("$.user_id").unwrap(),
        modulus: 0,
        bucket: 0,
    };
    assert!(bad.validate().is_err());
}

#[test]
fn validate_catches_llm_judge_with_bad_condition() {
    let mut spec = EvalSpec::new(one_task_map()).unwrap();
    let judge_id = TaskId::new("judge").unwrap();
    let mut judge = LlmJudgeTask::new(
        judge_id.clone(),
        prompt_ref("judge-prompt"),
        ComparisonOperator::ContainsIgnoreCase,
        serde_json::json!("pass"),
    )
    .unwrap();
    judge.condition = Some(EvalCondition {
        path: JsonPath::new("$.flag").unwrap(),
        operator: ComparisonOperator::IsTruthy,
        expected: serde_json::Value::Null,
        combinator: Some(ConditionCombinator::And),
        subsequent: None,
    });
    spec.tasks.insert(judge_id, EvalTask::LlmJudge(judge));
    assert!(spec.validate().is_err());
}

#[test]
fn spec_rejects_over_max_tasks() {
    let mut tasks = BTreeMap::new();
    for i in 0..=MAX_EVAL_TASKS {
        let id = TaskId::new(format!("t{i}")).unwrap();
        tasks.insert(
            id.clone(),
            EvalTask::Assertion(AssertionTask {
                id,
                context_path: Some(JsonPath::new("$.x").unwrap()),
                item_context_path: None,
                operator: ComparisonOperator::IsNotNull,
                expected: serde_json::Value::Null,
                depends_on: vec![],
                condition: None,
            }),
        );
    }
    assert!(EvalSpec::new(tasks).is_err());
}

#[test]
fn validate_catches_invalid_regex_pattern() {
    let mut spec = EvalSpec::new(one_task_map()).unwrap();
    let id = TaskId::new("rx").unwrap();
    spec.tasks.insert(
        id.clone(),
        EvalTask::Assertion(AssertionTask {
            id,
            context_path: Some(JsonPath::new("$.x").unwrap()),
            item_context_path: None,
            operator: ComparisonOperator::MatchesRegex {
                pattern: "([unclosed".to_string(),
            },
            expected: serde_json::Value::Null,
            depends_on: vec![],
            condition: None,
        }),
    );
    assert!(spec.validate().is_err());
}

#[test]
fn validate_catches_overlong_regex_pattern() {
    use wyrd_spec::vala::eval::operator::MAX_REGEX_PATTERN_LEN;
    let mut spec = EvalSpec::new(one_task_map()).unwrap();
    let id = TaskId::new("rx").unwrap();
    spec.tasks.insert(
        id.clone(),
        EvalTask::Assertion(AssertionTask {
            id,
            context_path: Some(JsonPath::new("$.x").unwrap()),
            item_context_path: None,
            operator: ComparisonOperator::MatchesRegex {
                pattern: "a".repeat(MAX_REGEX_PATTERN_LEN + 1),
            },
            expected: serde_json::Value::Null,
            depends_on: vec![],
            condition: None,
        }),
    );
    assert!(spec.validate().is_err());
}

#[test]
fn spec_execution_plan_returns_stages() {
    let spec = EvalSpec::new(one_task_map()).unwrap();
    let plan = spec.execution_plan().unwrap();
    assert_eq!(plan.task_count, 1);
    assert_eq!(plan.stages.len(), 1);
    assert_eq!(plan.stages[0].index, 0);
    assert_eq!(plan.stages[0].tasks[0].as_str(), "a");
}

#[test]
fn eval_context_capture_round_trips() {
    for variant in [
        EvalContextCapture::Full,
        EvalContextCapture::Hash,
        EvalContextCapture::Redact,
    ] {
        let s = serde_json::to_string(&variant).unwrap();
        let back: EvalContextCapture = serde_json::from_str(&s).unwrap();
        assert_eq!(variant, back);
    }
}

#[test]
fn eval_spec_with_context_capture_redact_round_trips() {
    let mut spec = EvalSpec::new(one_task_map()).unwrap();
    spec.context_capture = Some(EvalContextCapture::Redact);
    let s = serde_json::to_string(&spec).unwrap();
    let back: EvalSpec = serde_json::from_str(&s).unwrap();
    assert_eq!(spec, back);
    assert_eq!(back.context_capture, Some(EvalContextCapture::Redact));
}

#[test]
fn assertion_result_actual_is_optional() {
    use chrono::{TimeZone, Utc};
    let r = AssertionResult {
        task_id: TaskId::new("a").unwrap(),
        passed: true,
        actual: None,
        expected: serde_json::json!("ok"),
        operator: ComparisonOperator::Equals,
        message: None,
        stage: 0,
        started_at: Utc.timestamp_opt(0, 0).unwrap(),
        duration_ms: 0,
    };
    let s = serde_json::to_string(&r).unwrap();
    assert!(
        !s.contains("actual"),
        "actual field must be omitted when None"
    );
    let back: AssertionResult = serde_json::from_str(&s).unwrap();
    assert_eq!(back.actual, None);
}

#[test]
fn dataset_ref_deserialization_rejects_non_data_kind() {
    let json = r#"{"kind":"Prompt","name":"my-prompt","version":"1.0.0"}"#;
    let result: Result<DatasetRef, _> = serde_json::from_str(json);
    assert!(result.is_err());
}
