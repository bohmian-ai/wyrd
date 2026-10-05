//! Accepted server Workflow runs.
//!
//! A registered Workflow is accepted once per scoped idempotency key,
//! prepared and executed on tracked process-local work by the one Skald
//! engine, and inspected or cancelled by its owner through the routes in
//! [`routes`]. [`WorkflowRuns`] owns admission, retention, and shutdown
//! drain; runs are not persisted and are lost with the process.

mod host;
mod routes;
mod runs;
mod tools;

pub use routes::workflow_runs_router;
pub use runs::WorkflowRuns;
