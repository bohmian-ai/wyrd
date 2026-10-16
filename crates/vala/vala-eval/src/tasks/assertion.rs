//! `AssertionTask` executor.
//!
//! Pure compute over JSON wrapped in `async` so the trait stays uniform. The
//! body returns immediately; no real `await` happens here.

use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;

use wyrd_spec::vala::eval::{
    AssertionResult, AssertionTask, ComparisonOperator, EvalTask, JsonPath, TaskId,
};

use crate::context::{ContextSnapshot, TaskOutput, extract_required_jsonpath_from};
use crate::error::EvalExecError;
use crate::executor::TaskExecutor;
use crate::operators;

/// Executor for deterministic assertion tasks.
#[derive(Default)]
pub struct AssertionTaskExecutor;

impl AssertionTaskExecutor {
    /// Fresh assertion executor.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Judge one `task` against `snapshot` at `stage`.
    ///
    /// The typed entry point shared by Eval graph dispatch and a standalone
    /// task Verifier: it resolves the task's path in the scoped view and
    /// applies its comparison operator, with no knowledge of the caller.
    ///
    /// # Errors
    /// Returns [`EvalExecError::ExtractPathMissing`] or
    /// [`EvalExecError::JsonPathFailure`] when a path does not resolve, and
    /// [`EvalExecError::OperatorTypeMismatch`] when the operator cannot
    /// compare the observed and expected values.
    pub fn assert(
        &self,
        task: &AssertionTask,
        snapshot: &ContextSnapshot,
        stage: u32,
    ) -> Result<AssertionResult, EvalExecError> {
        execute_assertion(task, snapshot, stage)
    }
}

#[async_trait]
impl TaskExecutor for AssertionTaskExecutor {
    async fn execute(
        &self,
        task: &EvalTask,
        snapshot: &ContextSnapshot,
        stage: u32,
    ) -> Result<TaskOutput, EvalExecError> {
        let EvalTask::Assertion(assertion) = task else {
            return Err(EvalExecError::DagInvalid {
                reason: format!(
                    "stage {stage} dispatched {} task {:?} to assertion executor",
                    task.discriminator(),
                    task.id().as_str()
                ),
            });
        };
        Ok(TaskOutput::Assertion(
            self.assert(assertion, snapshot, stage)?,
        ))
    }
}

/// Judge one assertion task against `snapshot`.
///
/// Selects the observed value from the task's scoped view (the record
/// context plus the declared dependencies' outputs under `$.task`), using
/// the item path in item mode or the context path otherwise; without a path
/// it compares the whole view, or the bare record when there are no
/// dependencies. Item mode
/// compares every selected item, single mode the one selected value. The
/// result is attributed to the task at `stage` with its start time and
/// duration; a false comparison is a Failed result, not an error.
///
/// # Errors
/// Returns the [`EvalExecError`] of a selection that cannot be extracted
/// (an invalid or missing required path) and of an operator that cannot
/// compare the selected value's type, unchanged.
fn execute_assertion(
    task: &AssertionTask,
    snapshot: &ContextSnapshot,
    stage: u32,
) -> Result<AssertionResult, EvalExecError> {
    let started_at = Utc::now();
    let view = snapshot.build_scoped_view(&task.depends_on);
    let observed = resolve_observed(task, &view, snapshot)?;
    let (passed, actual, expected, message) = match &task.item_context_path {
        Some(_) => evaluate_item_mode(&task.id, &task.operator, &task.expected, &observed)?,
        None => evaluate_single(&task.id, &task.operator, &task.expected, &observed)?,
    };

    Ok(AssertionResult {
        task_id: task.id.clone(),
        passed,
        actual,
        expected,
        operator: task.operator.clone(),
        message,
        stage,
        started_at,
        duration_ms: elapsed_ms(started_at),
    })
}

fn resolve_observed(
    task: &AssertionTask,
    view: &Value,
    snapshot: &ContextSnapshot,
) -> Result<Value, EvalExecError> {
    if let Some(item_path) = &task.item_context_path {
        return extract(view, item_path, &task.id);
    }
    if let Some(path) = &task.context_path {
        return extract(view, path, &task.id);
    }
    // No explicit path: compare against the whole scoped view when deps
    // contribute outputs, otherwise the bare observation document.
    if task.depends_on.is_empty() {
        Ok((*snapshot.base_context).clone())
    } else {
        Ok(view.clone())
    }
}

