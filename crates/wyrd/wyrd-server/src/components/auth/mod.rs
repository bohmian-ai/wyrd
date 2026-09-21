//! Authentication HTTP adapters and server-local auth state.

pub mod audit_writer;
/// The [`Caller`] extractor: token-derived tenant, principal, and delegation chain.
pub mod caller_extractor;
pub mod platform_extractor;
pub mod policy_hook;
/// The authenticated-principal extractor for routes that need the verified token.
pub mod principal_extractor;
pub mod routes;
/// Server-local authentication and authorization state.
pub mod state;
pub(crate) mod token_extract;

pub use caller_extractor::Caller;
pub use platform_extractor::PlatformCaller;
pub use principal_extractor::AuthenticatedPrincipal;
pub use routes::auth_router;
pub use state::{ServerAuth, ServerAuthz};
