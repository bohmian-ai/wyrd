//! Task executors - `Assertion`, `LlmJudge`, `TraceAssertion`,
//! `AgentAssertion`, plus per-record media bindings.
//!
//! Bodies land in Commits 8-10.

pub mod agent;
pub mod assertion;
pub mod llm_judge;
pub mod media;
pub mod trace;
