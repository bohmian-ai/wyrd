use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::time::Instant;
use uuid::Uuid;
use vala_eval::context::{ContextSnapshot, ExecutionContext, TaskOutput};
use vala_eval::error::EvalExecError;
use vala_eval::executor::{
    EvalReport, Executors, SkipReason, TaskExecutor, TaskRunOutcome, execute_plan,
};
use vala_eval::store::{JudgeOutcome, TaskRegistry};
use vala_eval::tasks::{
    AgentTaskExecutor, AssertionTaskExecutor, JudgeTaskExecutor, TraceTaskExecutor,
};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::CardName;
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::eval::{
    AssertionResult, AssertionTask, ComparisonOperator, EvalCondition, EvalSpec, EvalTask,
    ExecutionPlan, JsonPath, LlmJudgeTask, RecordId, RunId, Stage, TaskId,
};
use wyrd_spec::version::VersionBlock;

fn tid(value: &str) -> TaskId {
    TaskId::new(value).expect("static test task id is valid")
}

fn jp(value: &str) -> JsonPath {
    JsonPath::new(value).expect("static test jsonpath is valid")
}

fn assertion(
    id: &str,
    path: Option<&str>,
    op: ComparisonOperator,
    expected: Value,
    deps: &[&str],
    condition: Option<EvalCondition>,
) -> EvalTask {
    EvalTask::Assertion(AssertionTask {
        id: tid(id),
        context_path: path.map(jp),
        item_context_path: None,
        operator: op,
        expected,
        depends_on: deps.iter().map(|dep| tid(dep)).collect(),
        condition,
    })
}

fn judge_card_ref() -> CardRef {
    CardRef {
        kind: CardKind::Prompt,
        name: CardName::new("test-judge").expect("static name valid"),
        version: VersionBlock::parse("1.0.0").expect("static version valid"),
        space: None,
        uid: None,
    }
}

fn llm_judge(id: &str, expected: Value, op: ComparisonOperator, deps: &[&str]) -> EvalTask {
    EvalTask::LlmJudge(LlmJudgeTask {
        id: tid(id),
        judge_ref: judge_card_ref(),
        context_path: None,
        expected,
        operator: op,
        depends_on: deps.iter().map(|dep| tid(dep)).collect(),
        max_retries: 0,
        condition: None,
    })
}

fn spec_of(tasks: Vec<EvalTask>) -> EvalSpec {
    let mut map = BTreeMap::new();
    for task in tasks {
        map.insert(task.id().clone(), task);
    }
    EvalSpec {
        subject_ref: None,
        dataset: None,
        tasks: map,
        workflow: None,
        sampling: None,
        pass_gate: None,
        context_capture: None,
    }
}

fn executors() -> Executors {
    Executors {
        assertion: Arc::new(AssertionTaskExecutor::new()),
        judge: Arc::new(JudgeTaskExecutor::new()),
        trace: Arc::new(TraceTaskExecutor::new()),
        agent: Arc::new(AgentTaskExecutor::new()),
    }
}

fn executors_with_judge(judge: Arc<dyn TaskExecutor>) -> Executors {
    Executors {
        assertion: Arc::new(AssertionTaskExecutor::new()),
        judge,
        trace: Arc::new(TraceTaskExecutor::new()),
        agent: Arc::new(AgentTaskExecutor::new()),
    }
}

fn registry_for(spec: &EvalSpec, plan: &ExecutionPlan) -> TaskRegistry {
    TaskRegistry::from_plan(plan, spec.tasks.clone()).expect("test spec registers")
}

fn context(base: Value) -> ExecutionContext {
    ExecutionContext::new(
        base,
        RunId::from_string("run-assertion-stage".to_string()),
        RecordId(Uuid::from_u128(7)),
        None,
    )
}

fn drive(spec: EvalSpec, base: Value) -> EvalReport {
    drive_with_executors(spec, base, executors())
}

fn drive_with_executors(spec: EvalSpec, base: Value, executors: Executors) -> EvalReport {
    let plan = spec.execution_plan().expect("test spec has valid dag");
    let registry = registry_for(&spec, &plan);
    let cx = context(base);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime builds");
    rt.block_on(execute_plan(&plan, &cx, &registry, &executors))
        .expect("plan executes")
}

