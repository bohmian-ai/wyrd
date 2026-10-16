//! Stage-walking driver for `ExecutionPlan`s.
//!
//! [`walk`] is the one stage lifecycle every `vala-eval` execution shares:
//!
//! 1. Walks the stages in topological order, settling every task into one
//!    [`RunLedger`].
//! 2. Per stage: the caller's `prepare` classifies each task synchronously
//!    (no IO) against the ledger of earlier stages, settling skips without
//!    running and handing back runnable futures.
//! 3. Fans the runnable tasks out concurrently on one `tokio::JoinSet`.
//! 4. At the stage barrier: appends the stage's settlements to the ledger in
//!    declaration order.
//!
//! [`execute_plan`] drives Eval task plans through it: each stage's
//! [`ContextSnapshot`] extends the record context with every earlier output
//! (a sequence of `Arc::clone` bumps, no JSON deep-clones), and each task runs
//! on its kind's executor. Verifier executions drive the same walk through
//! `crate::blocks::VerificationEngine`.
//!
//! An executor error aborts the run and propagates to the caller: an error is
//! an execution failure, never a subject failure, so it is never recorded as a
//! Failed `AssertionResult`. Only a completed comparison produces a result.
//! Executors return outputs rather than writing into shared state, so no locks
//! are needed across the stage barrier.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::task::{JoinError, JoinSet};
use tracing::Instrument as _;

use wyrd_spec::vala::eval::{
    AssertionResult, ConditionCombinator, EvalCondition, EvalTask, ExecutionPlan, Stage, TaskId,
};

use crate::context::{
    ContextSnapshot, ExecutionContext, TaskOutput, extract_required_jsonpath_from,
};
use crate::error::EvalExecError;
use crate::operators;
use crate::store::{EvalTaskKind, TaskRegistry};

/// Outcome of one task in the plan.
///
/// Engine-local; never on the wire. `EvalStatus` (in `wyrd-spec`) is the
/// *inbox lifecycle* of an eval record; `TaskRunOutcome` is the *per-task*
/// runtime fact a stage produces.
#[derive(Debug, Clone, PartialEq)]
pub enum TaskRunOutcome {
    /// The executor produced a result.
    ///
    /// Boxed because a result is an order of magnitude larger than a skip, and
    /// every stage collects both in one `Vec`: inline, each skipped task would
    /// carry a result-sized hole.
    Ran(Box<AssertionResult>),
    /// The task did not run.
    Skipped {
        /// Skipped task id.
        task_id: TaskId,
        /// Why the task skipped.
        reason: SkipReason,
    },
}

/// Why a task was skipped.
#[derive(Debug, Clone, PartialEq)]
pub enum SkipReason {
    /// The task's own condition gate evaluated to false.
    ConditionFalse,
    /// One declared upstream dependency was skipped first.
    DependencySkipped {
        /// The skipped upstream dependency.
        upstream: TaskId,
    },
}

/// Stage-loop report.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EvalReport {
    /// Outcomes in stage order; within a stage, in task-declaration order.
    pub outcomes: Vec<TaskRunOutcome>,
}

/// Rollup summary of an [`EvalReport`]'s workflow-level pass/fail counts.
///
/// Serializes with its field names so a verification result persists it
/// verbatim as the canonical Eval `details` JSON.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct EvalWorkflowSummary {
    /// Total tasks that ran (skipped tasks excluded).
    pub total_tasks: i32,
    /// Tasks that ran and passed.
    pub passed_tasks: i32,
    /// Tasks that ran and failed.
    pub failed_tasks: i32,
    /// `passed_tasks / total_tasks`; `0.0` when `total_tasks == 0`.
    pub pass_rate: f64,
    /// Sum of per-task `duration_ms` across all ran tasks.
    pub duration_ms: i64,
}

