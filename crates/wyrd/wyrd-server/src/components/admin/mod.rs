//! Tenant administration HTTP adapters.

pub mod identity;
pub mod routes;

pub use identity::identity_router;
pub use routes::admin_router;
