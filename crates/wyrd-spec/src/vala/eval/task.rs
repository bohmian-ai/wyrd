//! The `EvalTask` enum wraps the four executable per-task structs. The
//! `Conditional` and `HumanValidation` predecessor variants are intentionally
//! absent (audit section 2):
//!
//! - `Conditional` is realized as the per-task `condition` field on every
//!   variant (see `crate::vala::eval::condition::EvalCondition`).
//! - `HumanValidation` is omitted from v1. Re-introducing it requires a new
//!   locked decision.

use serde::{Deserialize, Serialize};

use super::agent::AgentAssertionTask;
use super::assertion::AssertionTask;
use super::condition::EvalCondition;
use super::ids::TaskId;
use super::llm_judge::LlmJudgeTask;
use super::trace::TraceAssertionTask;

/// One task in an `EvalSpec` DAG. Adjacently-tagged on the wire by
/// `"kind"`; the inner record fields live alongside.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EvalTask {
    /// Programmatic assertion against the workflow context.
    Assertion(AssertionTask),
    /// LLM-as-judge evaluation via a `Prompt` card.
    LlmJudge(LlmJudgeTask),
    /// Assertion over the assembled OTel trace document.
    TraceAssertion(TraceAssertionTask),
    /// Assertion over the assembled GenAI workflow envelope.
    AgentAssertion(AgentAssertionTask),
}

impl EvalTask {
    /// The task's id within its enclosing `EvalSpec.tasks` map.
    #[must_use]
    pub fn id(&self) -> &TaskId {
        match self {
            EvalTask::Assertion(task) => &task.id,
            EvalTask::LlmJudge(task) => &task.id,
            EvalTask::TraceAssertion(task) => &task.id,
            EvalTask::AgentAssertion(task) => &task.id,
        }
    }

    /// Ids of upstream tasks whose completion gates this task.
    #[must_use]
    pub fn depends_on(&self) -> &[TaskId] {
        match self {
            EvalTask::Assertion(task) => &task.depends_on,
            EvalTask::LlmJudge(task) => &task.depends_on,
            EvalTask::TraceAssertion(task) => &task.depends_on,
            EvalTask::AgentAssertion(task) => &task.depends_on,
        }
    }

    /// Optional gate predicate evaluated at execution time. `None` means
    /// always-run, subject only to `depends_on`.
    #[must_use]
    pub fn condition(&self) -> Option<&EvalCondition> {
        match self {
            EvalTask::Assertion(task) => task.condition.as_ref(),
            EvalTask::LlmJudge(task) => task.condition.as_ref(),
            EvalTask::TraceAssertion(task) => task.condition.as_ref(),
            EvalTask::AgentAssertion(task) => task.condition.as_ref(),
        }
    }

    /// Stable wire discriminator for this variant. Mirrors the
    /// `#[serde(tag)]` value.
    #[must_use]
    pub fn discriminator(&self) -> &'static str {
        match self {
            EvalTask::Assertion(_) => "assertion",
            EvalTask::LlmJudge(_) => "llm_judge",
            EvalTask::TraceAssertion(_) => "trace_assertion",
            EvalTask::AgentAssertion(_) => "agent_assertion",
        }
    }
}