impl EvalReport {
    /// Iterate over the assertion results of tasks that actually ran.
    pub fn ran(&self) -> impl Iterator<Item = &AssertionResult> + '_ {
        self.outcomes.iter().filter_map(|outcome| match outcome {
            TaskRunOutcome::Ran(result) => Some(result.as_ref()),
            TaskRunOutcome::Skipped { .. } => None,
        })
    }

    /// Iterate over the skipped tasks and their reasons.
    pub fn skipped(&self) -> impl Iterator<Item = (&TaskId, &SkipReason)> + '_ {
        self.outcomes.iter().filter_map(|outcome| match outcome {
            TaskRunOutcome::Skipped { task_id, reason } => Some((task_id, reason)),
            TaskRunOutcome::Ran(_) => None,
        })
    }

    /// Compute the workflow-level pass/fail rollup across every task that ran.
    pub fn workflow_summary(&self) -> EvalWorkflowSummary {
        let mut total = 0i32;
        let mut passed = 0i32;
        let mut duration_ms = 0i64;
        for result in self.ran() {
            total += 1;
            if result.passed {
                passed += 1;
            }
            duration_ms += i64::try_from(result.duration_ms).unwrap_or(i64::MAX);
        }
        let pass_rate = if total > 0 {
            f64::from(passed) / f64::from(total)
        } else {
            0.0
        };
        EvalWorkflowSummary {
            total_tasks: total,
            passed_tasks: passed,
            failed_tasks: total - passed,
            pass_rate,
            duration_ms,
        }
    }
}

/// Per-run state shared by every stage-walking execution: each settled task
/// by id, in the order the stage walk settled it.
///
/// [`walk`] owns it and appends one stage at a time in declaration order, so
/// iteration order is stage order and, within a stage, declaration order.
/// Preparation of a later stage reads earlier settlements through
/// [`RunLedger::get`]; nothing mutates a settled entry.
#[derive(Debug, Clone)]
pub struct RunLedger<O> {
    /// Settled tasks in settlement order.
    entries: Vec<(TaskId, O)>,
    /// Position of each settled task in `entries`.
    index: HashMap<TaskId, usize>,
}

impl<O> Default for RunLedger<O> {
    /// An empty ledger: nothing has settled.
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            index: HashMap::new(),
        }
    }
}

impl<O> RunLedger<O> {
    /// The settlement of `task`, when an earlier stage settled it.
    #[must_use]
    pub fn get(&self, task: &TaskId) -> Option<&O> {
        self.index.get(task).map(|&at| &self.entries[at].1)
    }

    /// Every settlement, in settlement order.
    pub fn iter(&self) -> impl Iterator<Item = (&TaskId, &O)> + '_ {
        self.entries.iter().map(|(id, settled)| (id, settled))
    }

    /// Record `task`'s settlement after every earlier one.
    fn push(&mut self, task: TaskId, settled: O) {
        self.index.insert(task.clone(), self.entries.len());
        self.entries.push((task, settled));
    }
}

impl<O> IntoIterator for RunLedger<O> {
    /// One owned settlement: the task id and what it settled as.
    type Item = (TaskId, O);
    /// Consumes the ledger, yielding settlements in settlement order.
    type IntoIter = std::vec::IntoIter<(TaskId, O)>;

    /// Every settlement, in settlement order.
    fn into_iter(self) -> Self::IntoIter {
        self.entries.into_iter()
    }
}

/// A pending task future spawned by [`walk`].
pub(crate) type Running<O, E> = Pin<Box<dyn Future<Output = Result<O, E>> + Send>>;

/// How one planned task proceeds, decided before its stage fans out.
pub(crate) enum Prepared<O, E> {
    /// Settled without running, such as a skip.
    Settled(O),
    /// Runs concurrently with the rest of its stage.
    Run(Running<O, E>),
}

