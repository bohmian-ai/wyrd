//! Eval primitive types — the typed spec body for the `Eval` card kind.
//!
//! Authors declare evaluations as DAGs of typed tasks. Each task references
//! the workflow context (or scenario `{response, expected_outcome}`) via
//! `JsonPath`, compares the extracted value against an `expected` payload
//! using one of ~56 `ComparisonOperator` variants, and reports
//! `AssertionResult` per-task. The DAG is topologically validated at spec
//! load (see `validate_dag` / Lock #12).
//!
//! Runtime execution lives in `vala-eval` (PR4.4). Python wrappers live in
//! `python/py-wyrd` (PR4.7). `wyrd-spec` ships only the contracts.
//!
//! Module map:
//! - `ids` — `TaskId`, `SessionId`, `RecordId`, `WorkflowUid`, `EntityUid`,
//!   `TraceId`, `SpanId`, `JsonPath`.
//! - `media` — `MediaRef` (URI-bearing descriptor; no inline blobs).
//! - `operator` — `ComparisonOperator`, `JsonValueType`,
//!   `DivergenceMetric`.
//! - `status` — `EvalStatus` (mirrors `eval_inbox.status`).
//! - `workflow` — `Workflow`, `WorkflowFieldType` (optional declared shape).
//! - `condition` — `EvalCondition` gate predicates shared by tasks.
//! - `assertion` — `AssertionTask` programmatic assertions.
//! - `llm_judge` — `LlmJudgeTask` Agent-backed judge task.
//! - `trace` — `TraceAssertionTask` trace document assertions.
//! - `agent` — `AgentAssertionTask` workflow envelope assertions.
//! - `task` — `EvalTask` executable task enum.
//! - `plan` — DAG validation and topological execution stages.
//! - `result` — per-task result records and workflow-level pass gates.
//! - `spec` — `EvalSpec`, dataset references, and sampling policy.
//! - `scenario` — offline scenario payloads for eval DataCards.
//!
//! Later commits add the remaining spec modules.

pub mod agent;
pub mod assertion;
pub mod condition;
pub mod ids;
pub mod llm_judge;
/// Media reference descriptors for eval records — object-storage URIs, no inline blobs.
pub mod media;
pub mod operator;
pub mod plan;
pub mod protocol;
pub mod record;
pub mod result;
pub mod scenario;
pub mod spec;
pub mod status;
pub mod task;
pub mod trace;
pub mod workflow;

// ─── Public re-exports ───────────────────────────────────────────────────
//
// External crates should reach this surface through these names.
// Adding a re-export here is a public-API change; review per AGENTS.md §3.

pub use agent::AgentAssertionTask;
pub use assertion::AssertionTask;
pub use condition::{ConditionCombinator, EvalCondition, MAX_CONDITION_DEPTH};
pub use ids::{
    EntityUid, JsonPath, LeaseToken, RecordId, RunId, ScenarioId, SessionId, SpanId, TaskId,
    TraceId, WorkflowUid,
};
pub use llm_judge::LlmJudgeTask;
pub use media::MediaRef;
pub use operator::{ComparisonOperator, DivergenceMetric, JsonValueType};
pub use plan::{DagError, ExecutionPlan, Stage, validate_dag};
pub use protocol::{
    AgentTurnSubmission, ConversationTurn, EvalRunOpenRequest, EvalRunOpenResponse,
    MAX_HISTORY_TURNS, ProtocolError, SimulatedUserMode, SimulatedUserTurn, TurnDirective,
    TurnRole, UserTurnSubmission,
};
pub use record::EvalRecordObservation;
pub use result::{AssertionResult, EvalContextCapture, EvalPassGate};
pub use scenario::{
    EvalScenario, EvalScenarioCollection, MAX_SCENARIOS_PER_COLLECTION, MAX_TURNS_HARD_CAP,
    ScenarioTask,
};
pub use spec::{DatasetRef, EvalSampling, EvalSpec, EvalSpecError, MAX_EVAL_TASKS};
pub use status::EvalStatus;
pub use task::EvalTask;
pub use trace::TraceAssertionTask;
pub use workflow::{Workflow, WorkflowFieldType};

#[cfg(test)]
mod agent_assertion_tests {
    use crate::vala::eval::agent::AgentAssertionTask;
    use crate::vala::eval::ids::{JsonPath, TaskId};
    use crate::vala::eval::operator::ComparisonOperator;

    #[test]
    fn agent_assertion_round_trip() {
        let task = AgentAssertionTask {
            id: TaskId::new("agent_one").unwrap(),
            workflow_field_path: JsonPath::new("$.tool_calls[0].name").unwrap(),
            operator: ComparisonOperator::Equals,
            expected: serde_json::json!("search"),
            depends_on: vec![],
            condition: None,
        };

        let serialized = serde_json::to_string(&task).unwrap();
        let deserialized: AgentAssertionTask = serde_json::from_str(&serialized).unwrap();
        assert_eq!(task, deserialized);
    }
}

#[cfg(test)]
mod assertion_tests {
    use crate::vala::eval::assertion::AssertionTask;
    use crate::vala::eval::ids::{JsonPath, TaskId};
    use crate::vala::eval::operator::ComparisonOperator;

    #[test]
    fn assertion_round_trip_minimal() {
        let task = AssertionTask {
            id: TaskId::new("step_one").unwrap(),
            context_path: Some(JsonPath::new("$.response").unwrap()),
            item_context_path: None,
            operator: ComparisonOperator::Contains,
            expected: serde_json::json!("hello"),
            depends_on: vec![],
            condition: None,
        };

        let serialized = serde_json::to_string(&task).unwrap();
        let deserialized: AssertionTask = serde_json::from_str(&serialized).unwrap();
        assert_eq!(task, deserialized);
    }

    #[test]
    fn assertion_round_trip_with_depends_on_and_condition() {
        use crate::vala::eval::condition::EvalCondition;

        let task = AssertionTask {
            id: TaskId::new("step_two").unwrap(),
            context_path: None,
            item_context_path: Some(JsonPath::new("$.docs").unwrap()),
            operator: ComparisonOperator::IsNonEmpty,
            expected: serde_json::Value::Null,
            depends_on: vec![TaskId::new("step_one").unwrap()],
            condition: Some(EvalCondition {
                path: JsonPath::new("$.flag").unwrap(),
                operator: ComparisonOperator::IsTruthy,
                expected: serde_json::Value::Null,
                combinator: None,
                subsequent: None,
            }),
        };

        let serialized = serde_json::to_string(&task).unwrap();
        let deserialized: AssertionTask = serde_json::from_str(&serialized).unwrap();
        assert_eq!(task, deserialized);
    }

