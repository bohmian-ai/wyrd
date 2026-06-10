//! In-memory eval engine for Wyrd's Eval card.
//!
//! Consumes the locked `wyrd_spec::vala::eval` contract (`EvalSpec`,
//! `EvalTask`, `ExecutionPlan`, `AssertionResult`, `ComparisonOperator`,
//! typed ids) and produces results. Mode-agnostic: this crate executes the
//! per-record / per-scenario tasks; how records arrive is the orchestrator's
//! or the server's problem.
//!
//! ## Engine boundary
//!
//! - The engine's default features never link `skald-agent`. The Skald-backed
//!   `JudgeInvoker` lives behind an opt-in `orchestrator` feature in the
//!   `orchestrator/` submodule of this crate.
//! - The engine never binds dynamic-language runtimes. There is no
//!   language-binding feature on this crate.
//! - The engine never touches IO directly. `TraceSource::fetch` is the only
//!   async boundary; the in-memory impl ships here, the server-backed impl
//!   lands in `vala-http`.
//!
//! ## Public surface
//!
//! - `operators` - runtime semantics for the 56-variant `ComparisonOperator`
//!   catalog.
//! - `context` - `ExecutionContext`, `ContextSnapshot`, `TaskOutput`,
//!   `RecordIdentity`. Per-stage state is rebuilt at each stage barrier via
//!   `ContextSnapshot::extend` (Arc bumps, no JSON cloning).
//! - `store` - immutable `TaskRegistry`, `EvalTaskKind`, `JudgeOutcome`.
//! - `tasks` - the four executors (`assertion`, `judge`, `trace`, `agent`)
//!   plus `media` bindings.
//! - `scenario` - loader plus mechanic/passenger pass.
//! - `results` - task -> scenario -> subject -> run aggregation,
//!   `pass_gate`, `context_capture` application.
//! - `compare` - four-quadrant comparison.
//! - `judge` - `JudgeInvoker` trait; impls live outside this crate.
//! - `trace_source` - `TraceSource` trait plus in-memory impl.
//!
//! ## Error model
//!
//! Two-phase per the wyrd-rust-python skill `references/errors.md`:
//!
//! - [`EvalExecError`] (crate-local, `thiserror`) - engine-only failures
//!   raised inside the executors and stores.
//! - [`EvalError`] (public, `#[wyrd_error(...)]` derive) - boundary-crossing
//!   shape used at external surfaces. `From<EvalExecError> for
//!   EvalError` translates at the surface.

pub mod compare;
pub mod context;
pub mod error;
pub mod executor;
pub mod judge;
pub mod operators;
pub mod results;
pub mod scenario;
pub mod store;
pub mod tasks;
pub mod trace_source;

pub use context::{
    ContextSnapshot, ExecutionContext, RecordIdentity, TaskOutput, extract_jsonpath_from,
    extract_required_jsonpath_from,
};
pub use error::{EvalError, EvalExecError};
pub use executor::{
    EvalReport, Executors, RunLedger, SkipReason, TaskExecutor, TaskRunOutcome, execute_plan,
};
pub use judge::{JudgeError, JudgeInvoker, MockJudgeInvoker};
pub use operators::{OperatorVerdict, evaluate_operator};
pub use store::{EvalTaskKind, JudgeOutcome, TaskRegistry};
pub use tasks::{EvalMediaBinding, MediaBindings};
pub use trace_source::{InMemoryTraceSource, MockTraceSource, TraceSource, TraceUnavailable};