/// Walk `stages` in order, settling every task into one [`RunLedger`].
///
/// This is the single stage lifecycle of `vala-eval`: per stage, `prepare`
/// sees the ledger of every earlier stage and returns one [`Prepared`] per
/// stage task, in declaration order; settled tasks are recorded as is, and
/// runnable ones are spawned on one `JoinSet` inside the caller's current
/// span, so a task's work stays in the caller's trace. At the stage barrier
/// the settlements are appended in declaration order. Contract-specific
/// classification, budgets and results stay in `prepare`.
///
/// # Errors
/// Returns the first `prepare` error, the first task error in completion
/// order, or `joined` applied to a task that panicked or was aborted.
///
/// # Cancellation
/// Returning early or dropping the future drops the `JoinSet`, which aborts
/// every task of the stage still running; the stage yields no partial
/// settlements. Work an aborted task already performed is not rolled back.
pub(crate) async fn walk<'s, O, E>(
    stages: impl IntoIterator<Item = &'s Stage>,
    mut prepare: impl FnMut(&RunLedger<O>, &Stage) -> Result<Vec<Prepared<O, E>>, E>,
    joined: fn(JoinError) -> E,
) -> Result<RunLedger<O>, E>
where
    O: Send + 'static,
    E: Send + 'static,
{
    let mut ledger = RunLedger::default();
    for stage in stages {
        let mut settled = HashMap::new();
        let mut running = JoinSet::new();
        for (at, prepared) in prepare(&ledger, stage)?.into_iter().enumerate() {
            match prepared {
                Prepared::Settled(done) => {
                    settled.insert(at, done);
                }
                Prepared::Run(task) => {
                    running.spawn(
                        async move { (at, task.await) }.instrument(tracing::Span::current()),
                    );
                }
            }
        }
        while let Some(done) = running.join_next().await {
            let (at, done) = done.map_err(joined)?;
            settled.insert(at, done?);
        }
        for (at, task) in stage.tasks.iter().enumerate() {
            if let Some(done) = settled.remove(&at) {
                ledger.push(task.clone(), done);
            }
        }
    }
    Ok(ledger)
}

/// One Eval task's settlement: the executor output, or why it was skipped.
type EvalSettled = Result<Arc<TaskOutput>, SkipReason>;

/// Per-variant task execution.
#[async_trait]
pub trait TaskExecutor: Send + Sync {
    /// Execute one task against the supplied snapshot.
    ///
    /// # Errors
    /// Returns [`EvalExecError`] when the task cannot produce an output.
    /// The driver propagates every error and aborts the run; an error is never
    /// recorded as a Failed `AssertionResult`.
    async fn execute(
        &self,
        task: &EvalTask,
        snapshot: &ContextSnapshot,
        stage: u32,
    ) -> Result<TaskOutput, EvalExecError>;
}

/// Bundle of per-variant executors injected by the orchestrator.
///
/// Held as `Arc<dyn TaskExecutor>` so tests can swap in mock implementations
/// while the production constructors return the concrete per-kind executors.
pub struct Executors {
    /// Assertion executor.
    pub assertion: Arc<dyn TaskExecutor>,
    /// LLM judge executor.
    pub judge: Arc<dyn TaskExecutor>,
    /// Trace assertion executor.
    pub trace: Arc<dyn TaskExecutor>,
    /// Agent assertion executor.
    pub agent: Arc<dyn TaskExecutor>,
}

impl Executors {
    /// The executor that runs tasks of `kind`.
    fn of(&self, kind: EvalTaskKind) -> &Arc<dyn TaskExecutor> {
        match kind {
            EvalTaskKind::Assertion => &self.assertion,
            EvalTaskKind::LlmJudge => &self.judge,
            EvalTaskKind::TraceAssertion => &self.trace,
            EvalTaskKind::AgentAssertion => &self.agent,
        }
    }
}

/// Walk `plan.stages` in order, returning the per-run report.
/// This is the main entry point for the driver.
///
/// Each stage runs through [`walk`]: its snapshot extends the record context
/// with every earlier output, dependency and condition skips settle without
/// running, and every other task runs concurrently on its kind's executor.
///
/// # Errors
/// Returns [`EvalExecError::DagInvalid`] when a plan references a task absent
/// from the registry or a task panics, and any condition-evaluation or
/// task-executor error unchanged; no error is converted into a Failed result.
pub async fn execute_plan(
    plan: &ExecutionPlan,
    cx: &ExecutionContext,
    registry: &TaskRegistry,
    executors: &Executors,
) -> Result<EvalReport, EvalExecError> {
    let base = cx.snapshot();
    let ledger = walk(
        &plan.stages,
        |ledger: &RunLedger<EvalSettled>, stage| {
            let outputs = ledger.iter().filter_map(|(id, settled)| {
                settled
                    .as_ref()
                    .ok()
                    .map(|output| (id.clone(), Arc::clone(output)))
            });
            let snapshot = Arc::new(base.extend(outputs));
            stage
                .tasks
                .iter()
                .map(|id| prepare_task(id, stage, &snapshot, registry, executors, ledger))
                .collect()
        },
        |error| EvalExecError::DagInvalid {
            reason: format!("task join failure: {error}"),
        },
    )
    .await?;
    Ok(EvalReport {
        outcomes: ledger
            .into_iter()
            .map(|(task_id, settled)| match settled {
                Ok(output) => TaskRunOutcome::Ran(Box::new(output.result().clone())),
                Err(reason) => TaskRunOutcome::Skipped { task_id, reason },
            })
            .collect(),
    })
}