    #[test]
    fn assertion_rejects_unknown_field() {
        let json = r#"{
            "id":"a","operator":"equals","expected":0,
            "context_path":"$.x","extra":"nope"
        }"#;
        let result: Result<AssertionTask, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod condition_tests {
    use crate::vala::eval::condition::{ConditionCombinator, EvalCondition, MAX_CONDITION_DEPTH};
    use crate::vala::eval::ids::JsonPath;
    use crate::vala::eval::operator::ComparisonOperator;

    fn simple_condition() -> EvalCondition {
        EvalCondition {
            path: JsonPath::new("$.response").unwrap(),
            operator: ComparisonOperator::Contains,
            expected: serde_json::json!("Paris"),
            combinator: None,
            subsequent: None,
        }
    }

    #[test]
    fn single_node_serde_round_trip() {
        let condition = simple_condition();
        let serialized = serde_json::to_string(&condition).unwrap();
        let deserialized: EvalCondition = serde_json::from_str(&serialized).unwrap();
        assert_eq!(condition, deserialized);
    }

    #[test]
    fn single_node_validates() {
        simple_condition().validate().unwrap();
    }

    #[test]
    fn chain_two_nodes_and() {
        let subsequent = simple_condition();
        let condition = EvalCondition {
            path: JsonPath::new("$.score").unwrap(),
            operator: ComparisonOperator::GreaterThan,
            expected: serde_json::json!(0.5),
            combinator: Some(ConditionCombinator::And),
            subsequent: Some(Box::new(subsequent)),
        };
        assert_eq!(condition.depth(), 2);
        condition.validate().unwrap();
    }

    #[test]
    fn combinator_without_subsequent_rejected() {
        let condition = EvalCondition {
            path: JsonPath::new("$.x").unwrap(),
            operator: ComparisonOperator::Equals,
            expected: serde_json::json!(0),
            combinator: Some(ConditionCombinator::And),
            subsequent: None,
        };
        assert!(condition.validate().is_err());
    }

    #[test]
    fn subsequent_without_combinator_rejected() {
        let condition = EvalCondition {
            path: JsonPath::new("$.x").unwrap(),
            operator: ComparisonOperator::Equals,
            expected: serde_json::json!(0),
            combinator: None,
            subsequent: Some(Box::new(simple_condition())),
        };
        assert!(condition.validate().is_err());
    }

    #[test]
    fn chain_depth_exceeds_max_rejected() {
        let mut current = simple_condition();
        for _ in 0..MAX_CONDITION_DEPTH {
            current = EvalCondition {
                path: JsonPath::new("$.x").unwrap(),
                operator: ComparisonOperator::Equals,
                expected: serde_json::json!(0),
                combinator: Some(ConditionCombinator::And),
                subsequent: Some(Box::new(current)),
            };
        }

        assert!(current.depth() > MAX_CONDITION_DEPTH);
        assert!(current.validate().is_err());
    }

    #[test]
    fn rejects_unknown_field() {
        let json = r#"{
            "path": "$.x", "operator": "equals",
            "expected": 0, "noise": true
        }"#;
        let result: Result<EvalCondition, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod dag_cycle_tests {
    use std::collections::BTreeMap;

    use crate::error::WyrdError;
    use crate::vala::eval::assertion::AssertionTask;
    use crate::vala::eval::ids::{JsonPath, TaskId};
    use crate::vala::eval::operator::ComparisonOperator;
    use crate::vala::eval::plan::{DagError, validate_dag};
    use crate::vala::eval::task::EvalTask;

    fn task(id: &str, deps: &[&str]) -> (TaskId, EvalTask) {
        let task_id = TaskId::new(id).unwrap();
        let task = EvalTask::Assertion(AssertionTask {
            id: task_id.clone(),
            context_path: Some(JsonPath::new("$.x").unwrap()),
            item_context_path: None,
            operator: ComparisonOperator::IsNotNull,
            expected: serde_json::Value::Null,
            depends_on: deps.iter().map(|dep| TaskId::new(*dep).unwrap()).collect(),
            condition: None,
        });
        (task_id, task)
    }

    #[test]
    fn cycle_converts_to_public_vala_error_code() {
        let mut tasks = BTreeMap::new();
        let (task_id, eval_task) = task("a", &["b"]);
        tasks.insert(task_id, eval_task);
        let (task_id, eval_task) = task("b", &["a"]);
        tasks.insert(task_id, eval_task);

        let err = validate_dag(&tasks).unwrap_err();
        let public: WyrdError = err.into();

        assert_eq!(public.code(), "WYRD_VALA_400_TASK_DAG_INVALID");
    }

    #[test]
    fn triangle_cycle_returns_witness() {
        let mut tasks = BTreeMap::new();
        let (task_id, eval_task) = task("a", &["c"]);
        tasks.insert(task_id, eval_task);
        let (task_id, eval_task) = task("b", &["a"]);
        tasks.insert(task_id, eval_task);
        let (task_id, eval_task) = task("c", &["b"]);
        tasks.insert(task_id, eval_task);

        let err = validate_dag(&tasks).unwrap_err();
        match err {
            DagError::Cycle { cycle } => {
                let names: Vec<&str> = cycle.iter().map(TaskId::as_str).collect();
                assert_eq!(names.first(), Some(&"a"));
                assert_eq!(names.last(), Some(&"a"));
                assert_eq!(cycle.len() - 1, 3);
            }
            other => panic!("expected Cycle, got {other:?}"),
        }
    }

    #[test]
    fn smallest_cycle_witness_picks_3_over_5() {
        let mut tasks = BTreeMap::new();
        for (id, dep) in [
            ("a", "c"),
            ("b", "a"),
            ("c", "b"),
            ("v", "w"),
            ("w", "z"),
            ("x", "v"),
            ("y", "x"),
            ("z", "y"),
        ] {
            let (task_id, eval_task) = task(id, &[dep]);
            tasks.insert(task_id, eval_task);
        }

        let err = validate_dag(&tasks).unwrap_err();
        match err {
            DagError::Cycle { cycle } => {
                assert_eq!(cycle.len() - 1, 3, "smallest cycle is 3, got {cycle:?}");
            }
            other => panic!("expected Cycle, got {other:?}"),
        }
    }
}

#[cfg(test)]
mod dag_happy_tests {
    use std::collections::BTreeMap;

    use crate::vala::eval::assertion::AssertionTask;
    use crate::vala::eval::ids::{JsonPath, TaskId};
    use crate::vala::eval::operator::ComparisonOperator;
    use crate::vala::eval::plan::validate_dag;
    use crate::vala::eval::task::EvalTask;

    fn task(id: &str, deps: &[&str]) -> (TaskId, EvalTask) {
        let task_id = TaskId::new(id).unwrap();
        let task = EvalTask::Assertion(AssertionTask {
            id: task_id.clone(),
            context_path: Some(JsonPath::new("$.x").unwrap()),
            item_context_path: None,
            operator: ComparisonOperator::IsNotNull,
            expected: serde_json::Value::Null,
            depends_on: deps.iter().map(|dep| TaskId::new(*dep).unwrap()).collect(),
            condition: None,
        });
        (task_id, task)
    }

    #[test]
    fn linear_three_stages() {
        let mut tasks = BTreeMap::new();
        let (task_id, eval_task) = task("a", &[]);
        tasks.insert(task_id, eval_task);
        let (task_id, eval_task) = task("b", &["a"]);
        tasks.insert(task_id, eval_task);
        let (task_id, eval_task) = task("c", &["b"]);
        tasks.insert(task_id, eval_task);

        let plan = validate_dag(&tasks).unwrap();
        assert_eq!(plan.task_count, 3);
        assert_eq!(plan.stages.len(), 3);
        assert_eq!(plan.stages[0].tasks[0].as_str(), "a");
        assert_eq!(plan.stages[1].tasks[0].as_str(), "b");
        assert_eq!(plan.stages[2].tasks[0].as_str(), "c");
    }

    #[test]
    fn diamond_three_stages_parallel_middle() {
        let mut tasks = BTreeMap::new();
        let (task_id, eval_task) = task("a", &[]);
        tasks.insert(task_id, eval_task);
        let (task_id, eval_task) = task("b", &["a"]);
        tasks.insert(task_id, eval_task);
        let (task_id, eval_task) = task("c", &["a"]);
        tasks.insert(task_id, eval_task);
        let (task_id, eval_task) = task("d", &["b", "c"]);
        tasks.insert(task_id, eval_task);

        let plan = validate_dag(&tasks).unwrap();
        assert_eq!(plan.stages.len(), 3);
        let mid: Vec<_> = plan.stages[1].tasks.iter().map(TaskId::as_str).collect();
        assert_eq!(mid, vec!["b", "c"]);
    }

    #[test]
    fn disconnected_components_share_stage_zero() {
        let mut tasks = BTreeMap::new();
        let (task_id, eval_task) = task("a", &[]);
        tasks.insert(task_id, eval_task);
        let (task_id, eval_task) = task("b", &[]);
        tasks.insert(task_id, eval_task);

        let plan = validate_dag(&tasks).unwrap();
        assert_eq!(plan.stages.len(), 1);
        let stage_zero: Vec<_> = plan.stages[0].tasks.iter().map(TaskId::as_str).collect();
        assert_eq!(stage_zero, vec!["a", "b"]);
    }
}

#[cfg(test)]
mod dag_missing_tests {
    use std::collections::BTreeMap;

    use crate::vala::eval::assertion::AssertionTask;
    use crate::vala::eval::ids::{JsonPath, TaskId};
    use crate::vala::eval::operator::ComparisonOperator;
    use crate::vala::eval::plan::{DagError, validate_dag};
    use crate::vala::eval::task::EvalTask;

    #[test]
    fn unknown_dependency_rejected() {
        let mut tasks = BTreeMap::new();
        let id = TaskId::new("a").unwrap();
        let missing = TaskId::new("ghost").unwrap();
        tasks.insert(
            id.clone(),
            EvalTask::Assertion(AssertionTask {
                id: id.clone(),
                context_path: Some(JsonPath::new("$.x").unwrap()),
                item_context_path: None,
                operator: ComparisonOperator::IsNotNull,
                expected: serde_json::Value::Null,
                depends_on: vec![missing.clone()],
                condition: None,
            }),
        );

        let err = validate_dag(&tasks).unwrap_err();
        match err {
            DagError::MissingDependency { task, dep } => {
                assert_eq!(task.as_str(), "a");
                assert_eq!(dep.as_str(), "ghost");
            }
            other => panic!("expected MissingDependency, got {other:?}"),
        }
    }
}

#[cfg(test)]
mod dag_self_loop_tests {
    use std::collections::BTreeMap;

    use crate::vala::eval::assertion::AssertionTask;
    use crate::vala::eval::ids::{JsonPath, TaskId};
    use crate::vala::eval::operator::ComparisonOperator;
    use crate::vala::eval::plan::{DagError, validate_dag};
    use crate::vala::eval::task::EvalTask;

    #[test]
    fn task_depending_on_itself_rejected() {
        let mut tasks = BTreeMap::new();
        let id = TaskId::new("loop").unwrap();
        tasks.insert(
            id.clone(),
            EvalTask::Assertion(AssertionTask {
                id: id.clone(),
                context_path: Some(JsonPath::new("$.x").unwrap()),
                item_context_path: None,
                operator: ComparisonOperator::IsNotNull,
                expected: serde_json::Value::Null,
                depends_on: vec![id],
                condition: None,
            }),
        );

        let err = validate_dag(&tasks).unwrap_err();
        match err {
            DagError::SelfLoop { task } => assert_eq!(task.as_str(), "loop"),
            other => panic!("expected SelfLoop, got {other:?}"),
        }
    }
}

#[cfg(test)]
mod ids_tests {
    use crate::vala::eval::{
        EntityUid, JsonPath, RecordId, ScenarioId, SessionId, SpanId, TaskId, TraceId, WorkflowUid,
    };

    #[test]
    fn task_id_accepts_simple_identifier() {
        let id = TaskId::new("step_one").expect("valid task id");
        assert_eq!(id.as_str(), "step_one");
    }

    #[test]
    fn task_id_accepts_hyphen_after_first_char() {
        TaskId::new("step-one").expect("valid task id");
    }

    #[test]
    fn task_id_rejects_leading_digit() {
        assert!(TaskId::new("1step").is_err());
    }

    #[test]
    fn task_id_rejects_empty() {
        assert!(TaskId::new("").is_err());
    }

    #[test]
    fn task_id_rejects_overlong() {
        let long = "a".repeat(129);
        assert!(TaskId::new(long).is_err());
    }

    #[test]
    fn task_id_serde_transparent() {
        let id = TaskId::new("alpha").expect("valid task id");
        let json = serde_json::to_string(&id).expect("task id serializes");
        assert_eq!(json, "\"alpha\"");
        let round_trip: TaskId = serde_json::from_str(&json).expect("task id deserializes");
        assert_eq!(id, round_trip);
    }

    #[test]
    fn task_id_deserialize_rejects_leading_digit() {
        let result: Result<TaskId, _> = serde_json::from_str("\"1step\"");
        assert!(result.is_err());
    }

    #[test]
    fn task_id_deserialize_rejects_overlong() {
        let long = format!("\"{}\"", "a".repeat(129));
        let result: Result<TaskId, _> = serde_json::from_str(&long);
        assert!(result.is_err());
    }

    #[test]
    fn task_id_deserialize_rejects_empty_string() {
        let result: Result<TaskId, _> = serde_json::from_str("\"\"");
        assert!(result.is_err());
    }

    #[test]
    fn task_id_deserialize_rejects_disallowed_char() {
        let result: Result<TaskId, _> = serde_json::from_str("\"has space\"");
        assert!(result.is_err());
    }

    #[test]
    fn scenario_id_accepts_simple_identifier() {
        let id = ScenarioId::new("scenario_one").expect("valid scenario id");
        assert_eq!(id.as_str(), "scenario_one");
    }

    #[test]
    fn scenario_id_rejects_leading_digit() {
        assert!(ScenarioId::new("1scenario").is_err());
    }

    #[test]
    fn scenario_id_rejects_empty() {
        assert!(ScenarioId::new("").is_err());
    }

    #[test]
    fn scenario_id_rejects_overlong() {
        let long = "a".repeat(129);
        assert!(ScenarioId::new(long).is_err());
    }

    #[test]
    fn scenario_id_serde_transparent() {
        let id = ScenarioId::new("alpha").expect("valid scenario id");
        let json = serde_json::to_string(&id).expect("scenario id serializes");
        assert_eq!(json, "\"alpha\"");
        let round_trip: ScenarioId = serde_json::from_str(&json).expect("scenario id deserializes");
        assert_eq!(id, round_trip);
    }

    #[test]
    fn scenario_id_deserialize_rejects_leading_digit() {
        let result: Result<ScenarioId, _> = serde_json::from_str("\"1scenario\"");
        assert!(result.is_err());
    }

    #[test]
    fn scenario_id_deserialize_rejects_overlong() {
        let long = format!("\"{}\"", "a".repeat(129));
        let result: Result<ScenarioId, _> = serde_json::from_str(&long);
        assert!(result.is_err());
    }

    #[test]
    fn scenario_id_deserialize_rejects_empty_string() {
        let result: Result<ScenarioId, _> = serde_json::from_str("\"\"");
        assert!(result.is_err());
    }

    #[test]
    fn json_path_accepts_root() {
        JsonPath::new("$").expect("valid json path");
    }

    #[test]
    fn json_path_accepts_dot_segments() {
        JsonPath::new("$.response.message").expect("valid json path");
    }

    #[test]
    fn json_path_accepts_bracket_indices() {
        JsonPath::new("$.docs[0].title").expect("valid json path");
    }

    #[test]
    fn json_path_accepts_quoted_keys() {
        JsonPath::new("$['weird key'].value").expect("valid json path");
    }

    #[test]
    fn json_path_rejects_missing_dollar() {
        assert!(JsonPath::new("response").is_err());
    }

    #[test]
    fn json_path_rejects_unbalanced_bracket() {
        assert!(JsonPath::new("$.docs[0").is_err());
    }

    #[test]
    fn json_path_rejects_unbalanced_quote() {
        assert!(JsonPath::new("$['unterminated").is_err());
    }

    #[test]
    fn json_path_rejects_control_char() {
        assert!(JsonPath::new("$.a\nb").is_err());
    }

    #[test]
    fn json_path_deserialize_rejects_missing_dollar() {
        let result: Result<JsonPath, _> = serde_json::from_str("\"response\"");
        assert!(result.is_err());
    }

    #[test]
    fn json_path_deserialize_rejects_unbalanced_bracket() {
        let result: Result<JsonPath, _> = serde_json::from_str("\"$.docs[0\"");
        assert!(result.is_err());
    }

    #[test]
    fn json_path_accepts_key_ending_in_escaped_backslash() {
        // $['a\\'] means the key "a\" (backslash at end).
        // Old code rejected this: the second `\` was `previous`, so it mistakenly
        // treated the closing `'` as escaped and never closed in_single_quote.
        // New code (escaped boolean) correctly toggles in_single_quote on the `'`.
        JsonPath::new(r"$['a\\']").expect("key ending with escaped backslash is valid");
    }

    #[test]
    fn json_path_accepts_escaped_backslash_then_bracket() {
        JsonPath::new(r"$['a\\'][0]").expect("escaped backslash then bracket is valid");
    }

    #[test]
    fn json_path_deserialize_rejects_unbalanced_quote() {
        let result: Result<JsonPath, _> = serde_json::from_str("\"$['unterminated\"");
        assert!(result.is_err());
    }

    #[test]
    fn json_path_deserialize_rejects_control_char() {
        let result: Result<JsonPath, _> = serde_json::from_str("\"$.a\\nb\"");
        assert!(result.is_err());
    }

    #[test]
    fn id_uuid_types_serde_round_trip() {
        let uuid = uuid::Uuid::nil();
        let session = SessionId(uuid);
        let record = RecordId(uuid);
        let workflow = WorkflowUid(uuid);

        assert_eq!(
            serde_json::to_string(&session).expect("session id serializes"),
            format!("\"{uuid}\"")
        );
        assert_eq!(
            session,
            serde_json::from_str(&format!("\"{uuid}\"")).expect("session id")
        );
        assert_eq!(
            record,
            serde_json::from_str(&format!("\"{uuid}\"")).expect("record id")
        );
        assert_eq!(
            workflow,
            serde_json::from_str(&format!("\"{uuid}\"")).expect("workflow id")
        );
    }

    #[test]
    fn entity_uid_serde_round_trip() {
        let entity = EntityUid::new("customer-123").expect("valid entity uid");
        let json = serde_json::to_string(&entity).expect("entity uid serializes");
        assert_eq!(json, "\"customer-123\"");
        let round_trip: EntityUid = serde_json::from_str(&json).expect("entity uid deserializes");
        assert_eq!(entity, round_trip);
    }

    #[test]
    fn trace_id_deserializes_legacy_byte_array_and_serializes_hex() {
        let trace: TraceId = serde_json::from_str("[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16]")
            .expect("legacy byte array deserializes");
        let json = serde_json::to_string(&trace).expect("trace id serializes");
        assert_eq!(json, "\"0102030405060708090a0b0c0d0e0f10\"");
    }

    #[test]
    fn span_id_deserializes_legacy_byte_array_and_serializes_hex() {
        let span: SpanId =
            serde_json::from_str("[1,2,3,4,5,6,7,8]").expect("legacy byte array deserializes");
        let json = serde_json::to_string(&span).expect("span id serializes");
        assert_eq!(json, "\"0102030405060708\"");
    }
}

#[cfg(test)]
mod llm_judge_tests {
    use crate::envelope::CardKind;
    use crate::ids::{CardName, SpaceName};
    use crate::reference::CardRef;
    use crate::vala::eval::ids::TaskId;
    use crate::vala::eval::llm_judge::LlmJudgeTask;
    use crate::vala::eval::operator::ComparisonOperator;
    use wyrd_semver::VersionBlock;

    fn agent_ref(name: &str) -> CardRef {
        CardRef {
            kind: CardKind::Agent,
            name: CardName::new(name).unwrap(),
            version: VersionBlock::parse("1.0.0").unwrap(),
            space: Some(SpaceName::new("default").unwrap()),
            uid: None,
        }
    }

    fn data_ref(name: &str) -> CardRef {
        CardRef {
            kind: CardKind::Data,
            name: CardName::new(name).unwrap(),
            version: VersionBlock::parse("1.0.0").unwrap(),
            space: Some(SpaceName::new("default").unwrap()),
            uid: None,
        }
    }

    #[test]
    fn new_accepts_agent_kind() {
        let task = LlmJudgeTask::new(
            TaskId::new("judge_one").unwrap(),
            agent_ref("factuality-judge"),
            ComparisonOperator::Equals,
            serde_json::json!("pass"),
        )
        .expect("agent-kind ref is valid");

        assert_eq!(task.max_retries, 2);
    }

    #[test]
    fn new_rejects_non_agent_kind() {
        let result = LlmJudgeTask::new(
            TaskId::new("judge_two").unwrap(),
            data_ref("dataset"),
            ComparisonOperator::Equals,
            serde_json::json!("pass"),
        );
        let err = result.unwrap_err();
        assert_eq!(err.code(), "WYRD_VALA_400_EVAL_REF_KIND_MISMATCH");
    }

    #[test]
    fn validate_catches_deserialized_kind_mismatch() {
        let task = LlmJudgeTask::new(
            TaskId::new("judge").unwrap(),
            agent_ref("prompt"),
            ComparisonOperator::Equals,
            serde_json::json!("ok"),
        )
        .unwrap();
        let mut value = serde_json::to_value(&task).unwrap();
        value["judge_ref"]["kind"] = serde_json::json!("Data");
        let bad: LlmJudgeTask = serde_json::from_value(value).unwrap();

        let err = bad.validate().unwrap_err();
        assert_eq!(err.code(), "WYRD_VALA_400_EVAL_REF_KIND_MISMATCH");
    }

    #[test]
    fn round_trip() {
        let task = LlmJudgeTask::new(
            TaskId::new("judge").unwrap(),
            agent_ref("prompt"),
            ComparisonOperator::ContainsIgnoreCase,
            serde_json::json!("paris"),
        )
        .unwrap();

        let serialized = serde_json::to_string(&task).unwrap();
        let deserialized: LlmJudgeTask = serde_json::from_str(&serialized).unwrap();
        assert_eq!(task, deserialized);
    }
}

#[cfg(test)]
mod operator_catalog_tests {
    use std::str::FromStr;

    use crate::vala::eval::operator::{ComparisonOperator, DivergenceMetric, JsonValueType};

    #[test]
    fn parameterless_round_trip_equals() {
        let op = ComparisonOperator::Equals;
        let json = serde_json::to_string(&op).expect("operator serializes");
        assert_eq!(json, r#""equals""#);
        let back: ComparisonOperator = serde_json::from_str(&json).expect("operator deserializes");
        assert_eq!(op, back);
    }

    #[test]
    fn parameterized_round_trip_in_range() {
        let op = ComparisonOperator::InRange {
            min: 0.0,
            max: 1.0,
            inclusive: true,
        };
        let json = serde_json::to_string(&op).expect("operator serializes");
        assert_eq!(
            json,
            r#"{"kind":"in_range","min":0.0,"max":1.0,"inclusive":true}"#,
        );
        let back: ComparisonOperator = serde_json::from_str(&json).expect("operator deserializes");
        assert_eq!(op, back);
    }

    #[test]
    fn parameterized_round_trip_matches_regex() {
        let op = ComparisonOperator::MatchesRegex {
            pattern: "^foo".into(),
        };
        let json = serde_json::to_string(&op).expect("operator serializes");
        let back: ComparisonOperator = serde_json::from_str(&json).expect("operator deserializes");
        assert_eq!(op, back);
    }

    #[test]
    fn parameterized_round_trip_is_type() {
        let op = ComparisonOperator::IsType {
            expected: JsonValueType::String,
        };
        let json = serde_json::to_string(&op).expect("operator serializes");
        assert_eq!(json, r#"{"kind":"is_type","expected":"string"}"#);
        let back: ComparisonOperator = serde_json::from_str(&json).expect("operator deserializes");
        assert_eq!(op, back);
    }

    #[test]
    fn redundant_object_shape_is_rejected() {
        let err = serde_json::from_str::<ComparisonOperator>(r#"{"operator":"is_not_null"}"#)
            .expect_err("redundant object shape must be rejected");

        assert!(
            err.to_string().contains("kind"),
            "error should point callers to the parameterized object discriminator: {err}"
        );
    }

    #[test]
    fn parameterless_object_shape_is_rejected() {
        let err = serde_json::from_str::<ComparisonOperator>(r#"{"kind":"is_not_null"}"#)
            .expect_err("parameterless operators must use scalar strings");

        assert!(
            err.to_string().contains("is_not_null"),
            "error should name the bad discriminator: {err}"
        );
    }

    #[test]
    fn parameterized_round_trip_divergence() {
        let op = ComparisonOperator::DivergenceLessThan {
            metric: DivergenceMetric::Kl,
            threshold: 0.05,
        };
        let json = serde_json::to_string(&op).expect("operator serializes");
        let back: ComparisonOperator = serde_json::from_str(&json).expect("operator deserializes");
        assert_eq!(op, back);
    }

    #[test]
    fn discriminator_matches_serde_tag() {
        use ComparisonOperator::*;

        let pairs: &[(ComparisonOperator, &str)] = &[
            (Equals, "equals"),
            (NotEquals, "not_equals"),
            (
                WithinAbsTolerance { tolerance: 0.0 },
                "within_abs_tolerance",
            ),
            (
                IsType {
                    expected: JsonValueType::Null,
                },
                "is_type",
            ),
            (
                DivergenceLessThan {
                    metric: DivergenceMetric::Js,
                    threshold: 0.0,
                },
                "divergence_less_than",
            ),
        ];

        for (op, want) in pairs {
            assert_eq!(op.discriminator(), *want);
            assert_eq!(op.as_str(), *want);
            let value = serde_json::to_value(op).expect("operator converts to json");
            if value.is_string() {
                assert_eq!(value.as_str(), Some(*want));
            } else {
                assert_eq!(value["kind"].as_str(), Some(*want));
            }
        }
    }

    #[test]
    fn from_str_parses_parameterless() {
        assert_eq!(
            ComparisonOperator::from_str("equals").expect("equals parses"),
            ComparisonOperator::Equals,
        );
        assert_eq!(
            ComparisonOperator::from_discriminator_parameterless("equals").expect("equals parses"),
            ComparisonOperator::Equals,
        );
        assert_eq!(
            ComparisonOperator::from_str("is_uuid").expect("is_uuid parses"),
            ComparisonOperator::IsUuid,
        );
    }

    #[test]
    fn from_str_rejects_parameterized_without_params() {
        assert!(ComparisonOperator::from_str("in_range").is_err());
        assert!(ComparisonOperator::from_str("matches_regex").is_err());
        assert!(ComparisonOperator::from_str("divergence_less_than").is_err());
    }

    #[test]
    fn from_str_rejects_unknown() {
        assert!(ComparisonOperator::from_str("not_a_real_operator").is_err());
    }

    #[test]
    fn catalog_count_locked_at_56() {
        let all = [
            ComparisonOperator::Equals,
            ComparisonOperator::NotEquals,
            ComparisonOperator::GreaterThan,
            ComparisonOperator::GreaterThanOrEquals,
            ComparisonOperator::LessThan,
            ComparisonOperator::LessThanOrEquals,
            ComparisonOperator::InRange {
                min: 0.0,
                max: 1.0,
                inclusive: true,
            },
            ComparisonOperator::NotInRange {
                min: 0.0,
                max: 1.0,
                inclusive: false,
            },
            ComparisonOperator::ApproximatelyEquals { tolerance: 0.0 },
            ComparisonOperator::IsPositive,
            ComparisonOperator::IsNegative,
            ComparisonOperator::IsZero,
            ComparisonOperator::Contains,
            ComparisonOperator::NotContains,
            ComparisonOperator::ContainsIgnoreCase,
            ComparisonOperator::StartsWith,
            ComparisonOperator::EndsWith,
            ComparisonOperator::MatchesRegex {
                pattern: "x".into(),
            },
            ComparisonOperator::NotMatchesRegex {
                pattern: "x".into(),
            },
            ComparisonOperator::IsEmail,
            ComparisonOperator::IsUrl,
            ComparisonOperator::IsUuid,
            ComparisonOperator::IsIpv4,
            ComparisonOperator::IsIpv6,
            ComparisonOperator::HasMinLength { min: 1 },
            ComparisonOperator::HasMaxLength { max: 100 },
            ComparisonOperator::IsJson,
            ComparisonOperator::In,
            ComparisonOperator::NotIn,
            ComparisonOperator::IsSubset,
            ComparisonOperator::IsSuperset,
            ComparisonOperator::IsDisjoint,
            ComparisonOperator::AllOf,
            ComparisonOperator::AnyOf,
            ComparisonOperator::NoneOf,
            ComparisonOperator::IsEmpty,
            ComparisonOperator::IsNonEmpty,
            ComparisonOperator::Length { expected: 1 },
            ComparisonOperator::LengthGreaterThan { min: 1 },
            ComparisonOperator::LengthLessThan { max: 1 },
            ComparisonOperator::UniqueValues,
            ComparisonOperator::IsTruthy,
            ComparisonOperator::IsFalsy,
            ComparisonOperator::IsNull,
            ComparisonOperator::IsNotNull,
            ComparisonOperator::IsType {
                expected: JsonValueType::Null,
            },
            ComparisonOperator::IsString,
            ComparisonOperator::IsNumber,
            ComparisonOperator::IsBoolean,
            ComparisonOperator::IsObject,
            ComparisonOperator::WithinAbsTolerance { tolerance: 0.0 },
            ComparisonOperator::WithinPctTolerance { pct: 0.0 },
            ComparisonOperator::WithinStdDev {
                sigma: 0.0,
                mean: 0.0,
                std_dev: 1.0,
            },
            ComparisonOperator::BetweenPercentiles {
                lower_pct: 0.0,
                upper_pct: 1.0,
            },
            ComparisonOperator::DivergenceLessThan {
                metric: DivergenceMetric::Kl,
                threshold: 0.0,
            },
            ComparisonOperator::CosineSimilarityAtLeast { threshold: 0.0 },
        ];

        assert_eq!(all.len(), 56, "catalog count locked at 56");

        for op in &all {
            let json = serde_json::to_string(op).expect("operator serializes");
            let back: ComparisonOperator =
                serde_json::from_str(&json).expect("operator deserializes");
            assert_eq!(*op, back, "round-trip failed for {}", op.discriminator());
        }
    }

    #[test]
    fn discriminators_are_unique() {
        use std::collections::HashSet;

        let names: HashSet<&str> = [
            "equals",
            "not_equals",
            "greater_than",
            "greater_than_or_equals",
            "less_than",
            "less_than_or_equals",
            "in_range",
            "not_in_range",
            "approximately_equals",
            "is_positive",
            "is_negative",
            "is_zero",
            "contains",
            "not_contains",
            "contains_ignore_case",
            "starts_with",
            "ends_with",
            "matches_regex",
            "not_matches_regex",
            "is_email",
            "is_url",
            "is_uuid",
            "is_ipv4",
            "is_ipv6",
            "has_min_length",
            "has_max_length",
            "is_json",
            "in",
            "not_in",
            "is_subset",
            "is_superset",
            "is_disjoint",
            "all_of",
            "any_of",
            "none_of",
            "is_empty",
            "is_non_empty",
            "length",
            "length_greater_than",
            "length_less_than",
            "unique_values",
            "is_truthy",
            "is_falsy",
            "is_null",
            "is_not_null",
            "is_type",
            "is_string",
            "is_number",
            "is_boolean",
            "is_object",
            "within_abs_tolerance",
            "within_pct_tolerance",
            "within_std_dev",
            "between_percentiles",
            "divergence_less_than",
            "cosine_similarity_at_least",
        ]
        .into_iter()
        .collect();

        assert_eq!(names.len(), 56);
    }
}

#[cfg(test)]
mod pass_gate_tests {
    use crate::vala::eval::result::EvalPassGate;

    #[test]
    fn overall_pass_rate_round_trip() {
        let g = EvalPassGate::OverallPassRate { threshold: 0.9 };
        let s = serde_json::to_string(&g).unwrap();
        assert_eq!(s, r#"{"kind":"overall_pass_rate","threshold":0.9}"#);
        let back: EvalPassGate = serde_json::from_str(&s).unwrap();
        assert_eq!(g, back);
    }

    #[test]
    fn per_judge_pass_rate_round_trip() {
        let g = EvalPassGate::PerJudgePassRate { threshold: 0.75 };
        let s = serde_json::to_string(&g).unwrap();
        let back: EvalPassGate = serde_json::from_str(&s).unwrap();
        assert_eq!(g, back);
    }

    #[test]
    fn all_pass_round_trip() {
        let g = EvalPassGate::AllPass;
        let s = serde_json::to_string(&g).unwrap();
        assert_eq!(s, r#"{"kind":"all_pass"}"#);
        let back: EvalPassGate = serde_json::from_str(&s).unwrap();
        assert_eq!(g, back);
    }

    #[test]
    fn validate_accepts_in_range() {
        EvalPassGate::OverallPassRate { threshold: 0.0 }
            .validate()
            .unwrap();
        EvalPassGate::OverallPassRate { threshold: 1.0 }
            .validate()
            .unwrap();
        EvalPassGate::OverallPassRate { threshold: 0.5 }
            .validate()
            .unwrap();
        EvalPassGate::AllPass.validate().unwrap();
    }

    #[test]
    fn validate_rejects_out_of_range() {
        assert!(
            EvalPassGate::OverallPassRate { threshold: -0.1 }
                .validate()
                .is_err()
        );
        assert!(
            EvalPassGate::OverallPassRate { threshold: 1.1 }
                .validate()
                .is_err()
        );
        assert!(
            EvalPassGate::PerJudgePassRate {
                threshold: f64::NAN,
            }
            .validate()
            .is_err()
        );
    }
}

#[cfg(test)]
mod protocol_tests {
    use crate::envelope::CardKind;
    use crate::ids::{CardName, SpaceName};
    use crate::reference::CardRef;
    use crate::vala::eval::ids::ScenarioId;
    use crate::vala::eval::protocol::{
        AgentTurnSubmission, ConversationTurn, EvalRunOpenRequest, EvalRunOpenResponse,
        MAX_HISTORY_TURNS, SimulatedUserMode, SimulatedUserTurn, TurnDirective, TurnRole,
        UserTurnSubmission,
    };
    use crate::vala::ids::{LeaseToken, RunId};
    use wyrd_semver::VersionBlock;

    fn eval_ref() -> CardRef {
        CardRef {
            kind: CardKind::Eval,
            name: CardName::new("rubric").unwrap(),
            version: VersionBlock::parse("1.0.0").unwrap(),
            space: Some(SpaceName::new("default").unwrap()),
            uid: None,
        }
    }

    #[test]
    fn open_request_round_trips() {
        let req = EvalRunOpenRequest {
            eval_ref: eval_ref(),
            simulated_user: SimulatedUserMode::Server,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: EvalRunOpenRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, back);
    }

    #[test]
    fn open_response_round_trips() {
        let resp = EvalRunOpenResponse {
            run_id: RunId::from_string("00000000-0000-7000-8000-000000000000".to_string()),
            lease_token: LeaseToken::new("abc-123").unwrap(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: EvalRunOpenResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, back);
    }

    #[test]
    fn open_response_wire_field_is_run_id() {
        let resp = EvalRunOpenResponse {
            run_id: RunId::from_string("00000000-0000-7000-8000-000000000000".to_string()),
            lease_token: LeaseToken::new("abc-123").unwrap(),
        };
        let v = serde_json::to_value(&resp).unwrap();
        assert!(v.get("run_id").is_some());
        let forbidden_field = ["eval", "run_id"].join("_");
        assert!(v.get(&forbidden_field).is_none());
        assert!(v.get("lease_token").is_some());
    }

    #[test]
    fn lease_token_rejects_empty_long_and_control() {
        assert!(LeaseToken::new("").is_err());
        assert!(LeaseToken::new("a".repeat(257)).is_err());
        assert!(LeaseToken::new("a\x00b").is_err());
    }

    #[test]
    fn simulated_user_mode_serialises_snake_case() {
        let s = serde_json::to_string(&SimulatedUserMode::Client).unwrap();
        assert_eq!(s, "\"client\"");
        let s = serde_json::to_string(&SimulatedUserMode::Server).unwrap();
        assert_eq!(s, "\"server\"");
    }

    #[test]
    fn turn_role_serialises_snake_case() {
        let s = serde_json::to_string(&TurnRole::Agent).unwrap();
        assert_eq!(s, "\"agent\"");
        let s = serde_json::to_string(&TurnRole::User).unwrap();
        assert_eq!(s, "\"user\"");
    }

    #[test]
    fn directive_agent_turn_wire_shape() {
        let directive = TurnDirective::AgentTurn {
            scenario_id: ScenarioId::new("happy_path").unwrap(),
            turn: 0,
            message: "Hi".to_string(),
            history: vec![],
        };
        let json = serde_json::to_value(&directive).unwrap();
        assert_eq!(json["kind"], "agent_turn");
        assert_eq!(json["turn"], 0);
    }

    #[test]
    fn directive_user_turn_needed_wire_shape() {
        let directive = TurnDirective::UserTurnNeeded {
            scenario_id: ScenarioId::new("happy_path").unwrap(),
            turn: 1,
            history: vec![ConversationTurn {
                role: TurnRole::Agent,
                content: "Sure".to_string(),
            }],
        };
        let json = serde_json::to_value(&directive).unwrap();
        assert_eq!(json["kind"], "user_turn_needed");
        assert_eq!(json["history"][0]["role"], "agent");
    }

    #[test]
    fn directive_scenario_complete_wire_shape() {
        let directive = TurnDirective::ScenarioComplete {
            scenario_id: ScenarioId::new("happy_path").unwrap(),
        };
        let json = serde_json::to_value(&directive).unwrap();
        assert_eq!(json["kind"], "scenario_complete");
    }

    #[test]
    fn directive_run_complete_wire_shape() {
        let directive = TurnDirective::RunComplete;
        let json = serde_json::to_value(&directive).unwrap();
        assert_eq!(json, serde_json::json!({"kind": "run_complete"}));
    }

    #[test]
    fn directive_round_trips_for_each_variant() {
        let cases = vec![
            TurnDirective::AgentTurn {
                scenario_id: ScenarioId::new("a").unwrap(),
                turn: 0,
                message: "m".to_string(),
                history: vec![],
            },
            TurnDirective::UserTurnNeeded {
                scenario_id: ScenarioId::new("a").unwrap(),
                turn: 1,
                history: vec![],
            },
            TurnDirective::ScenarioComplete {
                scenario_id: ScenarioId::new("a").unwrap(),
            },
            TurnDirective::RunComplete,
        ];
        for directive in cases {
            let json = serde_json::to_string(&directive).unwrap();
            let back: TurnDirective = serde_json::from_str(&json).unwrap();
            assert_eq!(directive, back);
        }
    }

    #[test]
    fn directive_validate_rejects_too_long_history() {
        let directive = TurnDirective::AgentTurn {
            scenario_id: ScenarioId::new("a").unwrap(),
            turn: 0,
            message: "m".to_string(),
            history: (0..MAX_HISTORY_TURNS + 1)
                .map(|_| ConversationTurn {
                    role: TurnRole::User,
                    content: "x".to_string(),
                })
                .collect(),
        };
        let err = directive.validate().unwrap_err();
        assert!(err.to_string().contains("MAX_HISTORY_TURNS"));
    }

    #[test]
    fn agent_turn_submission_round_trips() {
        let submission = AgentTurnSubmission {
            scenario_id: ScenarioId::new("a").unwrap(),
            turn: 0,
            response: "ok".to_string(),
            records: vec![],
        };
        let json = serde_json::to_string(&submission).unwrap();
        let back: AgentTurnSubmission = serde_json::from_str(&json).unwrap();
        assert_eq!(submission, back);
    }

    #[test]
    fn user_turn_submission_round_trips() {
        let submission = UserTurnSubmission {
            scenario_id: ScenarioId::new("a").unwrap(),
            turn: 1,
            message: "and another thing".to_string(),
        };
        let json = serde_json::to_string(&submission).unwrap();
        let back: UserTurnSubmission = serde_json::from_str(&json).unwrap();
        assert_eq!(submission, back);
    }

    #[test]
    fn simulated_user_turn_round_trips() {
        let turn = SimulatedUserTurn {
            message: "ok".to_string(),
            goal_achieved: true,
        };
        let json = serde_json::to_string(&turn).unwrap();
        let back: SimulatedUserTurn = serde_json::from_str(&json).unwrap();
        assert_eq!(turn, back);
    }

    #[test]
    fn deny_unknown_fields_on_submissions() {
        let payload = serde_json::json!({
            "scenario_id": "a",
            "turn": 0,
            "response": "ok",
            "records": [],
            "rogue": true
        });
        let err = serde_json::from_value::<AgentTurnSubmission>(payload).unwrap_err();
        assert!(err.to_string().contains("unknown field"));
    }
}

#[cfg(test)]
mod record_tests {
    use std::fs;
    use std::path::PathBuf;

    use crate::envelope::CardKind;
    use crate::ids::{CardName, SpaceName};
    use crate::reference::CardRef;
    use crate::vala::eval::record::EvalRecordObservation;
    use crate::vala::ids::{RecordId, RunId, SessionId, SpanId};
    use chrono::TimeZone;
    use schemars::schema_for;
    use uuid::Uuid;
    use wyrd_semver::VersionBlock;

    fn eval_ref(name: &str) -> CardRef {
        CardRef {
            kind: CardKind::Eval,
            name: CardName::new(name).unwrap(),
            version: VersionBlock::parse("1.0.0").unwrap(),
            space: Some(SpaceName::new("default").unwrap()),
            uid: None,
        }
    }

    fn fixture() -> EvalRecordObservation {
        EvalRecordObservation {
            record_id: RecordId(Uuid::nil()),
            run_id: RunId::from_string("r1".to_owned()),
            session_id: Some(SessionId(Uuid::nil())),
            eval_ref: Some(eval_ref("retriever-quality")),
            context: serde_json::json!({"response": "ok"}),
            trace_id: None,
            span_id: None,
            created_at: chrono::Utc.with_ymd_and_hms(2026, 6, 10, 0, 0, 0).unwrap(),
            media: None,
        }
    }

    fn fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/eval/schemas")
    }

    #[test]
    fn round_trips() {
        let rec = fixture();
        let json = serde_json::to_string(&rec).unwrap();
        let back: EvalRecordObservation = serde_json::from_str(&json).unwrap();
        assert_eq!(rec, back);
    }

    #[test]
    fn span_without_trace_rejects() {
        let mut rec = fixture();
        rec.span_id = Some(SpanId::from_hex("0102030405060708").unwrap());
        let err = rec.validate().unwrap_err();
        assert_eq!(err.code(), "WYRD_SPEC_400_VALIDATION");
    }

    #[test]
    fn deny_unknown_fields() {
        let mut value = serde_json::to_value(fixture()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("agent_id".into(), serde_json::json!("a1"));
        let err = serde_json::from_value::<EvalRecordObservation>(value).unwrap_err();
        assert!(
            err.to_string().contains("unknown field"),
            "expected unknown-field rejection, got {err}"
        );
    }

    #[test]
    fn schema_matches_golden() {
        let mut schema = schema_for!(EvalRecordObservation);
        schema.meta_schema = Some("https://json-schema.org/draft/2020-12/schema".to_string());
        let actual = format!(
            "{}\n",
            serde_json::to_string_pretty(&schema).expect("schema serializes")
        );
        let path = fixture_dir().join("eval_record_observation.schema.json");
        let expected = fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!(
                "missing golden schema at {path:?}: {error}; regenerate with \
                 `cargo run -p wyrd-spec --example gen_schemas --features server`",
            )
        });
        assert_eq!(
            actual, expected,
            "schema drift in eval_record_observation.schema.json; run mise run codegen:regen"
        );
    }
}

#[cfg(test)]
mod result_tests {
    use crate::vala::eval::ids::TaskId;
    use crate::vala::eval::operator::ComparisonOperator;
    use crate::vala::eval::result::AssertionResult;
    use chrono::{TimeZone, Utc};

    #[test]
    fn assertion_result_round_trip_minimal() {
        let r = AssertionResult {
            task_id: TaskId::new("a").unwrap(),
            passed: true,
            actual: Some(serde_json::json!("hello")),
            expected: serde_json::json!("hello"),
            operator: ComparisonOperator::Equals,
            message: None,
            stage: 0,
            started_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
            duration_ms: 42,
        };
        let s = serde_json::to_string(&r).unwrap();
        let back: AssertionResult = serde_json::from_str(&s).unwrap();
        assert_eq!(r, back);
    }

    #[test]
    fn assertion_result_with_message() {
        let r = AssertionResult {
            task_id: TaskId::new("a").unwrap(),
            passed: false,
            actual: Some(serde_json::json!("hi")),
            expected: serde_json::json!("hello"),
            operator: ComparisonOperator::Equals,
            message: Some("equality mismatch at byte 1".into()),
            stage: 0,
            started_at: Utc.timestamp_opt(0, 0).unwrap(),
            duration_ms: 0,
        };
        let s = serde_json::to_string(&r).unwrap();
        let back: AssertionResult = serde_json::from_str(&s).unwrap();
        assert_eq!(r, back);
    }

    #[test]
    fn assertion_result_rejects_unknown_field() {
        let json = r#"{
            "task_id":"a","passed":true,"actual":0,"expected":0,
            "operator":"equals",
            "stage":0,"started_at":"1970-01-01T00:00:00Z","duration_ms":0,
            "extra":"nope"
        }"#;
        let r: Result<AssertionResult, _> = serde_json::from_str(json);
        assert!(r.is_err());
    }
}

#[cfg(test)]
mod scenario_tests {
    use crate::vala::eval::condition::{ConditionCombinator, EvalCondition};
    use crate::vala::eval::ids::{JsonPath, ScenarioId, TaskId};
    use crate::vala::eval::operator::ComparisonOperator;
    use crate::vala::eval::scenario::{
        EvalScenario, EvalScenarioCollection, MAX_SCENARIOS_PER_COLLECTION, MAX_TURNS_HARD_CAP,
        ScenarioTask,
    };

    fn task(id: &str) -> ScenarioTask {
        ScenarioTask {
            id: TaskId::new(id).unwrap(),
            operator: ComparisonOperator::Contains,
            expected: serde_json::json!("ok"),
            condition: None,
        }
    }

    fn scenario(id: &str) -> EvalScenario {
        EvalScenario {
            id: ScenarioId::new(id).unwrap(),
            initial_query: "Hello".into(),
            expected_outcome: Some("World".into()),
            predefined_turns: vec![],
            simulated_user_persona: None,
            termination_signal: None,
            max_turns: 8,
            tasks: vec![task("t1")],
        }
    }

    #[test]
    fn scenario_default_max_turns_is_eight() {
        let json = r#"{
            "id": "s1",
            "initial_query": "Hello",
            "tasks": []
        }"#;
        let s: EvalScenario = serde_json::from_str(json).unwrap();
        assert_eq!(s.max_turns, 8);
    }

    #[test]
    fn scenario_round_trip() {
        let s = scenario("s1");
        let json = serde_json::to_string(&s).unwrap();
        let back: EvalScenario = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn scenario_validates_happy() {
        scenario("s1").validate().unwrap();
    }

    #[test]
    fn scenario_id_constructor_rejects_empty() {
        assert!(ScenarioId::new("").is_err());
    }

    #[test]
    fn scenario_rejects_empty_initial_query() {
        let mut s = scenario("s1");
        s.initial_query = String::new();
        assert!(s.validate().is_err());
    }

    #[test]
    fn scenario_rejects_zero_max_turns() {
        let mut s = scenario("s1");
        s.max_turns = 0;
        assert!(s.validate().is_err());
    }

    #[test]
    fn scenario_rejects_overlarge_max_turns() {
        let mut s = scenario("s1");
        s.max_turns = MAX_TURNS_HARD_CAP + 1;
        assert!(s.validate().is_err());
    }

    #[test]
    fn scenario_rejects_duplicate_task_ids() {
        let mut s = scenario("s1");
        s.tasks.push(task("t1"));
        assert!(s.validate().is_err());
    }

    #[test]
    fn scenario_propagates_bad_condition() {
        let mut s = scenario("s1");
        s.tasks[0].condition = Some(EvalCondition {
            path: JsonPath::new("$.x").unwrap(),
            operator: ComparisonOperator::Equals,
            expected: serde_json::Value::Null,
            combinator: Some(ConditionCombinator::And),
            subsequent: None,
        });
        assert!(s.validate().is_err());
    }

    #[test]
    fn collection_round_trip() {
        let c = EvalScenarioCollection {
            collection_id: "qa-v1".into(),
            scenarios: vec![scenario("s1"), scenario("s2")],
        };
        let json = serde_json::to_string(&c).unwrap();
        let back: EvalScenarioCollection = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn collection_validates_happy() {
        EvalScenarioCollection {
            collection_id: "qa-v1".into(),
            scenarios: vec![scenario("s1"), scenario("s2")],
        }
        .validate()
        .unwrap();
    }

    #[test]
    fn collection_rejects_duplicate_scenario_ids() {
        let c = EvalScenarioCollection {
            collection_id: "qa-v1".into(),
            scenarios: vec![scenario("s1"), scenario("s1")],
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn collection_rejects_empty_collection_id() {
        let c = EvalScenarioCollection {
            collection_id: String::new(),
            scenarios: vec![scenario("s1")],
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn collection_rejects_unknown_field() {
        let json = r#"{
            "collection_id":"c", "scenarios":[], "extra": 1
        }"#;
        let r: Result<EvalScenarioCollection, _> = serde_json::from_str(json);
        assert!(r.is_err());
    }

    #[test]
    fn collection_rejects_over_cap() {
        let scenarios: Vec<EvalScenario> = (0..=MAX_SCENARIOS_PER_COLLECTION)
            .map(|i| scenario(&format!("s{i}")))
            .collect();
        let c = EvalScenarioCollection {
            collection_id: "big".into(),
            scenarios,
        };
        assert!(c.validate().is_err());
    }
}

#[cfg(test)]
mod schema_drift_tests {
    use std::fs;
    use std::path::PathBuf;

    use crate::vala::eval::{
        ComparisonOperator, DagError, EvalCondition, EvalPassGate, EvalSampling,
        EvalScenarioCollection, EvalSpec, EvalTask, ExecutionPlan,
    };
    use schemars::schema_for;

    fn fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/eval/schemas")
    }

    fn assert_schema_matches<T: schemars::JsonSchema>(name: &str) {
        let mut schema = schema_for!(T);
        schema.meta_schema = Some("https://json-schema.org/draft/2020-12/schema".to_string());
        let actual = format!(
            "{}\n",
            serde_json::to_string_pretty(&schema).expect("schema serializes")
        );
        let path = fixture_dir().join(format!("{name}.schema.json"));
        let expected = fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!(
                "missing golden schema at {path:?}: {error}; regenerate with \
                 `cargo run -p wyrd-spec --example gen_schemas --features server`",
            )
        });
        assert_eq!(
            actual, expected,
            "schema drift in {name}.schema.json; run mise run codegen:regen"
        );
    }

    #[test]
    fn eval_spec_schema() {
        assert_schema_matches::<EvalSpec>("eval_spec");
    }

    #[test]
    fn eval_task_schema() {
        assert_schema_matches::<EvalTask>("eval_task");
    }

    #[test]
    fn eval_condition_schema() {
        assert_schema_matches::<EvalCondition>("eval_condition");
    }

    #[test]
    fn comparison_operator_schema() {
        assert_schema_matches::<ComparisonOperator>("comparison_operator");
    }

    #[test]
    fn execution_plan_schema() {
        assert_schema_matches::<ExecutionPlan>("execution_plan");
    }

    #[test]
    fn dag_error_schema() {
        assert_schema_matches::<DagError>("dag_error");
    }

    #[test]
    fn eval_scenario_collection_schema() {
        assert_schema_matches::<EvalScenarioCollection>("eval_scenario_collection");
    }

    #[test]
    fn eval_pass_gate_schema() {
        assert_schema_matches::<EvalPassGate>("eval_pass_gate");
    }

    #[test]
    fn eval_sampling_schema() {
        assert_schema_matches::<EvalSampling>("eval_sampling");
    }
}

#[cfg(test)]
mod spec_tests {
    use std::collections::BTreeMap;

    use crate::envelope::CardKind;
    use crate::error::WyrdError;
    use crate::ids::{CardName, SpaceName};
    use crate::reference::CardRef;
    use crate::vala::eval::assertion::AssertionTask;
    use crate::vala::eval::condition::{ConditionCombinator, EvalCondition};
    use crate::vala::eval::ids::{JsonPath, TaskId};
    use crate::vala::eval::llm_judge::LlmJudgeTask;
    use crate::vala::eval::operator::ComparisonOperator;
    use crate::vala::eval::result::{AssertionResult, EvalContextCapture, EvalPassGate};
    use crate::vala::eval::spec::{DatasetRef, EvalSampling, EvalSpec, MAX_EVAL_TASKS};
    use crate::vala::eval::task::EvalTask;
    use wyrd_semver::VersionBlock;

    fn data_ref(name: &str) -> CardRef {
        CardRef {
            kind: CardKind::Data,
            name: CardName::new(name).unwrap(),
            version: VersionBlock::parse("1.0.0").unwrap(),
            space: Some(SpaceName::new("default").unwrap()),
            uid: None,
        }
    }

    fn prompt_ref(name: &str) -> CardRef {
        CardRef {
            kind: CardKind::Prompt,
            name: CardName::new(name).unwrap(),
            version: VersionBlock::parse("1.0.0").unwrap(),
            space: Some(SpaceName::new("default").unwrap()),
            uid: None,
        }
    }

    fn agent_ref(name: &str) -> CardRef {
        CardRef {
            kind: CardKind::Agent,
            name: CardName::new(name).unwrap(),
            version: VersionBlock::parse("1.0.0").unwrap(),
            space: Some(SpaceName::new("default").unwrap()),
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
        spec.dataset = Some(DatasetRef::new(data_ref("eval-set").into()).unwrap());
        spec.sampling = Some(EvalSampling::Ratio { ratio: 0.1 });
        spec.pass_gate = Some(EvalPassGate::OverallPassRate { threshold: 0.9 });

        let serialized = serde_json::to_string(&spec).unwrap();
        let back: EvalSpec = serde_json::from_str(&serialized).unwrap();
        assert_eq!(spec, back);
    }

    #[test]
    fn dataset_ref_rejects_non_data_kind() {
        let err = DatasetRef::new(prompt_ref("prompt").into()).unwrap_err();
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

        if let EvalTask::Assertion(assertion) =
            spec.tasks.get_mut(&TaskId::new("a").unwrap()).unwrap()
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
        use crate::vala::eval::workflow::{Workflow, WorkflowFieldType};

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
            agent_ref("judge-agent"),
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
        use crate::vala::eval::operator::MAX_REGEX_PATTERN_LEN;
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
}

#[cfg(test)]
mod status_tests {
    use crate::vala::eval::EvalStatus;

    #[test]
    fn eval_status_round_trips_snake_case() {
        let json = serde_json::to_string(&EvalStatus::AwaitingTrace).expect("status serializes");
        assert_eq!(json, "\"awaiting_trace\"");
        let round_trip: EvalStatus = serde_json::from_str(&json).expect("status deserializes");
        assert_eq!(round_trip, EvalStatus::AwaitingTrace);
    }

    #[test]
    fn eval_status_rejects_unknown_variant() {
        let result: Result<EvalStatus, _> = serde_json::from_str("\"unknown\"");
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod task_enum_tests {
    use crate::vala::eval::agent::AgentAssertionTask;
    use crate::vala::eval::assertion::AssertionTask;
    use crate::vala::eval::ids::{JsonPath, TaskId};
    use crate::vala::eval::operator::ComparisonOperator;
    use crate::vala::eval::task::EvalTask;
    use crate::vala::eval::trace::TraceAssertionTask;

    fn assertion(id: &str, deps: Vec<&str>) -> EvalTask {
        EvalTask::Assertion(AssertionTask {
            id: TaskId::new(id).unwrap(),
            context_path: Some(JsonPath::new("$.x").unwrap()),
            item_context_path: None,
            operator: ComparisonOperator::IsNotNull,
            expected: serde_json::Value::Null,
            depends_on: deps.into_iter().map(|d| TaskId::new(d).unwrap()).collect(),
            condition: None,
        })
    }

    #[test]
    fn assertion_variant_round_trip() {
        let task = assertion("a", vec![]);
        let serialized = serde_json::to_string(&task).unwrap();
        assert!(serialized.starts_with(r#"{"kind":"assertion","#));
        let back: EvalTask = serde_json::from_str(&serialized).unwrap();
        assert_eq!(task, back);
    }

    #[test]
    fn id_accessor_matches_inner() {
        let task = assertion("alpha", vec![]);
        assert_eq!(task.id().as_str(), "alpha");
    }

    #[test]
    fn depends_on_accessor_matches_inner() {
        let task = assertion("beta", vec!["alpha"]);
        assert_eq!(task.depends_on().len(), 1);
        assert_eq!(task.depends_on()[0].as_str(), "alpha");
    }

    #[test]
    fn condition_accessor_returns_inner_option() {
        use crate::vala::eval::condition::EvalCondition;

        let mut assertion_task = match assertion("c", vec![]) {
            EvalTask::Assertion(assertion_task) => assertion_task,
            _ => unreachable!(),
        };
        assertion_task.condition = Some(EvalCondition {
            path: JsonPath::new("$.flag").unwrap(),
            operator: ComparisonOperator::IsTruthy,
            expected: serde_json::Value::Null,
            combinator: None,
            subsequent: None,
        });
        let task = EvalTask::Assertion(assertion_task);
        assert!(task.condition().is_some());
    }

    #[test]
    fn discriminator_matches_serde_tag() {
        let cases: &[(EvalTask, &str)] = &[
            (assertion("a", vec![]), "assertion"),
            (
                EvalTask::TraceAssertion(TraceAssertionTask {
                    id: TaskId::new("t").unwrap(),
                    span_selector: JsonPath::new("$.spans").unwrap(),
                    operator: ComparisonOperator::IsNonEmpty,
                    expected: serde_json::Value::Null,
                    depends_on: vec![],
                    condition: None,
                }),
                "trace_assertion",
            ),
            (
                EvalTask::AgentAssertion(AgentAssertionTask {
                    id: TaskId::new("ag").unwrap(),
                    workflow_field_path: JsonPath::new("$.tool_calls").unwrap(),
                    operator: ComparisonOperator::IsNonEmpty,
                    expected: serde_json::Value::Null,
                    depends_on: vec![],
                    condition: None,
                }),
                "agent_assertion",
            ),
        ];
        for (task, want) in cases {
            assert_eq!(task.discriminator(), *want);
            let value = serde_json::to_value(task).unwrap();
            assert_eq!(value["kind"].as_str().unwrap(), *want);
        }
    }

    #[test]
    fn rejects_unknown_kind() {
        let json = r#"{"kind":"conditional","id":"x"}"#;
        let result: Result<EvalTask, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "predecessor Conditional kind must be rejected"
        );
    }

    #[test]
    fn rejects_human_validation_kind() {
        let json = r#"{"kind":"human_validation","id":"x"}"#;
        let result: Result<EvalTask, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "predecessor HumanValidation kind must be rejected"
        );
    }
}

#[cfg(test)]
mod trace_assertion_tests {
    use crate::vala::eval::ids::{JsonPath, TaskId};
    use crate::vala::eval::operator::ComparisonOperator;
    use crate::vala::eval::trace::TraceAssertionTask;

    #[test]
    fn trace_assertion_round_trip() {
        let task = TraceAssertionTask {
            id: TaskId::new("trace_one").unwrap(),
            span_selector: JsonPath::new("$.spans[?(@.name=='retrieve')]").unwrap(),
            operator: ComparisonOperator::IsNonEmpty,
            expected: serde_json::Value::Null,
            depends_on: vec![],
            condition: None,
        };

        let serialized = serde_json::to_string(&task).unwrap();
        let deserialized: TraceAssertionTask = serde_json::from_str(&serialized).unwrap();
        assert_eq!(task, deserialized);
    }
}

#[cfg(test)]
mod workflow_tests {
    use std::collections::BTreeMap;

    use crate::vala::eval::{Workflow, WorkflowFieldType};

    #[test]
    fn workflow_serializes_field_map_ordered() {
        let mut fields = BTreeMap::new();
        fields.insert("response".to_string(), WorkflowFieldType::String);
        fields.insert("score".to_string(), WorkflowFieldType::Float);

        let workflow = Workflow { fields };
        let json = serde_json::to_string(&workflow).expect("workflow serializes");
        assert_eq!(json, r#"{"fields":{"response":"string","score":"float"}}"#);
    }

    #[test]
    fn workflow_field_type_round_trips() {
        for field_type in [
            WorkflowFieldType::String,
            WorkflowFieldType::Integer,
            WorkflowFieldType::Float,
            WorkflowFieldType::Boolean,
            WorkflowFieldType::Object,
            WorkflowFieldType::Array,
            WorkflowFieldType::Any,
        ] {
            let json = serde_json::to_string(&field_type).expect("field type serializes");
            let round_trip: WorkflowFieldType =
                serde_json::from_str(&json).expect("field type deserializes");
            assert_eq!(field_type, round_trip);
        }
    }
}
