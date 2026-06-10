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
//! Predecessor reference (parity citation only):
//! `scouter/crates/scouter_evaluate/src/evaluate/evaluator.rs` -
//! `tokio::try_join!(assertions, judges, traces, agents)`. Wyrd keeps the
//! four-bucket fan-out shape and drops the `Arc<RwLock<…>>` stores by having
//! executors return outputs instead of writing into shared state.

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
    Ran(AssertionResult),
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

impl EvalReport {
    /// Iterate over the assertion results of tasks that actually ran.
    pub fn ran(&self) -> impl Iterator<Item = &AssertionResult> + '_ {
        self.outcomes.iter().filter_map(|outcome| match outcome {
            TaskRunOutcome::Ran(result) => Some(result),
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
}

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

async fn execute_stage(
    stage: &Stage,
    snapshot: Arc<ContextSnapshot>,
    registry: &TaskRegistry,
    executors: &Executors,
    ledger: &mut RunLedger,
) -> Result<Arc<ContextSnapshot>, EvalExecError> {
    // First pass: resolve skips synchronously and bucket the rest by kind.
    let mut buckets: BTreeMap<EvalTaskKind, Vec<EvalTask>> = BTreeMap::new();
    let mut staged: Vec<TaskRunOutcome> = Vec::with_capacity(stage.tasks.len());

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
            let outcome = TaskRunOutcome::Skipped {
                task_id: task_id.clone(),
                reason: SkipReason::DependencySkipped { upstream },
            };
            ledger.skipped.insert(task_id.clone());
            staged.push(outcome);
            continue;
        }

        if let Some(condition) = task.condition() {
            if !evaluate_condition(task_id, task.depends_on(), condition, &snapshot)? {
                let outcome = TaskRunOutcome::Skipped {
                    task_id: task_id.clone(),
                    reason: SkipReason::ConditionFalse,
                };
                ledger.skipped.insert(task_id.clone());
                staged.push(outcome);
                continue;
            }
        }

        buckets
            .entry(EvalTaskKind::from_task(task))
            .or_default()
            .push(task.clone());
    }

    // Second pass: fan the four buckets out concurrently.
    let assertion_tasks = buckets.remove(&EvalTaskKind::Assertion).unwrap_or_default();
    let judge_tasks = buckets.remove(&EvalTaskKind::LlmJudge).unwrap_or_default();
    let trace_tasks = buckets
        .remove(&EvalTaskKind::TraceAssertion)
        .unwrap_or_default();
    let agent_tasks = buckets
        .remove(&EvalTaskKind::AgentAssertion)
        .unwrap_or_default();

    let stage_index = stage.index;
    let (assertion_out, judge_out, trace_out, agent_out) = tokio::try_join!(
        fan_out_bucket(
            assertion_tasks,
            Arc::clone(&snapshot),
            Arc::clone(&executors.assertion),
            stage_index,
        ),
        fan_out_bucket(
            judge_tasks,
            Arc::clone(&snapshot),
            Arc::clone(&executors.judge),
            stage_index,
        ),
        fan_out_bucket(
            trace_tasks,
            Arc::clone(&snapshot),
            Arc::clone(&executors.trace),
            stage_index,
        ),
        fan_out_bucket(
            agent_tasks,
            Arc::clone(&snapshot),
            Arc::clone(&executors.agent),
            stage_index,
        ),
    )?;

    // Collect new outputs into a (task_id -> TaskOutput) map for ordered emit.
    let mut by_id: HashMap<TaskId, TaskOutput> = HashMap::with_capacity(stage.tasks.len());
    for (task_id, output) in assertion_out
        .into_iter()
        .chain(judge_out)
        .chain(trace_out)
        .chain(agent_out)
    {
        by_id.insert(task_id, output);
    }

    // Emit outcomes in stage-declaration order, interleaving skips and runs.
    let mut new_outputs: Vec<(TaskId, Arc<TaskOutput>)> = Vec::with_capacity(by_id.len());
    let mut ordered: Vec<TaskRunOutcome> = Vec::with_capacity(stage.tasks.len());
    let mut skips: HashMap<TaskId, TaskRunOutcome> = staged
        .into_iter()
        .map(|outcome| (outcome_task_id(&outcome).clone(), outcome))
        .collect();
    for task_id in &stage.tasks {
        if let Some(skip) = skips.remove(task_id) {
            ordered.push(skip);
            continue;
        }
        if let Some(output) = by_id.remove(task_id) {
            ordered.push(TaskRunOutcome::Ran(output.result().clone()));
            new_outputs.push((task_id.clone(), Arc::new(output)));
        }
    }

    ledger.outcomes.extend(ordered);

    Ok(Arc::new(snapshot.extend(new_outputs)))
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
