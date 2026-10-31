//! Verifier-shared Bifrost tables.
//!
//! Each verification fact is stored once:
//! `bifrost.verification.execution_inputs` holds each execution's one input
//! record, `bifrost.verification.execution_outputs` each result's detailed
//! judgment, and `bifrost.verification.results` each result's payload-free
//! summary.

mod execution_inputs;
mod execution_outputs;
mod results;

pub use execution_inputs::{ACTIVATION, ExecutionInputsTable, INPUTS};
pub use execution_outputs::{ExecutionOutputsTable, OUTPUT};
pub use results::ResultsTable;
