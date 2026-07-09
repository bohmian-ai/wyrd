//! Typed ValaQueryService — HTTP + gRPC read surface for Bifrost domain tables.
//!
//! Each submodule corresponds to one concern:
//!   - `page_token`: signed opaque pagination tokens
//!   - `service`: typed LogicalPlan builders (one per ValaQueryService method)
//!   - `routes`: axum HTTP projection
//!   - `grpc`: tonic ValaQueryService implementation

pub mod grpc;
pub mod page_token;
pub mod routes;
pub mod service;
