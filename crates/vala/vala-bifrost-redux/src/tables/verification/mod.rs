//! Verifier-shared Bifrost tables.
//!
//! `bifrost.verification.results` is the one result surface every Verifier
//! execution writes; `bifrost.verification.execution_inputs` pins each
//! execution's named inputs and a queued execution's accepted request context.

mod execution_inputs;
mod results;

pub use execution_inputs::{ACTIVATION, ExecutionInputsTable, INPUTS};
pub use results::{OUTCOME, ResultsTable};
