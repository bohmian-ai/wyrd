//! Per-scenario accumulating stores for task results and judge outputs.
//!
//! **Cross-scenario isolation rule.** Stores are owned by a single
//! [`crate::context::ExecutionContext`] for one scenario's execution. Keys are
//! `(TaskId, RecordId)` with no scenario discriminator, which is correct only
//! because callers instantiate fresh stores per scenario.

use std::collections::{BTreeSet, HashMap};

use serde_json::Value;
use wyrd_spec::vala::eval::ids::{RecordId, TaskId};
use wyrd_spec::vala::eval::plan::ExecutionPlan;
use wyrd_spec::vala::eval::result::AssertionResult;
use wyrd_spec::vala::eval::task::EvalTask;

use crate::error::EvalExecError;

/// Stable discriminator for an [`EvalTask`] variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EvalTaskKind {
    /// `EvalTask::Assertion`.
    Assertion,
    /// `EvalTask::LlmJudge`.
    LlmJudge,
    /// `EvalTask::TraceAssertion`.
    TraceAssertion,
    /// `EvalTask::AgentAssertion`.
    AgentAssertion,
}

impl EvalTaskKind {
    /// Derive the kind from a spec task.
    #[must_use]
    pub fn from_task(task: &EvalTask) -> Self {
        match task {
            EvalTask::Assertion(_) => Self::Assertion,
            EvalTask::LlmJudge(_) => Self::LlmJudge,
            EvalTask::TraceAssertion(_) => Self::TraceAssertion,
            EvalTask::AgentAssertion(_) => Self::AgentAssertion,
        }
    }
}

/// Per-`ExecutionPlan` registry: tasks indexed by id and by kind.
#[derive(Debug, Clone)]
pub struct TaskRegistry {
    tasks: HashMap<TaskId, EvalTask>,
    index_by_kind: HashMap<EvalTaskKind, Vec<TaskId>>,
}

impl TaskRegistry {
    /// Build a registry from a fully validated `ExecutionPlan` plus the raw
    /// task map the spec declared.
    ///
    /// # Errors
    /// Returns [`EvalExecError::DuplicateTaskId`] if the same id appears
    /// twice, or [`EvalExecError::UnknownDependency`] if a task references an
    /// absent dependency.
    pub fn from_plan<I>(plan: &ExecutionPlan, tasks: I) -> Result<Self, EvalExecError>
    where
        I: IntoIterator<Item = (TaskId, EvalTask)>,
    {
        let mut by_id = HashMap::new();
        let mut index_by_kind: HashMap<EvalTaskKind, Vec<TaskId>> = HashMap::new();

        for (id, task) in tasks {
            if by_id.contains_key(&id) {
                return Err(EvalExecError::DuplicateTaskId { task: id });
            }
            let kind = EvalTaskKind::from_task(&task);
            index_by_kind.entry(kind).or_default().push(id.clone());
            by_id.insert(id, task);
        }

        for (id, task) in &by_id {
            for dep in task.depends_on() {
                if !by_id.contains_key(dep) {
                    return Err(EvalExecError::UnknownDependency {
                        task: id.clone(),
                        dep: dep.clone(),
                    });
                }
            }
        }

        let plan_set: BTreeSet<TaskId> = plan
            .stages
            .iter()
            .flat_map(|stage| stage.tasks.iter().cloned())
            .collect();

        if let Some(missing) = plan_set.iter().find(|id| !by_id.contains_key(*id)) {
            return Err(EvalExecError::UnknownDependency {
                task: missing.clone(),
                dep: missing.clone(),
            });
        }

        if let Some(extra) = by_id.keys().find(|id| !plan_set.contains(*id)) {
            return Err(EvalExecError::UnknownDependency {
                task: extra.clone(),
                dep: extra.clone(),
            });
        }

        for ids in index_by_kind.values_mut() {
            ids.sort();
        }

        Ok(Self {
            tasks: by_id,
            index_by_kind,
        })
    }

    /// Borrow the task for `id`, if registered.
    #[must_use]
    pub fn get(&self, id: &TaskId) -> Option<&EvalTask> {
        self.tasks.get(id)
    }

    /// True iff a task with this id is registered.
    #[must_use]
    pub fn contains(&self, id: &TaskId) -> bool {
        self.tasks.contains_key(id)
    }

    /// Kind discriminator for a registered task.
    #[must_use]
    pub fn kind_of(&self, id: &TaskId) -> Option<EvalTaskKind> {
        self.tasks.get(id).map(EvalTaskKind::from_task)
    }

