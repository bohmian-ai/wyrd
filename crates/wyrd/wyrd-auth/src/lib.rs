//! Server-tier auth domain logic for Wyrd.
//!
//! This crate owns database-backed auth resolvers and card-scope auth helpers.
//! HTTP extractors, handlers, middleware, and response mapping stay in
//! `wyrd-server`.

pub mod card_scope;
pub mod exchange_api_key;
pub mod issuer;
pub mod issue_api_key;
pub mod permission_resolver;
pub mod pg_resolvers;
pub mod refresh;
pub mod repo;
pub mod revocation_listener;
pub mod revocation_resolver;
pub mod revoke;
pub mod roles;
pub mod seed;
pub mod service_accounts;
