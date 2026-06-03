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
//!
//! Later commits add the remaining task, plan, result, and spec modules.

pub mod ids;
pub mod operator;
pub mod status;
pub mod workflow;

pub use ids::{
    EntityUid, JsonPath, RecordId, ScenarioId, SessionId, SpanId, TaskId, TraceId, WorkflowUid,
};
pub use operator::{ComparisonOperator, DivergenceMetric, JsonValueType};
pub use status::EvalStatus;
pub use workflow::{Workflow, WorkflowFieldType};
