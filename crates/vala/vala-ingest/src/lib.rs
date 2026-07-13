//! `vala-ingest` — the `wyrd.v1.BifrostIngestService` gRPC ingest server.
//!
//! An authenticated client streams Arrow IPC frames (carrying per-row `run_id` /
//! `card_ref` correlation columns) plus a `wyrd_batch_id`, and the server commits
//! them as one durably-idempotent 2PC batch with real audit attribution — after
//! validating every `card_ref` against the principal's card scope.
//!
//! This crate owns no listener; S3.C2 mounts it on the shared tonic listener.
//! tonic / prost / tonic-types are consumed only through `wyrd-tonic` re-exports.

pub mod auth;
pub mod collector;
pub mod decode;
pub mod error;
pub mod limits;
pub mod orchestrator;
pub mod service;

pub use auth::{AuthContext, IngestAuthInterceptor, ingest_auth_interceptor};
pub use collector::{IngestOutcome, OtlpTraceService, ingest_resource_spans};
pub use error::IngestError;
pub use limits::{IngestLimits, StreamSemaphores};
pub use service::BifrostIngestGrpc;

// Re-export the generated server type so wyrd-server (S3.C2) mounts it without
// touching wyrd-tonic's module path directly.
pub use wyrd_tonic::wyrd::v1::bifrost_ingest_service_server::BifrostIngestServiceServer;
pub use wyrd_tonic::wyrd::v1::{InsertBatchRequest, InsertBatchResponse};
