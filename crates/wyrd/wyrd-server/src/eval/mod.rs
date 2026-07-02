//! Native eval pull-protocol transport.
//!
//! Serves the Vala eval pull-protocol as `/v1/eval/*` routes inside the
//! authenticated `/v1` group. The eval engine (`RunState`, submission state
//! machine) is consumed from `vala-eval` as a library; the run/lease/session
//! map and its tenant/owner enforcement are owned here.
//!
//! Tenant isolation (spec §2a) is enforced on every path: the run map is keyed
//! by `(tenant_id, run_id)` and every card row read runs under a
//! `TenantConn` so Postgres RLS is the backstop. Server-side scoring (the judge
//! path) is intentionally deferred on this surface — see [`resolver`] for the
//! tenant-bound card resolver that any future judge/prompt resolution must use.

pub mod audit;
pub mod error;
pub mod resolver;
pub mod routes;
pub mod state;

pub use audit::{EvalAuditEvent, EvalAuditKind, EvalAuditWriter, TracingEvalAuditWriter};
pub use state::{EvalRuns, MAX_CONCURRENT_RUNS, RUN_TTL, RunEntry, RunKey, new_run_map};
