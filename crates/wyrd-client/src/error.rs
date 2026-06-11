//! Concrete client error returned by `Flushable::flush` and config
//! `validate()` methods.
//!
//! Additional variants land in PR4.0 section 22.

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

    /// A flush failed because the transport itself is unavailable
    /// (DNS, TCP, or gRPC/HTTP connect failure). The buffered records remain
    /// in the queue; `Flushable::len()` reflects what was not flushed. Maps
    /// to `WYRD_CLIENT_503_TRANSPORT_DOWN` at the public boundary.
    #[error("transport unavailable ({transport}): {message}")]
    TransportDown {
        /// Transport tag (`"grpc"`, `"http"`, `"mock"`). Echoed into the
        /// catalog payload's `transport`.
        transport: String,
        /// Human-readable failure description. Echoed into the catalog
        /// payload's `message`.
        message: String,
    },

    /// A flush failed because the request body exceeded the server limit.
    /// Caller should reduce `flush_max_rows`. Maps to
    /// `WYRD_CLIENT_413_PAYLOAD_TOO_LARGE` at the public boundary.
    #[error("payload too large ({transport}): {message}")]
    PayloadTooLarge {
        /// Transport tag. Echoed into the catalog payload's `transport`.
        transport: String,
        /// Human-readable failure description. Echoed into the catalog
        /// payload's `message`.
        message: String,
    },

    /// A flush exceeded its per-call timeout. Records remain queued. Maps to
    /// `WYRD_CLIENT_504_FLUSH_TIMEOUT` at the public boundary.
    #[error("flush timed out ({transport}) after {timeout_ms} ms")]
    FlushTimeout {
        /// Transport tag. Echoed into the catalog payload's `transport`.
        transport: String,
        /// Timeout (ms) that fired. Echoed into the catalog payload's
        /// `timeout_ms`.
        timeout_ms: u64,
    },
}
