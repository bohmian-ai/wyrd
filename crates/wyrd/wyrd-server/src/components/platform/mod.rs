//! Platform control plane: the tenant directory and its lifecycle.
//!
//! This plane manages tenants; it never reaches inside one. Its routes are
//! reachable only with a platform session, and its decisions are recorded in
//! the transaction that acts on them.
//!
//! `identity` adds the optional federated way in: the deployment's one OIDC
//! connection, the humans registered against it, and their login. It never
//! replaces the global credential, so losing the provider costs the deployment
//! nothing but individual sign-in.

pub mod identity;
pub mod provisioning;
pub mod recovery;
pub mod routes;

pub use identity::{platform_identity_router, platform_login_router};
pub use routes::{platform_auth_router, platform_router};
