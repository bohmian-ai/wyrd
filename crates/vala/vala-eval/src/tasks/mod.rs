//! Per-variant task executors.
//!
//! The stage driver in `executor.rs` dispatches on the `EvalTask`
//! discriminator and calls into one of these.

pub mod agent;
pub mod assertion;
pub mod trace;

pub use agent::AgentTaskExecutor;
pub use assertion::AssertionTaskExecutor;
pub use trace::TraceTaskExecutor;
