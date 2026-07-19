//! Per-`ExecutionPlan` task registry plus the engine-local [`JudgeOutcome`]
//! shape.
//!
//! Built once per plan, immutable thereafter. Per-run state (skip ledger,
//! task outputs) lives on [`crate::executor::RunLedger`] and
//! [`crate::context::ContextSnapshot`]; the registry only answers "what task
//! has this id and what kind is it".

use std::collections::{BTreeSet, HashMap};

use serde_json::Value;
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::eval::ids::TaskId;
use wyrd_spec::vala::eval::plan::ExecutionPlan;
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
///
/// Built once per plan, immutable thereafter. The stage driver consults this
/// on every task dispatch and every dependency check. Per-run state lives
/// elsewhere: skip ledger and per-task outputs are owned by
/// [`crate::executor::RunLedger`] and [`crate::context::ContextSnapshot`].
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
    /// absent dependency or the plan and spec task sets disagree.
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

/// One judge invocation's raw and parsed payload.
///
/// The judge executor (commit 9) produces this; the engine retains it on the
/// snapshot's `task_outputs` so downstream tasks can JSONPath into
/// `$.task.<judge_id>.parsed`.
#[derive(Debug, Clone, PartialEq)]
pub struct JudgeOutcome {
    /// Raw provider response payload as the judge invoker returned it.
    pub raw: Value,
    /// Structured projection the judge executor extracted from `raw`.
    pub parsed: Value,
    /// Durable Agent identity when this judge was registered.
    pub judge_ref: Option<CardRef>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;
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
    fn judge_outcome_round_trip_is_value_type() {
        let outcome = JudgeOutcome {
            raw: json!({"choices": [{"verdict": "pass"}]}),
            parsed: json!({"verdict": "pass"}),
            judge_ref: None,
        };
        let cloned = outcome.clone();
        assert_eq!(outcome, cloned);
    }
}
