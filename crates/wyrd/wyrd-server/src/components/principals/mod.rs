//! Tenant-scoped principal and credential administration.
//!
//! The tenant plane's answer to "who else may act here, and with what": machine
//! principals for automation, their roles, and the credentials that authenticate
//! them. Everything is scoped to the authenticated caller's tenant.

pub mod routes;

pub use routes::principals_router;
