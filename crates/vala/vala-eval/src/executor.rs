//! Stage-walking driver for `ExecutionPlan`.
//!
//! The driver:
//!
//! 1. Walks `ExecutionPlan::stages` in topological order, threading an
//!    [`Arc<ContextSnapshot>`] through each stage.
//! 2. Per stage: resolves dependency-skip and condition-gate skips
//!    sequentially (no IO), then buckets the runnable tasks by
//!    [`crate::store::EvalTaskKind`] and fans the four buckets out
//!    concurrently via `tokio::JoinSet` coordinated by `tokio::try_join!`.
//! 3. At each stage barrier: rebuilds the next snapshot by extending the
//!    prior one with the new [`crate::context::TaskOutput`]s. The rebuild is
//!    a sequence of `Arc::clone` bumps; no JSON deep-clones happen here.
//! 4. Maintains a single-threaded [`RunLedger`] for per-run state (skip set
//!    plus outcome stream). [`crate::store::TaskRegistry`] stays immutable.
//!
//! Per-task errors are folded into a Failed `AssertionResult` so the run
//! keeps walking the DAG. Plan-level invariants (`EvalExecError::DagInvalid`)
//! bubble out and abort the run.
//!
//! The four-bucket fan-out shape keeps task kinds concurrent within a stage.
//! Executors return outputs rather than writing into shared state, so no locks
//! are needed across the stage barrier.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;
use tokio::task::JoinSet;

