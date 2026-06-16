//! Authentication extractors for Wyrd HTTP handlers.

pub mod caller_extractor;
pub mod permission_resolver;
pub mod principal_extractor;
pub mod repo;
pub mod roles;
pub mod seed;

pub use caller_extractor::Caller;
pub use principal_extractor::AuthenticatedPrincipal;