/// Classify one Eval task of `stage`: a skipped dependency or a false
/// condition settles it as skipped, otherwise it runs on its kind's executor
/// against the stage `snapshot`.
///
/// # Errors
/// Returns [`EvalExecError::DagInvalid`] when the registry lacks the task and
/// the condition-evaluation error of an unevaluable condition.
fn prepare_task(
    task_id: &TaskId,
    stage: &Stage,
    snapshot: &Arc<ContextSnapshot>,
    registry: &TaskRegistry,
    executors: &Executors,
    ledger: &RunLedger<EvalSettled>,
) -> Result<Prepared<EvalSettled, EvalExecError>, EvalExecError> {
    let task = registry
        .get(task_id)
        .ok_or_else(|| EvalExecError::DagInvalid {
            reason: format!(
                "plan stage {} references unknown task {:?}",
                stage.index,
                task_id.as_str()
            ),
        })?;
    if let Some(upstream) = first_skipped_dependency(task, ledger) {
        return Ok(Prepared::Settled(Err(SkipReason::DependencySkipped {
            upstream,
        })));
    }
    if let Some(condition) = task.condition()
        && !evaluate_condition(task_id, task.depends_on(), condition, snapshot)?
    {
        return Ok(Prepared::Settled(Err(SkipReason::ConditionFalse)));
    }
    let task = task.clone();
    let snapshot = Arc::clone(snapshot);
    let executor = Arc::clone(executors.of(EvalTaskKind::from_task(&task)));
    let stage = stage.index;
    Ok(Prepared::Run(Box::pin(async move {
        let output = executor.execute(&task, &snapshot, stage).await?;
        Ok(Ok(Arc::new(output)))
    })))
}

/// The first declared dependency of `task` that settled as skipped.
fn first_skipped_dependency(task: &EvalTask, ledger: &RunLedger<EvalSettled>) -> Option<TaskId> {
    task.depends_on()
        .iter()
        .find(|dep| matches!(ledger.get(dep), Some(Err(_))))
        .cloned()
}

