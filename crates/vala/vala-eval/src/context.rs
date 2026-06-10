//! `ExecutionContext` - per-record runtime state.
//!
//! Body lands in Commit 7 (`08-execution-context-and-stores.md`).

use crate::error::EvalExecError;

/// Placeholder so `lib.rs` exposes the module shape commits 7-12 fill in.
///
/// The full shape (base-context JSON, dependency outputs, per-record stores)
/// lands in Commit 7.
#[derive(Debug)]
pub struct ExecutionContext;

impl ExecutionContext {
    /// Body lands in Commit 7.
    ///
    /// # Errors
    /// Always panics in this stage. Commit 7 replaces this placeholder with
    /// real validation.
    pub fn new() -> Result<Self, EvalExecError> {
        unimplemented!("ExecutionContext::new lands in Commit 7")
    }
}
