//! Producer/queue configuration.

/// Tuning knobs for the two-stage producer.
///
/// Defaults are sane local-development values; a surface crate overrides them
/// per deployment. All capacities are row counts except `max_message_bytes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueConfig {
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

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            channel_capacity: 1024,
            staging_capacity: 4096,
            flush_max_rows: 50_000,
            flush_interval_ms: 1000,
            flush_timeout_ms: 30_000,
            max_message_bytes: 4 * 1024 * 1024,
        }
    }
}
