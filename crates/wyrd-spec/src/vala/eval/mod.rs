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
//! - `media` — `MediaRef` (URI-bearing descriptor; no inline blobs).
//! - `operator` — `ComparisonOperator`, `JsonValueType`,
//!   `DivergenceMetric`.
//! - `status` — `EvalStatus` (mirrors `eval_inbox.status`).
//! - `workflow` — `Workflow`, `WorkflowFieldType` (optional declared shape).
//! - `condition` — `EvalCondition` gate predicates shared by tasks.
//! - `assertion` — `AssertionTask` programmatic assertions.
//! - `llm_judge` — `LlmJudgeTask` prompt-backed judge task.
//! - `trace` — `TraceAssertionTask` trace document assertions.
//! - `agent` — `AgentAssertionTask` workflow envelope assertions.
//! - `task` — `EvalTask` executable task enum.
//! - `plan` — DAG validation and topological execution stages.
//! - `result` — per-task result records and workflow-level pass gates.
//! - `spec` — `EvalSpec`, dataset references, and sampling policy.
//! - `scenario` — offline scenario payloads for eval DataCards.
//!
//! Later commits add the remaining spec modules.

pub mod agent;
pub mod assertion;
pub mod condition;
pub mod ids;
pub mod llm_judge;
/// Media reference descriptors for eval records — object-storage URIs, no inline blobs.
pub mod media;
pub mod operator;
pub mod plan;
pub mod protocol;
pub mod record;
pub mod result;
pub mod scenario;
pub mod spec;
pub mod status;
pub mod task;
pub mod trace;
pub mod workflow;

// ─── Public re-exports ───────────────────────────────────────────────────
//
// External crates should reach this surface through these names.
// Adding a re-export here is a public-API change; review per AGENTS.md §3.

pub use agent::AgentAssertionTask;
pub use assertion::AssertionTask;
pub use condition::{ConditionCombinator, EvalCondition, MAX_CONDITION_DEPTH};
pub use ids::{
    EntityUid, JsonPath, LeaseToken, RecordId, RunId, ScenarioId, SessionId, SpanId, TaskId,
    TraceId, WorkflowUid,
};
pub use llm_judge::LlmJudgeTask;
pub use media::MediaRef;
pub use operator::{ComparisonOperator, DivergenceMetric, JsonValueType};
pub use plan::{DagError, ExecutionPlan, Stage, validate_dag};
pub use protocol::{
    AgentTurnSubmission, ConversationTurn, EvalRunOpenRequest, EvalRunOpenResponse,
    MAX_HISTORY_TURNS, ProtocolError, SimulatedUserMode, SimulatedUserTurn, TurnDirective,
    TurnRole, UserTurnSubmission,
};
pub use record::EvalRecordObservation;
pub use result::{AssertionResult, EvalContextCapture, EvalPassGate};
pub use scenario::{
    EvalScenario, EvalScenarioCollection, MAX_SCENARIOS_PER_COLLECTION, MAX_TURNS_HARD_CAP,
    ScenarioTask,
};
pub use spec::{DatasetRef, EvalSampling, EvalSpec, EvalSpecError, MAX_EVAL_TASKS};
pub use status::EvalStatus;
pub use task::EvalTask;
pub use trace::TraceAssertionTask;
pub use workflow::{Workflow, WorkflowFieldType};
