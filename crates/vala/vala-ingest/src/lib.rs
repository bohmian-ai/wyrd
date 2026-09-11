//! `vala-ingest` — compatibility façade for transport-neutral Bifrost helpers.
//!
//! An authenticated client streams Arrow IPC frames (carrying per-row `run_id` /
//! `card_ref` correlation columns) plus a `wyrd_batch_id`, and the server commits
//! them as bounded per-frame admissions with real audit attribution — after
//! validating every `card_ref` against the principal's card scope. WAL,
//! Parquet, and later Forge publication remain downstream of the admission ACK.
//!
//! Gate, authentication, limits, catalog resolution, and whole-stream
//! orchestration live in `vala-bifrost-redux`. This crate keeps only reusable
//! Arrow frame decoding and OTLP projection exports for clients that still
//! depend on the package name.

pub mod decode;

pub use wyrd_tonic::wyrd::v1::{InsertBatchRequest, InsertBatchResponse};