#[test]
fn happy_path_single_assertion_stage_all_operator_families() {
    let spec = spec_of(vec![
        assertion(
            "num",
            Some("$.score"),
            ComparisonOperator::GreaterThan,
            json!(0.5),
            &[],
            None,
        ),
        assertion(
            "str",
            Some("$.response"),
            ComparisonOperator::Contains,
            json!("hello"),
            &[],
            None,
        ),
        assertion(
            "coll",
            Some("$.tools_used"),
            ComparisonOperator::IsSubset,
            json!(["search", "calculator"]),
            &[],
            None,
        ),
        assertion(
            "typ",
            Some("$.response"),
            ComparisonOperator::IsString,
            json!(null),
            &[],
            None,
        ),
        assertion(
            "tol",
            Some("$.score"),
            ComparisonOperator::WithinPctTolerance { pct: 0.25 },
            json!(0.7),
            &[],
            None,
        ),
    ]);
    let report = drive(
        spec,
        json!({
            "score": 0.72,
            "response": "hello world",
            "tools_used": ["search"],
        }),
    );
    assert_eq!(report.outcomes.len(), 5);
    for outcome in &report.outcomes {
        match outcome {
            TaskRunOutcome::Ran(result) => assert!(result.passed, "{:?} failed", result.task_id),
            TaskRunOutcome::Skipped { .. } => panic!("no skips expected"),
        }
    }
}

#[test]
fn multi_stage_dependency_chain_reads_a_result() {
    let cond_a_passed = EvalCondition {
        path: jp("$.task.A.passed"),
        operator: ComparisonOperator::Equals,
        expected: json!(true),
        combinator: None,
        subsequent: None,
    };
    let spec = spec_of(vec![
        assertion(
            "A",
            Some("$.score"),
            ComparisonOperator::GreaterThan,
            json!(0.0),
            &[],
            None,
        ),
        assertion(
            "B",
            Some("$.response"),
            ComparisonOperator::IsString,
            json!(null),
            &["A"],
            Some(cond_a_passed),
        ),
    ]);
    let report = drive(spec, json!({ "score": 1.0, "response": "ok" }));
    assert_eq!(report.outcomes.len(), 2);
    assert!(matches!(&report.outcomes[0], TaskRunOutcome::Ran(r) if r.task_id == tid("A")));
    assert!(
        matches!(&report.outcomes[1], TaskRunOutcome::Ran(r) if r.task_id == tid("B") && r.passed)
    );
}

#[test]
fn conditional_false_skips_task_with_condition_false_reason() {
    let cond_low_score = EvalCondition {
        path: jp("$.score"),
        operator: ComparisonOperator::GreaterThan,
        expected: json!(0.99),
        combinator: None,
        subsequent: None,
    };
    let spec = spec_of(vec![assertion(
        "skip_me",
        Some("$.score"),
        ComparisonOperator::GreaterThan,
        json!(0.0),
        &[],
        Some(cond_low_score),
    )]);
    let report = drive(spec, json!({ "score": 0.1 }));
    assert_eq!(report.outcomes.len(), 1);
    assert!(matches!(
        &report.outcomes[0],
        TaskRunOutcome::Skipped {
            reason: SkipReason::ConditionFalse,
            ..
        },
    ));
}

#[test]
fn dependency_skip_propagation() {
    let cond_never = EvalCondition {
        path: jp("$.score"),
        operator: ComparisonOperator::Equals,
        expected: json!(-1),
        combinator: None,
        subsequent: None,
    };
    let spec = spec_of(vec![
        assertion(
            "A",
            Some("$.score"),
            ComparisonOperator::GreaterThan,
            json!(0.0),
            &[],
            Some(cond_never),
        ),
        assertion(
            "B",
            Some("$.score"),
            ComparisonOperator::GreaterThan,
            json!(0.0),
            &["A"],
            None,
        ),
    ]);
    let report = drive(spec, json!({ "score": 0.5 }));
    assert_eq!(report.outcomes.len(), 2);
    assert!(matches!(
        &report.outcomes[0],
        TaskRunOutcome::Skipped {
            reason: SkipReason::ConditionFalse,
            ..
        },
    ));
    match &report.outcomes[1] {
        TaskRunOutcome::Skipped {
            reason: SkipReason::DependencySkipped { upstream },
            ..
        } => assert_eq!(upstream, &tid("A")),
        other => panic!("expected DependencySkipped, got {other:?}"),
    }
}

#[test]
fn missing_jsonpath_surfaces_typed_error_as_failed_result() {
    let spec = spec_of(vec![assertion(
        "broken",
        Some("$.does_not_exist"),
        ComparisonOperator::IsString,
        json!(null),
        &[],
        None,
    )]);
    let report = drive(spec, json!({ "score": 1.0 }));
    assert_eq!(report.outcomes.len(), 1);
    match &report.outcomes[0] {
        TaskRunOutcome::Ran(result) => {
            assert!(!result.passed);
            assert!(
                result
                    .message
                    .as_deref()
                    .unwrap_or("")
                    .contains("extract path")
            );
        }
        other => panic!("expected Ran(Failed), got {other:?}"),
    }
}

