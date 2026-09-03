//! Forge subsystem tier-2 tests.
//!
//! The promotion group and the managed-rewrite group are live. The remaining
//! files in this directory are archived bodies from the erased rewrite route
//! and stay unlisted until their tests are restored against a current
//! production invariant.

mod expired_cleanup;
mod managed_rewrite;
mod promotion;
mod publication;
mod reader_expiry_ordering;
mod rewrite_support;
pub(crate) mod snapshot_expiration;
pub(crate) mod support;
