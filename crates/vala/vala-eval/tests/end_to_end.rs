//! End-to-end demonstration of the `vala-eval` engine against an in-memory
//! record + spans.
//!
//! The spec exercised here is deliberately representative of a real eval:
//!
//! ```text
//!     Stage 0 (no deps; run in parallel)
//!       ┌────────────────────────────┐    ┌────────────────────────────┐
//!       │ agent_response_nonempty    │    │ trace_call_recorded        │
//!       │   (AgentAssertion)         │    │   (TraceAssertion)         │
//!       │   $.response is non-empty  │    │   $.spans[0].name = "..."  │
//!       └────────────┬───────────────┘    └────────────────────────────┘
//!                    │
//!     Stage 1 (depend on stage 0)
//!       ┌────────────▼───────────────┐    ┌────────────▼───────────────┐
//!       │ score_clears_threshold     │    │ judge_response             │
//!       │   (Assertion)              │    │   (LlmJudge)               │
//!       │   $.score >= 0.5           │    │   judge → {"passed": true} │
//!       └────────────┬───────────────┘    └────────────┬───────────────┘
//!                    │                                 │
//!     Stage 2 (depend on stage 1; read prior outputs)
//!       ┌────────────▼───────────────┐    ┌────────────▼───────────────┐
//!       │ regression_branch_skipped  │    │ propagate_judge_verdict    │
//!       │   (Assertion, condition:   │    │   (Assertion)              │
//!       │     score_clears_threshold │    │   $.task.judge_response    │
//!       │     .passed == false)      │    │     .parsed.passed == true │
//!       │   → ConditionFalse skip    │    │   reads upstream output    │
//!       └────────────────────────────┘    └────────────────────────────┘
//! ```
//!
//! After execution, the per-task outcomes are folded into an
//! `AggregationInput` and `aggregate_run` is asserted to roll up to a
//! 5-of-5 run-level pass that clears the configured `OverallPassRate` gate.
//!
//! What this test pins:
//! - All four `EvalTask` variants run inside one plan (Assertion,
//!   LlmJudge, TraceAssertion, AgentAssertion).
//! - The DAG has multiple stages, and stage-2 tasks read outputs of stage-1
//!   tasks via the `$.task.<id>.*` scoped view (`propagate_judge_verdict`
//!   reads the judge's `parsed.passed`).
//! - Conditional skip propagates correctly: a task whose condition resolves
//!   to `false` is recorded as `SkipReason::ConditionFalse` and does NOT
//!   inflate the denominator of the run-level metrics.
//! - `aggregate_run` produces a stable JSON artifact that round-trips.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{TimeZone, Utc};
use serde_json::json;
use uuid::Uuid;

use vala_eval::context::ExecutionContext;
use vala_eval::executor::{Executors, TaskRunOutcome, execute_plan};
use vala_eval::store::TaskRegistry;
use vala_eval::tasks::{
    AgentTaskExecutor, AssertionTaskExecutor, JudgeTaskExecutor, TraceTaskExecutor,
};
use vala_eval::{
    AggregationInput, EvalResults, InMemoryTraceSource, MechanicSubjectInput, MockJudgeInvoker,
    ResultsConfig, RunIdentity, ScenarioAggregationInput, SkipReason, SubjectKey, aggregate_run,
};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardName, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::eval::{
    AgentAssertionTask, AssertionResult, AssertionTask, ComparisonOperator, EvalCondition,
    EvalPassGate, EvalSpec, EvalTask, JsonPath, LlmJudgeTask, RecordId, RunId, ScenarioId, TaskId,
    TraceAssertionTask,
};
use wyrd_spec::vala::ids::{DataTenantId, SpanId, TraceId};
use wyrd_spec::vala::trace::{InstrumentationScope, Resource, SpanKind, SpanRecord, SpanStatus};
use wyrd_spec::version::VersionBlock;

// ---- helpers ---------------------------------------------------------------

fn tid(value: &str) -> TaskId {
    TaskId::new(value).expect("static task id is valid")
}

fn jp(value: &str) -> JsonPath {
    JsonPath::new(value).expect("static JSONPath is valid")
}