#[test]
fn operator_type_mismatch_surfaces_typed_error_as_failed_result() {
    let spec = spec_of(vec![assertion(
        "mismatch",
        Some("$.response"),
        ComparisonOperator::GreaterThan,
        json!(0.5),
        &[],
        None,
    )]);
    let report = drive(spec, json!({ "response": "not a number" }));
    assert_eq!(report.outcomes.len(), 1);
    match &report.outcomes[0] {
        TaskRunOutcome::Ran(result) => {
            assert!(!result.passed);
            assert!(
                result
                    .message
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains("operator")
            );
        }
        other => panic!("expected Ran(Failed), got {other:?}"),
    }
}

#[test]
fn cross_task_isolation_via_scoped_view() {
    let spec = spec_of(vec![
        assertion(
            "A",
            Some("$.score"),
            ComparisonOperator::GreaterThan,
            json!(0.0),
            &[],
            None,
        ),
        assertion(
            "B",
            Some("$.task.A.passed"),
            ComparisonOperator::Equals,
            json!(true),
            &["A"],
            None,
        ),
    ]);
    let report = drive(spec, json!({ "score": 1.0 }));
    assert_eq!(report.outcomes.len(), 2);
    let b = report
        .outcomes
        .iter()
        .find_map(|outcome| match outcome {
            TaskRunOutcome::Ran(result) if result.task_id == tid("B") => Some(result),
            _ => None,
        })
        .expect("B ran");
    assert!(b.passed, "B reads A's output through the scoped view");
}

#[test]
fn malformed_plan_surfaces_dag_invalid() {
    let spec = spec_of(vec![assertion(
        "real",
        Some("$.score"),
        ComparisonOperator::GreaterThan,
        json!(0.0),
        &[],
        None,
    )]);
    let bogus_plan = ExecutionPlan {
        stages: vec![Stage {
            index: 0,
            tasks: vec![tid("ghost")],
        }],
        task_count: 1,
    };
    let valid_plan = spec.execution_plan().expect("spec dag valid");
    let registry = registry_for(&spec, &valid_plan);
    let cx = context(json!({"score": 1.0}));
    let executors = executors();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime builds");
    let error = rt
        .block_on(execute_plan(&bogus_plan, &cx, &registry, &executors))
        .expect_err("malformed plan fails");
    assert!(matches!(error, EvalExecError::DagInvalid { .. }));
}

// --- New shared-context-across-stages tests ---

#[derive(Default)]
struct StubJudgeExecutor {
    parsed_verdict: Value,
}

#[async_trait]
impl TaskExecutor for StubJudgeExecutor {
    async fn execute(
        &self,
        task: &EvalTask,
        _snapshot: &ContextSnapshot,
        stage: u32,
    ) -> Result<TaskOutput, EvalExecError> {
        let EvalTask::LlmJudge(judge) = task else {
            return Err(EvalExecError::DagInvalid {
                reason: format!(
                    "stub judge invoked on non-judge task {:?}",
                    task.id().as_str()
                ),
            });
        };
        let outcome = JudgeOutcome {
            raw: json!({"choices": [{"parsed": self.parsed_verdict.clone()}]}),
            parsed: self.parsed_verdict.clone(),
        };
        let result = AssertionResult {
            task_id: judge.id.clone(),
            passed: true,
            actual: Some(self.parsed_verdict.clone()),
            expected: judge.expected.clone(),
            operator: judge.operator.clone(),
            message: None,
            stage,
            started_at: chrono::Utc::now(),
            duration_ms: 0,
        };
        Ok(TaskOutput::Judge { result, outcome })
    }
}

struct SleepingJudgeExecutor {
    sleep: Duration,
}

#[async_trait]
impl TaskExecutor for SleepingJudgeExecutor {
    async fn execute(
        &self,
        task: &EvalTask,
        _snapshot: &ContextSnapshot,
        stage: u32,
    ) -> Result<TaskOutput, EvalExecError> {
        tokio::time::sleep(self.sleep).await;
        let EvalTask::LlmJudge(judge) = task else {
            return Err(EvalExecError::DagInvalid {
                reason: format!(
                    "sleeping judge invoked on non-judge task {:?}",
                    task.id().as_str()
                ),
            });
        };
        let parsed = json!({"verdict": "pass"});
        let outcome = JudgeOutcome {
            raw: parsed.clone(),
            parsed: parsed.clone(),
        };
        let result = AssertionResult {
            task_id: judge.id.clone(),
            passed: true,
            actual: Some(parsed),
            expected: judge.expected.clone(),
            operator: judge.operator.clone(),
            message: None,
            stage,
            started_at: chrono::Utc::now(),
            duration_ms: 0,
        };
        Ok(TaskOutput::Judge { result, outcome })
    }
}