fn evaluate_condition(
    task_id: &TaskId,
    deps: &[TaskId],
    condition: &EvalCondition,
    snapshot: &ContextSnapshot,
) -> Result<bool, EvalExecError> {
    let view = snapshot.build_scoped_view(deps);
    let mut current = Some(condition);
    let mut acc: Option<bool> = None;
    let mut pending: Option<ConditionCombinator> = None;
    while let Some(node) = current {
        let observed = extract_required_jsonpath_from(&view, &node.path, task_id).map_err(
            |error| match error {
                EvalExecError::ExtractPathMissing { .. } => EvalExecError::ConditionPathMissing {
                    task_id: task_id.clone(),
                    path: node.path.as_str().to_owned(),
                },
                other => other,
            },
        )?;
        let this = operators::evaluate_operator(&observed, &node.operator, &node.expected)
            .map_err(|error| EvalExecError::OperatorTypeMismatch {
                task_id: task_id.clone(),
                operator: format!("{:?}", node.operator),
                reason: error.to_string(),
            })?
            .passed;

        acc = match (acc, pending) {
            (None, _) => Some(this),
            (Some(prev), Some(ConditionCombinator::And) | None) => Some(prev && this),
            (Some(prev), Some(ConditionCombinator::Or)) => Some(prev || this),
            // Trailing link without a combinator: treat as conjunction. Chain
            // validation in `wyrd_spec` already rejects non-terminal links
            // missing a combinator.
        };
        pending = node.combinator;

        match (acc, pending) {
            (Some(false), Some(ConditionCombinator::And)) => return Ok(false),
            (Some(true), Some(ConditionCombinator::Or)) => return Ok(true),
            _ => {}
        }

        current = node.subsequent.as_deref();
    }
    Ok(acc.unwrap_or(true))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::error::Error;

    use serde_json::{Value, json};
    use uuid::Uuid;
    use wyrd_spec::vala::eval::{
        AssertionTask, ComparisonOperator, EvalTask, JsonPath, RecordId, RunId,
    };

    use super::*;

    fn tid(value: &str) -> Result<TaskId, Box<dyn Error>> {
        Ok(TaskId::new(value)?)
    }

    fn jp(value: &str) -> Result<JsonPath, Box<dyn Error>> {
        Ok(JsonPath::new(value)?)
    }

    fn equals_condition(path: &str, expected: Value) -> Result<EvalCondition, Box<dyn Error>> {
        Ok(EvalCondition {
            path: jp(path)?,
            operator: ComparisonOperator::Equals,
            expected,
            combinator: None,
            subsequent: None,
        })
    }

    fn assertion(id: &str, deps: &[&str]) -> Result<EvalTask, Box<dyn Error>> {
        let depends_on = deps
            .iter()
            .map(|dep| tid(dep))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(EvalTask::Assertion(AssertionTask {
            id: tid(id)?,
            context_path: None,
            item_context_path: None,
            operator: ComparisonOperator::Equals,
            expected: json!(true),
            depends_on,
            condition: None,
        }))
    }

    fn snapshot(value: Value) -> Arc<ContextSnapshot> {
        let cx = ExecutionContext::new(
            value,
            RunId::from_string("run-unit".to_owned()),
            RecordId(Uuid::nil()),
            None,
        );
        Arc::clone(cx.snapshot())
    }

    #[test]
    fn evaluate_condition_handles_and_or_chains() -> Result<(), Box<dyn Error>> {
        let chain = EvalCondition {
            path: jp("$.score")?,
            operator: ComparisonOperator::GreaterThan,
            expected: json!(0.5),
            combinator: Some(ConditionCombinator::And),
            subsequent: Some(Box::new(EvalCondition {
                path: jp("$.ok")?,
                operator: ComparisonOperator::Equals,
                expected: json!(true),
                combinator: Some(ConditionCombinator::Or),
                subsequent: Some(Box::new(equals_condition("$.fallback", json!(true))?)),
            })),
        };

        let snap = snapshot(json!({"score": 0.7, "ok": false, "fallback": true}));
        assert!(evaluate_condition(&tid("task")?, &[], &chain, &snap)?);
        Ok(())
    }

    /// Skip propagation names the first skipped dependency in the task's
    /// declared order, not in ledger settlement order: `b` settles before `a`,
    /// yet `c` (declaring `a` then `b`) reports `a`.
    ///
    /// # Errors
    /// Returns the task-id, DAG-validation, or registry-construction error of
    /// the static fixture.
    ///
    /// # Panics
    /// Panics when task `c` is not registered or the reported upstream is not
    /// `a`.
    #[test]
    fn first_skipped_dependency_returns_declaration_order() -> Result<(), Box<dyn Error>> {
        let tasks = vec![
            assertion("a", &[])?,
            assertion("b", &[])?,
            assertion("c", &["a", "b"])?,
        ];
        let map: BTreeMap<_, _> = tasks
            .into_iter()
            .map(|task| (task.id().clone(), task))
            .collect();
        let plan = wyrd_spec::vala::eval::validate_dag(&map)?;
        let registry = TaskRegistry::from_plan(&plan, map.clone())?;
        let mut ledger = RunLedger::default();
        ledger.push(tid("b")?, Err(SkipReason::ConditionFalse));
        ledger.push(tid("a")?, Err(SkipReason::ConditionFalse));

        let task_c = tid("c")?;
        let task = registry.get(&task_c).expect("task c registered");
        assert_eq!(first_skipped_dependency(task, &ledger), Some(tid("a")?));
        Ok(())
    }

    /// Executor that fails every task with a transient trace error.
    struct UnavailableExecutor;

    #[async_trait]
    impl TaskExecutor for UnavailableExecutor {
        /// Fail with [`EvalExecError::TraceUnavailableForTask`].
        ///
        /// # Errors
        /// Always returns the trace-unavailable error.
        async fn execute(
            &self,
            task: &EvalTask,
            _snapshot: &ContextSnapshot,
            _stage: u32,
        ) -> Result<TaskOutput, EvalExecError> {
            Err(EvalExecError::TraceUnavailableForTask {
                task_id: task.id().clone(),
                source: crate::TraceUnavailable::Failed {
                    reason: "source down".to_owned(),
                },
            })
        }
    }

    /// Run a single `Equals true` assertion over `record` with `assertion_executor`.
    ///
    /// # Errors
    /// Returns plan validation errors and whatever the driver propagates.
    async fn run_single(
        record: Value,
        assertion_executor: Arc<dyn TaskExecutor>,
    ) -> Result<EvalReport, Box<dyn Error>> {
        let map: BTreeMap<_, _> = [assertion("a", &[])?]
            .into_iter()
            .map(|task| (task.id().clone(), task))
            .collect();
        let plan = wyrd_spec::vala::eval::validate_dag(&map)?;
        let registry = TaskRegistry::from_plan(&plan, map)?;
        let unused: Arc<dyn TaskExecutor> = Arc::new(UnavailableExecutor);
        let executors = Executors {
            assertion: assertion_executor,
            judge: Arc::clone(&unused),
            trace: Arc::clone(&unused),
            agent: unused,
        };
        let cx = ExecutionContext::new(
            record,
            RunId::from_string("run-unit".to_owned()),
            RecordId(Uuid::nil()),
            None,
        );
        Ok(execute_plan(&plan, &cx, &registry, &executors).await?)
    }

    /// An executor error aborts the run instead of becoming a Failed result,
    /// while a completed false comparison stays a Failed result.
    #[tokio::test]
    async fn executor_error_propagates_and_false_comparison_fails() -> Result<(), Box<dyn Error>> {
        let error = run_single(json!(true), Arc::new(UnavailableExecutor))
            .await
            .expect_err("an executor error must not produce a report");
        assert!(error.to_string().contains("trace unavailable"), "{error}");

        let assertion = Arc::new(crate::tasks::AssertionTaskExecutor::new());
        let report = run_single(json!(false), assertion).await?;
        let ran: Vec<_> = report.ran().collect();
        assert_eq!(ran.len(), 1);
        assert!(!ran[0].passed, "a false comparison is a subject failure");
        Ok(())
    }
}

