//! `JudgeInvoker` async trait.
//!
//! The trait is owned by the engine; concrete impls live behind the opt-in
//! `orchestrator` feature or in other crates:
//!
//! - production: `vala-eval::orchestrator::SkaldJudgeInvoker` wraps
//!   `skald-agent::Agent::run_with` with structured output.
//! - tests: a deterministic mock invoker ships alongside the trait body in
//!   Commit 9 so engine tests stay skald-free.
//!
//! Body lands in Commit 9 (`10-judge-invoker-and-media.md`).
