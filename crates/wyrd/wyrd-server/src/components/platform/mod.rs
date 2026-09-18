//! Platform control plane: the tenant directory and its lifecycle.
//!
//! This plane manages tenants; it never reaches inside one. Its routes are
//! reachable only with a platform session, and its decisions are recorded in
//! the transaction that acts on them.

pub mod provisioning;
pub mod recovery;
pub mod routes;

pub use routes::platform_router;