    /// Task ids of every task with the given kind, sorted.
    #[must_use]
    pub fn ids_of_kind(&self, kind: EvalTaskKind) -> &[TaskId] {
        self.index_by_kind
            .get(&kind)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Number of registered tasks.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    /// True iff no tasks are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }
}

/// Accumulating store for [`AssertionResult`] rows keyed by
/// `(TaskId, RecordId)`.
#[derive(Debug, Default, Clone)]
pub struct AssertionResultStore {
    results: HashMap<(TaskId, RecordId), AssertionResult>,
}

impl AssertionResultStore {
    /// Fresh empty store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            results: HashMap::new(),
        }
    }

    /// Record one assertion result.
    pub fn record(&mut self, task: TaskId, record: RecordId, result: AssertionResult) {
        self.results.insert((task, record), result);
    }

    /// Borrow a previously recorded result.
    #[must_use]
    pub fn get(&self, task: &TaskId, record: &RecordId) -> Option<&AssertionResult> {
        self.results.get(&(task.clone(), record.clone()))
    }

    /// Drain every result associated with `record`, in `TaskId` order.
    pub fn take_for_record(&mut self, record: &RecordId) -> Vec<AssertionResult> {
        let mut keys: Vec<(TaskId, RecordId)> = self
            .results
            .keys()
            .filter(|(_, recorded)| recorded == record)
            .cloned()
            .collect();
        keys.sort_by(|(left, _), (right, _)| left.cmp(right));

        let mut drained = Vec::with_capacity(keys.len());
        for key in keys {
            if let Some(value) = self.results.remove(&key) {
                drained.push(value);
            }
        }
        drained
    }

    /// Iterate every `(task, record) -> result` triple currently stored.
    pub fn iter(&self) -> impl Iterator<Item = (&TaskId, &RecordId, &AssertionResult)> {
        self.results
            .iter()
            .map(|((task, record), value)| (task, record, value))
    }

    /// Number of recorded results.
    #[must_use]
    pub fn len(&self) -> usize {
        self.results.len()
    }

    /// True iff no results are recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.results.is_empty()
    }
}

/// One judge invocation's raw and parsed payload.
#[derive(Debug, Clone, PartialEq)]
pub struct JudgeOutcome {
    /// Raw provider response payload as the judge invoker returned it.
    pub raw: Value,
    /// Structured projection the judge executor extracted from `raw`.
    pub parsed: Value,
}

/// Accumulating store for judge outputs keyed by `(TaskId, RecordId)`.
#[derive(Debug, Default, Clone)]
pub struct LlmResponseStore {
    responses: HashMap<(TaskId, RecordId), JudgeOutcome>,
}

