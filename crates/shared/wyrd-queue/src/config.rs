//! Producer/queue configuration.

use std::time::Duration;

/// Tuning knobs for the two-stage producer.
///
/// Defaults are sane local-development values; a surface crate overrides them
/// per deployment. All capacities are row counts except `max_message_bytes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueConfig {
    /// Handle-wide ceiling for all client-owned row, IPC, and retry bytes.
    /// Values above 32 MiB are clamped by [`Self::client_byte_limit`].
    pub client_byte_limit_bytes: usize,
    /// Maximum distinct producers a handle may register. Values above 64 are
    /// clamped by [`Self::max_producers`].
    pub max_producers: usize,
    /// Stage-1 bounded `tokio::mpsc` depth (the caller-facing hand-off).
    pub channel_capacity: usize,
    /// Stage-2 `crossbeam_queue::ArrayQueue` staging depth.
    pub staging_capacity: usize,
    /// Size trigger: seal once staging holds at least this many rows.
    pub flush_max_rows: usize,
    /// Time trigger: seal every this many milliseconds. `0` disables the timer.
    pub flush_interval_ms: u64,
    /// Drain/await deadline (ms) for an in-flight `send` → `WYRD_CLIENT_504_FLUSH_TIMEOUT`.
    pub flush_timeout_ms: u64,
    /// Sealed IPC ceiling (bytes). An oversize seal splits across batches; a
    /// single row over the ceiling → `WYRD_CLIENT_413_PAYLOAD_TOO_LARGE`.
    pub max_message_bytes: usize,
}

impl QueueConfig {
    /// The accepted default and maximum Rust-client byte ownership ceiling.
    pub const MAX_CLIENT_BYTE_LIMIT: usize = 32 * 1024 * 1024;
    /// The accepted maximum producer, live-batch, retry, and command cardinality.
    pub const MAX_LIVE_ENTRIES: usize = 64;

    /// Returns the handle byte ceiling after enforcing the accepted policy cap.
    #[must_use]
    pub fn client_byte_limit(&self) -> usize {
        self.client_byte_limit_bytes
            .min(Self::MAX_CLIENT_BYTE_LIMIT)
    }

    /// Returns the producer cardinality after enforcing the accepted policy cap.
    #[must_use]
    pub fn max_producers(&self) -> usize {
        self.max_producers.min(Self::MAX_LIVE_ENTRIES)
    }

    /// Returns the accepted stage-one row ceiling without permitting growth.
    #[must_use]
    pub(crate) fn channel_capacity(&self) -> usize {
        self.channel_capacity.clamp(1, 1_024)
    }

    /// Returns the accepted staging row ceiling without permitting growth.
    #[must_use]
    pub(crate) fn staging_capacity(&self) -> usize {
        self.staging_capacity.clamp(1, 4_096)
    }

    /// Returns the accepted per-producer seal row ceiling without permitting growth.
    #[must_use]
    pub(crate) fn flush_max_rows(&self) -> usize {
        self.flush_max_rows.clamp(1, 50_000)
    }

    /// Returns the bounded send deadline, treating zero as one millisecond.
    ///
    /// A nonzero timeout guarantees that a borrowed sink future can be
    /// cancelled while the queue retains its sealed owner for ambiguity
    /// resolution; zero would otherwise create an accidental busy timeout.
    #[must_use]
    pub(crate) fn flush_timeout(&self) -> Duration {
        Duration::from_millis(self.flush_timeout_ms.max(1))
    }
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            client_byte_limit_bytes: Self::MAX_CLIENT_BYTE_LIMIT,
            max_producers: Self::MAX_LIVE_ENTRIES,
            channel_capacity: 1024,
            staging_capacity: 4096,
            flush_max_rows: 50_000,
            flush_interval_ms: 1000,
            flush_timeout_ms: 30_000,
            max_message_bytes: 4 * 1024 * 1024,
        }
    }
}
