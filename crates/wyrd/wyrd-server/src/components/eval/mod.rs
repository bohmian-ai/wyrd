//! Native eval pull-protocol HTTP adapters.

pub mod error;
pub mod resolver;
pub mod routes;
pub mod state;

pub use routes::eval_router;
pub use state::{EvalRuns, MAX_CONCURRENT_RUNS, RUN_TTL, RunEntry, RunKey, new_run_map};
