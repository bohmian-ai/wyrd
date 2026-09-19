//! The eval pull protocol from a client.
//!
//! The server owns an eval run: it holds the scenario queue, decides whose turn
//! is next, and records the observations a turn produced. A client only pulls
//! the next directive and reports what happened, which is why this surface is a
//! loop of four calls rather than a single request/response.
//!
//! Opening a run mints a per-run lease. Every later call on that run carries it
//! beside the caller's Wyrd token, so a principal that may run evals still
//! cannot drive a run it did not open. [`EvalRun`] holds the lease and attaches
//! it, so no caller assembles that header — or sees the token at all.

mod handle;

pub use handle::{EvalProtocol, EvalRun};

// The wire contract this handle speaks, re-exported so a caller reaches one
// module for the capability and its types rather than depending on `wyrd-spec`
// directly.
pub use wyrd_spec::error::WyrdError;
pub use wyrd_spec::vala::eval::protocol::{
    AgentTurnSubmission, EvalRunOpenRequest, EvalRunOpenResponse, TurnDirective, UserTurnSubmission,
};
pub use wyrd_spec::vala::ids::{LeaseToken, RunId};