impl LlmResponseStore {
    /// Fresh empty store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            responses: HashMap::new(),
        }
    }

    /// Record one judge outcome.
    pub fn record(&mut self, task: TaskId, record: RecordId, outcome: JudgeOutcome) {
        self.responses.insert((task, record), outcome);
    }

    /// Borrow a stored outcome.
    #[must_use]
    pub fn get(&self, task: &TaskId, record: &RecordId) -> Option<&JudgeOutcome> {
        self.responses.get(&(task.clone(), record.clone()))
    }

    /// Remove and return a stored outcome.
    pub fn take(&mut self, task: &TaskId, record: &RecordId) -> Option<JudgeOutcome> {
        self.responses.remove(&(task.clone(), record.clone()))
    }

    /// Number of recorded outcomes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.responses.len()
    }

    /// True iff no outcomes are recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.responses.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use serde_json::json;
    use std::collections::BTreeMap;
    use uuid::Uuid;
    use wyrd_spec::vala::eval::assertion::AssertionTask;
    use wyrd_spec::vala::eval::operator::ComparisonOperator;
    use wyrd_spec::vala::eval::plan::validate_dag;

    fn task_id(id: &str) -> TaskId {
        TaskId::new(id).expect("static test ids are valid")
    }

    fn assertion(id: &str, depends_on: &[&str]) -> EvalTask {
        EvalTask::Assertion(AssertionTask {
            id: task_id(id),
            context_path: None,
            item_context_path: None,
            operator: ComparisonOperator::Equals,
            expected: json!(true),
            depends_on: depends_on.iter().map(|dep| task_id(dep)).collect(),
            condition: None,
        })
    }

    fn plan_with(tasks: Vec<EvalTask>) -> (ExecutionPlan, Vec<(TaskId, EvalTask)>) {
        let map: BTreeMap<TaskId, EvalTask> = tasks
            .into_iter()
            .map(|task| (task.id().clone(), task))
            .collect();
        let plan = validate_dag(&map).expect("test fixture is a valid DAG");
        let pairs = map.into_iter().collect();
        (plan, pairs)
    }

    fn result_for(id: &str, stage: u32) -> AssertionResult {
        AssertionResult {
            task_id: task_id(id),
            passed: true,
            actual: Some(json!(1)),
            expected: json!(1),
            operator: ComparisonOperator::Equals,
            message: None,
            stage,
            started_at: chrono::Utc
                .with_ymd_and_hms(2026, 6, 10, 0, 0, 0)
                .single()
                .expect("static timestamp is valid"),
            duration_ms: 1,
        }
    }

    #[test]
    fn registry_indexes_by_id_and_kind() {
        let (plan, pairs) = plan_with(vec![assertion("alpha", &[]), assertion("beta", &["alpha"])]);
        let registry = TaskRegistry::from_plan(&plan, pairs).expect("valid plan registers");

        assert_eq!(registry.len(), 2);
        assert!(registry.contains(&task_id("alpha")));
        assert!(registry.contains(&task_id("beta")));
        assert_eq!(
            registry.kind_of(&task_id("alpha")),
            Some(EvalTaskKind::Assertion)
        );
        assert_eq!(
            registry.ids_of_kind(EvalTaskKind::Assertion),
            &[task_id("alpha"), task_id("beta")]
        );
        assert!(registry.ids_of_kind(EvalTaskKind::LlmJudge).is_empty());
    }

    #[test]
    fn registry_rejects_duplicate_ids() {
        let (plan, _) = plan_with(vec![assertion("alpha", &[])]);
        let pairs = vec![
            (task_id("alpha"), assertion("alpha", &[])),
            (task_id("alpha"), assertion("alpha", &[])),
        ];
        let error = TaskRegistry::from_plan(&plan, pairs).expect_err("duplicate id rejected");
        assert!(matches!(
            error,
            EvalExecError::DuplicateTaskId { ref task } if task == &task_id("alpha")
        ));
    }

    #[test]
    fn registry_rejects_unknown_dependency() {
        let (plan, _) = plan_with(vec![assertion("alpha", &[])]);
        let pairs = vec![(task_id("alpha"), assertion("alpha", &["ghost"]))];
        let error = TaskRegistry::from_plan(&plan, pairs).expect_err("unknown dependency rejected");
        assert!(matches!(
            error,
            EvalExecError::UnknownDependency { ref task, ref dep }
                if task == &task_id("alpha") && dep == &task_id("ghost")
        ));
    }

    #[test]
    fn assertion_store_round_trip_drains_per_record() {
        let mut store = AssertionResultStore::new();
        let record_a = RecordId(Uuid::from_u128(1));
        let record_b = RecordId(Uuid::from_u128(2));

        store.record(task_id("alpha"), record_a.clone(), result_for("alpha", 0));
        store.record(task_id("beta"), record_a.clone(), result_for("beta", 1));
        store.record(task_id("alpha"), record_b.clone(), result_for("alpha", 0));

        assert_eq!(store.len(), 3);
        assert_eq!(
            store
                .get(&task_id("alpha"), &record_a)
                .map(|result| result.stage),
            Some(0)
        );

        let drained = store.take_for_record(&record_a);
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].task_id, task_id("alpha"));
        assert_eq!(drained[1].task_id, task_id("beta"));
        assert_eq!(store.len(), 1);
        assert!(store.get(&task_id("alpha"), &record_b).is_some());
    }

    #[test]
    fn llm_response_store_round_trip() {
        let mut store = LlmResponseStore::new();
        let record = RecordId(Uuid::from_u128(7));
        let outcome = JudgeOutcome {
            raw: json!({"choices": [{"verdict": "pass"}]}),
            parsed: json!({"verdict": "pass"}),
        };

        store.record(task_id("judge"), record.clone(), outcome.clone());
        assert_eq!(store.len(), 1);
        assert_eq!(store.get(&task_id("judge"), &record), Some(&outcome));

        let taken = store
            .take(&task_id("judge"), &record)
            .expect("fixture outcome is present");
        assert_eq!(taken, outcome);
        assert!(store.is_empty());
        assert!(store.get(&task_id("judge"), &record).is_none());
    }

    #[test]
    fn cross_scenario_key_collision_is_silent_without_reset() {
        let mut store = AssertionResultStore::new();
        let task = task_id("greeting_present");
        let record = RecordId(Uuid::from_u128(42));

        let pass = AssertionResult {
            passed: true,
            ..result_for("greeting_present", 0)
        };
        store.record(task.clone(), record.clone(), pass.clone());
        assert_eq!(
            store.get(&task, &record).map(|result| result.passed),
            Some(true)
        );

        let fail = AssertionResult {
            passed: false,
            ..result_for("greeting_present", 0)
        };
        store.record(task.clone(), record.clone(), fail.clone());
        assert_eq!(
            store.get(&task, &record).map(|result| result.passed),
            Some(false)
        );
        assert_eq!(store.len(), 1);

        let mut store_a = AssertionResultStore::new();
        let mut store_b = AssertionResultStore::new();
        store_a.record(task.clone(), record.clone(), pass);
        store_b.record(task.clone(), record.clone(), fail);
        assert_eq!(
            store_a.get(&task, &record).map(|result| result.passed),
            Some(true)
        );
        assert_eq!(
            store_b.get(&task, &record).map(|result| result.passed),
            Some(false)
        );
    }
}