fn extract(value: &Value, path: &JsonPath, task_id: &TaskId) -> Result<Value, EvalExecError> {
    extract_required_jsonpath_from(value, path, task_id)
}

/// The compared outcome of one assertion: passed, observed actual, the
/// expected value compared, and a failure message.
pub(crate) type Compared = (bool, Option<Value>, Value, Option<String>);

/// Compare one `observed` value with `expected` under `operator`.
///
/// Shared by Eval assertion tasks and Verifier `assert` checks over a `path`.
///
/// # Errors
/// Returns [`EvalExecError::OperatorTypeMismatch`] for `task_id` when the
/// operator cannot compare the values.
pub(crate) fn evaluate_single(
    task_id: &TaskId,
    operator: &ComparisonOperator,
    expected: &Value,
    observed: &Value,
) -> Result<Compared, EvalExecError> {
    let verdict = operators::evaluate_operator(observed, operator, expected).map_err(|error| {
        EvalExecError::OperatorTypeMismatch {
            task_id: task_id.clone(),
            operator: format!("{operator:?}"),
            reason: error.to_string(),
        }
    })?;
    let message = if verdict.passed {
        None
    } else {
        Some(format!("operator {operator:?} returned false"))
    };
    Ok((
        verdict.passed,
        verdict.observed,
        verdict.expected.unwrap_or_else(|| expected.clone()),
        message,
    ))
}

/// Compare every element of the `observed` array with `expected`; the
/// assertion passes only when every element passes, so an empty array passes.
///
/// Shared by Eval item-mode assertions and Verifier `assert` checks over an
/// `each` selector.
///
/// # Errors
/// Returns [`EvalExecError::OperatorTypeMismatch`] for `task_id` when
/// `observed` is not an array or the operator cannot compare an element.
pub(crate) fn evaluate_item_mode(
    task_id: &TaskId,
    operator: &ComparisonOperator,
    expected: &Value,
    observed: &Value,
) -> Result<Compared, EvalExecError> {
    let mismatch = |reason: String| EvalExecError::OperatorTypeMismatch {
        task_id: task_id.clone(),
        operator: format!("{operator:?}"),
        reason,
    };
    let Value::Array(items) = observed else {
        return Err(mismatch(
            "item_context_path resolved to a non-array value".to_string(),
        ));
    };

    let mut failures = 0_usize;
    for item in items {
        let verdict = operators::evaluate_operator(item, operator, expected)
            .map_err(|error| mismatch(error.to_string()))?;
        if !verdict.passed {
            failures += 1;
        }
    }

    let message = if failures == 0 {
        None
    } else {
        Some(format!(
            "{failures}/{} items failed operator {operator:?}",
            items.len(),
        ))
    };
    Ok((
        failures == 0,
        Some(observed.clone()),
        expected.clone(),
        message,
    ))
}

fn elapsed_ms(start: chrono::DateTime<chrono::Utc>) -> u64 {
    let diff = Utc::now().signed_duration_since(start);
    u64::try_from(diff.num_milliseconds()).unwrap_or(0)
}

#[cfg(test)]
mod assertion_stage {
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::time::Duration;

    use crate::InMemoryTraceSource;
    use crate::context::{ContextSnapshot, ExecutionContext, TaskOutput};
    use crate::error::EvalExecError;
    use crate::executor::{
        EvalReport, Executors, SkipReason, TaskExecutor, TaskRunOutcome, execute_plan,
    };
    use crate::store::TaskRegistry;
    use crate::tasks::{AgentTaskExecutor, AssertionTaskExecutor, TraceTaskExecutor};
    use async_trait::async_trait;
    use serde_json::{Value, json};
    use tokio::time::Instant;
    use uuid::Uuid;
    use wyrd_spec::vala::eval::{
        AssertionTask, ComparisonOperator, EvalCondition, EvalSpec, EvalTask, ExecutionPlan,
        JsonPath, RecordId, RunId, Stage, TaskId,
    };

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

    fn spec_of(tasks: Vec<EvalTask>) -> EvalSpec {
        let mut map = BTreeMap::new();
        for task in tasks {
            map.insert(task.id().clone(), task);
        }
        EvalSpec {
            dataset: None,
            tasks: map,
            workflow: None,
            sampling: None,
            pass_gate: None,
            context_capture: None,
        }
    }

    fn executors() -> Executors {
        executors_with_assertion(Arc::new(AssertionTaskExecutor::new()))
    }

