//! `vala.gateway` built-in tables owned by the gateway capture path.

pub mod calls;

pub use calls::{CallsTable, REQUEST_PAYLOAD, RESOLVED_MODEL, RESPONSE_PAYLOAD};
