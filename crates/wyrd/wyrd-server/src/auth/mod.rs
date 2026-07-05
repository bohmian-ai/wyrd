//! Authentication extractors for Wyrd HTTP handlers.

pub mod callback;
pub mod jwt_bearer;
pub mod login;
pub mod revoke;

pub(crate) use wyrd_auth::card_scope;
pub use wyrd_auth::{
    exchange_api_key, issue_api_key, permission_resolver, pg_resolvers, refresh, repo,
    revocation_listener, revocation_resolver, roles, seed,
};
