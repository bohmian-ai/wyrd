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
//! - `status` — `EvalStatus` (mirrors `eval_inbox.status`).
//! - `workflow` — `Workflow`, `WorkflowFieldType` (optional declared shape).
//! - (`media` — deferred; lands with its first real consumer.)
//! - `operator` — `ComparisonOperator` catalog (~56 variants).
//! - `condition` — `EvalCondition` (per-task gate predicate).
//! - `assertion`, `llm_judge`, `trace`, `agent` — per-task structs.
//! - `task` — `EvalTask` enum wrapping the 4 per-task structs.
//! - `plan` — `ExecutionPlan`, `Stage`, `validate_dag`, `DagError`.
//! - `result` — `AssertionResult`, `EvalPassGate`.
//! - `spec` — `EvalSpec` (the card kind's typed spec body) + sampling
//!   and `DatasetRef`. (Per-rubric alerting is deferred to a follow-up
//!   commit gated on drift primitives + a locked `Secret` card kind.)
//! - `scenario` — `EvalScenario`, `ScenarioTask`, `EvalScenarioCollection`
//!   (the payload shape for DataCards with
//!   `content_kind = "EvalScenarioCollection"`).
//!
//! Future commits add each module declaration with the file that defines its
//! concrete contracts. Re-exports are added in their owning commits (02
//! onward).