fn card_ref(kind: CardKind, name: &str) -> CardRef {
    CardRef {
        kind,
        name: CardName::new(name).expect("static card name is valid"),
        version: VersionBlock::parse("1.0.0").expect("static version is valid"),
        space: Some(SpaceName::new("tests").expect("static space is valid")),
        uid: None,
    }
}

fn eval_ref() -> CardRef {
    card_ref(CardKind::Eval, "end-to-end-rubric")
}

fn subject_ref() -> CardRef {
    card_ref(CardKind::Agent, "agent-under-test")
}

fn judge_ref() -> CardRef {
    card_ref(CardKind::Prompt, "judge")
}

fn trace_id() -> TraceId {
    TraceId::from_hex("11111111111111111111111111111111").expect("static trace id is valid")
}

fn span_id() -> SpanId {
    SpanId::from_hex("2222222222222222").expect("static span id is valid")
}

fn agent_span() -> SpanRecord {
    let start_time = Utc
        .with_ymd_and_hms(2026, 6, 10, 12, 0, 0)
        .single()
        .expect("static timestamp is valid");
    let end_time = Utc
        .with_ymd_and_hms(2026, 6, 10, 12, 0, 1)
        .single()
        .expect("static timestamp is valid");
    SpanRecord {
        trace_id: trace_id(),
        span_id: span_id(),
        parent_span_id: None,
        flags: 0,
        trace_state: String::new(),
        name: "agent_call".to_owned(),
        kind: SpanKind::Internal,
        start_time,
        end_time,
        duration_ms: 1000,
        status: SpanStatus::Ok,
        attributes: serde_json::Map::new(),
        dropped_attributes_count: 0,
        events: Vec::new(),
        dropped_events_count: 0,
        links: Vec::new(),
        dropped_links_count: 0,
        scope: InstrumentationScope {
            name: "vala-eval-end-to-end".to_owned(),
            version: None,
            attributes: serde_json::Map::new(),
        },
        resource: Resource {
            service_name: "eval-test".to_owned(),
            service_namespace: None,
            service_version: None,
            service_instance_id: None,
            attributes: serde_json::Map::new(),
        },
        data_tenant_id: DataTenantId::new("tenant-a").expect("static tenant is valid"),
    }
}

// ---- spec construction -----------------------------------------------------

/// Build the six-task spec sketched in the module doc above.
fn full_spec() -> EvalSpec {
    // Stage 0: hits the record and the trace; no upstream deps.
    let agent_response_nonempty = EvalTask::AgentAssertion(AgentAssertionTask {
        id: tid("agent_response_nonempty"),
        workflow_field_path: jp("$.response"),
        operator: ComparisonOperator::IsNonEmpty,
        expected: json!(null),
        depends_on: Vec::new(),
        condition: None,
    });
    let trace_call_recorded = EvalTask::TraceAssertion(TraceAssertionTask {
        id: tid("trace_call_recorded"),
        span_selector: jp("$.spans[0].name"),
        operator: ComparisonOperator::Equals,
        expected: json!("agent_call"),
        depends_on: Vec::new(),
        condition: None,
    });

    // Stage 1: depend on stage 0 having run successfully.
    let score_clears_threshold = EvalTask::Assertion(AssertionTask {
        id: tid("score_clears_threshold"),
        context_path: Some(jp("$.score")),
        item_context_path: None,
        operator: ComparisonOperator::GreaterThanOrEquals,
        expected: json!(0.5),
        depends_on: vec![tid("agent_response_nonempty")],
        condition: None,
    });
    let judge_response = EvalTask::LlmJudge(LlmJudgeTask {
        id: tid("judge_response"),
        judge_ref: judge_ref(),
        context_path: None,
        operator: ComparisonOperator::Equals,
        expected: json!({"passed": true}),
        depends_on: vec![tid("agent_response_nonempty")],
        max_retries: 0,
        condition: None,
    });

    // Stage 2 (a): consume the prior judge's parsed verdict through the
    // `$.task.<id>.parsed.*` scoped view. This is the real cross-task
    // dataflow the engine guarantees.
    let propagate_judge_verdict = EvalTask::Assertion(AssertionTask {
        id: tid("propagate_judge_verdict"),
        context_path: Some(jp("$.task.judge_response.parsed.passed")),
        item_context_path: None,
        operator: ComparisonOperator::Equals,
        expected: json!(true),
        depends_on: vec![tid("judge_response")],
        condition: None,
    });

    // Stage 2 (b): a regression-only assertion that fires only when the score
    // assertion failed. In this fixture the score assertion passes, so the
    // condition resolves to `false` and the engine records ConditionFalse.
    let regression_branch_skipped = EvalTask::Assertion(AssertionTask {
        id: tid("regression_branch_skipped"),
        context_path: Some(jp("$.response")),
        item_context_path: None,
        operator: ComparisonOperator::IsString,
        expected: json!(null),
        depends_on: vec![tid("score_clears_threshold")],
        condition: Some(EvalCondition {
            path: jp("$.task.score_clears_threshold.passed"),
            operator: ComparisonOperator::Equals,
            expected: json!(false),
            combinator: None,
            subsequent: None,
        }),
    });

    let mut tasks = BTreeMap::new();
    for task in [
        agent_response_nonempty,
        trace_call_recorded,
        score_clears_threshold,
        judge_response,
        propagate_judge_verdict,
        regression_branch_skipped,
    ] {
        tasks.insert(task.id().clone(), task);
    }
    EvalSpec {
        subject_ref: Some(subject_ref()),
        dataset: None,
        tasks,
        workflow: None,
        sampling: None,
        pass_gate: Some(EvalPassGate::OverallPassRate { threshold: 0.75 }),
        context_capture: None,
    }
}

