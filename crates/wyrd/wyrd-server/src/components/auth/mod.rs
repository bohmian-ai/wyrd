//! Authentication HTTP adapters and server-local auth state.

/// The [`Caller`] extractor: token-derived tenant, principal, and delegation chain.
pub mod caller_extractor;
/// The OTLP-only API-key authentication entrance shared by HTTP and gRPC.
pub(crate) mod otlp_api_key;
pub mod platform_extractor;
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
