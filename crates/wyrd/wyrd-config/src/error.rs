//! TODO(commit 04): WyrdConfigError thiserror enum and From impl
//! to wyrd_spec::error::WyrdError.
//!
//! Note: this is the *crate-local* error enum, not the wyrd-spec
//! catalog extension which lands in commit 02.

use thiserror::Error;

/// Errors raised by the `wyrd-config` crate.
#[derive(Debug, Error)]
pub enum WyrdConfigError {}