use wyrd_spec::vala::eval::{
    AssertionResult, ComparisonOperator, ConditionCombinator, EvalCondition, EvalTask,
    ExecutionPlan, Stage, TaskId,
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

/// Output of the task-classification pass: runnable buckets by kind and skip outcomes in declaration order.
type ClassifiedStage = (BTreeMap<EvalTaskKind, Vec<EvalTask>>, Vec<TaskRunOutcome>);

/// Per-run state owned by the driver. Single-threaded — no locks.
///
/// `skipped` is the cumulative skip set used for `DependencySkipped`
/// propagation across stages. `outcomes` is the stream consumed by the
/// final [`EvalReport`].
#[derive(Debug, Default, Clone)]
pub struct RunLedger {
    /// Task ids that were skipped during this run, in any order.
    pub skipped: BTreeSet<TaskId>,
    /// Outcomes in the order the stage loop produced them.
    pub outcomes: Vec<TaskRunOutcome>,
}

/// Per-variant task execution.
#[async_trait]
pub trait TaskExecutor: Send + Sync {
    /// Execute one task against the supplied snapshot.
    ///
    /// # Errors
    /// Returns [`EvalExecError`] when the task cannot produce an output.
    /// Per-task errors are folded into a Failed `AssertionResult` by the
    /// driver; only plan-level invariants abort the run.
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

/// Walk `plan.stages` in order, returning the per-run report.
/// This is the main entry point for the driver.
///
/// # Errors
/// Returns [`EvalExecError::DagInvalid`] when a plan references a task absent
/// from the registry, or any condition-evaluation error that the driver
/// cannot fold into a Failed result.
pub async fn execute_plan(
    plan: &ExecutionPlan,
    cx: &ExecutionContext,
    registry: &TaskRegistry,
    executors: &Executors,
) -> Result<EvalReport, EvalExecError> {
    let mut snapshot = Arc::clone(cx.snapshot());
    let mut ledger = RunLedger::default();
    for stage in &plan.stages {
        snapshot = execute_stage(stage, snapshot, registry, executors, &mut ledger).await?;
    }
    Ok(EvalReport {
        outcomes: ledger.outcomes,
    })
}

/// Execute one stage: classify tasks, fan out concurrently, and extend the snapshot.
///
/// Returns the new snapshot to thread into the next stage.
async fn execute_stage(
    stage: &Stage,
    snapshot: Arc<ContextSnapshot>,
    registry: &TaskRegistry,
    executors: &Executors,
    ledger: &mut RunLedger,
) -> Result<Arc<ContextSnapshot>, EvalExecError> {
    // prepare the tasks: resolve skips and bucket the runnable tasks by kind
    let (buckets, skips) = classify_stage_tasks(stage, &snapshot, registry, ledger)?;

    // run the buckets concurrently and collect their outputs flat; no ordering guarantees within the stage yet
    let outputs =
        fan_out_all_buckets(buckets, Arc::clone(&snapshot), executors, stage.index).await?;
    let by_id: HashMap<TaskId, TaskOutput> = outputs.into_iter().collect();

    // merge the skips and outputs back into declaration order, extending the ledger and preparing the snapshot extension
    let (ordered, new_outputs) = order_stage_outcomes(stage, skips, by_id);
    ledger.outcomes.extend(ordered);
    Ok(Arc::new(snapshot.extend(new_outputs)))
}

/// First pass: resolve skips synchronously and bucket the runnable tasks by kind.
///
/// Dependency-skip propagation and condition-gate evaluation are both pure
/// (no IO). `ledger.skipped` is extended in place so later stages — and later
/// tasks in this same stage — can propagate skips transitively.
///
/// Returns the runnable-task buckets and the skip outcomes in declaration order.
fn classify_stage_tasks(
    stage: &Stage,
    snapshot: &ContextSnapshot,
    registry: &TaskRegistry,
    ledger: &mut RunLedger,
) -> Result<ClassifiedStage, EvalExecError> {
    let mut buckets: BTreeMap<EvalTaskKind, Vec<EvalTask>> = BTreeMap::new();
    let mut skips: Vec<TaskRunOutcome> = Vec::new();

    for task_id in &stage.tasks {
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
            ledger.skipped.insert(task_id.clone());
            skips.push(TaskRunOutcome::Skipped {
                task_id: task_id.clone(),
                reason: SkipReason::DependencySkipped { upstream },
            });
            continue;
        }

        if let Some(condition) = task.condition()
            && !evaluate_condition(task_id, task.depends_on(), condition, snapshot)?
        {
            ledger.skipped.insert(task_id.clone());
            skips.push(TaskRunOutcome::Skipped {
                task_id: task_id.clone(),
                reason: SkipReason::ConditionFalse,
            });
            continue;
        }

        buckets
            .entry(EvalTaskKind::from_task(task))
            .or_default()
            .push(task.clone());
    }

    Ok((buckets, skips))
}

/// Second pass: run all four task-kind buckets concurrently and return their outputs flat.
///
/// `tokio::try_join!` keeps the four kinds parallel. Within each kind,
/// [`fan_out_bucket`] spawns one Tokio task per eval task via `JoinSet`.
/// The flat output vec arrives in assertion → judge → trace → agent order;
/// declaration order is restored by [`order_stage_outcomes`].
async fn fan_out_all_buckets(
    mut buckets: BTreeMap<EvalTaskKind, Vec<EvalTask>>,
    snapshot: Arc<ContextSnapshot>,
    executors: &Executors,
    stage_index: u32,
) -> Result<Vec<(TaskId, TaskOutput)>, EvalExecError> {
    let assertion_tasks = buckets.remove(&EvalTaskKind::Assertion).unwrap_or_default();
    let judge_tasks = buckets.remove(&EvalTaskKind::LlmJudge).unwrap_or_default();
    let trace_tasks = buckets
        .remove(&EvalTaskKind::TraceAssertion)
        .unwrap_or_default();
    let agent_tasks = buckets
        .remove(&EvalTaskKind::AgentAssertion)
        .unwrap_or_default();

    let (assertion_out, judge_out, trace_out, agent_out) = tokio::try_join!(
        fan_out_bucket(
            assertion_tasks,
            Arc::clone(&snapshot),
            Arc::clone(&executors.assertion),
            stage_index
        ),
        fan_out_bucket(
            judge_tasks,
            Arc::clone(&snapshot),
            Arc::clone(&executors.judge),
            stage_index
        ),
        fan_out_bucket(
            trace_tasks,
            Arc::clone(&snapshot),
            Arc::clone(&executors.trace),
            stage_index
        ),
        fan_out_bucket(
            agent_tasks,
            Arc::clone(&snapshot),
            Arc::clone(&executors.agent),
            stage_index
        ),
    )?;

    Ok(assertion_out
        .into_iter()
        .chain(judge_out)
        .chain(trace_out)
        .chain(agent_out)
        .collect())
}

/// Merge skip outcomes and task outputs into stage-declaration order.
///
/// Skips arrived in declaration order from [`classify_stage_tasks`]; they are
/// re-indexed by task id so the single pass over `stage.tasks` can interleave
/// them with ran outputs without a second linear scan.
///
/// Returns the ordered outcomes (for the ledger) and the new outputs (for
/// snapshot extension).
fn order_stage_outcomes(
    stage: &Stage,
    skips: Vec<TaskRunOutcome>,
    mut by_id: HashMap<TaskId, TaskOutput>,
) -> (Vec<TaskRunOutcome>, Vec<(TaskId, Arc<TaskOutput>)>) {
    let mut skips_by_id: HashMap<TaskId, TaskRunOutcome> = skips
        .into_iter()
        .map(|outcome| (outcome_task_id(&outcome).clone(), outcome))
        .collect();

    let mut ordered: Vec<TaskRunOutcome> = Vec::with_capacity(stage.tasks.len());
    let mut new_outputs: Vec<(TaskId, Arc<TaskOutput>)> = Vec::new();

    for task_id in &stage.tasks {
        if let Some(skip) = skips_by_id.remove(task_id) {
            ordered.push(skip);
        } else if let Some(output) = by_id.remove(task_id) {
            ordered.push(TaskRunOutcome::Ran(Box::new(output.result().clone())));
            new_outputs.push((task_id.clone(), Arc::new(output)));
        }
    }

    (ordered, new_outputs)
}

async fn fan_out_bucket(
    tasks: Vec<EvalTask>,
    snapshot: Arc<ContextSnapshot>,
    executor: Arc<dyn TaskExecutor>,
    stage: u32,
) -> Result<Vec<(TaskId, TaskOutput)>, EvalExecError> {
    if tasks.is_empty() {
        return Ok(Vec::new());
    }

    let mut set: JoinSet<Result<(TaskId, TaskOutput), EvalExecError>> = JoinSet::new();
    for task in tasks {
        let snapshot = Arc::clone(&snapshot);
        let executor = Arc::clone(&executor);
        set.spawn(async move {
            let task_id = task.id().clone();
            let outcome = match executor.execute(&task, &snapshot, stage).await {
                Ok(output) => output,
                Err(error) => TaskOutput::Assertion(synthesize_failed_result(&task, stage, &error)),
            };
            Ok((task_id, outcome))
        });
    }

    let mut collected = Vec::new();
    while let Some(joined) = set.join_next().await {
        match joined {
            Ok(Ok(pair)) => collected.push(pair),
            Ok(Err(error)) => return Err(error),
            Err(join_err) => {
                return Err(EvalExecError::DagInvalid {
                    reason: format!("task join failure: {join_err}"),
                });
            }
        }
    }
    Ok(collected)
}

fn first_skipped_dependency(task: &EvalTask, ledger: &RunLedger) -> Option<TaskId> {
    for dep in task.depends_on() {
        if ledger.skipped.contains(dep) {
            return Some(dep.clone());
        }
    }
    None
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
            (Some(prev), Some(ConditionCombinator::And)) => Some(prev && this),
            (Some(prev), Some(ConditionCombinator::Or)) => Some(prev || this),
            // Trailing link without a combinator: treat as conjunction. Chain
            // validation in `wyrd_spec` already rejects non-terminal links
            // missing a combinator.
            (Some(prev), None) => Some(prev && this),
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

fn synthesize_failed_result(task: &EvalTask, stage: u32, error: &EvalExecError) -> AssertionResult {
    let started_at = Utc::now();
    AssertionResult {
        task_id: task.id().clone(),
        passed: false,
        actual: None,
        expected: expected_of(task),
        operator: operator_of(task),
        message: Some(error.to_string()),
        stage,
        started_at,
        duration_ms: 0,
    }
}

fn expected_of(task: &EvalTask) -> Value {
    match task {
        EvalTask::Assertion(task) => task.expected.clone(),
        EvalTask::LlmJudge(task) => task.expected.clone(),
        EvalTask::TraceAssertion(task) => task.expected.clone(),
        EvalTask::AgentAssertion(task) => task.expected.clone(),
    }
}

fn operator_of(task: &EvalTask) -> ComparisonOperator {
    match task {
        EvalTask::Assertion(task) => task.operator.clone(),
        EvalTask::LlmJudge(task) => task.operator.clone(),
        EvalTask::TraceAssertion(task) => task.operator.clone(),
        EvalTask::AgentAssertion(task) => task.operator.clone(),
    }
}

fn outcome_task_id(outcome: &TaskRunOutcome) -> &TaskId {
    match outcome {
        TaskRunOutcome::Ran(result) => &result.task_id,
        TaskRunOutcome::Skipped { task_id, .. } => task_id,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::error::Error;

    use serde_json::json;
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
        ledger.skipped.insert(tid("b")?);
        ledger.skipped.insert(tid("a")?);

        let task_c = tid("c")?;
        let task = registry.get(&task_c).expect("task c registered");
        assert_eq!(first_skipped_dependency(task, &ledger), Some(tid("a")?));
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
