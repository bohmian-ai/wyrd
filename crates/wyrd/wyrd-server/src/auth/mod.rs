//! Authentication extractors for Wyrd HTTP handlers.

pub mod audit_writer;
pub mod caller_extractor;
pub mod exchange_api_key;
pub mod issue_api_key;
pub mod permission_resolver;
pub mod principal_extractor;
pub mod repo;
pub mod roles;
pub mod routes;
pub mod seed;

pub use caller_extractor::Caller;
pub use principal_extractor::AuthenticatedPrincipal;
