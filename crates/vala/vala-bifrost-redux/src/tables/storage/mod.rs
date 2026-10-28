//! Storage-owned Bifrost tables.
//!
//! `bifrost.storage.objects` is the one inventory of completed uploaded
//! objects: Card files, verification File uploads, and Card envelopes.

mod objects;

pub use objects::{ObjectOrigin, ObjectState, ObjectsTable};