#[cfg(test)]
mod end_to_end {
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

    use crate::context::ExecutionContext;
    use crate::executor::{Executors, TaskRunOutcome, execute_plan};
    use crate::store::TaskRegistry;
    use crate::tasks::{
        AgentTaskExecutor, AssertionTaskExecutor, JudgeTaskExecutor, TraceTaskExecutor,
    };
    use crate::{
        AggregationInput, EvalResults, InMemoryTraceSource, MechanicSubjectInput, MockJudgeInvoker,
        ResultsConfig, RunIdentity, ScenarioAggregationInput, SkipReason, SubjectKey,
        aggregate_run,
    };
    use wyrd_semver::VersionBlock;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::CardRef;
    use wyrd_spec::vala::eval::{
        AgentAssertionTask, AssertionResult, AssertionTask, ComparisonOperator, EvalCondition,
        EvalPassGate, EvalSpec, EvalTask, JsonPath, LlmJudgeTask, RecordId, RunId, ScenarioId,
        TaskId, TraceAssertionTask,
    };
    use wyrd_spec::vala::ids::{SpanId, TraceId};
    use wyrd_spec::vala::trace::{
        InstrumentationScope, Resource, SpanKind, SpanRecord, SpanStatus,
    };

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
        card_ref(CardKind::Verifier, "end-to-end-rubric")
    }

    fn subject_ref() -> CardRef {
        card_ref(CardKind::Agent, "agent-under-test")
    }

    fn judge_ref() -> CardRef {
        card_ref(CardKind::Agent, "judge")
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
            judge_ref: judge_ref().into(),
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
            dataset: None,
            tasks,
            workflow: None,
            sampling: None,
            pass_gate: Some(EvalPassGate::OverallPassRate { threshold: 0.75 }),
            context_capture: None,
        }
    }

    fn executors_with_trace_source(source: Arc<dyn crate::TraceSource>) -> Executors {
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
        report: &crate::EvalReport,
    ) -> (BTreeMap<TaskId, AssertionResult>, Vec<(TaskId, SkipReason)>) {
        let mut ran = BTreeMap::new();
        let mut skipped = Vec::new();
        for outcome in &report.outcomes {
            match outcome {
                TaskRunOutcome::Ran(result) => {
                    ran.insert(result.task_id.clone(), result.as_ref().clone());
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
        let registry =
            TaskRegistry::from_plan(&plan, spec.tasks.clone()).expect("registry from plan");

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
}