fn executors_with_trace_source(source: Arc<dyn vala_eval::TraceSource>) -> Executors {
    // One scripted judge response: deterministic verdict for this fixture.
    let judge = MockJudgeInvoker::new([Ok(json!({"passed": true}))]);
    Executors {
        assertion: Arc::new(AssertionTaskExecutor::new()),
        judge: Arc::new(JudgeTaskExecutor::new(judge)),
        trace: Arc::new(TraceTaskExecutor::new(source, Duration::from_millis(250))),
        agent: Arc::new(AgentTaskExecutor::new()),
    }
}

fn record_context() -> ExecutionContext {
    ExecutionContext::new(
        json!({
            "response": "hello world",
            "score": 0.82,
        }),
        RunId::from_string("run-end-to-end".to_owned()),
        RecordId(Uuid::from_u128(0x42)),
        Some(ScenarioId::new("scen_e2e").expect("static scenario id is valid")),
    )
    .with_trace_id(trace_id())
}

fn partition_outcomes(
    report: &vala_eval::EvalReport,
) -> (BTreeMap<TaskId, AssertionResult>, Vec<(TaskId, SkipReason)>) {
    let mut ran = BTreeMap::new();
    let mut skipped = Vec::new();
    for outcome in &report.outcomes {
        match outcome {
            TaskRunOutcome::Ran(result) => {
                ran.insert(result.task_id.clone(), result.clone());
            }
            TaskRunOutcome::Skipped { task_id, reason } => {
                skipped.push((task_id.clone(), reason.clone()));
            }
        }
    }
    (ran, skipped)
}

// ---- the test -------------------------------------------------------------

