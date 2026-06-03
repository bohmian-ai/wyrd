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
//! - `operator` — `ComparisonOperator`, `JsonValueType`,
//!   `DivergenceMetric`.
//! - `status` — `EvalStatus` (mirrors `eval_inbox.status`).
//! - `workflow` — `Workflow`, `WorkflowFieldType` (optional declared shape).
//! - (`media` — deferred; lands with its first real consumer.)
//! - `condition` — `EvalCondition` gate predicates shared by tasks.
//! - `assertion` — `AssertionTask` programmatic assertions.
//! - `llm_judge` — `LlmJudgeTask` prompt-backed judge task.
//! - `trace` — `TraceAssertionTask` trace document assertions.
//! - `agent` — `AgentAssertionTask` workflow envelope assertions.
//! - `task` — `EvalTask` executable task enum.
//! - `plan` — DAG validation and topological execution stages.
//!
//! Later commits add the remaining plan, result, and spec modules.

pub mod agent;
pub mod assertion;
pub mod condition;
pub mod ids;
pub mod llm_judge;
pub mod operator;
pub mod plan;
pub mod status;
pub mod task;
pub mod trace;
pub mod workflow;

pub use agent::AgentAssertionTask;
pub use assertion::AssertionTask;
pub use condition::{ConditionCombinator, EvalCondition, MAX_CONDITION_DEPTH};
pub use ids::{
    EntityUid, JsonPath, RecordId, ScenarioId, SessionId, SpanId, TaskId, TraceId, WorkflowUid,
};
pub use llm_judge::LlmJudgeTask;
pub use operator::{ComparisonOperator, DivergenceMetric, JsonValueType};
pub use plan::{DagError, ExecutionPlan, Stage, validate_dag};
pub use status::EvalStatus;
pub use task::EvalTask;
pub use trace::TraceAssertionTask;
pub use workflow::{Workflow, WorkflowFieldType};
