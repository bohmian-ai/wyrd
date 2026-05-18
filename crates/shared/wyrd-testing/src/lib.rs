//! Shared testing helpers.

#![deny(missing_docs)]

/// Marker that the testing crate is linked.
#[must_use]
pub fn crate_ready() -> bool {
    true
}
