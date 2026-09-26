//! Verification control plane: binding status, manual runs, and run status.
//!
//! [`service::VerificationControl`] owns the operations; HTTP and MCP are thin
//! projections of it.

pub mod routes;
pub(crate) mod service;

pub use routes::verification_router;
