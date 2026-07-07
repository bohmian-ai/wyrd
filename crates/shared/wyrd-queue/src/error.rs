//! Queue error taxonomy.
//!
//! [`WyrdQueueError`] keeps the stable client-tier `WYRD_CLIENT_*` code strings
//! for the queue/producer-domain failures and the shared `WYRD_VALA_*` codes for
//! the serialization-domain failures. It maps to
//! [`wyrd_spec::error::WyrdError`] at the surface boundary; the `Sink` variant
//! carries an already-mapped server error (the sink does the transport→`WyrdError`
//! translation in `vala-sdk`/`wyrd-client`).

use wyrd_spec::error::WyrdError;

/// Concrete error produced by the producer, queue, and batch builder.
#[derive(thiserror::Error, Debug)]
pub enum WyrdQueueError {
    /// The bounded ingestion channel is saturated and, after bounded backoff,
    /// still cannot accept the row (or the queue has been drained and is closed).
    /// Client-tier `WYRD_CLIENT_429_QUEUE_FULL`. Always paired with a drop-counter
    /// bump — never a silent drop.
    #[error("queue full: ingestion channel saturated")]
    QueueFull,

    /// A drain (`flush`/`shutdown`) could not complete an in-flight `send` before
    /// the `flush_timeout_ms` deadline. Client-tier `WYRD_CLIENT_504_FLUSH_TIMEOUT`.
    #[error("flush timed out before the drain deadline")]
    FlushTimeout,

    /// A single sealed batch (or a single row that alone exceeds the ceiling)
    /// cannot fit under `max_message_bytes`. Client-tier
    /// `WYRD_CLIENT_413_PAYLOAD_TOO_LARGE`.
    #[error("payload too large: sealed row exceeds max_message_bytes")]
    PayloadTooLarge,

    /// A schema could not be mapped, or a row value failed its column's
    /// `DataTypeSpec` at build time. Serialization-domain `WYRD_VALA_400_SCHEMA_PARSE`.
    #[error("schema parse: {0}")]
    SchemaParse(String),

    /// A reserved column name (`wyrd_*`, or `card_ref`/`run_id` presented as a
    /// payload key rather than via its argument) appeared where user fields are
    /// expected. Serialization-domain `WYRD_VALA_400_BIFROST_RESERVED_COLUMN`.
    #[error("reserved column: {0}")]
    ReservedColumn(String),

    /// A sink-reported server error, already mapped to the stable catalog.
    #[error("sink error: {0}")]
    Sink(#[source] WyrdError),
}

impl WyrdQueueError {
    /// Stable code string for this error.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::QueueFull => "WYRD_CLIENT_429_QUEUE_FULL",
            Self::FlushTimeout => "WYRD_CLIENT_504_FLUSH_TIMEOUT",
            Self::PayloadTooLarge => "WYRD_CLIENT_413_PAYLOAD_TOO_LARGE",
            Self::SchemaParse(_) => "WYRD_VALA_400_SCHEMA_PARSE",
            Self::ReservedColumn(_) => "WYRD_VALA_400_BIFROST_RESERVED_COLUMN",
            Self::Sink(err) => err.code(),
        }
    }
}

impl From<WyrdQueueError> for WyrdError {
    /// Map to the stable [`WyrdError`] catalog at the surface boundary.
    ///
    /// The queue-domain codes project onto typed `WyrdError` variants via the
    /// shared `WyrdError::from_code` reconstruction (so the client boundary
    /// reports the real status/code); the `Sink` variant passes its already-mapped
    /// error straight through.
    fn from(err: WyrdQueueError) -> Self {
        if let WyrdQueueError::Sink(inner) = err {
            return inner;
        }
        let code = err.code();
        let message = err.to_string();
        WyrdError::from_code(code, message.clone(), serde_json::json!({})).unwrap_or(
            WyrdError::Internal {
                message,
                details: serde_json::json!({ "original_code": code }),
            },
        )
    }
}
