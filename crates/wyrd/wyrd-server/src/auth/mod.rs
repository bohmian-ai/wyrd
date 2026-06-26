//! Authentication extractors for Wyrd HTTP handlers.

pub mod audit_writer;
pub mod caller_extractor;
pub mod exchange_api_key;
pub mod issue_api_key;
pub mod permission_resolver;
pub mod policy_hook;
pub mod principal_extractor;
pub mod refresh;
pub mod repo;
pub mod revocation_listener;
pub mod revocation_resolver;
pub mod revoke;
pub mod roles;
pub mod routes;
pub mod seed;
pub(crate) mod token_extract;

pub use caller_extractor::Caller;
pub use principal_extractor::AuthenticatedPrincipal;