    /// Build the executor bundle with `assertion` serving assertion tasks.
    fn executors_with_assertion(assertion: Arc<dyn TaskExecutor>) -> Executors {
        Executors {
            assertion,
            trace: Arc::new(TraceTaskExecutor::new(
                Arc::new(InMemoryTraceSource::new()),
                Duration::from_millis(250),
            )),
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

    /// Drive `spec` over `base` with the default executors.
    ///
    /// # Errors
    /// Returns whatever execution error the driver propagates.
    fn try_drive(spec: EvalSpec, base: Value) -> Result<EvalReport, crate::EvalExecError> {
        let plan = spec.execution_plan().expect("test spec has valid dag");
        let registry = registry_for(&spec, &plan);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime builds");
        rt.block_on(execute_plan(&plan, &context(base), &registry, &executors()))
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

    /// A false comparison is a normal Failed result whose message never echoes
    /// the observed value, so redacted or hashed capture cannot leak it.
    #[test]
    fn false_comparison_message_omits_observed_value() {
        let spec = spec_of(vec![assertion(
            "secret",
            Some("$.ssn"),
            ComparisonOperator::Equals,
            json!("expected"),
            &[],
            None,
        )]);
        let report = drive(spec, json!({ "ssn": "123-45-6789" }));
        let TaskRunOutcome::Ran(result) = &report.outcomes[0] else {
            panic!("a false comparison must run");
        };
        assert!(!result.passed);
        let message = result
            .message
            .as_deref()
            .expect("failed result has message");
        assert!(!message.contains("123-45-6789"), "{message}");
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
                TaskRunOutcome::Ran(result) => {
                    assert!(result.passed, "{:?} failed", result.task_id);
                }
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

    /// Missing required context is an execution error, not a failed result.
    #[test]
    fn missing_jsonpath_propagates_typed_error() {
        let spec = spec_of(vec![assertion(
            "broken",
            Some("$.does_not_exist"),
            ComparisonOperator::IsString,
            json!(null),
            &[],
            None,
        )]);
        let error = try_drive(spec, json!({ "score": 1.0 }))
            .expect_err("missing context must not produce a result");
        assert!(error.to_string().contains("extract path"), "{error}");
    }

    /// An operator type mismatch is an execution error, not a failed result.
    #[test]
    fn operator_type_mismatch_propagates_typed_error() {
        let spec = spec_of(vec![assertion(
            "mismatch",
            Some("$.response"),
            ComparisonOperator::GreaterThan,
            json!(0.5),
            &[],
            None,
        )]);
        let error = try_drive(spec, json!({ "response": "not a number" }))
            .expect_err("an operator type mismatch must not produce a result");
        assert!(
            error.to_string().to_lowercase().contains("operator"),
            "{error}"
        );
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

    /// Assertion executor that sleeps before delegating, so a test can tell
    /// concurrent same-stage fan-out from serial execution by wall clock.
    struct SleepingAssertionExecutor {
        /// Delay before each delegated assertion.
        sleep: Duration,
    }

    #[async_trait]
    impl TaskExecutor for SleepingAssertionExecutor {
        /// Sleep for `self.sleep`, then judge `task` with the real assertion
        /// executor.
        ///
        /// # Errors
        /// Returns whatever [`AssertionTaskExecutor`] returns for `task`.
        async fn execute(
            &self,
            task: &EvalTask,
            snapshot: &ContextSnapshot,
            stage: u32,
        ) -> Result<TaskOutput, EvalExecError> {
            tokio::time::sleep(self.sleep).await;
            AssertionTaskExecutor::new()
                .execute(task, snapshot, stage)
                .await
        }
    }

    /// Four same-stage tasks that each sleep ~100ms run concurrently under the
    /// per-task-type `JoinSet` fan-out: wall clock ~100ms, not ~400ms.
    ///
    /// # Panics
    /// Panics when the plan fails, a task does not run, or the stage runs
    /// serially.
    #[test]
    fn concurrent_same_stage_tasks_complete_in_parallel() {
        let spec = spec_of(
            ["t0", "t1", "t2", "t3"]
                .into_iter()
                .map(|id| {
                    assertion(
                        id,
                        None,
                        ComparisonOperator::IsNotNull,
                        json!(null),
                        &[],
                        None,
                    )
                })
                .collect(),
        );
        let sleeper = Arc::new(SleepingAssertionExecutor {
            sleep: Duration::from_millis(100),
        });
        let executors = executors_with_assertion(sleeper);
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
            "four 100ms tasks should complete in parallel, took {elapsed:?}"
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
}
