//! Card registration HTTP surface and its server-owned orchestration.

mod mapping;
pub(crate) mod reconciler;
mod resolve;
pub(crate) mod routes;
pub(crate) mod service;

pub use routes::cards_router;
