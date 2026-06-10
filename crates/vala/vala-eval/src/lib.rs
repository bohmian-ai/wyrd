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
//! - The engine never binds Python. There is no `python` feature on this crate.
//!   Python is a thin protocol client.
//! - The engine never touches IO directly. `TraceSource::fetch` is the only
//!   async boundary; the in-memory impl ships here, the server-backed impl
//!   lands in `vala-http`.
//!
//! ## Public surface
//!
//! - `operators` - runtime semantics for the 56-variant `ComparisonOperator`
//!   catalog.
//! - `context` / `store` - `ExecutionContext`, `TaskRegistry`,
//!   `AssertionResultStore`, `LlmResponseStore`.
//! - `task` - the four executors (`assertion`, `llm_judge`, `trace`, `agent`)
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
//!   shape used at HTTP / Python / MCP surfaces. `From<EvalExecError> for
//!   EvalError` translates at the surface.

pub mod compare;
pub mod context;
pub mod error;
pub mod judge;
pub mod operators;
pub mod results;
pub mod scenario;
pub mod store;
pub mod task;
pub mod trace_source;

pub use error::{EvalError, EvalExecError};
