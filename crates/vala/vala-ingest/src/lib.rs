//! `vala-ingest` — transport-neutral Bifrost projection and validation helpers.
//!
//! An authenticated client streams Arrow IPC frames (carrying per-row `run_id` /
//! `card_ref` correlation columns) plus a `wyrd_batch_id`, and the server commits
//! them as one durably-idempotent 2PC batch with real audit attribution — after
//! validating every `card_ref` against the principal's card scope.
//!
//! The server-owned Gate mounts the protocol service; this crate owns reusable
//! decode, projection, authorization-context, and error helpers only.

pub mod auth;
pub mod collector;
pub mod decode;
pub mod error;
pub mod limits;
pub mod orchestrator;

pub use auth::{AuthContext, IngestAuthInterceptor, ingest_auth_interceptor};
pub use collector::{
    IngestOutcome, LogsOutcome, MetricsOutcome, ingest_resource_logs_to_scribe,
    ingest_resource_metrics_to_scribe, ingest_resource_spans_to_scribe,
};
pub use error::IngestError;
pub use limits::{IngestLimits, StreamSemaphores};
pub use wyrd_tonic::wyrd::v1::{InsertBatchRequest, InsertBatchResponse};
