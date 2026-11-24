//! Operator connections: tenant-managed Slack, PagerDuty, and HTTP
//! credentials stored as the common stored-secret envelope.
//!
//! [`secrets::OperatorSecrets`] owns sealing, renewal, and opening, and
//! [`service::OperatorConnectionControl`] the management operations; HTTP
//! and MCP are thin projections of it.

pub mod routes;
pub mod secrets;
pub(crate) mod service;

pub use routes::operator_connections_router;
