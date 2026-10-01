//! Operator connections: tenant-managed Slack, PagerDuty, and HTTP
//! credentials stored as envelope ciphertext.
//!
//! [`keys::OperatorKeys`] owns the key-encryption keys and
//! [`service::OperatorConnectionControl`] the management operations; HTTP
//! and MCP are thin projections of it.

pub mod keys;
pub mod routes;
pub(crate) mod service;

pub use routes::operator_connections_router;
