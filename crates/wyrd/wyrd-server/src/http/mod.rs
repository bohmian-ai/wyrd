//! Cross-feature HTTP router and middleware.

pub mod error;
pub mod middleware;
pub mod router;

pub use router::build_router;
