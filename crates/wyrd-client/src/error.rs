//! Concrete client errors returned by transport configuration and queues.

use thiserror::Error;

/// Error returned by `wyrd-client` config validation and flush operations.
#[derive(Debug, Error)]
pub enum WyrdClientError {
    /// Structural validation failed on a config struct. Maps to
    /// `WYRD_CLIENT_400_CONFIG_INVALID` at the public boundary.
    #[error("config validation failed: {field}: {reason}")]
    Config {
        /// Dotted-path field name, echoed into the catalog payload's `field`.
        field: String,
        /// Short human reason, echoed into the catalog payload's `reason`.
        reason: String,
    },

    /// A flush failed because the selected transport is unavailable.
    #[error("transport unavailable ({transport}): {message}")]
    TransportDown {
        /// Transport tag (`"grpc"`, `"http"`, `"mock"`).
        transport: String,
        /// Human-readable failure description.
        message: String,
    },

    /// A flush failed because the request body exceeded the server limit.
    #[error("payload too large ({transport}): {message}")]
    PayloadTooLarge {
        /// Transport tag (`"grpc"`, `"http"`, `"mock"`).
        transport: String,
        /// Human-readable failure description.
        message: String,
    },

    /// A flush exceeded its per-call timeout.
    #[error("flush timed out ({transport}) after {timeout_ms} ms")]
    FlushTimeout {
        /// Transport tag (`"grpc"`, `"http"`, `"mock"`).
        transport: String,
        /// Timeout, in milliseconds, that fired.
        timeout_ms: u64,
    },
}
