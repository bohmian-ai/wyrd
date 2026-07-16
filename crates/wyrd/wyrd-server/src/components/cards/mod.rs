//! Card registration HTTP surface and its server-owned orchestration.

mod mapping;
mod post_commit;
mod resolve;
pub(crate) mod routes;
mod service;

pub use routes::cards_router;
