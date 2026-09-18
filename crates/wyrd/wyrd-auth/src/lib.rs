//! Server-tier auth domain logic for Wyrd.
//!
//! This crate owns database-backed auth resolvers and card-scope auth helpers.
//! HTTP extractors, handlers, middleware, and response mapping stay in
//! `wyrd-server`.

pub mod audit;
pub mod callback;
pub mod card_scope;
pub(crate) mod error;
pub mod exchange_api_key;
pub mod issue_api_key;
pub mod issuer;
pub mod jwt_bearer;
pub mod login;
pub mod permission_resolver;
pub mod pg_resolvers;
pub mod platform_authz;
pub mod platform_credentials;
pub mod platform_login;
pub mod platform_sessions;
pub mod refresh;
pub mod repo;
pub mod revocation_listener;
pub mod revocation_resolver;
pub mod revoke;
pub mod roles;
pub mod seed;
pub mod service_accounts;

pub use callback::AuthorizationCodeExchange;
pub use exchange_api_key::{DelegateToken, ExchangeApiKey};
pub use jwt_bearer::JwtBearer;
