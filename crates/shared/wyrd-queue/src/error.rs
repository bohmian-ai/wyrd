//! Queue error taxonomy.
//!
//! [`WyrdQueueError`] keeps the stable client-tier `WYRD_CLIENT_*` code strings
//! for the queue/producer-domain failures and the shared `WYRD_VALA_*` codes for
//! the serialization-domain failures. It maps to
//! [`wyrd_spec::error::WyrdError`] at the surface boundary; the `Sink` variant
//! carries an already-mapped server error (the sink does the transport→`WyrdError`
//! translation in `wyrd_client::bifrost`).

use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::BifrostError;

/// Concrete error produced by the producer, queue, and batch builder.
#[derive(thiserror::Error, Debug)]
pub enum WyrdQueueError {
    /// The producer has been drained and is closed, so it admits nothing.
    /// Client-tier `WYRD_CLIENT_429_QUEUE_FULL`. Always paired with a
    /// drop-counter bump — never a silent drop.
    #[error("queue full: the producer is closed to admission")]
    QueueFull,

    /// The handle-wide byte budget cannot hold every row of the record now,
    /// or a producer's one control slot is occupied. Admission is immediate
    /// and all-or-none: no row of the refused record was admitted, so the
    /// caller may flush or back off and resubmit it without duplication.
    /// Client-tier `WYRD_CLIENT_429_QUEUE_FULL`.
    #[error("queue backpressure: handle-wide bounded envelope is saturated")]
    Backpressure,

    /// A drain (`flush`/`shutdown`) could not complete an in-flight `send` before
    /// the `flush_timeout_ms` deadline. Client-tier `WYRD_CLIENT_504_FLUSH_TIMEOUT`.
    #[error("flush timed out before the drain deadline")]
    FlushTimeout,

    /// A single row alone cannot fit under `max_message_bytes`, or one
    /// record is larger than the whole admission budget. Retrying cannot
    /// succeed. Client-tier `WYRD_CLIENT_413_PAYLOAD_TOO_LARGE`.
    #[error(
        "payload too large: a row exceeds max_message_bytes or a record exceeds the client byte budget"
    )]
    PayloadTooLarge,

    /// The queue configuration cannot admit and seal one message.
    /// Client-tier `WYRD_CLIENT_400_CONFIG_INVALID`.
    #[error("queue configuration invalid: {field}: {reason}")]
    ConfigInvalid {
        /// The `QueueConfig` field that was refused.
        field: &'static str,
        /// Why the value was refused.
        reason: String,
    },

    /// A schema could not be mapped, or a row value failed its column's
    /// `DataTypeSpec` at build time. Serialization-domain `WYRD_VALA_400_SCHEMA_PARSE`.
    #[error("schema parse: {0}")]
    SchemaParse(String),

    /// A reserved column name (`wyrd_*`, or `card_ref`/`run_id` presented as a
    /// payload key rather than via its argument) appeared where user fields are
    /// expected. Serialization-domain `WYRD_VALA_400_BIFROST_RESERVED_COLUMN`.
    #[error("reserved column: {0}")]
    ReservedColumn(String),

    /// A catalogued Bifrost contract refusal raised before admission: an
    /// unsupported or undeclared field, or a Variant value that is invalid,
    /// out of range, too deep, or too large. Carries the stable error with
    /// its details.
    #[error("bifrost contract refused: {0}")]
    Contract(#[source] BifrostError),

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
            Self::Backpressure => "WYRD_CLIENT_429_QUEUE_FULL",
            Self::FlushTimeout => "WYRD_CLIENT_504_FLUSH_TIMEOUT",
            Self::PayloadTooLarge => "WYRD_CLIENT_413_PAYLOAD_TOO_LARGE",
            Self::ConfigInvalid { .. } => "WYRD_CLIENT_400_CONFIG_INVALID",
            Self::SchemaParse(_) => "WYRD_VALA_400_SCHEMA_PARSE",
            Self::ReservedColumn(_) => "WYRD_VALA_400_BIFROST_RESERVED_COLUMN",
            Self::Contract(err) => err.code(),
            Self::Sink(err) => err.code(),
        }
    }
}

impl From<&WyrdQueueError> for WyrdError {
    /// Map to the stable [`WyrdError`] catalog at the surface boundary.
    ///
    /// Saturation, drain, payload, and configuration refusals keep their own
    /// `WYRD_CLIENT_*` codes so a caller can retry a full queue without parsing
    /// error text; the serialization-domain refusals become their catalogued
    /// Bifrost error with its details, and a sink failure is already a catalog
    /// error and passes through unchanged.
    fn from(error: &WyrdQueueError) -> Self {
        let message = error.to_string();
        let details = serde_json::json!({});
        match error {
            WyrdQueueError::QueueFull | WyrdQueueError::Backpressure => {
                WyrdError::ClientQueueFull { message, details }
            }
            WyrdQueueError::FlushTimeout => WyrdError::ClientFlushTimeout { message, details },
            WyrdQueueError::PayloadTooLarge => {
                WyrdError::ClientPayloadTooLarge { message, details }
            }
            WyrdQueueError::ConfigInvalid { field, reason } => WyrdError::ClientConfigInvalid {
                message,
                details: serde_json::json!({ "field": field, "reason": reason }),
            },
            WyrdQueueError::SchemaParse(detail) => BifrostError::SchemaParse {
                detail: detail.clone(),
            }
            .into(),
            WyrdQueueError::ReservedColumn(column) => BifrostError::ReservedColumn {
                column: column.clone(),
            }
            .into(),
            WyrdQueueError::Contract(error) => error.clone().into(),
            WyrdQueueError::Sink(inner) => inner.clone(),
        }
    }
}
