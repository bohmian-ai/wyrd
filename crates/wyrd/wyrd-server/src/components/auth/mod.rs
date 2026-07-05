//! Authentication HTTP adapters and server-local auth state.

pub mod audit_writer;
pub mod caller_extractor;
pub mod policy_hook;
pub mod principal_extractor;
pub mod routes;
pub mod state;
pub(crate) mod token_extract;

pub use caller_extractor::Caller;
pub use principal_extractor::AuthenticatedPrincipal;
pub use routes::auth_router;
pub use state::{ServerAuth, ServerAuthz};