#[test]
fn multi_stage_judge_then_assertion_reads_judge_output() {
    // Stage 0 runs a stub judge producing parsed = {"verdict": "pass"}.
    // Stage 1 has an assertion with depends_on=["J"] reading $.task.J.parsed.verdict.
    // The shared-context model must propagate the judge's parsed payload via
    // ContextSnapshot::extend so the downstream JSONPath resolves.
    let spec = spec_of(vec![
        llm_judge("J", json!("pass"), ComparisonOperator::Equals, &[]),
        assertion(
            "A",
            Some("$.task.J.parsed.verdict"),
            ComparisonOperator::Equals,
            json!("pass"),
            &["J"],
            None,
        ),
    ]);
    let stub = Arc::new(StubJudgeExecutor {
        parsed_verdict: json!({"verdict": "pass"}),
    });
    let executors = executors_with_judge(stub);
    let report = drive_with_executors(spec, json!({ "score": 1.0 }), executors);
    assert_eq!(report.outcomes.len(), 2);
    let a = report
        .outcomes
        .iter()
        .find_map(|outcome| match outcome {
            TaskRunOutcome::Ran(result) if result.task_id == tid("A") => Some(result),
            _ => None,
        })
        .expect("A ran");
    assert!(
        a.passed,
        "A must read judge's parsed output via $.task.J.parsed.verdict, got message: {:?}",
        a.message
    );
}

#[test]
fn concurrent_same_stage_judges_complete_in_parallel() {
    // Four judges in stage 0, each sleeping ~100ms. With per-task-type
    // JoinSet fan-out, the four should run concurrently — wall-clock ~100ms
    // not ~400ms.
    let spec = spec_of(vec![
        llm_judge("j0", json!("pass"), ComparisonOperator::Equals, &[]),
        llm_judge("j1", json!("pass"), ComparisonOperator::Equals, &[]),
        llm_judge("j2", json!("pass"), ComparisonOperator::Equals, &[]),
        llm_judge("j3", json!("pass"), ComparisonOperator::Equals, &[]),
    ]);
    let sleeper = Arc::new(SleepingJudgeExecutor {
        sleep: Duration::from_millis(100),
    });
    let executors = executors_with_judge(sleeper);
    let plan = spec.execution_plan().expect("spec dag valid");
    let registry = registry_for(&spec, &plan);
    let cx = context(json!({}));
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime builds");
    let elapsed = rt.block_on(async {
        let start = Instant::now();
        let report = execute_plan(&plan, &cx, &registry, &executors)
            .await
            .expect("plan executes");
        let elapsed = start.elapsed();
        assert_eq!(report.outcomes.len(), 4);
        for outcome in &report.outcomes {
            assert!(matches!(outcome, TaskRunOutcome::Ran(_)));
        }
        elapsed
    });
    assert!(
        elapsed < Duration::from_millis(400),
        "four 100ms judges should complete in parallel, took {elapsed:?}"
    );
}

#[test]
fn dependency_skip_propagates_across_stages_via_ledger() {
    // A: stage 0, condition-skipped.
    // B: stage 1, depends on A → DependencySkipped { upstream: A }.
    // C: stage 2, depends on B → DependencySkipped { upstream: B }.
    // Proves the RunLedger survives stage transitions and the skip set
    // carries forward across multiple stage barriers.
    let cond_never = EvalCondition {
        path: jp("$.score"),
        operator: ComparisonOperator::Equals,
        expected: json!(-1),
        combinator: None,
        subsequent: None,
    };
    let spec = spec_of(vec![
        assertion(
            "A",
            Some("$.score"),
            ComparisonOperator::GreaterThan,
            json!(0.0),
            &[],
            Some(cond_never),
        ),
        assertion(
            "B",
            Some("$.score"),
            ComparisonOperator::GreaterThan,
            json!(0.0),
            &["A"],
            None,
        ),
        assertion(
            "C",
            Some("$.score"),
            ComparisonOperator::GreaterThan,
            json!(0.0),
            &["B"],
            None,
        ),
    ]);
    let report = drive(spec, json!({ "score": 0.5 }));
    assert_eq!(report.outcomes.len(), 3);
    assert!(matches!(
        &report.outcomes[0],
        TaskRunOutcome::Skipped {
            reason: SkipReason::ConditionFalse,
            ..
        }
    ));
    match &report.outcomes[1] {
        TaskRunOutcome::Skipped {
            reason: SkipReason::DependencySkipped { upstream },
            task_id,
        } => {
            assert_eq!(task_id, &tid("B"));
            assert_eq!(upstream, &tid("A"));
        }
        other => panic!("expected B DependencySkipped, got {other:?}"),
    }
    match &report.outcomes[2] {
        TaskRunOutcome::Skipped {
            reason: SkipReason::DependencySkipped { upstream },
            task_id,
        } => {
            assert_eq!(task_id, &tid("C"));
            assert_eq!(upstream, &tid("B"));
        }
        other => panic!("expected C DependencySkipped, got {other:?}"),
    }
}