#[tokio::test]
async fn full_spec_against_in_memory_record_and_spans_aggregates_correctly() {
    // Arrange: in-memory record JSON + one span on the trace + one scripted
    // judge response. The fixture is deliberately a happy path so the
    // engine's structural guarantees (skips, scoped view, aggregation) are
    // the only thing this assertion is measuring.
    let spec = full_spec();
    let plan = spec.execution_plan().expect("plan validates");
    let registry = TaskRegistry::from_plan(&plan, spec.tasks.clone()).expect("registry from plan");

    let trace_source = Arc::new(InMemoryTraceSource::new());
    trace_source.insert(trace_id(), vec![agent_span()]).await;
    let executors = executors_with_trace_source(trace_source);
    let cx = record_context();

    // Act: drive the plan to completion.
    let report = execute_plan(&plan, &cx, &registry, &executors)
        .await
        .expect("plan executes");

    // Assert: six tasks in, five ran, one skipped via ConditionFalse.
    assert_eq!(report.outcomes.len(), 6, "six declared tasks");
    let (ran, skipped) = partition_outcomes(&report);
    assert_eq!(ran.len(), 5, "five tasks should have run");
    assert_eq!(skipped.len(), 1, "one task should have been skipped");

    for id in [
        "agent_response_nonempty",
        "trace_call_recorded",
        "score_clears_threshold",
        "judge_response",
        "propagate_judge_verdict",
    ] {
        let result = ran
            .get(&tid(id))
            .unwrap_or_else(|| panic!("task `{id}` should have run"));
        assert!(result.passed, "task `{id}` should have passed");
    }

    let (skip_task, skip_reason) = skipped
        .into_iter()
        .next()
        .expect("one skip outcome recorded");
    assert_eq!(skip_task, tid("regression_branch_skipped"));
    assert!(
        matches!(skip_reason, SkipReason::ConditionFalse),
        "expected SkipReason::ConditionFalse, got {skip_reason:?}",
    );

    // The stage-2 → stage-1 cross-task read is the engine guarantee
    // that's easiest to silently lose; pin it explicitly.
    let propagate = &ran[&tid("propagate_judge_verdict")];
    assert_eq!(
        propagate.actual.as_ref(),
        Some(&json!(true)),
        "stage-2 assertion must have read true out of the judge's parsed verdict",
    );

    // Fold the engine outcomes into the per-scenario aggregation input.
    let mut task_results: BTreeMap<TaskId, Vec<AssertionResult>> = BTreeMap::new();
    for (id, result) in ran {
        task_results.insert(id, vec![result]);
    }
    let subject = subject_ref();
    let mut mechanic_results = BTreeMap::new();
    mechanic_results.insert(
        SubjectKey::from_ref(&subject),
        MechanicSubjectInput {
            subject_ref: subject.clone(),
            task_results,
        },
    );
    let scenario_input = ScenarioAggregationInput {
        scenario_id: ScenarioId::new("scen_e2e").expect("static scenario id is valid"),
        mechanic_results,
        passenger_tasks: Vec::new(),
        conversation_history: Vec::new(),
        started_at: Utc::now(),
        duration_ms: report
            .outcomes
            .iter()
            .filter_map(|outcome| match outcome {
                TaskRunOutcome::Ran(result) => Some(result.duration_ms),
                TaskRunOutcome::Skipped { .. } => None,
            })
            .sum(),
    };

    let aggregated = aggregate_run(AggregationInput {
        identity: RunIdentity {
            run_id: RunId::from_string("run-end-to-end".to_owned()),
            eval_ref: eval_ref(),
            started_at: Utc::now(),
            ended_at: Utc::now(),
        },
        scenarios: vec![scenario_input],
        config: ResultsConfig {
            context_capture: None,
            pass_gate: spec.pass_gate.clone(),
        },
    })
    .expect("aggregate_run succeeds");

    // The skipped task does NOT inflate the denominator.
    assert_eq!(aggregated.metrics.total_tasks, 5);
    assert_eq!(aggregated.metrics.passed_tasks, 5);
    assert!((aggregated.metrics.pass_rate - 1.0).abs() < f64::EPSILON);
    assert_eq!(aggregated.metrics.total_scenarios, 1);
    assert_eq!(aggregated.metrics.passed_scenarios, 1);
    assert!((aggregated.metrics.scenario_pass_rate - 1.0).abs() < f64::EPSILON);

    let verdict = aggregated
        .pass_gate_verdict
        .as_ref()
        .expect("pass gate verdict present");
    assert!(verdict.passed, "1.0 pass rate clears the 0.75 threshold");

    // JSON round-trip pins the aggregated artifact's stability.
    let raw = aggregated.to_json().expect("serialize aggregated");
    let loaded = EvalResults::from_json(&raw).expect("deserialize aggregated");
    assert_eq!(aggregated, loaded);
}
