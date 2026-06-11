//! Concrete client error returned by `Flushable::flush` and config
//! `validate()` methods.
//!
//! The remaining enum variants land in commit 6 and PR4.0 §22.

use thiserror::Error;

/// Concrete client error returned by `Flushable::flush` and config
/// `validate()` methods.
#[derive(Debug, Error)]
pub enum WyrdClientError {
    /// Structural validation failed on a config struct. Maps to
    /// `WYRD_CLIENT_400_CONFIG_INVALID` at the public boundary
    /// (`WyrdClient::new()` and the Python wrapper). Owned by the
    /// `WyrdClientError` catalog block in
    /// `02-foundations/03-error-catalog.md`. Not `WYRD_SPEC_400_VALIDATION`;
    /// that code is reserved for `wyrd-spec` contract violations only.
    #[error("config validation failed: {field}: {reason}")]
    Config {
        /// Dotted-path name of the offending config field, e.g.
        /// `"grpc_config.endpoint"` or `"queue_config.sample_ratio"`. Echoed
        /// verbatim into the catalog payload's `field`.
        field: String,
        /// Short human reason, e.g. `"must not be empty"` or
        /// `"must be in [0.0, 1.0]"`. Echoed verbatim into the catalog
        /// payload's `reason`.
        reason: String,
    },
}
