//! Verifier-shared Bifrost tables.
//!
//! Each verification fact is stored once:
//! `bifrost.verification.execution_inputs` holds each execution's one input
//! record, `bifrost.verification.execution_outputs` each result's detailed
//! judgment, and `bifrost.verification.results` each result's payload-free
//! summary. `bifrost.verification.execution_events` holds the progress
//! history of queued executions while and after they run.
//! `bifrost.verification.evidence` holds published deferred evidence before
//! and while the executions that consume it run.

mod evidence;
mod execution_events;
mod execution_inputs;
mod execution_outputs;
mod results;

pub use evidence::{EVIDENCE, EvidenceTable};
pub use execution_events::{ExecutionEventsTable, VALUE};
pub use execution_inputs::{ACTIVATION, ExecutionInputsTable, INPUTS};
pub use execution_outputs::{ExecutionOutputsTable, OUTPUT};
pub use results::ResultsTable;
